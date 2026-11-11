//! FormDesign's native GPU compositor: the canvas of the opt-in native shell drawn by the graphics processor, through wgpu
//! (Metal on macOS, Vulkan on Linux, Direct3D 12 on Windows). The layout core (formdesign-render, CPU raster through
//! vello_cpu) produces page tiles of 256 by 256 pixels as straight (unpremultiplied) RGBA8, the same bytes it hands the
//! web canvas. This crate keeps those tiles on the GPU in a `TileAtlas`, and a `Compositor` draws the visible spread
//! from them: the pasteboard colour, each page's drop shadow, and the page's tiles through the view's zoom, pan and
//! device pixel ratio. Pixels are straight RGBA8 on the GPU, as formdesign-render emits them; the shader premultiplies when
//! it samples, so readback returns exactly the bytes that were uploaded.

use std::collections::HashMap;

pub mod budget;
pub mod composite;
pub mod display;

/// The side of a tile in raster pixels, formdesign-render's tile size.
pub const TILE: u32 = 256;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// The device the compositor draws with, and what it is.
pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub info: wgpu::AdapterInfo,
    pub limits: wgpu::Limits,
    pub adapter: wgpu::Adapter,
}

impl Gpu {
    /// The best adapter there is (high performance first), without a window; None when the machine has no usable GPU
    /// backend at all.
    pub fn new() -> Option<Gpu> {
        Self::build(None).map(|(g, _)| g)
    }

    /// A device for drawing into a window, with the window's surface: the adapter is chosen to be able to present to it.
    pub fn for_window(target: impl Into<wgpu::SurfaceTarget<'static>>) -> Option<(Gpu, wgpu::Surface<'static>)> {
        let (g, s) = Self::build(Some(target.into()))?;
        Some((g, s?))
    }

    fn build(target: Option<wgpu::SurfaceTarget<'static>>) -> Option<(Gpu, Option<wgpu::Surface<'static>>)> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let surface = match target {
            Some(t) => Some(instance.create_surface(t).ok()?),
            None => None,
        };
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: surface.as_ref(),
            apply_limit_buckets: false,
        }))
        .ok()?;
        let info = adapter.get_info();
        let limits = adapter.limits();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("formdesign-gpu"),
            required_limits: limits.clone(),
            ..Default::default()
        }))
        .ok()?;
        device.on_uncaptured_error(std::sync::Arc::new(|e: wgpu::Error| eprintln!("[formdesign-gpu] {e}")));
        Some((Gpu { device, queue, info, limits, adapter }, surface))
    }
}

/// Which tile of which page: the page's index in the document, the zoom level the tile was rastered at, and the tile's
/// column and row. At `level` the raster has 2^level pixels per page unit (level 0 is one pixel per unit, 1 is double,
/// -1 is half), so a level is a scale bucket: one raster serves every view scale that rounds up to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileKey {
    pub page: u32,
    pub level: i8,
    pub tx: u32,
    pub ty: u32,
}

/// Raster pixels per page unit at `level`.
pub fn level_scale(level: i8) -> f32 {
    (level as f32).exp2()
}

struct Entry {
    texture: wgpu::Texture,
    /// The pixels the upload covered (edge tiles are smaller than TILE).
    w: u32,
    h: u32,
    /// The atlas tick of the last upload or use; the smallest is evicted first.
    used: u64,
}

/// Page tiles resident on the GPU, keyed by (page, level, tx, ty).
///
/// One texture per tile, not one big atlas texture: a texture of TILE by TILE is the unit the budget counts and the unit
/// eviction frees, a tile can be replaced or dropped without moving any other, there is no packing or shelf bookkeeping,
/// and bleeding between neighbours cannot happen. The price is one bind group per drawn tile, which at the few dozen
/// tiles of a spread is nothing against the frame's other costs. Eviction is least recently used against a byte budget:
/// an upload that would pass the budget first drops the tiles unused for longest. The compositor marks every tile it
/// draws as used, so what is on screen is the last to go.
pub struct TileAtlas {
    tiles: HashMap<TileKey, Entry>,
    budget: u64,
    tick: u64,
}

impl TileAtlas {
    /// An atlas that may hold `budget` bytes of tiles (at least one tile is always kept).
    pub fn new(budget: u64) -> TileAtlas {
        TileAtlas { tiles: HashMap::new(), budget, tick: 0 }
    }

    /// An atlas with this machine's budget (see `budget::system_budget`).
    pub fn with_system_budget() -> TileAtlas {
        Self::new(budget::system_budget().bytes)
    }

    pub fn budget(&self) -> u64 {
        self.budget
    }

    /// Changes the budget; tiles over it are evicted at the next upload.
    pub fn set_budget(&mut self, bytes: u64) {
        self.budget = bytes;
    }

    /// How many tiles are resident.
    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    /// The bytes the resident tiles take on the GPU.
    pub fn bytes(&self) -> u64 {
        self.tiles.len() as u64 * budget::TILE_BYTES
    }

    pub fn contains(&self, key: &TileKey) -> bool {
        self.tiles.contains_key(key)
    }

    /// Puts straight RGBA8 pixels (w by h, rows packed, both at most TILE) in the tile `key`, replacing any earlier
    /// pixels, after evicting least recently used tiles as far as the budget needs. Returns the keys evicted, so the
    /// caller knows what it must raster again if it comes back into view.
    pub fn upload(&mut self, gpu: &Gpu, key: TileKey, w: u32, h: u32, rgba: &[u8]) -> Vec<TileKey> {
        assert!(w > 0 && h > 0 && w <= TILE && h <= TILE, "a tile is 1 to {TILE} pixels on each side");
        assert_eq!(rgba.len(), (w * h * 4) as usize, "tile pixels are w * h * 4 bytes of RGBA8");
        self.tick += 1;
        let mut evicted = Vec::new();
        if !self.tiles.contains_key(&key) {
            while !self.tiles.is_empty() && self.bytes() + budget::TILE_BYTES > self.budget {
                let oldest = self.tiles.iter().min_by_key(|(_, e)| e.used).map(|(k, _)| *k).expect("the atlas is not empty");
                self.tiles.remove(&oldest);
                evicted.push(oldest);
            }
        }
        let tick = self.tick;
        let entry = self.tiles.entry(key).or_insert_with(|| Entry {
            texture: gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("page tile"),
                size: wgpu::Extent3d { width: TILE, height: TILE, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            }),
            w,
            h,
            used: tick,
        });
        entry.w = w;
        entry.h = h;
        entry.used = tick;
        gpu.queue.write_texture(
            entry.texture.as_image_copy(),
            rgba,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        evicted
    }

    /// Drops one tile; true when it was resident.
    pub fn remove(&mut self, key: &TileKey) -> bool {
        self.tiles.remove(key).is_some()
    }

    /// Drops every tile of a page (its content changed), at any level.
    pub fn remove_page(&mut self, page: u32) {
        self.tiles.retain(|k, _| k.page != page);
    }

    /// Drops every tile.
    pub fn clear(&mut self) {
        self.tiles.clear();
    }

    /// Marks a tile as just used, so eviction leaves it for last.
    pub(crate) fn touch(&mut self, key: &TileKey) {
        self.tick += 1;
        let tick = self.tick;
        if let Some(e) = self.tiles.get_mut(key) {
            e.used = tick;
        }
    }

    pub(crate) fn texture(&self, key: &TileKey) -> Option<&wgpu::Texture> {
        self.tiles.get(key).map(|e| &e.texture)
    }

    /// A page's raster pixels in a rectangle at `level` (raster pixels, origin at the page's top-left), straight RGBA8;
    /// transparent where there is no tile. For verification, not for the frame loop: each tile is copied back whole.
    pub fn readback(&self, gpu: &Gpu, page: u32, level: i8, x: u32, y: u32, w: u32, h: u32) -> Vec<u8> {
        let mut out = vec![0u8; (w as usize) * (h as usize) * 4];
        if w == 0 || h == 0 {
            return out;
        }
        for ty in y / TILE..=(y + h - 1) / TILE {
            for tx in x / TILE..=(x + w - 1) / TILE {
                let Some(e) = self.tiles.get(&TileKey { page, level, tx, ty }) else { continue };
                let px = read_rgba(gpu, &e.texture, TILE, TILE);
                for py in 0..e.h {
                    let ly = ty * TILE + py;
                    if ly < y || ly >= y + h {
                        continue;
                    }
                    for qx in 0..e.w {
                        let lx = tx * TILE + qx;
                        if lx < x || lx >= x + w {
                            continue;
                        }
                        let s = ((py * TILE + qx) * 4) as usize;
                        let d = (((ly - y) * w + (lx - x)) * 4) as usize;
                        out[d..d + 4].copy_from_slice(&px[s..s + 4]);
                    }
                }
            }
        }
        out
    }
}

/// The top-left w by h pixels of an RGBA8 texture copied back to the CPU, rows packed.
pub fn read_rgba(gpu: &Gpu, tex: &wgpu::Texture, w: u32, h: u32) -> Vec<u8> {
    // Rows in the copy buffer start on 256-byte boundaries.
    let padded = (w * 4).div_ceil(256) * 256;
    let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor { label: Some("read"), size: (padded * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        tex.as_image_copy(),
        wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(h) } },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    gpu.queue.submit([encoder.finish()]);
    let slice = buf.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
    let data = slice.get_mapped_range().expect("the read buffer is mapped").to_vec();
    buf.unmap();
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for row in 0..h as usize {
        out.extend_from_slice(&data[row * padded as usize..row * padded as usize + (w * 4) as usize]);
    }
    out
}

/// A small buffer filled with `bytes`, for `usage`.
pub(crate) fn buffer_with(device: &wgpu::Device, bytes: &[u8], usage: wgpu::BufferUsages) -> wgpu::Buffer {
    use wgpu::util::DeviceExt;
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("formdesign-gpu"), contents: bytes, usage })
}
