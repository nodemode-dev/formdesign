//! The frame: the pasteboard, then for each visible page its drop shadow, its paper and its tiles, drawn in one render
//! pass from the tiles in a `TileAtlas`. The view maps page units to screen: a point at page position p is at device
//! pixel ((p * scale) + (tx, ty)) * dpr, with the view's scale and translation in CSS pixels. Tiles are picked at the
//! level (scale bucket) that is the smallest raster at least as sharp as the view, so they are only ever minified:
//! drawn with linear filtering, or with nearest filtering and pixel-snapped edges when the view is exactly that level's
//! scale (1:1 on the device). The page's paper is always drawn under its tiles, so a tile that is not on the GPU yet
//! shows white, never a hole, and the frame returns the keys of those tiles for the caller to raster and upload.

use crate::{buffer_with, level_scale, Gpu, TileAtlas, TileKey, FORMAT, TILE};

const WGSL: &str = include_str!("composite.wgsl");

/// The shadow's blur sigma, in CSS pixels.
pub const SHADOW_BLUR: f32 = 8.0;
/// The shadow's offset below the page, in CSS pixels.
pub const SHADOW_OFFSET: f32 = 3.0;
/// The shadow's opacity at the page's edge.
pub const SHADOW_ALPHA: f32 = 0.35;
/// The coarsest and finest levels a frame picks.
pub const MIN_LEVEL: i8 = -8;
pub const MAX_LEVEL: i8 = 6;

/// One page of the spread, in page units: its index in the document and its rectangle in the pasteboard's space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageRect {
    pub index: u32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// What to draw: the target in device pixels, its pixel ratio, the pasteboard colour (straight RGBA, 0 to 1), the view
/// as (scale, tx, ty) with the translation in CSS pixels, and the pages that may be visible (those off screen are
/// skipped).
#[derive(Clone, Debug)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub dpr: f32,
    pub pasteboard: [f32; 4],
    pub view: [f32; 3],
    pub pages: Vec<PageRect>,
}

impl Frame {
    /// The level this frame draws its tiles from: the smallest whose raster is at least as sharp as the screen.
    pub fn level(&self) -> i8 {
        level_for(self.view[0], self.dpr)
    }
}

/// The level for a view scale and pixel ratio: ceil(log2(scale * dpr)), clamped.
pub fn level_for(scale: f32, dpr: f32) -> i8 {
    let device = (scale * dpr).max(1e-6);
    ((device.log2() - 1e-3).ceil() as i32).clamp(MIN_LEVEL as i32, MAX_LEVEL as i32) as i8
}

/// The per-quad data the shader reads (see composite.wgsl).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Instance {
    rect: [f32; 4],
    aux: [f32; 4],
    bounds: [f32; 4],
    params: [f32; 4],
}

enum Item {
    Shadow(Instance),
    Page(Instance),
    Tile(TileKey, Instance, bool),
}

pub struct Compositor {
    shadow: wgpu::RenderPipeline,
    page: wgpu::RenderPipeline,
    tile: wgpu::RenderPipeline,
    frame_layout: wgpu::BindGroupLayout,
    tile_layout: wgpu::BindGroupLayout,
    nearest: wgpu::Sampler,
    linear: wgpu::Sampler,
    format: wgpu::TextureFormat,
}

impl Compositor {
    /// A compositor that draws into targets of `format` (a surface's format, or `Compositor::TEXTURE_FORMAT`).
    pub fn new(gpu: &Gpu, format: wgpu::TextureFormat) -> Compositor {
        let module = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("composite"), source: wgpu::ShaderSource::Wgsl(WGSL.into()) });
        let frame_layout = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let tile_layout = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tile"),
            entries: &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let flat_layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("flat"), bind_group_layouts: &[Some(&frame_layout)], ..Default::default() });
        let tile_pl = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("tile"), bind_group_layouts: &[Some(&frame_layout), Some(&tile_layout)], ..Default::default() });
        let attrs = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4];
        // Premultiplied source over, for colour and alpha alike.
        let over = wgpu::BlendState {
            color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
            alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
        };
        let make = |label: &'static str, layout: &wgpu::PipelineLayout, fs: &'static str| {
            gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs"),
                    buffers: &[Some(wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Instance>() as u64, step_mode: wgpu::VertexStepMode::Instance, attributes: &attrs })],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(fs),
                    targets: &[Some(wgpu::ColorTargetState { format, blend: Some(over), write_mask: wgpu::ColorWrites::ALL })],
                    compilation_options: Default::default(),
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let shadow = make("shadow", &flat_layout, "fs_shadow");
        let page = make("page", &flat_layout, "fs_page");
        let tile = make("tile", &tile_pl, "fs_tile");
        let sampler = |filter| gpu.device.create_sampler(&wgpu::SamplerDescriptor { mag_filter: filter, min_filter: filter, address_mode_u: wgpu::AddressMode::ClampToEdge, address_mode_v: wgpu::AddressMode::ClampToEdge, ..Default::default() });
        let (nearest, linear) = (sampler(wgpu::FilterMode::Nearest), sampler(wgpu::FilterMode::Linear));
        Compositor { shadow, page, tile, frame_layout, tile_layout, nearest, linear, format }
    }

    /// The format `render` draws into.
    pub const TEXTURE_FORMAT: wgpu::TextureFormat = FORMAT;

    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// The tiles of `frame` that are visible and not in `atlas`, without drawing.
    pub fn missing(&self, atlas: &TileAtlas, frame: &Frame) -> Vec<TileKey> {
        self.plan(frame, frame.width, frame.height)
            .into_iter()
            .filter_map(|i| match i {
                Item::Tile(k, _, _) if !atlas.contains(&k) => Some(k),
                _ => None,
            })
            .collect()
    }

    /// Everything the frame draws, in order, in device pixels. A tile appears whether or not it is resident; the
    /// resident ones are drawn, the others are what `draw` reports missing.
    fn plan(&self, frame: &Frame, width: u32, height: u32) -> Vec<Item> {
        let (sw, sh) = (width as f32, height as f32);
        let [scale, tx, ty] = frame.view;
        let dpr = frame.dpr.max(1e-3);
        let level = frame.level();
        let k = level_scale(level);
        // Device pixels per raster pixel at this level; at most 1, since the level is picked from above.
        let r = scale * dpr / k;
        let nearest = r >= 1.0 - 1e-3;
        let mut items = Vec::new();
        for p in &frame.pages {
            let dx = (p.x * scale + tx) * dpr;
            let dy = (p.y * scale + ty) * dpr;
            let (dw, dh) = (p.w * scale * dpr, p.h * scale * dpr);
            let (blur, off) = (SHADOW_BLUR * dpr, SHADOW_OFFSET * dpr);
            let reach = blur * 2.0;
            let (shx, shy) = (dx, dy + off);
            // Off screen entirely, shadow included.
            if shx - reach >= sw || shy - reach >= sh || shx + dw + reach <= 0.0 || shy + dh + reach <= 0.0 {
                continue;
            }
            items.push(Item::Shadow(Instance {
                rect: [shx - reach, shy - reach, dw + reach * 2.0, dh + reach * 2.0],
                aux: [shx, shy, dw, dh],
                bounds: [0.0; 4],
                params: [blur, SHADOW_ALPHA, 0.0, 0.0],
            }));
            if dx < sw && dy < sh && dx + dw > 0.0 && dy + dh > 0.0 {
                items.push(Item::Page(Instance { rect: [dx, dy, dw, dh], aux: [0.0; 4], bounds: [0.0; 4], params: [0.0; 4] }));
            } else {
                continue;
            }
            // The page's raster at this level, in pixels, and the tile grid over it.
            let (rw, rh) = (p.w * k, p.h * k);
            let (nx, ny) = ((rw.ceil() as u32).div_ceil(TILE), (rh.ceil() as u32).div_ceil(TILE));
            let step = TILE as f32 * r;
            if nx == 0 || ny == 0 || step <= 0.0 {
                continue;
            }
            let range = |origin: f32, extent: f32, n: u32| {
                let lo = ((-origin) / step).floor().max(0.0) as u32;
                let hi = (((extent - origin) / step).ceil().max(0.0) as u32).min(n);
                lo..hi
            };
            for j in range(dy, sh, ny) {
                for i in range(dx, sw, nx) {
                    // The pixels of this tile that lie inside the page (the last row and column can be partial).
                    let vis_w = (rw - (i * TILE) as f32).min(TILE as f32);
                    let vis_h = (rh - (j * TILE) as f32).min(TILE as f32);
                    let (tw, th) = (vis_w.ceil().max(1.0), vis_h.ceil().max(1.0));
                    let (mut x0, mut y0) = (dx + (i * TILE) as f32 * r, dy + (j * TILE) as f32 * r);
                    let (mut x1, mut y1) = (x0 + vis_w * r, y0 + vis_h * r);
                    if nearest {
                        // Whole device pixels at 1:1, so every raster pixel lands on one device pixel.
                        (x0, y0, x1, y1) = (x0.round(), y0.round(), x1.round(), y1.round());
                    }
                    if x1 <= x0 || y1 <= y0 {
                        continue;
                    }
                    let t = TILE as f32;
                    items.push(Item::Tile(
                        TileKey { page: p.index, level, tx: i, ty: j },
                        Instance {
                            rect: [x0, y0, x1 - x0, y1 - y0],
                            aux: [0.0, 0.0, vis_w / t, vis_h / t],
                            bounds: [0.5 / t, 0.5 / t, (tw - 0.5) / t, (th - 0.5) / t],
                            params: [0.0; 4],
                        },
                        nearest,
                    ));
                }
            }
        }
        items
    }

    /// Draws `frame` into `target` (device pixels, `width` by `height`, of this compositor's format) and returns the
    /// visible tiles that are not in the atlas, in drawing order. The tiles drawn are marked used for eviction.
    pub fn draw(&self, gpu: &Gpu, atlas: &mut TileAtlas, frame: &Frame, target: &wgpu::TextureView, width: u32, height: u32) -> Vec<TileKey> {
        let items = self.plan(frame, width, height);
        let mut missing = Vec::new();
        let mut instances = Vec::with_capacity(items.len());
        for it in &items {
            match it {
                Item::Shadow(i) | Item::Page(i) => instances.push(*i),
                Item::Tile(k, i, _) => {
                    if atlas.contains(k) {
                        atlas.touch(k);
                    } else {
                        missing.push(*k);
                    }
                    instances.push(*i);
                }
            }
        }
        let frame_uniform = buffer_with(&gpu.device, bytemuck::cast_slice(&[width as f32, height as f32, frame.dpr, 0.0f32]), wgpu::BufferUsages::UNIFORM);
        let frame_bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("frame"),
            layout: &self.frame_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: frame_uniform.as_entire_binding() }],
        });
        let instance_buffer = if instances.is_empty() { None } else { Some(buffer_with(&gpu.device, bytemuck::cast_slice(&instances), wgpu::BufferUsages::VERTEX)) };
        // The tile bind groups, one per resident tile drawn, built before the pass so they outlive it.
        let views: Vec<Option<wgpu::TextureView>> = items
            .iter()
            .map(|it| match it {
                Item::Tile(k, _, _) => atlas.texture(k).map(|t| t.create_view(&Default::default())),
                _ => None,
            })
            .collect();
        let binds: Vec<Option<wgpu::BindGroup>> = items
            .iter()
            .zip(&views)
            .map(|(it, v)| match (it, v) {
                (Item::Tile(_, _, nearest), Some(view)) => Some(gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("tile"),
                    layout: &self.tile_layout,
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(if *nearest { &self.nearest } else { &self.linear }) },
                    ],
                })),
                _ => None,
            })
            .collect();
        let [pr, pg, pb, pa] = frame.pasteboard;
        let mut enc = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: pr as f64, g: pg as f64, b: pb as f64, a: pa as f64 }), store: wgpu::StoreOp::Store },
                })],
                ..Default::default()
            });
            if let Some(buf) = &instance_buffer {
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.set_bind_group(0, &frame_bind, &[]);
                for (n, it) in items.iter().enumerate() {
                    let n = n as u32;
                    match it {
                        Item::Shadow(_) => {
                            pass.set_pipeline(&self.shadow);
                            pass.draw(0..6, n..n + 1);
                        }
                        Item::Page(_) => {
                            pass.set_pipeline(&self.page);
                            pass.draw(0..6, n..n + 1);
                        }
                        Item::Tile(..) => {
                            if let Some(bind) = &binds[n as usize] {
                                pass.set_pipeline(&self.tile);
                                pass.set_bind_group(1, bind, &[]);
                                pass.draw(0..6, n..n + 1);
                            }
                        }
                    }
                }
            }
        }
        gpu.queue.submit([enc.finish()]);
        missing
    }

    /// Draws `frame` into a new RGBA8 texture of the frame's size, for verification and readback (see `read_rgba`);
    /// returns it with the missing tiles. Only valid for a compositor made with `Compositor::TEXTURE_FORMAT`.
    pub fn render(&self, gpu: &Gpu, atlas: &mut TileAtlas, frame: &Frame) -> (wgpu::Texture, Vec<TileKey>) {
        let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("frame"),
            size: wgpu::Extent3d { width: frame.width.max(1), height: frame.height.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        let missing = self.draw(gpu, atlas, frame, &view, frame.width.max(1), frame.height.max(1));
        (tex, missing)
    }
}
