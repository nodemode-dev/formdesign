//! The canvas on screen: the spread composited by the Compositor into a window's GPU surface. The surface format is a
//! non-sRGB one, since FormDesign's pixel values are already sRGB-encoded bytes and an sRGB surface would encode them
//! again. The frame is drawn at the surface's size, whatever width and height the Frame carries; resize the display
//! when the window changes size.

use crate::composite::{Compositor, Frame};
use crate::{Gpu, TileAtlas, TileKey};

/// What presenting a frame came to.
#[derive(Debug, PartialEq, Eq)]
pub enum Presented {
    /// Drawn and shown.
    Shown,
    /// Not drawn this time (the surface was out of date, lost, timed out or hidden); try the next frame.
    Skipped,
}

pub struct Display {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    compositor: Compositor,
}

impl Display {
    pub fn new(gpu: &Gpu, surface: wgpu::Surface<'static>, width: u32, height: u32) -> Display {
        let caps = surface.get_capabilities(&gpu.adapter);
        let format = caps.formats.iter().copied().find(|f| !f.is_srgb()).unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&gpu.device, &config);
        let compositor = Compositor::new(gpu, format);
        Display { surface, config, compositor }
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    /// The surface's size in device pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    pub fn compositor(&self) -> &Compositor {
        &self.compositor
    }

    pub fn resize(&mut self, gpu: &Gpu, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&gpu.device, &self.config);
    }

    /// Draws `frame` and shows it. Returns the visible tiles that are not in the atlas (the page is drawn white there),
    /// for the caller to raster and upload; empty when the frame was skipped.
    pub fn present(&mut self, gpu: &Gpu, atlas: &mut TileAtlas, frame: &Frame) -> (Presented, Vec<TileKey>) {
        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&gpu.device, &self.config);
                return (Presented::Skipped, Vec::new());
            }
            _ => return (Presented::Skipped, Vec::new()),
        };
        let target = surface_texture.texture.create_view(&Default::default());
        let missing = self.compositor.draw(gpu, atlas, frame, &target, self.config.width, self.config.height);
        gpu.queue.present(surface_texture);
        (Presented::Shown, missing)
    }
}
