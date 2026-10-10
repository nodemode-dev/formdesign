use std::collections::HashMap;
use std::sync::Arc;

use formdes_model::color::Color;
use formdes_model::doc::Doc;
use formdes_model::ids::AssetId;
use krilla::action::LinkAction;
use krilla::annotation::{Annotation, LinkAnnotation, Target};
use krilla::blend::BlendMode;
use krilla::color::{cmyk, luma, rgb};
use krilla::destination::XyzDestination;
use krilla::geom::{Path, PathBuilder, Point, Rect, Size, Transform};
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{Fill, FillRule, LineCap, LineJoin, LinearGradient, Paint, RadialGradient, SpreadMethod, Stop, Stroke, StrokeDash};
use krilla::text::{Font, GlyphId, KrillaGlyph};
use krilla::Document;
use kurbo::{Affine, BezPath, PathEl};

use crate::images::{Images, is_pdf};
use crate::scene::{Brush, GlyphOp, LinkTarget, Op, PageScene, StrokeStyle};

#[derive(Clone, Debug, Default)]
pub struct PdfOpts {
    pub title: String,
}

fn kpath(p: &BezPath) -> Option<Path> {
    let mut b = PathBuilder::new();
    for el in p.elements() {
        match *el {
            PathEl::MoveTo(a) => b.move_to(a.x as f32, a.y as f32),
            PathEl::LineTo(a) => b.line_to(a.x as f32, a.y as f32),
            PathEl::QuadTo(a, c) => b.quad_to(a.x as f32, a.y as f32, c.x as f32, c.y as f32),
            PathEl::CurveTo(a, c, d) => b.cubic_to(a.x as f32, a.y as f32, c.x as f32, c.y as f32, d.x as f32, d.y as f32),
            PathEl::ClosePath => b.close(),
        }
    }
    b.finish()
}

fn kxf(a: Affine) -> Transform {
    let c = a.as_coeffs();
    Transform::from_row(c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32, c[4] as f32, c[5] as f32)
}

fn kcolor(c: Color) -> krilla::color::Color {
    let u = |x: f64| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    match c {
        Color::Cmyk([c0, m, y, k]) => cmyk::Color::new(u(c0), u(m), u(y), u(k)).into(),
        Color::Gray(g) => luma::Color::new(u(1.0 - g)).into(),
        other => {
            let [r, g, b] = other.to_rgb();
            rgb::Color::new(u(r), u(g), u(b)).into()
        }
    }
}

fn nf(x: f64) -> NormalizedF32 {
    NormalizedF32::new(x.clamp(0.0, 1.0) as f32).unwrap_or(NormalizedF32::ONE)
}

fn stops(v: &[(f64, Color, f64)], alpha: f64) -> Vec<Stop> {
    let all_cmyk = v.iter().all(|(_, c, _)| matches!(c, Color::Cmyk(_)));
    v.iter()
        .map(|(o, c, a)| {
            let c = if all_cmyk { *c } else { Color::Rgb(c.to_rgb()) };
            Stop { offset: nf(*o), color: kcolor(c), opacity: nf(alpha * a) }
        })
        .collect()
}

fn kpaint(b: &Brush) -> (Paint, f64) {
    match b {
        Brush::Solid(c, a) => (kcolor(*c).into(), *a),
        Brush::Linear { start, end, stops: s, alpha } => (
            LinearGradient {
                x1: start.x as f32,
                y1: start.y as f32,
                x2: end.x as f32,
                y2: end.y as f32,
                transform: Transform::identity(),
                spread_method: SpreadMethod::Pad,
                stops: stops(s, 1.0),
                anti_alias: false,
            }
            .into(),
            *alpha,
        ),
        Brush::Radial { center, radius, stops: s, alpha } => (
            RadialGradient {
                fx: center.x as f32,
                fy: center.y as f32,
                fr: 0.0,
                cx: center.x as f32,
                cy: center.y as f32,
                cr: *radius as f32,
                transform: Transform::identity(),
                spread_method: SpreadMethod::Pad,
                stops: stops(s, 1.0),
                anti_alias: false,
            }
            .into(),
            *alpha,
        ),
    }
}

fn kfill(b: &Brush, even_odd: bool) -> Fill {
    let (paint, a) = kpaint(b);
    Fill { paint, opacity: nf(a), rule: if even_odd { FillRule::EvenOdd } else { FillRule::NonZero } }
}

fn kstroke(b: &Brush, s: &StrokeStyle) -> Stroke {
    let (paint, a) = kpaint(b);
    Stroke {
        paint,
        width: s.width as f32,
        miter_limit: 4.0,
        line_cap: match s.cap.as_str() {
            "round" => LineCap::Round,
            "square" | "projecting" => LineCap::Square,
            _ => LineCap::Butt,
        },
        line_join: match s.join.as_str() {
            "round" => LineJoin::Round,
            "bevel" => LineJoin::Bevel,
            _ => LineJoin::Miter,
        },
        opacity: nf(a),
        dash: (!s.dash.is_empty()).then(|| StrokeDash { array: s.dash.iter().map(|d| *d as f32).collect(), offset: 0.0 }),
    }
}

fn blend(name: &str) -> BlendMode {
    match name {
        "multiply" => BlendMode::Multiply,
        "screen" => BlendMode::Screen,
        "overlay" => BlendMode::Overlay,
        "darken" => BlendMode::Darken,
        "lighten" => BlendMode::Lighten,
        "colorDodge" | "color-dodge" => BlendMode::ColorDodge,
        "colorBurn" | "color-burn" => BlendMode::ColorBurn,
        "hardLight" | "hard-light" => BlendMode::HardLight,
        "softLight" | "soft-light" => BlendMode::SoftLight,
        "difference" => BlendMode::Difference,
        "exclusion" => BlendMode::Exclusion,
        "hue" => BlendMode::Hue,
        "saturation" => BlendMode::Saturation,
        "color" => BlendMode::Color,
        "luminosity" => BlendMode::Luminosity,
        _ => BlendMode::Normal,
    }
}

struct Fonts(HashMap<u32, Option<Font>>);

impl Fonts {
    fn get(&mut self, op: &GlyphOp) -> Option<Font> {
        self.0
            .entry(op.face.id)
            .or_insert_with(|| {
                let data: Arc<dyn AsRef<[u8]> + Send + Sync> = op.face.data.clone();
                Font::new(data.into(), op.face.index)
            })
            .clone()
    }
}

fn draw_glyphs(surface: &mut krilla::surface::Surface, op: &GlyphOp, font: Font) {
    let size = op.size as f32;
    surface.set_fill(op.fill.as_ref().map(|b| kfill(b, false)));
    surface.set_stroke(op.stroke.as_ref().map(|(b, w)| kstroke(b, &StrokeStyle { width: *w, cap: "butt".into(), join: "miter".into(), dash: Vec::new() })));
    if op.fill.is_none() && op.stroke.is_none() {
        return;
    }
    let skew = Affine::new([1.0, 0.0, -op.skew.to_radians().tan(), 1.0, 0.0, 0.0]);
    let glyph = |i: usize, adv: f64, off: f64| KrillaGlyph {
        glyph_id: GlyphId::new(op.glyphs[i].0),
        text_range: op.ranges.get(i).cloned().unwrap_or(0..0),
        x_advance: (adv / op.hscale.max(1e-6) / op.size) as f32,
        x_offset: (off / op.hscale.max(1e-6) / op.size) as f32,
        y_offset: (-op.glyphs[i].2 / op.size) as f32,
        y_advance: 0.0,
        location: None,
    };
    if let Some(per) = &op.per_glyph {
        for (i, xf) in per.iter().enumerate() {
            surface.push_transform(&kxf(*xf * Affine::scale_non_uniform(op.hscale, op.vscale) * skew));
            let g = [glyph(i, op.advances.get(i).copied().unwrap_or(0.0), 0.0)];
            surface.draw_glyphs(Point::from_xy(0.0, 0.0), &g, font.clone(), &op.text, size, false);
            surface.pop();
        }
        return;
    }
    let x0 = op.glyphs.first().map(|g| g.1).unwrap_or(0.0);
    let mut pen = x0;
    let mut out = Vec::with_capacity(op.glyphs.len());
    for i in 0..op.glyphs.len() {
        let adv = op.advances.get(i).copied().unwrap_or(0.0);
        out.push(glyph(i, adv, op.glyphs[i].1 - pen));
        pen += adv;
    }
    surface.push_transform(&kxf(op.xf * Affine::translate((x0, 0.0)) * Affine::scale_non_uniform(op.hscale, op.vscale) * skew));
    surface.draw_glyphs(Point::from_xy(0.0, 0.0), &out, font, &op.text, size, false);
    surface.pop();
}

fn image_for(asset: &formdes_model::item::Asset, images: &Images) -> Option<krilla::image::Image> {
    let data = || -> krilla::Data { asset.data.clone().into() };
    let m = asset.mime.as_str();
    let direct = match m {
        "image/jpeg" | "image/jpg" => krilla::image::Image::from_jpeg(data(), true).ok(),
        "image/png" => krilla::image::Image::from_png(data(), true).ok(),
        "image/gif" => krilla::image::Image::from_gif(data(), true).ok(),
        "image/webp" => krilla::image::Image::from_webp(data(), true).ok(),
        _ => None,
    };
    direct.or_else(|| {
        let d = images.get(asset, 1.0)?;
        Some(krilla::image::Image::from_rgba8(d.rgba.clone(), d.width, d.height))
    })
}

pub fn export(scenes: &[PageScene], doc: &Doc, images: &Images, opts: &PdfOpts) -> Result<Vec<u8>, String> {
    let mut document = Document::new();
    let mut fonts = Fonts(HashMap::new());
    let mut img_cache: HashMap<AssetId, Option<krilla::image::Image>> = HashMap::new();
    let mut pdf_cache: HashMap<AssetId, Option<krilla::pdf::PdfDocument>> = HashMap::new();
    let index_of: HashMap<usize, usize> = scenes.iter().enumerate().map(|(i, s)| (s.page, i)).collect();
    for scene in scenes {
        let b = &scene.bleed;
        let full_w = scene.width + b.inside + b.outside;
        let full_h = scene.height + b.top + b.bottom;
        let size = Size::from_wh(full_w as f32, full_h as f32).ok_or("page has no size")?;
        let trim = Rect::from_xywh(b.inside as f32, b.top as f32, scene.width as f32, scene.height as f32);
        let mut settings = PageSettings::new(size).with_trim_box(trim);
        if b.top + b.bottom + b.inside + b.outside > 0.0 {
            settings = settings.with_bleed_box(Rect::from_xywh(0.0, 0.0, full_w as f32, full_h as f32));
        }
        let mut page = document.start_page_with(settings);
        let mut links = Vec::new();
        {
            let mut surface = page.surface();
            surface.push_transform(&Transform::from_translate(b.inside as f32, b.top as f32));
            for op in &scene.ops {
                match op {
                    Op::Fill { path, brush, even_odd } => {
                        if let Some(p) = kpath(path) {
                            surface.set_stroke(None);
                            surface.set_fill(Some(kfill(brush, *even_odd)));
                            surface.draw_path(&p);
                        }
                    }
                    Op::Stroke { path, brush, style } => {
                        if let Some(p) = kpath(path) {
                            surface.set_fill(None);
                            surface.set_stroke(Some(kstroke(brush, style)));
                            surface.draw_path(&p);
                        }
                    }
                    Op::Glyphs(g) => {
                        if let Some(font) = fonts.get(g) {
                            draw_glyphs(&mut surface, g, font);
                        }
                    }
                    Op::Image { asset, xf, w, h } => {
                        let Some(a) = doc.assets.get(asset) else { continue };
                        let Some(sz) = Size::from_wh(*w as f32, *h as f32) else { continue };
                        surface.push_transform(&kxf(*xf));
                        if is_pdf(a) {
                            let pdf = pdf_cache
                                .entry(*asset)
                                .or_insert_with(|| krilla::pdf::Pdf::new(a.data.clone()).ok().map(|p| krilla::pdf::PdfDocument::new(Arc::new(p))))
                                .clone();
                            if let Some(pdf) = pdf {
                                surface.draw_pdf_page(&pdf, sz, a.page as usize);
                            }
                        } else if let Some(img) = img_cache.entry(*asset).or_insert_with(|| image_for(a, images)).clone() {
                            surface.draw_image(img, sz);
                        }
                        surface.pop();
                    }
                    Op::PushClip(path) => {
                        let p = kpath(path).unwrap_or_else(|| {
                            let mut e = PathBuilder::new();
                            e.move_to(0.0, 0.0);
                            e.line_to(0.0, 0.0);
                            e.finish().expect("degenerate clip path")
                        });
                        surface.push_clip_path(&p, &FillRule::NonZero);
                    }
                    Op::PushOpacity(o) => surface.push_opacity(nf(*o)),
                    Op::PushBlend(m) => surface.push_blend_mode(blend(m)),
                    Op::Pop => surface.pop(),
                    Op::Link { rect, target } => links.push((*rect, target.clone())),
                }
            }
            surface.pop();
            surface.finish();
        }
        for (r, t) in links {
            let Some(rect) = Rect::from_ltrb((r.x0 + b.inside) as f32, (r.y0 + b.top) as f32, (r.x1 + b.inside) as f32, (r.y1 + b.top) as f32) else { continue };
            let target = match t {
                LinkTarget::Url(u) => Target::Action(LinkAction::new(u).into()),
                LinkTarget::Page(p) => match index_of.get(&p) {
                    Some(i) => Target::Destination(XyzDestination::new(*i, Point::from_xy(0.0, 0.0)).into()),
                    None => continue,
                },
            };
            page.add_annotation(Annotation::new_link(LinkAnnotation::new(rect, target), None));
        }
        page.finish();
    }
    if let Some(serde_json::Value::Array(marks)) = doc.prefs.get("bookmarks") {
        let mut outline = krilla::outline::Outline::new();
        let mut top: Vec<krilla::outline::OutlineNode> = Vec::new();
        for m in marks {
            let Some(name) = m.get("name").and_then(|v| v.as_str()) else { continue };
            let Some(page) = m.get("page").and_then(|v| v.as_u64()).and_then(|p| index_of.get(&(p as usize))) else { continue };
            let y = m.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            let node = krilla::outline::OutlineNode::new(name.to_string(), XyzDestination::new(*page, Point::from_xy(0.0, y)));
            match (m.get("level").and_then(|v| v.as_u64()).unwrap_or(1), top.last_mut()) {
                (l, Some(parent)) if l > 1 => parent.push_child(node),
                _ => top.push(node),
            }
        }
        if !top.is_empty() {
            for n in top {
                outline.push_child(n);
            }
            document.set_outline(outline);
        }
    }
    let mut meta = krilla::metadata::Metadata::new();
    if !opts.title.is_empty() {
        meta = meta.title(opts.title.clone());
    }
    meta = meta.creator("formdes".into());
    document.set_metadata(meta);
    document.finish().map_err(|e| format!("{e:?}"))
}
