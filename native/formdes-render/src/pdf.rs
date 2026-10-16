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

#[derive(Clone)]
pub struct MarkFont(pub Arc<formdes_text::fonts::Face>);

impl std::fmt::Debug for MarkFont {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MarkFont({})", self.0.family)
    }
}

#[derive(Clone, Debug, Default)]
pub struct PdfOpts {
    pub title: String,
    pub tagged: bool,
    pub standard: Option<String>,
    pub language: Option<String>,
    pub cmyk_profile: Option<Vec<u8>>,
    /// Printer's marks drawn in a slug around each page, which grows the page to hold them.
    pub marks: Option<crate::marks::Marks>,
    /// The font the page information is set in.
    pub mark_font: Option<MarkFont>,
    /// Write one separation plate (an index into `Doc::plate_names`) as a grey page instead of the colours.
    pub plate: Option<u8>,
    /// PDF/X-1a: every colour is written as CMYK or grey (RGB and Lab are converted with the plain formula).
    pub cmyk_only: bool,
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
        Color::Ink { cmyk: Some([c0, m, y, k]), .. } => cmyk::Color::new(u(c0), u(m), u(y), u(k)).into(),
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
        miter_limit: crate::scene::join_parts(&s.join).1 as f32,
        line_cap: match s.cap.as_str() {
            "round" => LineCap::Round,
            "square" | "projecting" => LineCap::Square,
            _ => LineCap::Butt,
        },
        line_join: match crate::scene::join_parts(&s.join).0 {
            "round" => LineJoin::Round,
            "bevel" => LineJoin::Bevel,
            _ => LineJoin::Miter,
        },
        opacity: nf(a),
        dash: (!s.dash.is_empty()).then(|| StrokeDash { array: s.dash.iter().map(|d| *d as f32).collect(), offset: 0.0 }),
    }
}

fn mask_ops(surface: &mut krilla::surface::Surface, ops: &[Op]) {
    for op in ops {
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
            Op::PushClip(path) => match kpath(path) {
                Some(p) => surface.push_clip_path(&p, &FillRule::NonZero),
                None => {
                    let mut e = PathBuilder::new();
                    e.move_to(0.0, 0.0);
                    e.line_to(0.0, 0.0);
                    if let Some(p) = e.finish() {
                        surface.push_clip_path(&p, &FillRule::NonZero);
                    }
                }
            },
            Op::PushOpacity(o) => surface.push_opacity(nf(*o)),
            Op::Pop => surface.pop(),
            _ => {}
        }
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
                if op.face.variations.is_empty() {
                    Font::new(data.into(), op.face.index)
                } else {
                    let coords: Vec<(krilla::text::Tag, f32)> = op.face.variations.iter().filter_map(|(t, v)| krilla::text::Tag::try_from_str(t).map(|t| (t, *v))).collect();
                    Font::new_variable(data.into(), op.face.index, &coords)
                }
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

fn configuration(standard: Option<&str>) -> Result<krilla::configure::Configuration, String> {
    use krilla::configure::{Archival, ConfigurationBuilder, Validator};
    let b = ConfigurationBuilder::new();
    let b = match standard.unwrap_or("none") {
        "none" | "" => b,
        "pdfA2b" | "PDF/A-2b" | "pdf/a-2b" | "a2b" => b.set_validator(Validator::A(Archival::A2_B)),
        "pdfA2a" | "PDF/A-2a" | "a2a" => b.set_validator(Validator::A(Archival::A2_A)),
        "pdfA2u" | "PDF/A-2u" | "a2u" => b.set_validator(Validator::A(Archival::A2_U)),
        "pdfA3b" | "PDF/A-3b" | "a3b" => b.set_validator(Validator::A(Archival::A3_B)),
        "pdfA4" | "PDF/A-4" | "a4" => b.set_validator(Validator::A(Archival::A4)),
        "pdfUA1" | "PDF/UA-1" | "ua1" => b.set_validator(Validator::Ua(krilla::configure::Accessibility::UA1)),
        other => return Err(format!("the PDF standard {other} is not supported: use PDF/A-2b, PDF/A-2a, PDF/A-2u, PDF/A-3b, PDF/A-4 or PDF/UA-1")),
    };
    b.finish().map_err(|e| format!("{e:?}"))
}

fn rgb_color(c: Color) -> Color {
    match c {
        Color::Cmyk(_) | Color::Ink { .. } => {
            let [r, g, b] = c.to_rgb();
            Color::Rgb([r, g, b])
        }
        other => other,
    }
}

fn rgb_brush(b: &Brush) -> Brush {
    match b {
        Brush::Solid(c, a) => Brush::Solid(rgb_color(*c), *a),
        Brush::Linear { start, end, stops, alpha } => Brush::Linear { start: *start, end: *end, stops: stops.iter().map(|(t, c, a)| (*t, rgb_color(*c), *a)).collect(), alpha: *alpha },
        Brush::Radial { center, radius, stops, alpha } => Brush::Radial { center: *center, radius: *radius, stops: stops.iter().map(|(t, c, a)| (*t, rgb_color(*c), *a)).collect(), alpha: *alpha },
    }
}

fn rgb_ops(ops: &[Op]) -> Vec<Op> {
    ops.iter()
        .map(|op| match op {
            Op::Fill { path, brush, even_odd } => Op::Fill { path: path.clone(), brush: rgb_brush(brush), even_odd: *even_odd },
            Op::Stroke { path, brush, style } => Op::Stroke { path: path.clone(), brush: rgb_brush(brush), style: style.clone() },
            Op::Glyphs(g) => {
                let mut g = g.clone();
                g.fill = g.fill.as_ref().map(rgb_brush);
                g.stroke = g.stroke.as_ref().map(|(b, w)| (rgb_brush(b), *w));
                Op::Glyphs(g)
            }
            other => other.clone(),
        })
        .collect()
}

fn cmyk_color(c: Color) -> Color {
    match c {
        Color::Cmyk(_) | Color::Gray(_) | Color::Ink { cmyk: Some(_), .. } => c,
        other => {
            let [r, g, b] = other.to_rgb();
            let k = 1.0 - r.max(g).max(b);
            if k >= 1.0 {
                return Color::Cmyk([0.0, 0.0, 0.0, 1.0]);
            }
            Color::Cmyk([(1.0 - r - k) / (1.0 - k), (1.0 - g - k) / (1.0 - k), (1.0 - b - k) / (1.0 - k), k])
        }
    }
}

fn cmyk_brush(b: &Brush) -> Brush {
    match b {
        Brush::Solid(c, a) => Brush::Solid(cmyk_color(*c), *a),
        Brush::Linear { start, end, stops, alpha } => Brush::Linear { start: *start, end: *end, stops: stops.iter().map(|(t, c, a)| (*t, cmyk_color(*c), *a)).collect(), alpha: *alpha },
        Brush::Radial { center, radius, stops, alpha } => Brush::Radial { center: *center, radius: *radius, stops: stops.iter().map(|(t, c, a)| (*t, cmyk_color(*c), *a)).collect(), alpha: *alpha },
    }
}

fn cmyk_ops(ops: &[Op]) -> Vec<Op> {
    ops.iter()
        .map(|op| match op {
            Op::Fill { path, brush, even_odd } => Op::Fill { path: path.clone(), brush: cmyk_brush(brush), even_odd: *even_odd },
            Op::Stroke { path, brush, style } => Op::Stroke { path: path.clone(), brush: cmyk_brush(brush), style: style.clone() },
            Op::Glyphs(g) => {
                let mut g = g.clone();
                g.fill = g.fill.as_ref().map(cmyk_brush);
                g.stroke = g.stroke.as_ref().map(|(b, w)| (cmyk_brush(b), *w));
                Op::Glyphs(g)
            }
            other => other.clone(),
        })
        .collect()
}

fn today() -> krilla::metadata::DateTime {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    let rem = secs.rem_euclid(86_400);
    krilla::metadata::DateTime::new(y as u16).month(m as u8).day(d as u8).hour((rem / 3600) as u8).minute(((rem % 3600) / 60) as u8).second((rem % 60) as u8)
}

pub fn export(scenes: &[PageScene], doc: &Doc, images: &Images, opts: &PdfOpts) -> Result<Vec<u8>, String> {
    let archival = opts.standard.as_deref().is_some_and(|s| s.to_ascii_lowercase().contains('a') && !s.to_ascii_lowercase().contains("ua"));
    let ua = opts.standard.as_deref().is_some_and(|s| s.to_ascii_lowercase().contains("ua"));
    let profile = opts.cmyk_profile.as_deref().and_then(krilla::icc::ICCProfile::<4>::new);
    let converted: Vec<PageScene>;
    let scenes: &[PageScene] = if opts.cmyk_only {
        converted = scenes.iter().map(|s| PageScene { page: s.page, width: s.width, height: s.height, bleed: s.bleed, ops: cmyk_ops(&s.ops) }).collect();
        &converted
    } else if (archival || ua) && profile.is_none() {
        converted = scenes.iter().map(|s| PageScene { page: s.page, width: s.width, height: s.height, bleed: s.bleed, ops: rgb_ops(&s.ops) }).collect();
        &converted
    } else {
        scenes
    };
    let settings = krilla::SerializeSettings { configuration: configuration(opts.standard.as_deref())?, enable_tagging: opts.tagged || ua, cmyk_profile: profile, ..Default::default() };
    let mut document = Document::new_with(settings);
    let mut tree = krilla::tagging::TagTree::new();
    let mut open: Vec<krilla::tagging::TagGroup> = Vec::new();
    let mut headings: Vec<(u16, String, usize)> = Vec::new();
    let mut page_no = 0usize;
    let mut fonts = Fonts(HashMap::new());
    let mut img_cache: HashMap<AssetId, Option<krilla::image::Image>> = HashMap::new();
    let mut pdf_cache: HashMap<AssetId, Option<krilla::pdf::PdfDocument>> = HashMap::new();
    let index_of: HashMap<usize, usize> = scenes.iter().enumerate().map(|(i, s)| (s.page, i)).collect();
    for scene in scenes {
        let b = &scene.bleed;
        let full_w = scene.width + b.inside + b.outside;
        let full_h = scene.height + b.top + b.bottom;
        // Marks sit in a slug of equal width on every side of the bleed box, so the media box grows by twice that.
        let marks = opts.marks.as_ref().filter(|m| m.any());
        let doc_bleed = crate::marks::page_bleed(doc, scene.page);
        let slug = marks.map(|m| m.slug(&doc_bleed)).unwrap_or(0.0);
        let size = Size::from_wh((full_w + 2.0 * slug) as f32, (full_h + 2.0 * slug) as f32).ok_or("page has no size")?;
        let trim = Rect::from_xywh((slug + b.inside) as f32, (slug + b.top) as f32, scene.width as f32, scene.height as f32);
        let mut settings = PageSettings::new(size).with_trim_box(trim);
        if b.top + b.bottom + b.inside + b.outside > 0.0 || slug > 0.0 {
            settings = settings.with_bleed_box(Rect::from_xywh(slug as f32, slug as f32, full_w as f32, full_h as f32));
        }
        let mut all_ops: Vec<Op> = scene.ops.clone();
        if let Some(m) = marks {
            let name = doc.page_name(scene.page);
            let plate_name = opts.plate.and_then(|p| doc.plate_names().get(p as usize).cloned());
            let left = format!("{}  {}", if opts.title.is_empty() { "Untitled" } else { opts.title.as_str() }, crate::marks::timestamp());
            let right = match &plate_name {
                Some(pn) => format!("{pn}  Page {name}"),
                None => format!("Page {name}"),
            };
            all_ops.extend(m.ops(scene.width, scene.height, &doc_bleed, &left, &right, opts.mark_font.as_ref().map(|f| &f.0)));
        }
        if let Some(p) = opts.plate {
            all_ops = crate::sep::plate_ops(doc, &all_ops, p);
        }
        let mut page = document.start_page_with(settings);
        let mut links = Vec::new();
        {
            let mut surface = page.surface();
            surface.push_transform(&Transform::from_translate((slug + b.inside) as f32, (slug + b.top) as f32));
            for op in &all_ops {
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
                        if let Some(p) = opts.plate {
                            // A plate has no colour pictures: the picture becomes its ink density as a grey image.
                            let img = images.get(a, 2.0).filter(|d| d.width > 0 && d.height > 0).map(|d| krilla::image::Image::from_rgba8(crate::sep::plate_rgba(&d.rgba, p), d.width, d.height));
                            if let Some(img) = img {
                                surface.draw_image(img, sz);
                            }
                        } else if is_pdf(a) {
                            let pdf = pdf_cache
                                .entry(*asset)
                                .or_insert_with(|| krilla::pdf::Pdf::new(crate::pdflayers::for_export(&a.data, a.page)).ok().map(|p| krilla::pdf::PdfDocument::new(Arc::new(p))))
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
                    Op::PushMask(inner) => {
                        let mut sb = surface.stream_builder();
                        {
                            let mut ms = sb.surface();
                            mask_ops(&mut ms, inner);
                            ms.finish();
                        }
                        let stream = sb.finish();
                        surface.push_mask(krilla::mask::Mask::new(stream, krilla::mask::MaskType::Luminosity));
                    }
                    Op::Pop => surface.pop(),
                    Op::Link { rect, target } => links.push((*rect, target.clone())),
                    Op::Tag(t) => {
                        use krilla::tagging::{ContentTag, SpanTag, Tag, TagGroup};
                        if !(opts.tagged || ua) {
                            continue;
                        }
                        match t {
                            crate::scene::Tagging::End => {
                                if let Some(g) = open.pop() {
                                    surface.end_tagged();
                                    tree.push(g);
                                }
                            }
                            other => {
                                if let Some(g) = open.pop() {
                                    surface.end_tagged();
                                    tree.push(g);
                                }
                                let (mut group, content) = match other {
                                    crate::scene::Tagging::Heading(l, title) => {
                                        headings.push((*l, title.clone(), page_no));
                                        (TagGroup::new(Tag::Hn(std::num::NonZeroU16::new(*l).unwrap_or(std::num::NonZeroU16::MIN), Some(if title.is_empty() { "Heading".to_string() } else { title.clone() }))), ContentTag::Span(SpanTag::empty()))
                                    }
                                    crate::scene::Tagging::Figure(alt) => (TagGroup::new(Tag::Figure(Some(if alt.is_empty() { "Image".to_string() } else { alt.clone() }))), ContentTag::Other),
                                    _ => (TagGroup::new(Tag::P), ContentTag::Span(SpanTag::empty())),
                                };
                                let id = surface.start_tagged(content);
                                group.push(id);
                                open.push(group);
                            }
                        }
                    }
                }
            }
            while let Some(g) = open.pop() {
                surface.end_tagged();
                tree.push(g);
            }
            surface.pop();
            surface.finish();
        }
        for (r, t) in links {
            let Some(rect) = Rect::from_ltrb((r.x0 + b.inside + slug) as f32, (r.y0 + b.top + slug) as f32, (r.x1 + b.inside + slug) as f32, (r.y1 + b.top + slug) as f32) else { continue };
            let (target, alt) = match t {
                LinkTarget::Url(u) => (Target::Action(LinkAction::new(u.clone()).into()), format!("Link to {u}")),
                LinkTarget::Page(p) => match index_of.get(&p) {
                    Some(i) => (Target::Destination(XyzDestination::new(*i, Point::from_xy(0.0, 0.0)).into()), format!("Link to page {}", doc.page_name(p))),
                    None => continue,
                },
            };
            page.add_annotation(Annotation::new_link(LinkAnnotation::new(rect, target), Some(alt)));
        }
        page.finish();
        page_no += 1;
    }
    let has_marks = doc.prefs.get("bookmarks").and_then(|v| v.as_array()).is_some_and(|a| !a.is_empty());
    if !has_marks && (ua || opts.tagged) && !headings.is_empty() {
        let mut outline = krilla::outline::Outline::new();
        let mut top: Vec<krilla::outline::OutlineNode> = Vec::new();
        for (level, title, page) in &headings {
            let node = krilla::outline::OutlineNode::new(if title.is_empty() { "Heading".into() } else { title.clone() }, XyzDestination::new(*page, Point::from_xy(0.0, 0.0)));
            match top.last_mut() {
                Some(parent) if *level > 1 => parent.push_child(node),
                _ => top.push(node),
            }
        }
        for n in top {
            outline.push_child(n);
        }
        document.set_outline(outline);
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
    meta = meta.creator("formdes".into()).creation_date(today());
    if let Some(l) = &opts.language {
        meta = meta.language(l.clone());
    }
    document.set_metadata(meta);
    if opts.tagged || ua {
        document.set_tag_tree(tree);
    }
    document.finish().map_err(|e| format!("{e:?}"))
}
