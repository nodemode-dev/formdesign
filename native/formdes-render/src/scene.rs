use std::ops::Range;
use std::sync::Arc;

use formdes_model::attrs::ParaProps;
use formdes_model::color::{Color, Gradient, Paint};
use formdes_model::doc::{Doc, LinkDest, LinkSource};
use formdes_model::geom::{Affine, BezPath, Edges, KShape, Point, Rect};
use formdes_model::ids::{AssetId, ItemId};
use formdes_model::item::{Item, ItemKind, StrokeAlign};
use formdes_text::compose::{GENERATED, GlyphRun, Line};
use formdes_text::fonts::Face;
use formdes_text::layout::{DocLayout, StoryLayout};
use formdes_text::shape::{ShapeReq, shape};

#[derive(Clone, Debug)]
pub enum Brush {
    Solid(Color, f64),
    Linear { start: Point, end: Point, stops: Vec<(f64, Color, f64)>, alpha: f64 },
    Radial { center: Point, radius: f64, stops: Vec<(f64, Color, f64)>, alpha: f64 },
}

#[derive(Clone, Debug)]
pub struct StrokeStyle {
    pub width: f64,
    pub cap: String,
    pub join: String,
    pub dash: Vec<f64>,
}

#[derive(Clone)]
pub struct GlyphOp {
    pub face: Arc<Face>,
    pub size: f64,
    pub hscale: f64,
    pub vscale: f64,
    pub skew: f64,
    pub xf: Affine,
    pub glyphs: Vec<(u32, f64, f64)>,
    pub advances: Vec<f64>,
    pub per_glyph: Option<Vec<Affine>>,
    pub text: String,
    pub ranges: Vec<Range<usize>>,
    pub fill: Option<Brush>,
    pub stroke: Option<(Brush, f64)>,
}

#[derive(Clone, Debug)]
pub enum LinkTarget {
    Url(String),
    Page(usize),
}

#[derive(Clone)]
pub enum Op {
    Fill { path: BezPath, brush: Brush, even_odd: bool },
    Stroke { path: BezPath, brush: Brush, style: StrokeStyle },
    Glyphs(GlyphOp),
    Image { asset: AssetId, xf: Affine, w: f64, h: f64 },
    PushClip(BezPath),
    PushOpacity(f64),
    PushBlend(String),
    PushMask(Vec<Op>),
    Pop,
    Link { rect: Rect, target: LinkTarget },
    Tag(Tagging),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Tagging {
    Para,
    Heading(u16, String),
    Figure(String),
    End,
}

fn heading_level(style: &str) -> Option<u16> {
    let s = style.to_ascii_lowercase();
    if s.contains("title") && !s.contains("subtitle") {
        return Some(1);
    }
    if !(s.contains("head") || s.starts_with('h')) || s.contains("header") || s.contains("headline deck") {
        return None;
    }
    let digit = s.chars().rev().find(|c| c.is_ascii_digit()).and_then(|c| c.to_digit(10)).map(|d| d as u16);
    match digit {
        Some(d) if (1..=6).contains(&d) => Some(d),
        _ if s.contains("head") => Some(2),
        _ => None,
    }
}

#[derive(Clone, Debug, Default)]
pub struct SceneOpts {
    pub printing: bool,
    pub bleed: bool,
    pub overprint: bool,
}

pub struct PageScene {
    pub page: usize,
    pub width: f64,
    pub height: f64,
    pub bleed: Edges,
    pub ops: Vec<Op>,
}

struct Ctx<'a> {
    doc: &'a Doc,
    lay: &'a DocLayout,
    opts: &'a SceneOpts,
    ops: Vec<Op>,
    page: Option<usize>,
}

pub fn brush_of(doc: &Doc, paint: &Paint, tint: f64, local: Rect, xf: Affine) -> Option<Brush> {
    if let Some(g) = doc.gradient_of(paint) {
        return Some(gradient_brush(&g, local, xf));
    }
    let c = doc.paint_color(paint)?;
    let c = if (tint - 1.0).abs() > 1e-6 { c.tinted(tint) } else { c };
    Some(Brush::Solid(c, 1.0))
}

fn gradient_brush(g: &Gradient, r: Rect, xf: Affine) -> Brush {
    let stops: Vec<(f64, Color, f64)> = g.stops.iter().map(|s| (s.at.clamp(0.0, 1.0), s.color, s.opacity.clamp(0.0, 1.0))).collect();
    let c = r.center();
    if g.radial {
        let radius = r.width().max(r.height()) / 2.0;
        let p = xf * c;
        let scale = xf.as_coeffs();
        let k = (scale[0] * scale[3] - scale[1] * scale[2]).abs().sqrt().max(1e-6);
        Brush::Radial { center: p, radius: radius * k, stops, alpha: 1.0 }
    } else {
        let a = g.angle.to_radians();
        let (dx, dy) = (a.cos(), -a.sin());
        let half = (r.width() * dx.abs() + r.height() * dy.abs()) / 2.0;
        let start = Point::new(c.x - dx * half, c.y - dy * half);
        let end = Point::new(c.x + dx * half, c.y + dy * half);
        Brush::Linear { start: xf * start, end: xf * end, stops, alpha: 1.0 }
    }
}

type FxMap = serde_json::Map<String, serde_json::Value>;

fn fx<'a>(it: &'a Item, key: &str) -> Option<&'a FxMap> {
    let s = it.effects.get(key).and_then(|v| v.as_object())?;
    if s.get("on").and_then(|v| v.as_bool()) == Some(false) {
        return None;
    }
    Some(s)
}

fn fnum(s: &FxMap, k: &str, d: f64) -> f64 {
    s.get(k).and_then(|v| v.as_f64()).unwrap_or(d)
}

fn fmode(s: &FxMap, d: &str) -> String {
    s.get("mode").and_then(|v| v.as_str()).unwrap_or(d).to_string()
}

fn fcolor(s: &FxMap, d: Color) -> Color {
    s.get("color").and_then(Color::from_value).unwrap_or(d)
}

fn smooth(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Soft edges built from vector bands. `profile(d)` is the wanted cumulative opacity at distance `d` from the
/// edge (it must not increase with `d`). Each band is a stroke of half-width `r` at opacity `a`; where bands overlap
/// near the edge their opacities accumulate to the profile. Vector bands keep effects native in the PDF.
fn bands(size: f64, n: usize, profile: impl Fn(f64) -> f64) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let mut prev = 0.0;
    for k in 0..n {
        let r = size * (n - k) as f64 / n as f64;
        let d = r - size / (2.0 * n as f64);
        let t = profile(d).clamp(0.0, 0.999);
        if t - prev <= 1e-4 {
            continue;
        }
        out.push((r, 1.0 - (1.0 - t) / (1.0 - prev)));
        prev = t;
    }
    out
}

fn glow_profile(opacity: f64, core: f64, size: f64) -> impl Fn(f64) -> f64 {
    move |d| if d <= core { opacity } else { opacity * (1.0 - smooth((d - core) / (size - core).max(1e-6))) }
}

fn band_stroke(path: &BezPath, color: Color, alpha: f64, half: f64, join: &str) -> Op {
    Op::Stroke { path: path.clone(), brush: Brush::Solid(color, alpha), style: StrokeStyle { width: half * 2.0, cap: "butt".into(), join: join.into(), dash: Vec::new() } }
}

/// The per-cell stroke on one side of a table cell, if the cell (or the cell across that edge) overrides the table
/// border there: weight, colour with tint applied and line type.
fn cell_edge(doc: &Doc, tb: &formdes_model::story::Table, cell: &formdes_model::story::TableCell, side: &str, bw: f64, bcol: Color) -> Option<(f64, Color, String)> {
    let own = cell.attrs.get("strokes").and_then(|s| s.get(side));
    let facing = || -> Option<&serde_json::Value> {
        let overlap = |x: &formdes_model::story::TableCell| match side {
            "top" => x.row + x.rows == cell.row && x.col < cell.col + cell.cols && x.col + x.cols > cell.col,
            "bottom" => x.row == cell.row + cell.rows && x.col < cell.col + cell.cols && x.col + x.cols > cell.col,
            "left" => x.col + x.cols == cell.col && x.row < cell.row + cell.rows && x.row + x.rows > cell.row,
            _ => x.col == cell.col + cell.cols && x.row < cell.row + cell.rows && x.row + x.rows > cell.row,
        };
        let opposite = match side {
            "top" => "bottom",
            "bottom" => "top",
            "left" => "right",
            _ => "left",
        };
        tb.cells.iter().filter(|x| overlap(x)).find_map(|x| x.attrs.get("strokes").and_then(|s| s.get(opposite)))
    };
    let o = own.or_else(facing)?.as_object()?;
    let weight = o.get("weight").and_then(|v| v.as_f64()).unwrap_or(bw);
    let tint = o.get("tint").and_then(|v| v.as_f64()).unwrap_or(1.0);
    let color = o.get("color").and_then(Paint::from_value).filter(|p| !p.is_none()).and_then(|p| doc.paint_color(&p)).unwrap_or(bcol);
    let color = if (tint - 1.0).abs() > 1e-6 { color.tinted(tint) } else { color };
    let kind = o.get("type").and_then(|v| v.as_str()).unwrap_or("solid").to_string();
    Some((weight, color, kind))
}

/// A path that covers `around` except for `path` (a rectangle wound against the path, for non-zero clips, or any
/// winding with the even-odd rule).
fn complement(path: &BezPath, around: Rect) -> BezPath {
    let (x0, y0, x1, y1) = (around.x0, around.y0, around.x1, around.y1);
    let mut out = BezPath::new();
    out.move_to((x0, y0));
    if path.area() < 0.0 {
        out.line_to((x1, y0));
        out.line_to((x1, y1));
        out.line_to((x0, y1));
    } else {
        out.line_to((x0, y1));
        out.line_to((x1, y1));
        out.line_to((x1, y0));
    }
    out.close_path();
    for el in path.elements() {
        out.push(*el);
    }
    out
}

impl Ctx<'_> {
    fn blend_open(&mut self, mode: &str) -> bool {
        if mode.is_empty() || mode == "normal" {
            return false;
        }
        self.ops.push(Op::PushBlend(mode.to_string()));
        true
    }

    fn outer_glow(&mut self, it: &Item, path: &BezPath) {
        let Some(s) = fx(it, "outerGlow") else { return };
        let size = fnum(s, "size", 7.0).max(0.0);
        if size <= 0.0 {
            return;
        }
        let opacity = fnum(s, "opacity", 0.75).clamp(0.0, 1.0);
        let core = fnum(s, "spread", 0.0).clamp(0.0, 0.99) * size;
        let color = fcolor(s, Color::Rgb([1.0, 0.75, 0.2]));
        let join = if s.get("technique").and_then(|v| v.as_str()) == Some("precise") { "miter" } else { "round" };
        let blended = self.blend_open(&fmode(s, "normal"));
        let closed = it.shape.closed();
        if closed {
            self.ops.push(Op::PushClip(complement(path, path.bounding_box().inflate(size + 8.0, size + 8.0))));
        }
        for (r, a) in bands(size, 18, glow_profile(opacity, core, size)) {
            self.ops.push(band_stroke(path, color, a, r, join));
        }
        if closed {
            self.ops.push(Op::Pop);
        }
        if blended {
            self.ops.push(Op::Pop);
        }
    }

    fn inner_band(&mut self, clip: &BezPath, src: &BezPath, outside: bool, color: Color, mode: &str, size: f64, profile: impl Fn(f64) -> f64) {
        let blended = self.blend_open(mode);
        self.ops.push(Op::PushClip(clip.clone()));
        let comp = outside.then(|| complement(src, clip.bounding_box().union(src.bounding_box()).inflate(size + 8.0, size + 8.0)));
        for (r, a) in bands(size, 18, profile) {
            self.ops.push(Op::PushOpacity(a));
            if let Some(c) = &comp {
                self.ops.push(Op::Fill { path: c.clone(), brush: Brush::Solid(color, 1.0), even_odd: true });
            }
            self.ops.push(band_stroke(src, color, 1.0, r, "round"));
            self.ops.push(Op::Pop);
        }
        self.ops.push(Op::Pop);
        if blended {
            self.ops.push(Op::Pop);
        }
    }

    fn inner_effects(&mut self, it: &Item, path: &BezPath) {
        if !it.shape.closed() {
            return;
        }
        if let Some(s) = fx(it, "innerShadow") {
            let size = fnum(s, "size", 5.0).max(0.0);
            let opacity = fnum(s, "opacity", 0.75).clamp(0.0, 1.0);
            let core = fnum(s, "choke", 0.0).clamp(0.0, 0.99) * size;
            let (dx, dy) = (fnum(s, "x", 3.5), fnum(s, "y", 3.5));
            let color = fcolor(s, Color::Cmyk([0.0, 0.0, 0.0, 1.0]));
            let src = Affine::translate((dx, dy)) * path.clone();
            if size > 0.0 || dx != 0.0 || dy != 0.0 {
                self.inner_band(path, &src, true, color, &fmode(s, "multiply"), size.max(0.01), glow_profile(opacity, core, size.max(0.01)));
            }
        }
        if let Some(s) = fx(it, "innerGlow") {
            let size = fnum(s, "size", 7.0).max(0.0);
            if size > 0.0 {
                let opacity = fnum(s, "opacity", 0.75).clamp(0.0, 1.0);
                let core = fnum(s, "choke", 0.0).clamp(0.0, 0.99) * size;
                let color = fcolor(s, Color::Rgb([1.0, 0.75, 0.2]));
                self.inner_band(path, path, false, color, &fmode(s, "normal"), size, glow_profile(opacity, core, size));
            }
        }
    }

    /// Feather effects mask everything the object draws; returns how many mask layers were opened.
    fn feather(&mut self, it: &Item, path: &BezPath) -> usize {
        let mut opened = 0;
        let white = Color::Rgb([1.0, 1.0, 1.0]);
        let black = Color::Rgb([0.0, 0.0, 0.0]);
        if let Some(s) = fx(it, "basicFeather")
            && it.shape.closed()
        {
            let width = fnum(s, "width", 9.0).max(0.0);
            let choke = fnum(s, "choke", 0.0).clamp(0.0, 0.95);
            let reach = width * (1.0 - choke);
            if reach > 0.0 {
                let join = if s.get("corners").and_then(|v| v.as_str()) == Some("sharp") { "miter" } else { "round" };
                let mut m = vec![Op::Fill { path: path.clone(), brush: Brush::Solid(white, 1.0), even_odd: false }, Op::PushClip(path.clone())];
                for (r, a) in bands(reach, 28, |d| 1.0 - smooth(d / reach)) {
                    m.push(band_stroke(path, black, a, r, join));
                }
                m.push(Op::Pop);
                self.ops.push(Op::PushMask(m));
                opened += 1;
            }
        }
        if let Some(s) = fx(it, "directionalFeather")
            && it.shape.closed()
        {
            let side = |k: &str| fnum(s, k, 0.0).max(0.0);
            let (wt, wb, wl, wr) = (side("top"), side("bottom"), side("left"), side("right"));
            if wt + wb + wl + wr > 0.0 {
                let choke = fnum(s, "choke", 0.0).clamp(0.0, 0.95);
                let rot = Affine::rotate_about(fnum(s, "angle", 0.0).to_radians(), path.bounding_box().center());
                let bb = (rot.inverse() * path.clone()).bounding_box();
                let (cx, cy) = (bb.center().x, bb.center().y);
                let pad = 4.0;
                let stops = || -> Vec<(f64, Color, f64)> {
                    let mut v: Vec<(f64, Color, f64)> = (0..=8).map(|i| ((1.0 - choke) * i as f64 / 8.0, black, 1.0 - smooth(i as f64 / 8.0))).collect();
                    if choke > 0.0 {
                        v.push((1.0, black, 0.0));
                    }
                    v
                };
                let mut m = vec![Op::Fill { path: path.clone(), brush: Brush::Solid(white, 1.0), even_odd: false }, Op::PushClip(path.clone())];
                let mut ramp = |p0: Point, p1: Point, r: Rect| {
                    m.push(Op::Fill { path: rot * r.to_path(0.1), brush: Brush::Linear { start: rot * p0, end: rot * p1, stops: stops(), alpha: 1.0 }, even_odd: false });
                };
                if wl > 0.0 {
                    ramp(Point::new(bb.x0, cy), Point::new(bb.x0 + wl, cy), Rect::new(bb.x0 - pad, bb.y0 - pad, bb.x0 + wl, bb.y1 + pad));
                }
                if wr > 0.0 {
                    ramp(Point::new(bb.x1, cy), Point::new(bb.x1 - wr, cy), Rect::new(bb.x1 - wr, bb.y0 - pad, bb.x1 + pad, bb.y1 + pad));
                }
                if wt > 0.0 {
                    ramp(Point::new(cx, bb.y0), Point::new(cx, bb.y0 + wt), Rect::new(bb.x0 - pad, bb.y0 - pad, bb.x1 + pad, bb.y0 + wt));
                }
                if wb > 0.0 {
                    ramp(Point::new(cx, bb.y1), Point::new(cx, bb.y1 - wb), Rect::new(bb.x0 - pad, bb.y1 - wb, bb.x1 + pad, bb.y1 + pad));
                }
                m.push(Op::Pop);
                self.ops.push(Op::PushMask(m));
                opened += 1;
            }
        }
        opened
    }

    fn visible(&self, it: &Item) -> bool {
        if it.hidden {
            return false;
        }
        let layer = self.doc.layer(it.layer);
        if layer.map(|l| !l.visible).unwrap_or(false) {
            return false;
        }
        !(self.opts.printing && (it.nonprinting || layer.map(|l| !l.printable).unwrap_or(false)))
    }

    fn emit(&mut self, id: ItemId, base: Affine) {
        let Some(it) = self.doc.item(id) else { return };
        if !self.visible(it) {
            return;
        }
        let xf = base * self.doc.world_xf(id);
        let mut pushed = 0;
        if it.opacity < 0.999 {
            self.ops.push(Op::PushOpacity(it.opacity.clamp(0.0, 1.0)));
            pushed += 1;
        }
        if it.blend != "normal" && !it.blend.is_empty() {
            self.ops.push(Op::PushBlend(it.blend.clone()));
            pushed += 1;
        }
        match &it.kind {
            ItemKind::Group { .. } | ItemKind::States { .. } => {
                for c in it.children() {
                    self.emit(c, base);
                }
            }
            _ => {
                let local = it.shape.to_path(&it.corner);
                let path = xf * local.clone();
                let bbox = it.shape.bbox();
                self.outer_glow(it, &path);
                self.shadow(it, &path);
                let masks = self.feather(it, &path);
                if it.shape.closed()
                    && let Some(b) = brush_of(self.doc, &it.fill, 1.0, bbox, xf)
                {
                    let even_odd = it.extra.get("fillRule").and_then(|v| v.as_str()) == Some("evenodd");
                    let over = self.opts.overprint && it.extra.get("overprintFill").and_then(|v| v.as_bool()).unwrap_or(false);
                    if over {
                        self.ops.push(Op::PushBlend("multiply".into()));
                    }
                    self.ops.push(Op::Fill { path: path.clone(), brush: b, even_odd });
                    if over {
                        self.ops.push(Op::Pop);
                    }
                }
                if !matches!(it.kind, ItemKind::Graphic(_)) {
                    self.inner_effects(it, &path);
                }
                match &it.kind {
                    ItemKind::Graphic(g) => {
                        if let Some(aid) = g.asset
                            && let Some(asset) = self.doc.assets.get(&aid)
                        {
                            let (w, h) = asset.natural_size();
                            self.ops.push(Op::Tag(Tagging::Figure(it.alt_text.clone())));
                            self.ops.push(Op::PushClip(path.clone()));
                            self.ops.push(Op::Image { asset: aid, xf: xf * g.content, w, h });
                            self.ops.push(Op::Pop);
                            self.ops.push(Op::Tag(Tagging::End));
                        }
                        self.inner_effects(it, &path);
                    }
                    ItemKind::Text(_) => self.text(id, xf),
                    _ => {}
                }
                let over = self.opts.overprint && it.extra.get("overprintStroke").and_then(|v| v.as_bool()).unwrap_or(false);
                if over {
                    self.ops.push(Op::PushBlend("multiply".into()));
                }
                self.stroke(it, &path, xf);
                if over {
                    self.ops.push(Op::Pop);
                }
                for _ in 0..masks {
                    self.ops.push(Op::Pop);
                }
                self.link_item(id, &path);
            }
        }
        for _ in 0..pushed {
            self.ops.push(Op::Pop);
        }
    }

    fn shadow(&mut self, it: &Item, path: &BezPath) {
        let Some(s) = it.effects.get("dropShadow").and_then(|v| v.as_object()) else { return };
        if s.get("on").and_then(|v| v.as_bool()) == Some(false) {
            return;
        }
        let n = |k: &str, d: f64| s.get(k).and_then(|v| v.as_f64()).unwrap_or(d);
        let color = s.get("color").and_then(Color::from_value).unwrap_or(Color::Cmyk([0.0, 0.0, 0.0, 1.0]));
        let (dx, dy) = (n("x", 4.0), n("y", 4.0));
        let alpha = n("opacity", 0.75);
        let shifted = Affine::translate((dx, dy)) * path.clone();
        if it.fill.is_none() && it.stroke.paint.is_none() {
            return;
        }
        self.ops.push(Op::Fill { path: shifted, brush: Brush::Solid(color, alpha), even_odd: false });
    }

    fn stroke(&mut self, it: &Item, path: &BezPath, xf: Affine) {
        let st = &it.stroke;
        if st.weight <= 0.0 || st.paint.is_none() {
            return;
        }
        let Some(b) = brush_of(self.doc, &st.paint, 1.0, it.shape.bbox(), xf) else { return };
        let k = {
            let c = xf.as_coeffs();
            (c[0] * c[3] - c[1] * c[2]).abs().sqrt()
        };
        let w = st.weight * k;
        let style = |width: f64| StrokeStyle { width, cap: st.cap.clone(), join: st.join.clone(), dash: st.dash.iter().map(|d| d * k).collect() };
        match st.align {
            StrokeAlign::Inside if it.shape.closed() => {
                self.ops.push(Op::PushClip(path.clone()));
                self.ops.push(Op::Stroke { path: path.clone(), brush: b, style: style(w * 2.0) });
                self.ops.push(Op::Pop);
            }
            _ => self.ops.push(Op::Stroke { path: path.clone(), brush: b, style: style(w) }),
        }
    }

    fn link_item(&mut self, id: ItemId, path: &BezPath) {
        if let Some(a) = self.doc.item(id).and_then(|it| it.extra.get("button")).and_then(|b| b.get("action")) {
            let last = self.doc.page_count().saturating_sub(1);
            let here = self.page.unwrap_or(0);
            let target = match a.get("kind").and_then(|k| k.as_str()) {
                Some("goToPage") => a.get("page").and_then(|p| p.as_u64()).map(|p| LinkTarget::Page(p as usize)),
                Some("goToFirstPage") => Some(LinkTarget::Page(0)),
                Some("goToLastPage") => Some(LinkTarget::Page(last)),
                Some("goToNextPage") => Some(LinkTarget::Page((here + 1).min(last))),
                Some("goToPreviousPage") => Some(LinkTarget::Page(here.saturating_sub(1))),
                Some("goToUrl") => a.get("url").and_then(|u| u.as_str()).map(|u| LinkTarget::Url(u.to_string())),
                _ => None,
            };
            if let Some(t) = target {
                self.ops.push(Op::Link { rect: path.bounding_box(), target: t });
            }
        }
        for h in &self.doc.hyperlinks {
            if let LinkSource::Item { item } = &h.source
                && *item == id
                && let Some(t) = link_target(&h.dest)
            {
                self.ops.push(Op::Link { rect: path.bounding_box(), target: t });
            }
        }
    }

    fn text(&mut self, id: ItemId, xf: Affine) {
        let Some((sl, fi)) = self.lay.frame_on(id, self.page) else { return };
        if let Some(op) = &sl.on_path {
            if fi != 0 {
                return;
            }
            for (r, xs) in op.runs.iter().zip(&op.xforms) {
                let fill = run_fill(self.doc, r);
                let glyphs: Vec<(u32, f64, f64)> = r.glyphs.iter().map(|g| (g.id, 0.0, 0.0)).collect();
                let advances = r.glyphs.iter().map(|g| g.adv).collect();
                let per: Vec<Affine> = xs.iter().map(|a| xf * *a).collect();
                self.ops.push(Op::Glyphs(GlyphOp {
                    face: r.face.clone(),
                    size: r.size,
                    hscale: r.hscale,
                    vscale: r.props.v_scale,
                    skew: r.props.skew,
                    xf,
                    ranges: vec![0..0; glyphs.len()],
                    glyphs,
                    advances,
                    per_glyph: Some(per),
                    text: String::new(),
                    fill,
                    stroke: run_stroke(self.doc, r),
                }));
            }
            return;
        }
        let story = self.doc.story(sl.story);
        self.para_decor(sl, fi, xf, story.map(|s| s.paras.as_slice()).unwrap_or(&[]), true);
        let flow = &sl.flow;
        for t in flow.tables.iter().filter(|t| t.frame == fi) {
            let table = story.and_then(|s| s.paras.get(t.para)).and_then(|p| {
                p.segs.iter().find_map(|sg| match sg {
                    formdes_model::story::Seg::Inline { inline: formdes_model::story::Inline::Table { table }, .. } => Some(table.as_ref()),
                    _ => None,
                })
            });
            let border = table.and_then(|tb| tb.attrs.get("border"));
            let bw = border.and_then(|b| b.get("weight")).and_then(|v| v.as_f64()).unwrap_or(1.0);
            let bcol = border.and_then(|b| b.get("color")).and_then(Paint::from_value).and_then(|p| self.doc.paint_color(&p)).unwrap_or(Color::Cmyk([0.0, 0.0, 0.0, 1.0]));
            for c in &t.cells {
                if let Some(tb) = table
                    && let Some(cell) = tb.cell(c.row, c.col)
                {
                    let tint_of = |v: Option<&serde_json::Value>| v.and_then(|x| x.as_f64()).unwrap_or(1.0);
                    let explicit = cell.attrs.get("fill").and_then(Paint::from_value).filter(|p| !p.is_none()).map(|p| (p, tint_of(cell.attrs.get("fillTint"))));
                    let alt = tb.attrs.get("altRows").and_then(|a| a.as_object()).and_then(|a| {
                        let body_end = tb.rows().saturating_sub(tb.footer_rows);
                        if c.row < tb.header_rows || c.row >= body_end {
                            return None;
                        }
                        let first = a.get("first").and_then(|v| v.as_u64()).unwrap_or(1).max(1) as usize;
                        let next = a.get("next").and_then(|v| v.as_u64()).unwrap_or(1).max(1) as usize;
                        let k = (c.row - tb.header_rows) % (first + next);
                        let (col, tint) = if k < first { (a.get("firstColor"), a.get("firstTint")) } else { (a.get("nextColor"), a.get("nextTint")) };
                        col.and_then(Paint::from_value).filter(|p| !p.is_none()).map(|p| (p, tint_of(tint)))
                    });
                    if let Some((paint, tint)) = explicit.or(alt)
                        && let Some(col) = self.doc.paint_color(&paint)
                    {
                        let col = if (tint - 1.0).abs() > 1e-6 { col.tinted(tint) } else { col };
                        self.ops.push(Op::Fill { path: xf * c.rect.to_path(0.1), brush: Brush::Solid(col, 1.0), even_odd: false });
                    }
                }
                let edges: [Option<(f64, Color, String)>; 4] = match table.and_then(|tb| tb.cell(c.row, c.col).map(|cell| (tb, cell))) {
                    Some((tb, cell)) => ["top", "bottom", "left", "right"].map(|s| cell_edge(self.doc, tb, cell, s, bw, bcol)),
                    None => [None, None, None, None],
                };
                if edges.iter().all(Option::is_none) {
                    if bw > 0.0 {
                        let path = xf * c.rect.to_path(0.1);
                        self.ops.push(Op::Stroke { path, brush: Brush::Solid(bcol, 1.0), style: StrokeStyle { width: bw, cap: "butt".into(), join: "miter".into(), dash: Vec::new() } });
                    }
                } else {
                    let r = c.rect;
                    let ends = [((r.x0, r.y0), (r.x1, r.y0)), ((r.x0, r.y1), (r.x1, r.y1)), ((r.x0, r.y0), (r.x0, r.y1)), ((r.x1, r.y0), (r.x1, r.y1))];
                    for (edge, (a, b)) in edges.into_iter().zip(ends) {
                        let (w, col, kind) = edge.unwrap_or((bw, bcol, "solid".into()));
                        if w <= 0.0 || kind == "none" {
                            continue;
                        }
                        let mut line = BezPath::new();
                        line.move_to(a);
                        line.line_to(b);
                        let (cap, dash) = match kind.as_str() {
                            "dashed" => ("butt", vec![w * 3.0, w * 2.0]),
                            "dotted" => ("round", vec![0.01, w * 2.0]),
                            _ => ("square", Vec::new()),
                        };
                        self.ops.push(Op::Stroke { path: xf * line, brush: Brush::Solid(col, 1.0), style: StrokeStyle { width: w, cap: cap.into(), join: "miter".into(), dash } });
                    }
                }
                let cell_texts: Vec<String> = table.and_then(|tb| tb.cell(c.row, c.col)).map(|cell| cell.paras.iter().map(|p| p.text()).collect()).unwrap_or_default();
                for l in &c.lines {
                    self.line(&l.line, l.baseline, xf, cell_texts.get(l.para).map(String::as_str).unwrap_or(""));
                }
            }
        }
        let mut open: Option<usize> = None;
        for l in flow.lines.iter().filter(|l| l.frame == fi) {
            if open != Some(l.para) {
                if open.is_some() {
                    self.ops.push(Op::Tag(Tagging::End));
                }
                let para = story.and_then(|s| s.paras.get(l.para));
                let style = para.map(|p| p.style.as_str()).unwrap_or("");
                let tag = match heading_level(style) {
                    Some(n) => Tagging::Heading(n, para.map(|p| p.plain().trim().to_string()).unwrap_or_default()),
                    None => Tagging::Para,
                };
                self.ops.push(Op::Tag(tag));
                open = Some(l.para);
            }
            let text = flow.para_texts.get(l.para).map(String::as_str).unwrap_or("");
            self.line(&l.line, l.baseline, xf, text);
            self.text_links(sl, l.para, &l.line, l.baseline, xf);
        }
        if open.is_some() {
            self.ops.push(Op::Tag(Tagging::End));
        }
        for d in flow.drops.iter().filter(|d| d.frame == fi) {
            let mut run = d.drop.run.clone();
            for g in &mut run.glyphs {
                g.x += d.x;
            }
            run.x0 += d.x;
            run.x1 += d.x;
            let text = flow.para_texts.get(d.para).map(String::as_str).unwrap_or("");
            self.run(&run, d.baseline, xf, text);
        }
        for n in flow.notes.iter().filter(|n| n.frame == fi) {
            if let Some(first) = n.lines.first() {
                let y = first.baseline - first.line.ascent - 4.0;
                let x0 = first.line.left;
                let mut p = BezPath::new();
                p.move_to((x0, y));
                p.line_to((x0 + 72.0, y));
                self.ops.push(Op::Stroke { path: xf * p, brush: Brush::Solid(Color::Cmyk([0.0, 0.0, 0.0, 1.0]), 1.0), style: StrokeStyle { width: 0.5, cap: "butt".into(), join: "miter".into(), dash: Vec::new() } });
            }
            for l in &n.lines {
                self.line(&l.line, l.baseline, xf, "");
            }
        }
        self.para_decor(sl, fi, xf, story.map(|s| s.paras.as_slice()).unwrap_or(&[]), false);
        let anchors: Vec<_> = flow.anchors.iter().filter(|a| a.frame == fi).cloned().collect();
        for a in anchors {
            let Some(it) = self.doc.item(a.item) else { continue };
            let b = it.xf.transform_rect_bbox(it.shape.bbox());
            let corner_y = if a.inline { b.y1 } else { b.y0 };
            let desired = xf * Affine::translate((a.x - b.x0, a.y - corner_y)) * it.xf;
            let base = desired * self.doc.world_xf(a.item).inverse();
            self.emit(a.item, base);
        }
    }

    fn para_decor(&mut self, sl: &StoryLayout, fi: usize, xf: Affine, paras: &[formdes_model::story::Para], under: bool) {
        let boxes: Vec<&formdes_text::flow::ParaBox> = sl.flow.boxes.iter().filter(|b| b.frame == fi).collect();
        for (bi, b) in boxes.iter().enumerate() {
            let Some(p) = paras.get(b.para) else { continue };
            let props: ParaProps = self.doc.styles.para_props(&p.style, &p.over);
            let lines: Vec<&formdes_text::flow::PlacedLine> = sl.flow.lines.iter().filter(|l| l.frame == fi && l.para == b.para && l.column == b.column).collect();
            let (Some(first), Some(last)) = (lines.first(), lines.last()) else { continue };
            if under {
                if let Some(sh) = &props.shading
                    && let Some(br) = brush_of(self.doc, &sh.color, sh.tint, Rect::ZERO, xf)
                {
                    let r = Rect::new(b.left - sh.left, b.top - sh.top, b.right + sh.right, b.bottom + sh.bottom);
                    self.ops.push(Op::Fill { path: xf * r.to_path(0.1), brush: br, even_odd: false });
                }
                if let Some(bd) = &props.border {
                    let same = |o: &formdes_text::flow::ParaBox| -> bool {
                        o.column == b.column
                            && paras.get(o.para).map(|q| self.doc.styles.para_props(&q.style, &q.over).border.as_ref() == Some(bd)).unwrap_or(false)
                    };
                    if bd.merge && bi > 0 && boxes[bi - 1].para + 1 == b.para && boxes[bi - 1].last && b.first && same(boxes[bi - 1]) {
                        continue;
                    }
                    let mut bottom = b.bottom;
                    let mut x_lo = lines.iter().map(|l| l.line.left).fold(f64::MAX, f64::min);
                    let mut x_hi = lines.iter().map(|l| l.line.left + l.line.width).fold(0.0, f64::max);
                    let mut k = bi;
                    while bd.merge && k + 1 < boxes.len() && boxes[k + 1].para == boxes[k].para + 1 && boxes[k].last && boxes[k + 1].first && same(boxes[k + 1]) {
                        k += 1;
                        bottom = boxes[k].bottom;
                        for l in sl.flow.lines.iter().filter(|l| l.frame == fi && l.para == boxes[k].para && l.column == b.column) {
                            x_lo = x_lo.min(l.line.left);
                            x_hi = x_hi.max(l.line.left + l.line.width);
                        }
                    }
                    let (cx0, cx1) = if bd.text_width { (x_lo, x_hi) } else { (b.left, b.right) };
                    let (x0, y0, x1, y1) = (cx0 - bd.offset_left, b.top - bd.offset_top, cx1 + bd.offset_right, bottom + bd.offset_bottom);
                    if let Some(br) = brush_of(self.doc, &bd.color, bd.tint, Rect::ZERO, xf) {
                        let dash = |w: f64| if bd.dashed { vec![(w * 4.0).max(3.0), (w * 3.0).max(2.0)] } else { Vec::new() };
                        let uniform = (bd.top - bd.bottom).abs() < 1e-6 && (bd.top - bd.left).abs() < 1e-6 && (bd.top - bd.right).abs() < 1e-6;
                        if bd.rounded && bd.corner > 0.0 {
                            let w = bd.top.max(bd.bottom).max(bd.left).max(bd.right);
                            let r = Rect::new(x0 - w / 2.0, y0 - w / 2.0, x1 + w / 2.0, y1 + w / 2.0);
                            let path = kurbo::RoundedRect::from_rect(r, bd.corner.min(r.width() / 2.0).min(r.height() / 2.0)).to_path(0.1);
                            self.ops.push(Op::Stroke { path: xf * path, brush: br, style: StrokeStyle { width: w, cap: "butt".into(), join: "round".into(), dash: dash(w) } });
                        } else if uniform {
                            let w = bd.top;
                            let r = Rect::new(x0 - w / 2.0, y0 - w / 2.0, x1 + w / 2.0, y1 + w / 2.0);
                            self.ops.push(Op::Stroke { path: xf * r.to_path(0.1), brush: br, style: StrokeStyle { width: w, cap: "butt".into(), join: "miter".into(), dash: dash(w) } });
                        } else {
                            let sides = [
                                (bd.top, (x0 - bd.left, y0 - bd.top / 2.0), (x1 + bd.right, y0 - bd.top / 2.0)),
                                (bd.bottom, (x0 - bd.left, y1 + bd.bottom / 2.0), (x1 + bd.right, y1 + bd.bottom / 2.0)),
                                (bd.left, (x0 - bd.left / 2.0, y0 - bd.top), (x0 - bd.left / 2.0, y1 + bd.bottom)),
                                (bd.right, (x1 + bd.right / 2.0, y0 - bd.top), (x1 + bd.right / 2.0, y1 + bd.bottom)),
                            ];
                            for (w, a, c) in sides {
                                if w <= 0.0 {
                                    continue;
                                }
                                let mut path = BezPath::new();
                                path.move_to(a);
                                path.line_to(c);
                                self.ops.push(Op::Stroke { path: xf * path, brush: br.clone(), style: StrokeStyle { width: w, cap: "butt".into(), join: "miter".into(), dash: dash(w) } });
                            }
                        }
                    }
                }
                continue;
            }
            let text_w = lines.iter().map(|l| l.line.width).fold(0.0, f64::max);
            let mut rule = |rule: &formdes_model::attrs::Rule, y: f64, line: &Line| {
                let (x0, x1) = if rule.text_width { (line.left, line.left + text_w) } else { (b.left, b.right) };
                let mut path = BezPath::new();
                path.move_to((x0 + rule.left, y));
                path.line_to((x1 - rule.right, y));
                if let Some(br) = brush_of(self.doc, &rule.color, rule.tint, Rect::ZERO, xf) {
                    self.ops.push(Op::Stroke { path: xf * path, brush: br, style: StrokeStyle { width: rule.weight, cap: "butt".into(), join: "miter".into(), dash: Vec::new() } });
                }
            };
            if b.first
                && let Some(r) = &props.rule_above
            {
                rule(r, first.baseline - r.offset - r.weight / 2.0, &first.line);
            }
            if b.last
                && let Some(r) = &props.rule_below
            {
                rule(r, last.baseline + r.offset + r.weight / 2.0, &last.line);
            }
        }
    }

    fn line(&mut self, line: &Line, baseline: f64, xf: Affine, text: &str) {
        for r in &line.runs {
            self.run(r, baseline, xf, text);
        }
        for t in &line.tabs {
            if t.leader.is_empty() || t.x1 - t.x0 < 1.0 {
                continue;
            }
            let Some(r) = line.runs.first() else { continue };
            let lead = shape(&ShapeReq { text: &t.leader, face: &r.face, size: r.size, kerning: true, ligatures: false, features: &[], rtl: false, language: "en" });
            let w: f64 = lead.iter().map(|g| g.advance).sum();
            if w <= 0.1 {
                continue;
            }
            let mut x = (t.x0 / w).ceil() * w;
            let mut glyphs = Vec::new();
            let mut advances = Vec::new();
            while x + w <= t.x1 + 0.01 {
                let mut gx = x;
                for g in &lead {
                    glyphs.push((g.id, gx, 0.0));
                    advances.push(g.advance);
                    gx += g.advance;
                }
                x += w;
            }
            if glyphs.is_empty() {
                continue;
            }
            self.ops.push(Op::Glyphs(GlyphOp {
                face: r.face.clone(),
                size: r.size,
                hscale: 1.0,
                vscale: 1.0,
                skew: 0.0,
                xf: xf * Affine::translate((0.0, baseline)),
                ranges: vec![0..0; glyphs.len()],
                glyphs,
                advances,
                per_glyph: None,
                text: String::new(),
                fill: run_fill(self.doc, r),
                stroke: None,
            }));
        }
    }

    fn run(&mut self, r: &GlyphRun, baseline: f64, xf: Affine, text: &str) {
        if r.glyphs.is_empty() {
            return;
        }
        let fill = run_fill(self.doc, r);
        let stroke = run_stroke(self.doc, r);
        let mut glyphs = Vec::with_capacity(r.glyphs.len());
        let mut advances = Vec::with_capacity(r.glyphs.len());
        let mut ranges = Vec::with_capacity(r.glyphs.len());
        let mut clusters: Vec<usize> = r.glyphs.iter().map(|g| g.cluster).filter(|c| *c != GENERATED).collect();
        clusters.sort_unstable();
        clusters.dedup();
        for g in &r.glyphs {
            glyphs.push((g.id, g.x, g.y));
            advances.push(g.adv);
            if g.cluster == GENERATED {
                ranges.push(0..0);
                continue;
            }
            let start = g.cluster.min(text.len());
            let next = clusters.iter().find(|c| **c > g.cluster).copied().unwrap_or_else(|| next_char(text, start));
            let end = next.min(text.len()).max(start);
            ranges.push(if text.is_char_boundary(start) && text.is_char_boundary(end) { start..end } else { 0..0 });
        }
        let line_xf = xf * Affine::translate((0.0, baseline));
        let m = &r.face.metrics;
        let k = r.size / m.upem;
        if r.props.underline || r.props.strike {
            let deco = |off: f64, th: f64| Rect::new(r.x0, -off - th / 2.0 - r.shift, r.x1, -off + th / 2.0 - r.shift);
            let lines = [
                (r.props.underline, &r.props.underline_style, m.underline_offset, m.underline_thickness, -1.0),
                (r.props.strike, &r.props.strike_style, m.strike_offset, m.strike_thickness, 1.0),
            ];
            for (on, st, font_off, font_th, sign) in lines {
                if !on {
                    continue;
                }
                let th = st.weight.unwrap_or(font_th * k);
                let off = st.offset.map(|o| o * sign).unwrap_or(font_off * k);
                let rect = deco(off, th);
                let brush = match &st.color {
                    Some(p) => brush_of(self.doc, p, st.tint, rect, Affine::IDENTITY),
                    None => fill.clone(),
                };
                if let Some(b) = brush {
                    self.ops.push(Op::Fill { path: line_xf * rect.to_path(0.1), brush: b, even_odd: false });
                }
            }
        }
        self.ops.push(Op::Glyphs(GlyphOp {
            face: r.face.clone(),
            size: r.size,
            hscale: r.hscale,
            vscale: r.props.v_scale,
            skew: r.props.skew,
            xf: line_xf,
            glyphs,
            advances,
            per_glyph: None,
            text: text.to_string(),
            ranges,
            fill,
            stroke,
        }));
    }

    fn text_links(&mut self, sl: &StoryLayout, para: usize, line: &Line, baseline: f64, xf: Affine) {
        for h in &self.doc.hyperlinks {
            let LinkSource::Text { story, start, end } = &h.source else { continue };
            if *story != sl.story {
                continue;
            }
            let Some(target) = link_target(&h.dest) else { continue };
            let (Some((pa, ba)), Some((pb, bb))) = (sl.flow.para_byte(*start), sl.flow.para_byte(*end)) else { continue };
            if para < pa || para > pb {
                continue;
            }
            let lo = if para == pa { ba.max(line.start) } else { line.start };
            let hi = if para == pb { bb.min(line.end) } else { line.end };
            if hi <= lo {
                continue;
            }
            let xs: Vec<f64> = line.carets.iter().filter(|c| c.0 >= lo && c.0 <= hi).map(|c| c.1).collect();
            let (Some(x0), Some(x1)) = (xs.iter().copied().reduce(f64::min), xs.iter().copied().reduce(f64::max)) else { continue };
            let r = Rect::new(x0, baseline - line.ascent, x1, baseline + line.descent);
            self.ops.push(Op::Link { rect: xf.transform_rect_bbox(r), target });
        }
    }
}

fn next_char(text: &str, at: usize) -> usize {
    let at = at.min(text.len());
    if !text.is_char_boundary(at) {
        return (at + 1..=text.len()).find(|i| text.is_char_boundary(*i)).unwrap_or(text.len());
    }
    text[at..].chars().next().map(|c| at + c.len_utf8()).unwrap_or(at)
}

fn link_target(d: &LinkDest) -> Option<LinkTarget> {
    match d {
        LinkDest::Url(u) => Some(LinkTarget::Url(u.clone())),
        LinkDest::Email(e) => Some(LinkTarget::Url(if e.starts_with("mailto:") { e.clone() } else { format!("mailto:{e}") })),
        LinkDest::Page(p) => Some(LinkTarget::Page(*p)),
        LinkDest::Anchor(_) => None,
    }
}

fn run_fill(doc: &Doc, r: &GlyphRun) -> Option<Brush> {
    brush_of(doc, &r.props.fill, r.props.fill_tint, Rect::new(r.x0, -r.size, r.x1, 0.0), Affine::IDENTITY)
}

fn run_stroke(doc: &Doc, r: &GlyphRun) -> Option<(Brush, f64)> {
    if r.props.stroke_weight <= 0.0 {
        return None;
    }
    brush_of(doc, &r.props.stroke, 1.0, Rect::ZERO, Affine::IDENTITY).map(|b| (b, r.props.stroke_weight))
}

fn layer_order(doc: &Doc) -> Vec<formdes_model::ids::LayerId> {
    doc.layers.iter().rev().map(|l| l.id).collect()
}

fn parent_items(doc: &Doc, pid: formdes_model::ids::ParentId, slot_from_right: bool, depth: u32) -> Vec<(ItemId, Affine)> {
    let Some(parent) = doc.parent(pid) else { return Vec::new() };
    let mut out = Vec::new();
    if depth < 8
        && let Some(b) = parent.based_on
    {
        out.extend(parent_items(doc, b, slot_from_right, depth + 1));
    }
    let slot = if slot_from_right { parent.pages.len().saturating_sub(1) } else { 0 };
    let px = parent.page_x(slot);
    let Some(pp) = parent.pages.get(slot) else { return out };
    let prect = Rect::new(px, 0.0, px + pp.width, pp.height);
    for id in &parent.items {
        if let Some(b) = doc.bounds(*id)
            && b.intersect(prect).area() > 0.0
        {
            out.push((*id, Affine::translate((-px, 0.0))));
        }
    }
    out
}

pub fn page_scene(doc: &Doc, lay: &DocLayout, abs: usize, opts: &SceneOpts) -> Option<PageScene> {
    let (si, pi) = doc.page_slot(abs)?;
    let spread = &doc.spreads[si];
    let page = &spread.pages[pi];
    let px = spread.page_x(pi);
    let right = doc.is_right_page(abs);
    let bleed = if opts.bleed {
        let b = &doc.settings.bleed;
        let (l, r) = b.left_right(!right);
        Edges { top: b.top, bottom: b.bottom, inside: l, outside: r }
    } else {
        Edges::default()
    };
    let area = Rect::new(px - bleed.inside, -bleed.top, px + page.width + bleed.outside, page.height + bleed.bottom);
    let to_page = Affine::translate((-px, 0.0));
    let mut parents: Vec<(ItemId, Affine)> = Vec::new();
    if let Some(pid) = page.parent {
        let use_right = doc.settings.facing && right;
        parents = parent_items(doc, pid, use_right, 0).into_iter().filter(|(id, _)| !page.detached.contains(id)).collect();
    }
    let mut ctx = Ctx { doc, lay, opts, ops: Vec::new(), page: None };
    for layer in layer_order(doc) {
        ctx.page = Some(abs);
        for (id, base) in &parents {
            if doc.item(*id).map(|i| i.layer == layer).unwrap_or(false) {
                ctx.emit(*id, *base);
            }
        }
        ctx.page = None;
        for id in &spread.items {
            let Some(it) = doc.item(*id) else { continue };
            if it.layer != layer || it.anchored_in.is_some() {
                continue;
            }
            if let Some(b) = doc.bounds(*id)
                && b.intersect(area).area() <= 0.0
                && !(b.width() == 0.0 || b.height() == 0.0)
            {
                continue;
            }
            ctx.emit(*id, to_page);
        }
    }
    Some(PageScene { page: abs, width: page.width, height: page.height, bleed, ops: ctx.ops })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    Spread(usize),
    Parent(usize),
}

pub fn spread_ops(doc: &Doc, lay: &DocLayout, target: Target, opts: &SceneOpts) -> Vec<Op> {
    let mut ctx = Ctx { doc, lay, opts, ops: Vec::new(), page: None };
    match target {
        Target::Spread(si) => {
            let Some(spread) = doc.spreads.get(si) else { return Vec::new() };
            let first = doc.first_page_of_spread(si);
            let mut parents: Vec<(ItemId, Affine, usize)> = Vec::new();
            for (pi, page) in spread.pages.iter().enumerate() {
                let Some(pid) = page.parent else { continue };
                let right = doc.settings.facing && doc.is_right_page(first + pi);
                let px = spread.page_x(pi);
                for (id, base) in parent_items(doc, pid, right, 0) {
                    if !page.detached.contains(&id) {
                        parents.push((id, Affine::translate((px, 0.0)) * base, first + pi));
                    }
                }
            }
            for layer in layer_order(doc) {
                for (id, base, abs) in &parents {
                    if doc.item(*id).map(|i| i.layer == layer).unwrap_or(false) {
                        ctx.page = Some(*abs);
                        ctx.emit(*id, *base);
                    }
                }
                ctx.page = None;
                for id in &spread.items {
                    if doc.item(*id).map(|i| i.layer == layer && i.anchored_in.is_none()).unwrap_or(false) {
                        ctx.emit(*id, Affine::IDENTITY);
                    }
                }
            }
        }
        Target::Parent(pi) => {
            let Some(parent) = doc.parents.get(pi) else { return Vec::new() };
            let mut base_items: Vec<ItemId> = Vec::new();
            let mut cur = parent.based_on;
            let mut guard = 0;
            while let Some(b) = cur.and_then(|id| doc.parent(id)) {
                base_items.splice(0..0, b.items.iter().copied());
                cur = b.based_on;
                guard += 1;
                if guard > 8 {
                    break;
                }
            }
            for layer in layer_order(doc) {
                for id in base_items.iter().chain(parent.items.iter()) {
                    if doc.item(*id).map(|i| i.layer == layer && i.anchored_in.is_none()).unwrap_or(false) {
                        ctx.emit(*id, Affine::IDENTITY);
                    }
                }
            }
        }
    }
    ctx.ops
}
