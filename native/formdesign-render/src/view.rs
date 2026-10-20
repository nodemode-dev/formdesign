use std::sync::Arc;

use formdesign_model::doc::Doc;
use kurbo::{Affine, BezPath, Rect, Shape};
use vello_cpu::color::AlphaColor;
use vello_cpu::{PaintType, Pixmap, RenderContext, Resources};

use crate::images::Images;
use crate::outline::Outlines;
use crate::raster::{DrawEnv, Raster, draw_ops};
use crate::scene::Op;

#[derive(Clone, Debug)]
pub struct ViewOpts {
    pub background: [u8; 4],
    pub paper: bool,
    pub shadow: bool,
    pub printing_only: bool,
    pub greek_px: f64,
    pub plate: Option<u8>,
}

impl Default for ViewOpts {
    fn default() -> Self {
        ViewOpts { background: [28, 27, 29, 255], paper: true, shadow: true, printing_only: false, greek_px: 3.0, plate: None }
    }
}

pub struct Placed {
    pub xf: Affine,
    pub pages: Vec<Rect>,
    pub ops: Arc<Vec<Op>>,
}

fn rgba(c: [u8; 4]) -> PaintType {
    PaintType::Solid(AlphaColor::new([c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, c[3] as f32 / 255.0]))
}

#[allow(clippy::too_many_arguments)]
pub fn render_view(doc: &Doc, placed: &[Placed], w: u32, h: u32, view: Affine, opts: &ViewOpts, outlines: &Outlines, images: &Images) -> Raster {
    let (w16, h16) = (w.clamp(1, 8192) as u16, h.clamp(1, 8192) as u16);
    let mut ctx = RenderContext::new(w16, h16);
    ctx.set_paint(rgba(opts.background));
    ctx.fill_rect(&Rect::new(0.0, 0.0, w16 as f64, h16 as f64));
    let screen = Rect::new(0.0, 0.0, w16 as f64, h16 as f64);
    let k = {
        let c = view.as_coeffs();
        (c[0] * c[3] - c[1] * c[2]).abs().sqrt()
    };
    for p in placed {
        let dev = view * p.xf;
        let on_screen = p.pages.iter().any(|r| dev.transform_rect_bbox(r.inflate(2000.0, 2000.0)).intersect(screen).area() > 0.0);
        if !on_screen {
            continue;
        }
        if opts.paper {
            for r in &p.pages {
                if opts.shadow {
                    ctx.set_paint(rgba([0, 0, 0, 90]));
                    ctx.fill_path(&(Affine::translate((3.0, 3.0)) * (dev * r.to_path(0.1))));
                }
                ctx.set_paint(rgba([255, 255, 255, 255]));
                ctx.fill_path(&(dev * r.to_path(0.1)));
            }
        }
        let clip = opts.printing_only && !p.pages.is_empty();
        if clip {
            let mut path = BezPath::new();
            for r in &p.pages {
                path.extend(r.to_path(0.1));
            }
            ctx.push_clip_layer(&(dev * path));
        }
        draw_ops(&mut ctx, &p.ops, &DrawEnv { doc, outlines, images, dev, scale: k, greek_px: opts.greek_px, plate: opts.plate });
        if clip {
            ctx.pop_layer();
        }
    }
    ctx.flush();
    let mut pixmap = Pixmap::new(w16, h16);
    let mut res = Resources::default();
    ctx.render(&mut pixmap, &mut res);
    let rgba: Vec<u8> = pixmap.take_unpremultiplied().into_iter().flat_map(|p| [p.r, p.g, p.b, p.a]).collect();
    Raster { width: w16 as u32, height: h16 as u32, rgba }
}
