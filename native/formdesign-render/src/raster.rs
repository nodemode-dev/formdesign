use formdesign_model::color::Color;
use formdesign_model::doc::Doc;
use kurbo::{Affine, BezPath, Cap, Join, Rect, Shape, Stroke};
use vello_cpu::color::{AlphaColor, Srgb};
use vello_cpu::peniko::{self, BlendMode, Compose, Mix};
use vello_cpu::{ImageSource, Mask, PaintType, Pixmap, RenderContext, Resources};

use crate::images::Images;
use crate::outline::Outlines;
use crate::scene::{Brush, GlyphOp, Op, PageScene, StrokeStyle};

pub struct RasterOpts {
    pub scale: f64,
    pub paper: Option<Color>,
    pub max_side: u32,
    /// Draw one separation plate (an index into `Doc::plate_names`) as ink density in grey.
    pub plate: Option<u8>,
}

impl Default for RasterOpts {
    fn default() -> Self {
        RasterOpts { scale: 1.0, paper: Some(Color::Rgb([1.0, 1.0, 1.0])), max_side: 12000, plate: None }
    }
}

pub struct Raster {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

fn rgb(c: Color, a: f64) -> AlphaColor<Srgb> {
    let [r, g, b] = c.to_rgb();
    AlphaColor::new([r as f32, g as f32, b as f32, a.clamp(0.0, 1.0) as f32])
}

fn paint(b: &Brush, dev: Affine) -> PaintType {
    match b {
        Brush::Solid(c, a) => PaintType::Solid(rgb(*c, *a)),
        Brush::Linear { start, end, stops, alpha } => {
            let st: Vec<(f32, AlphaColor<Srgb>)> = stops.iter().map(|(o, c, a)| (*o as f32, rgb(*c, *alpha * *a))).collect();
            PaintType::Gradient(peniko::Gradient::new_linear(dev * *start, dev * *end).with_stops(st.as_slice()))
        }
        Brush::Radial { center, radius, stops, alpha } => {
            let st: Vec<(f32, AlphaColor<Srgb>)> = stops.iter().map(|(o, c, a)| (*o as f32, rgb(*c, *alpha * *a))).collect();
            let k = dev.as_coeffs()[0].abs().max(1e-6);
            PaintType::Gradient(peniko::Gradient::new_radial(dev * *center, (*radius * k) as f32).with_stops(st.as_slice()))
        }
    }
}

fn blend(name: &str) -> BlendMode {
    let mix = match name {
        "multiply" => Mix::Multiply,
        "screen" => Mix::Screen,
        "overlay" => Mix::Overlay,
        "darken" => Mix::Darken,
        "lighten" => Mix::Lighten,
        "colorDodge" | "color-dodge" => Mix::ColorDodge,
        "colorBurn" | "color-burn" => Mix::ColorBurn,
        "hardLight" | "hard-light" => Mix::HardLight,
        "softLight" | "soft-light" => Mix::SoftLight,
        "difference" => Mix::Difference,
        "exclusion" => Mix::Exclusion,
        "hue" => Mix::Hue,
        "saturation" => Mix::Saturation,
        "color" => Mix::Color,
        "luminosity" => Mix::Luminosity,
        _ => Mix::Normal,
    };
    BlendMode::new(mix, Compose::SrcOver)
}

fn stroke_of(s: &StrokeStyle, k: f64) -> Stroke {
    let mut st = Stroke::new(s.width * k);
    st = st.with_caps(match s.cap.as_str() {
        "round" => Cap::Round,
        "square" | "projecting" => Cap::Square,
        _ => Cap::Butt,
    });
    let (join, limit) = crate::scene::join_parts(&s.join);
    st = st.with_join(match join {
        "round" => Join::Round,
        "bevel" => Join::Bevel,
        _ => Join::Miter,
    });
    st = st.with_miter_limit(limit);
    if !s.dash.is_empty() {
        st = st.with_dashes(0.0, s.dash.iter().map(|d| d * k).collect::<Vec<f64>>());
    }
    st
}

pub fn glyph_xf(op: &GlyphOp, i: usize) -> Affine {
    let (_, x, y) = op.glyphs[i];
    let k = op.size / op.face.metrics.upem;
    let skew = Affine::new([1.0, 0.0, op.skew.to_radians().tan(), 1.0, 0.0, 0.0]);
    let place = match &op.per_glyph {
        Some(v) => v[i],
        None => op.xf * Affine::translate((x, y)),
    };
    place * Affine::scale_non_uniform(k * op.hscale, -k * op.vscale) * skew
}

pub struct DrawEnv<'a> {
    pub doc: &'a Doc,
    pub outlines: &'a Outlines,
    pub images: &'a Images,
    pub dev: Affine,
    pub scale: f64,
    pub greek_px: f64,
    pub plate: Option<u8>,
}

fn plate_brush(b: &Brush, env: &DrawEnv) -> Brush {
    match env.plate {
        Some(p) => crate::sep::plate_brush(env.doc, b, p),
        None => b.clone(),
    }
}

pub fn draw_ops(ctx: &mut RenderContext, ops: &[Op], env: &DrawEnv) {
    let (dev, s) = (env.dev, env.scale);
    for op in ops {
        match op {
            Op::Fill { path, brush, even_odd } => {
                ctx.set_fill_rule(if *even_odd { peniko::Fill::EvenOdd } else { peniko::Fill::NonZero });
                ctx.set_paint(paint(&plate_brush(brush, env), dev));
                ctx.fill_path(&(dev * path.clone()));
            }
            Op::Stroke { path, brush, style } => {
                ctx.set_paint(paint(&plate_brush(brush, env), dev));
                ctx.set_stroke(stroke_of(style, s));
                ctx.stroke_path(&(dev * path.clone()));
            }
            Op::Glyphs(g) => {
                ctx.set_fill_rule(peniko::Fill::NonZero);
                let px_size = g.size * s * {
                    let c = g.xf.as_coeffs();
                    (c[0] * c[3] - c[1] * c[2]).abs().sqrt()
                };
                if env.greek_px > 0.0 && px_size < env.greek_px && g.per_glyph.is_none() && !g.glyphs.is_empty() {
                    let x0 = g.glyphs.first().map(|q| q.1).unwrap_or(0.0);
                    let x1 = g.glyphs.last().map(|q| q.1).unwrap_or(0.0) + g.advances.last().copied().unwrap_or(0.0);
                    let bar = Rect::new(x0, -g.size * 0.5, x1, -g.size * 0.1);
                    ctx.set_paint(PaintType::Solid(AlphaColor::new([0.6, 0.6, 0.6, 1.0])));
                    ctx.fill_path(&((dev * g.xf) * bar.to_path(0.1)));
                    continue;
                }
                for i in 0..g.glyphs.len() {
                    let outline = env.outlines.get(&g.face, g.glyphs[i].0);
                    if outline.elements().is_empty() {
                        continue;
                    }
                    let p: BezPath = (dev * glyph_xf(g, i)) * outline.as_ref().clone();
                    let brush_xf = if g.per_glyph.is_none() { dev * g.xf } else { dev };
                    if let Some(f) = &g.fill {
                        ctx.set_paint(paint(&plate_brush(f, env), brush_xf));
                        ctx.fill_path(&p);
                    }
                    if let Some((b, wgt)) = &g.stroke {
                        ctx.set_paint(paint(&plate_brush(b, env), brush_xf));
                        ctx.set_stroke(Stroke::new(wgt * s));
                        ctx.stroke_path(&p);
                    }
                }
            }
            Op::Image { asset, xf, w: iw, h: ih } => {
                let Some(a) = env.doc.assets.get(asset) else { continue };
                let k = xf.as_coeffs();
                let page_scale = ((k[0] * k[3] - k[1] * k[2]).abs().sqrt() * s).max(0.01);
                let pixmap = match env.plate {
                    Some(p) => env.images.get(a, page_scale).filter(|d| d.width > 0 && d.height > 0 && d.width <= 65535 && d.height <= 65535).map(|d| {
                        let px: Vec<vello_cpu::color::PremulRgba8> = crate::sep::plate_rgba(&d.rgba, p)
                            .chunks_exact(4)
                            .map(|c| {
                                let a = c[3] as u32;
                                let m = |v: u8| ((v as u32 * a + 127) / 255) as u8;
                                vello_cpu::color::PremulRgba8 { r: m(c[0]), g: m(c[1]), b: m(c[2]), a: c[3] }
                            })
                            .collect();
                        std::sync::Arc::new(Pixmap::from_parts(px, d.width as u16, d.height as u16))
                    }),
                    None => env.images.pixmap(a, page_scale),
                };
                let Some(pixmap) = pixmap else { continue };
                let (pw, ph) = (pixmap.width() as f64, pixmap.height() as f64);
                let to_dev = dev * *xf * Affine::scale_non_uniform(iw / pw, ih / ph);
                ctx.set_paint(PaintType::Image(vello_cpu::Image { image: ImageSource::Pixmap(pixmap), sampler: peniko::ImageSampler::default() }));
                ctx.set_paint_transform(to_dev);
                ctx.fill_path(&(to_dev * Rect::new(0.0, 0.0, pw, ph).to_path(0.1)));
                ctx.reset_paint_transform();
            }
            Op::PushClip(path) => ctx.push_clip_layer(&(dev * path.clone())),
            Op::PushOpacity(o) => ctx.push_opacity_layer(*o as f32),
            Op::PushBlend(b) => ctx.push_blend_layer(blend(b)),
            Op::PushMask(inner) => {
                let (w, h) = (ctx.width(), ctx.height());
                let mut mctx = RenderContext::new(w, h);
                let menv = DrawEnv { doc: env.doc, outlines: env.outlines, images: env.images, dev: env.dev, scale: env.scale, greek_px: env.greek_px, plate: None };
                draw_ops(&mut mctx, inner, &menv);
                mctx.flush();
                let mut pm = Pixmap::new(w, h);
                mctx.render(&mut pm, &mut Resources::default());
                ctx.push_mask_layer(Mask::new_luminance(&pm));
            }
            Op::Pop => ctx.pop_layer(),
            Op::Mesh { fallback, .. } => draw_ops(ctx, fallback, env),
            Op::Link { .. } | Op::Tag(_) => {}
        }
    }
}

pub fn rasterize(scene: &PageScene, doc: &Doc, outlines: &Outlines, images: &Images, opts: &RasterOpts) -> Raster {
    let full_w = scene.width + scene.bleed.inside + scene.bleed.outside;
    let full_h = scene.height + scene.bleed.top + scene.bleed.bottom;
    let mut s = opts.scale.max(0.01);
    let longest = full_w.max(full_h) * s;
    if longest > opts.max_side as f64 {
        s *= opts.max_side as f64 / longest;
    }
    let w = (full_w * s).ceil().clamp(1.0, 65535.0) as u16;
    let h = (full_h * s).ceil().clamp(1.0, 65535.0) as u16;
    let dev = Affine::scale(s) * Affine::translate((scene.bleed.inside, scene.bleed.top));
    let mut ctx = RenderContext::new(w, h);
    if let Some(p) = opts.paper {
        ctx.set_paint(PaintType::Solid(rgb(p, 1.0)));
        ctx.fill_rect(&Rect::new(0.0, 0.0, w as f64, h as f64));
    }
    draw_ops(&mut ctx, &scene.ops, &DrawEnv { doc, outlines, images, dev, scale: s, greek_px: 0.0, plate: opts.plate });
    ctx.flush();
    let mut pixmap = Pixmap::new(w, h);
    let mut res = Resources::default();
    ctx.render(&mut pixmap, &mut res);
    let rgba: Vec<u8> = pixmap.take_unpremultiplied().into_iter().flat_map(|p| [p.r, p.g, p.b, p.a]).collect();
    Raster { width: w as u32, height: h as u32, rgba }
}

pub fn encode_png(r: &Raster) -> Option<Vec<u8>> {
    let mut out = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_raw(r.width, r.height, r.rgba.clone())?.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

pub fn encode_jpeg(r: &Raster, quality: u8) -> Option<Vec<u8>> {
    let rgb: Vec<u8> = r.rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
    let mut out = Vec::new();
    let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality.clamp(1, 100));
    image::ImageEncoder::write_image(enc, &rgb, r.width, r.height, image::ExtendedColorType::Rgb8).ok()?;
    Some(out)
}
