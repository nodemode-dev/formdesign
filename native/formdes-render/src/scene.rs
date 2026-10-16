use std::ops::Range;
use std::sync::Arc;

use formdes_model::attrs::ParaProps;
use formdes_model::color::{Color, Gradient, Paint};
use formdes_model::doc::{Doc, LinkDest, LinkSource};
use formdes_model::geom::{Affine, BezPath, Edges, KShape, Point, Rect};
use formdes_model::ids::{AssetId, ItemId};
use formdes_model::item::{Item, ItemKind, StrokeAlign};
use formdes_text::compose::{GENERATED, GlyphRun, Line, Orient};
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

pub fn join_parts(join: &str) -> (&str, f64) {
    let mut it = join.split_whitespace();
    let kind = it.next().unwrap_or("miter");
    let limit = it.next().and_then(|v| v.parse::<f64>().ok()).unwrap_or(4.0).max(1.0);
    (kind, limit)
}

fn aligned_dashes(path: &BezPath, dash: &[f64], closed: bool) -> Vec<f64> {
    use kurbo::ParamCurveArclen;
    let pattern: Vec<f64> = if dash.len() % 2 == 1 { dash.iter().chain(dash.iter()).copied().collect() } else { dash.to_vec() };
    let period: f64 = pattern.iter().sum();
    let len: f64 = path.segments().map(|s| s.arclen(0.05)).sum();
    if period <= 1e-6 || len <= 1e-6 {
        return dash.to_vec();
    }
    let last_gap = *pattern.last().unwrap_or(&0.0);
    let target = |n: f64| if closed { n * period } else { n * period - last_gap };
    let n = ((len + if closed { 0.0 } else { last_gap }) / period).round().max(1.0);
    let k = len / target(n).max(1e-6);
    pattern.iter().map(|d| d * k).collect()
}

fn path_end(path: &BezPath, at_start: bool) -> Option<(Point, kurbo::Vec2)> {
    use kurbo::{ParamCurve, ParamCurveDeriv, PathSeg};
    let segs: Vec<PathSeg> = path.segments().collect();
    let seg = if at_start { segs.first()? } else { segs.last()? };
    let t = if at_start { 0.0 } else { 1.0 };
    let d = match seg {
        PathSeg::Line(l) => l.p1 - l.p0,
        PathSeg::Quad(q) => q.deriv().eval(t).to_vec2(),
        PathSeg::Cubic(c) => c.deriv().eval(t).to_vec2(),
    };
    let d = if d.hypot() > 1e-9 { d.normalize() } else { (seg.end() - seg.start()).normalize() };
    Some((seg.eval(t), if at_start { -d } else { d }))
}

fn arrowhead(kind: &str, tip: Point, dir: kurbo::Vec2, s: f64) -> (BezPath, bool) {
    let n = kurbo::Vec2::new(-dir.y, dir.x);
    let s = s.max(0.25);
    let mut p = BezPath::new();
    let wide = kind.ends_with("Wide");
    match kind {
        "simple" | "simpleWide" => {
            let back = tip - dir * (4.5 * s);
            let half = if wide { 3.2 } else { 2.2 } * s;
            p.move_to(back + n * half);
            p.line_to(tip);
            p.line_to(back - n * half);
            (p, false)
        }
        "circle" | "circleSolid" => {
            let c = tip - dir * (2.0 * s);
            p.extend(kurbo::Shape::path_elements(&kurbo::Circle::new(c, 2.0 * s), 0.05));
            (p, kind == "circleSolid")
        }
        "square" | "squareSolid" => {
            let c = tip - dir * (2.0 * s);
            let (a, b) = (dir * (2.0 * s), n * (2.0 * s));
            p.move_to(c + a + b);
            p.line_to(c + a - b);
            p.line_to(c - a - b);
            p.line_to(c - a + b);
            p.close_path();
            (p, kind == "squareSolid")
        }
        "bar" => {
            p.move_to(tip + n * (2.5 * s));
            p.line_to(tip - n * (2.5 * s));
            (p, false)
        }
        "barbed" | "curved" => {
            let back = tip - dir * (5.0 * s);
            let half = 2.6 * s;
            p.move_to(tip);
            p.line_to(back + n * half);
            if kind == "curved" { p.quad_to(tip - dir * (3.2 * s), back - n * half) } else { p.line_to(tip - dir * (3.6 * s)); p.line_to(back - n * half) }
            p.close_path();
            (p, true)
        }
        _ => {
            let back = tip - dir * (4.5 * s);
            let half = if wide { 3.2 } else { 2.2 } * s;
            p.move_to(tip);
            p.line_to(back + n * half);
            p.line_to(back - n * half);
            p.close_path();
            (p, true)
        }
    }
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
    vert: bool,
    depth: u8,
}

/// The first subpath of a path as a polyline with its running length, for bending art along it.
struct Spine {
    pts: Vec<Point>,
    acc: Vec<f64>,
}

impl Spine {
    fn new(path: &BezPath) -> Spine {
        let mut pts: Vec<Point> = Vec::new();
        let mut started = false;
        kurbo::flatten(path.iter(), 0.1, |el| match el {
            kurbo::PathEl::MoveTo(p) if !started => {
                started = true;
                pts.push(p);
            }
            kurbo::PathEl::LineTo(p) if pts.len() < 20_000 && pts.last().is_some_and(|q| q.distance(p) > 1e-6) => pts.push(p),
            kurbo::PathEl::ClosePath if pts.len() > 1 => {
                let first = pts[0];
                pts.push(first);
            }
            _ => {}
        });
        let mut acc = vec![0.0; pts.len()];
        for i in 1..pts.len() {
            acc[i] = acc[i - 1] + pts[i].distance(pts[i - 1]);
        }
        Spine { pts, acc }
    }

    fn at(&self, s: f64) -> (Point, kurbo::Vec2) {
        let n = self.pts.len();
        if n < 2 {
            return (self.pts.first().copied().unwrap_or(Point::ZERO), kurbo::Vec2::new(0.0, 1.0));
        }
        let i = self.acc.partition_point(|a| *a < s).clamp(1, n - 1);
        let (a, b) = (self.pts[i - 1], self.pts[i]);
        let len = (self.acc[i] - self.acc[i - 1]).max(1e-9);
        let t = ((s - self.acc[i - 1]) / len).clamp(0.0, 1.0);
        let d = (b - a) / len;
        (a.lerp(b, t), kurbo::Vec2::new(-d.y, d.x))
    }

    /// Maps art lying in `frame` onto the spine: x across the frame becomes distance along the spine's length,
    /// y from the frame's centre line becomes offset along the normal, times `across`.
    fn bend(&self, art: &BezPath, frame: Rect, across: f64, flip: bool) -> BezPath {
        let total = self.acc.last().copied().unwrap_or(0.0);
        let map = |p: Point| {
            let u = (p.x - frame.x0) / frame.width();
            let v = (p.y - frame.center().y) * across * if flip { -1.0 } else { 1.0 };
            let (q, nrm) = self.at(u * total);
            q + nrm * v
        };
        let mut out = BezPath::new();
        let mut last = Point::ZERO;
        let mut first = Point::ZERO;
        let step = (frame.width() / 200.0).max(0.25);
        let run = |out: &mut BezPath, a: Point, b: Point| {
            let steps = ((b.distance(a) / step).ceil() as usize).clamp(1, 400);
            for j in 1..=steps {
                out.line_to(map(a.lerp(b, j as f64 / steps as f64)));
            }
        };
        kurbo::flatten(art.iter(), 0.05, |el| match el {
            kurbo::PathEl::MoveTo(p) => {
                out.move_to(map(p));
                last = p;
                first = p;
            }
            kurbo::PathEl::LineTo(p) => {
                run(&mut out, last, p);
                last = p;
            }
            kurbo::PathEl::ClosePath => {
                if last.distance(first) > 1e-6 {
                    run(&mut out, last, first);
                }
                out.close_path();
                last = first;
            }
            _ => {}
        });
        out
    }
}

/// Points every `step` along a path's length with the path's direction there (radians), each subpath from its start.
fn along(path: &BezPath, step: f64) -> Vec<(Point, f64)> {
    let mut out = Vec::new();
    let mut prev: Option<Point> = None;
    let mut start: Option<Point> = None;
    let mut next_at = 0.0;
    let mut walked = 0.0;
    let visit = |a: Point, b: Point, out: &mut Vec<(Point, f64)>, walked: &mut f64, next_at: &mut f64| {
        let len = a.distance(b);
        if len < 1e-9 {
            return;
        }
        let ang = (b.y - a.y).atan2(b.x - a.x);
        while *next_at <= *walked + len && out.len() < 5000 {
            let t = (*next_at - *walked) / len;
            out.push((a.lerp(b, t), ang));
            *next_at += step;
        }
        *walked += len;
    };
    kurbo::flatten(path.iter(), 0.1, |el| match el {
        kurbo::PathEl::MoveTo(p) => {
            prev = Some(p);
            start = Some(p);
            walked = 0.0;
            next_at = 0.0;
        }
        kurbo::PathEl::LineTo(p) => {
            if let Some(a) = prev {
                visit(a, p, &mut out, &mut walked, &mut next_at);
            }
            prev = Some(p);
        }
        kurbo::PathEl::ClosePath => {
            if let (Some(a), Some(s)) = (prev, start) {
                visit(a, s, &mut out, &mut walked, &mut next_at);
            }
            prev = start;
        }
        _ => {}
    });
    out
}

pub fn symbol_master(doc: &Doc, name: &str) -> Option<ItemId> {
    doc.prefs.get("symbols").and_then(|s| s.get(name)).and_then(|v| v.as_u64()).map(ItemId).filter(|id| doc.item(*id).is_some())
}

pub fn brush_of(doc: &Doc, paint: &Paint, tint: f64, local: Rect, xf: Affine) -> Option<Brush> {
    if let Some(g) = doc.gradient_of(paint) {
        return Some(gradient_brush(&g, local, xf));
    }
    let c = doc.paint_ink_color(paint)?;
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

/// The placements of a Repeat group in spread coordinates, the original first. Radial copies turn about a centre
/// `radius` below the art's centre; grid copies step by the art's size plus the gaps, mirrored on alternate rows or
/// columns when asked; a mirror adds one copy reflected across an axis `gap / 2` beyond the art's right edge.
pub fn repeat_xfs(r: &serde_json::Value, bounds: Option<Rect>) -> Vec<Affine> {
    let Some(bb) = bounds else { return vec![Affine::IDENTITY] };
    let n = |k: &str, d: f64| r.get(k).and_then(|v| v.as_f64()).unwrap_or(d);
    let c = bb.center();
    match r.get("kind").and_then(|v| v.as_str()).unwrap_or("") {
        "radial" => {
            let count = n("count", 8.0).round().clamp(1.0, 360.0) as usize;
            let radius = n("radius", bb.width().max(bb.height()));
            let centre = Point::new(c.x, c.y + radius);
            (0..count).map(|i| Affine::rotate_about(std::f64::consts::TAU * i as f64 / count as f64, centre)).collect()
        }
        "grid" => {
            let (cols, rows) = (n("cols", 3.0).round().clamp(1.0, 100.0) as usize, n("rows", 3.0).round().clamp(1.0, 100.0) as usize);
            let (sx, sy) = (bb.width() + n("gapX", 10.0), bb.height() + n("gapY", 10.0));
            let flip = r.get("flip").and_then(|v| v.as_str()).unwrap_or("none");
            let mut out = Vec::new();
            for j in 0..rows {
                for i in 0..cols {
                    let mut t = Affine::translate((i as f64 * sx, j as f64 * sy));
                    let fx = (flip == "cols" || flip == "both") && i % 2 == 1;
                    let fy = (flip == "rows" || flip == "both") && j % 2 == 1;
                    if fx || fy {
                        t = t * Affine::translate(c.to_vec2()) * Affine::scale_non_uniform(if fx { -1.0 } else { 1.0 }, if fy { -1.0 } else { 1.0 }) * Affine::translate(-c.to_vec2());
                    }
                    out.push(t);
                }
            }
            out
        }
        "mirror" => {
            let a = n("angle", 90.0).to_radians();
            let p = Point::new(bb.x1 + n("gap", 0.0) / 2.0, c.y);
            let reflect = Affine::translate(p.to_vec2()) * Affine::rotate(a) * Affine::scale_non_uniform(1.0, -1.0) * Affine::rotate(-a) * Affine::translate(-p.to_vec2());
            vec![Affine::IDENTITY, reflect]
        }
        _ => vec![Affine::IDENTITY],
    }
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

fn m_push_gradient_mask(ops: &mut Vec<Op>, area: Rect, brush: Brush) {
    ops.push(Op::PushMask(vec![Op::Fill { path: area.to_path(0.1), brush, even_odd: false }]));
}

/// The per-cell stroke on one side of a table cell, if the cell (or the cell across that edge) overrides the table
/// border there: weight, colour with tint applied and line type.
fn cell_edge(doc: &Doc, tb: &formdes_model::story::Table, effs: &[formdes_model::attrs::Attrs], cell: &formdes_model::story::TableCell, side: &str, bw: f64, bcol: Color) -> Option<(f64, Color, String)> {
    let own = tb.cells.iter().position(|x| std::ptr::eq(x, cell)).and_then(|i| effs.get(i)).and_then(|a| a.get("strokes")).and_then(|s| s.get(side));
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
        tb.cells.iter().zip(effs).filter(|(x, _)| overlap(x)).find_map(|(_, a)| a.get("strokes").and_then(|s| s.get(opposite)))
    };
    let o = own.or_else(facing)?.as_object()?;
    let weight = o.get("weight").and_then(|v| v.as_f64()).unwrap_or(bw);
    let tint = o.get("tint").and_then(|v| v.as_f64()).unwrap_or(1.0);
    let color = o.get("color").and_then(Paint::from_value).filter(|p| !p.is_none()).and_then(|p| doc.paint_color(&p)).unwrap_or(bcol);
    let color = if (tint - 1.0).abs() > 1e-6 { color.tinted(tint) } else { color };
    let kind = o.get("type").and_then(|v| v.as_str()).unwrap_or("solid").to_string();
    Some((weight, color, kind))
}

/// The table style's alternating row or column stroke on one side of a cell: rows own a cell's top and bottom
/// edges, columns its left and right edges. `spec` is {first, next, skipFirst, skipLast, firstWeight, firstColor,
/// firstTint, firstType, nextWeight, ...}.
fn alt_edge(doc: &Doc, tb: &formdes_model::story::Table, specs: [&Option<serde_json::Value>; 2], cell: &formdes_model::story::TableCell, side: &str) -> Option<(f64, Color, String)> {
    let (spec, line, count, head, foot) = match side {
        "top" => (specs[0], cell.row, tb.rows(), tb.header_rows, tb.footer_rows),
        "bottom" => (specs[0], cell.row + cell.rows - 1, tb.rows(), tb.header_rows, tb.footer_rows),
        "left" => (specs[1], cell.col, tb.cols(), 0, 0),
        _ => (specs[1], cell.col + cell.cols - 1, tb.cols(), 0, 0),
    };
    let spec = spec.as_ref()?;
    let first = formdes_model::story::Table::alt_phase(line, count, head, foot, spec)?;
    let k = if first { "first" } else { "next" };
    let weight = spec.get(format!("{k}Weight")).and_then(|v| v.as_f64()).unwrap_or(1.0);
    let tint = spec.get(format!("{k}Tint")).and_then(|v| v.as_f64()).unwrap_or(1.0);
    let color = spec.get(format!("{k}Color")).and_then(Paint::from_value).filter(|p| !p.is_none()).and_then(|p| doc.paint_color(&p))?;
    let color = if (tint - 1.0).abs() > 1e-6 { color.tinted(tint) } else { color };
    let kind = spec.get(format!("{k}Type")).and_then(|v| v.as_str()).unwrap_or("solid").to_string();
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
        if let Some(s) = fx(it, "gradientFeather") {
            // Opacity stops become grey stops of a gradient fill; the mask's luminance is the object's opacity.
            let mut stops: Vec<(f64, Color, f64)> = s
                .get("stops")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .map(|st| {
                            let o = st.get("opacity").and_then(|v| v.as_f64()).unwrap_or(1.0).clamp(0.0, 1.0);
                            (st.get("location").and_then(|v| v.as_f64()).unwrap_or(0.0).clamp(0.0, 1.0), Color::Rgb([o, o, o]), 1.0)
                        })
                        .collect()
                })
                .unwrap_or_default();
            if stops.len() < 2 {
                stops = vec![(0.0, white, 1.0), (1.0, black, 1.0)];
            }
            stops.sort_by(|a, b| a.0.total_cmp(&b.0));
            let radial = s.get("type").and_then(|v| v.as_str()) == Some("radial");
            let g = Gradient {
                radial,
                angle: fnum(s, "angle", 0.0),
                stops: stops.iter().map(|(at, c, o)| formdes_model::color::Stop { at: *at, color: *c, opacity: *o }).collect(),
            };
            let bb = path.bounding_box();
            let brush = gradient_brush(&g, bb, Affine::IDENTITY);
            let pad = it.stroke.weight * 2.0 + 4.0;
            m_push_gradient_mask(&mut self.ops, bb.inflate(pad, pad), brush);
            opened += 1;
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
                let id_of = |k: &str| it.extra.get(k).and_then(|v| v.as_u64()).map(ItemId).filter(|m| it.children().contains(m));
                let (omask, cmask) = (id_of("opacityMask"), id_of("clipMask"));
                let mut wrap = 0;
                if it.extra.get("isolate").and_then(|v| v.as_bool()).unwrap_or(false) {
                    self.ops.push(Op::PushOpacity(1.0));
                    wrap += 1;
                }
                if let Some(m) = cmask
                    && let Some(mi) = self.doc.item(m)
                {
                    let clip = (base * self.doc.world_xf(m)) * formdes_model::effects::apply(&mi.extra, mi.shape.to_path(&mi.corner));
                    self.ops.push(Op::PushClip(clip));
                    wrap += 1;
                }
                if let Some(m) = omask {
                    let saved = std::mem::take(&mut self.ops);
                    if !it.extra.get("maskClip").and_then(|v| v.as_bool()).unwrap_or(true) {
                        self.ops.push(Op::Fill { path: Rect::new(-1e5, -1e5, 1e5, 1e5).to_path(0.1), brush: Brush::Solid(Color::Rgb([1.0, 1.0, 1.0]), 1.0), even_odd: false });
                    }
                    self.emit(m, base);
                    let mops = std::mem::replace(&mut self.ops, saved);
                    self.ops.push(Op::PushMask(mops));
                    wrap += 1;
                }
                let copies = it.extra.get("repeat").map(|r| repeat_xfs(r, self.doc.bounds(id))).unwrap_or_else(|| vec![Affine::IDENTITY]);
                for t in &copies {
                    for c in it.children() {
                        if Some(c) != omask && Some(c) != cmask {
                            self.emit(c, base * *t);
                        }
                    }
                }
                for _ in 0..wrap {
                    self.ops.push(Op::Pop);
                }
            }
            _ => {
                let mut local = formdes_model::effects::apply(&it.extra, it.shape.to_path(&it.corner));
                if let Some((frame, cols, rows, grid)) = formdes_model::effects::envelope_of(&it.extra) {
                    let w = self.doc.world_xf(id);
                    local = w.inverse() * formdes_model::effects::envelope(&(w * local), frame, cols, rows, &grid);
                }
                let path = xf * local.clone();
                let bbox = it.shape.bbox();
                self.outer_glow(it, &path);
                self.shadow(it, &path);
                let masks = self.feather(it, &path);
                self.appearance(it, &local, xf, false);
                let free = it.shape.closed() && self.freeform(it, &local, &path, xf);
                if it.shape.closed()
                    && !free
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
                            let cut = it.image_path("clipping").map(|p| xf * p);
                            if let Some(cp) = &cut {
                                self.ops.push(Op::PushClip(cp.clone()));
                            }
                            let shown = it.extra.get("layers").and_then(|l| l.get("view")).and_then(|v| v.as_u64()).map(formdes_model::ids::AssetId).filter(|a| self.doc.assets.contains_key(a)).unwrap_or(aid);
                            self.ops.push(Op::Image { asset: shown, xf: xf * g.content, w, h });
                            if cut.is_some() {
                                self.ops.push(Op::Pop);
                            }
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
                self.appearance(it, &local, xf, true);
                if let Some(m) = it.extra.get("symbol").and_then(|v| v.as_str()).and_then(|n| symbol_master(self.doc, n))
                    && self.depth < 6
                {
                    self.depth += 1;
                    self.emit(m, xf);
                    self.depth -= 1;
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
        let Some(s) = fx(it, "dropShadow") else { return };
        let color = fcolor(s, Color::Cmyk([0.0, 0.0, 0.0, 1.0]));
        let (dx, dy) = (fnum(s, "x", 4.0), fnum(s, "y", 4.0));
        let alpha = fnum(s, "opacity", 0.75).clamp(0.0, 1.0);
        let shifted = Affine::translate((dx, dy)) * path.clone();
        if it.fill.is_none() && it.stroke.paint.is_none() {
            return;
        }
        let blended = self.blend_open(&fmode(s, "normal"));
        // Size is the blur: the edge ramps from full to nothing across `size` points, centred on the edge; Spread
        // moves the 50 percent point outward by that share of half the size. Built as a luminance mask of vector
        // bands (white outside the edge, black inside) over one flat fill, so the PDF keeps a vector soft mask.
        let size = fnum(s, "size", 0.0).max(0.0);
        let h = size / 2.0;
        let sp = fnum(s, "spread", 0.0).clamp(0.0, 1.0) * h;
        if size < 0.05 || !it.shape.closed() {
            if it.shape.closed() {
                self.ops.push(Op::Fill { path: shifted, brush: Brush::Solid(color, alpha), even_odd: false });
            } else if it.stroke.weight > 0.0 {
                self.ops.push(Op::Stroke { path: shifted, brush: Brush::Solid(color, alpha), style: StrokeStyle { width: it.stroke.weight, cap: "butt".into(), join: "round".into(), dash: Vec::new() } });
            }
        } else {
            let white = Color::Rgb([1.0, 1.0, 1.0]);
            let black = Color::Rgb([0.0, 0.0, 0.0]);
            let reach = h + sp;
            let around = shifted.bounding_box().inflate(reach + 8.0, reach + 8.0);
            // The outside bands straddle the edge and the opaque fill laid over them hides the inner halves, which
            // leaves no seam where clipped edges would meet.
            let mut m = Vec::new();
            for (r, a) in bands(reach, 24, |d| 1.0 - smooth((d - sp + h) / (2.0 * h))) {
                m.push(band_stroke(&shifted, white, a, r, "round"));
            }
            m.push(Op::Fill { path: shifted.clone(), brush: Brush::Solid(white, 1.0), even_odd: false });
            if h - sp > 0.01 {
                m.push(Op::PushClip(shifted.clone()));
                for (r, a) in bands(h - sp, 24, |x| smooth((h - sp - x) / (2.0 * h))) {
                    m.push(band_stroke(&shifted, black, a, r, "round"));
                }
                m.push(Op::Pop);
            }
            self.ops.push(Op::PushMask(m));
            self.ops.push(Op::Fill { path: around.to_path(0.1), brush: Brush::Solid(color, alpha), even_odd: false });
            self.ops.push(Op::Pop);
        }
        if blended {
            self.ops.push(Op::Pop);
        }
    }

    fn freeform(&mut self, it: &Item, local: &BezPath, path: &BezPath, xf: Affine) -> bool {
        let Some(pts) = it.extra.get("freeform").and_then(|v| v.as_array()) else { return false };
        let pts: Vec<[f64; 5]> = pts.iter().filter_map(|q| {
            let q = q.as_array()?;
            let n = |i: usize| q.get(i).and_then(|v| v.as_f64());
            Some([n(0)?, n(1)?, n(2)?, n(3)?, n(4)?])
        }).collect();
        if pts.is_empty() {
            return false;
        }
        let bb = local.bounding_box();
        if bb.width() <= 0.0 || bb.height() <= 0.0 {
            return false;
        }
        let k = {
            let c = xf.as_coeffs();
            (c[0] * c[3] - c[1] * c[2]).abs().sqrt()
        };
        let n = ((bb.width().max(bb.height()) * k / 3.0).ceil() as usize).clamp(16, 96);
        let (cw, ch) = (bb.width() / n as f64, bb.height() / n as f64);
        let power = it.extra.get("freeformSpread").and_then(|v| v.as_f64()).unwrap_or(2.0).clamp(0.5, 8.0);
        self.ops.push(Op::PushClip(path.clone()));
        for j in 0..n {
            for i in 0..n {
                let (u, v) = ((i as f64 + 0.5) / n as f64, (j as f64 + 0.5) / n as f64);
                let mut acc = [0.0; 3];
                let mut wsum = 0.0;
                let mut exact = None;
                for p in &pts {
                    let d2 = ((u - p[0]) * bb.width()).powi(2) + ((v - p[1]) * bb.height()).powi(2);
                    if d2 < 1e-9 {
                        exact = Some([p[2], p[3], p[4]]);
                        break;
                    }
                    let w = 1.0 / d2.powf(power / 2.0);
                    for c in 0..3 {
                        acc[c] += p[2 + c] * w;
                    }
                    wsum += w;
                }
                let rgb = exact.unwrap_or([acc[0] / wsum, acc[1] / wsum, acc[2] / wsum]);
                let x0 = bb.x0 + i as f64 * cw;
                let y0 = bb.y0 + j as f64 * ch;
                let cell = Rect::new(x0 - cw * 0.3, y0 - ch * 0.3, x0 + cw * 1.3, y0 + ch * 1.3);
                self.ops.push(Op::Fill { path: xf * cell.to_path(0.1), brush: Brush::Solid(Color::Rgb(rgb), 1.0), even_odd: false });
            }
        }
        self.ops.push(Op::Pop);
        true
    }

    fn appearance(&mut self, it: &Item, local: &BezPath, xf: Affine, above: bool) {
        let Some(list) = it.extra.get("appearance").and_then(|v| v.as_array()) else { return };
        let k = {
            let c = xf.as_coeffs();
            (c[0] * c[3] - c[1] * c[2]).abs().sqrt()
        };
        for e in list {
            if e.get("above").and_then(|v| v.as_bool()).unwrap_or(false) != above || e.get("on").and_then(|v| v.as_bool()) == Some(false) {
                continue;
            }
            let Some(paint) = e.get("paint").and_then(|v| serde_json::from_value::<Paint>(v.clone()).ok()) else { continue };
            let Some(b) = brush_of(self.doc, &paint, 1.0, it.shape.bbox(), xf) else { continue };
            let fx = e.get("effects").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            let path = xf * fx.iter().fold(local.clone(), |p, f| formdes_model::effects::one(f, &p));
            let opacity = e.get("opacity").and_then(|v| v.as_f64()).unwrap_or(100.0) / 100.0;
            if opacity < 0.999 {
                self.ops.push(Op::PushOpacity(opacity.clamp(0.0, 1.0)));
            }
            if e.get("kind").and_then(|v| v.as_str()) == Some("stroke") {
                let w = e.get("weight").and_then(|v| v.as_f64()).unwrap_or(1.0) * k;
                let s = |key: &str, d: &str| e.get(key).and_then(|v| v.as_str()).unwrap_or(d).to_string();
                let dash = e.get("dash").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_f64()).map(|x| x * k).collect()).unwrap_or_default();
                self.ops.push(Op::Stroke { path, brush: b, style: StrokeStyle { width: w, cap: s("cap", "butt"), join: s("join", "miter"), dash } });
            } else {
                self.ops.push(Op::Fill { path, brush: b, even_odd: e.get("evenOdd").and_then(|v| v.as_bool()).unwrap_or(false) });
            }
            if opacity < 0.999 {
                self.ops.push(Op::Pop);
            }
        }
    }

    /// Paints a stroke's gradient along its length (bands across the stroke, each a step along it) or across it
    /// (bands running the length, each a step from the left edge to the right). With opaque stops each band
    /// reaches into the next so no seam shows between them.
    fn gradient_ribbon(&mut self, g: &formdes_model::color::Gradient, along: bool, left: &[Point], right: &[Point], ts: &[f64], closed: bool) {
        let n = left.len();
        if n < 2 {
            return;
        }
        let overlap = g.stops.iter().all(|s| s.opacity >= 0.999);
        if along {
            let segs = if closed { n } else { n - 1 };
            for i in 0..segs {
                let j = (i + 1) % n;
                let k = if overlap && i + 1 < segs { (i + 2) % n } else { j };
                let mut p = BezPath::new();
                p.move_to(left[i]);
                p.line_to(left[j]);
                p.line_to(left[k]);
                p.line_to(right[k]);
                p.line_to(right[j]);
                p.line_to(right[i]);
                p.close_path();
                let t = if j == 0 { (ts[i] + 1.0) / 2.0 } else { (ts[i] + ts[j]) / 2.0 };
                if let Some((c, a)) = g.at(t) {
                    self.ops.push(Op::Fill { path: p, brush: Brush::Solid(c, a), even_odd: false });
                }
            }
        } else {
            const BANDS: usize = 24;
            let at = |f: f64, i: usize| left[i] + (right[i] - left[i]) * f;
            for b in 0..BANDS {
                let f0 = b as f64 / BANDS as f64;
                let f1 = ((b + if overlap { 2 } else { 1 }) as f64 / BANDS as f64).min(1.0);
                let mut p = BezPath::new();
                p.move_to(at(f0, 0));
                for i in 1..n {
                    p.line_to(at(f0, i));
                }
                if closed {
                    p.close_path();
                    p.move_to(at(f1, 0));
                    for i in 1..n {
                        p.line_to(at(f1, i));
                    }
                } else {
                    for i in (0..n).rev() {
                        p.line_to(at(f1, i));
                    }
                }
                p.close_path();
                if let Some((c, a)) = g.at((b as f64 + 0.5) / BANDS as f64) {
                    self.ops.push(Op::Fill { path: p, brush: Brush::Solid(c, a), even_odd: closed });
                }
            }
        }
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
        if let Some(br) = it.extra.get("brush") {
            let n = |key: &str, d: f64| br.get(key).and_then(|v| v.as_f64()).unwrap_or(d);
            match br.get("kind").and_then(|v| v.as_str()).unwrap_or("") {
                "calligraphic" => {
                    let shape = formdes_model::widths::nib(path, n("size", st.weight) * k, n("angle", 45.0), n("roundness", 30.0), it.shape.closed());
                    self.ops.push(Op::Fill { path: shape, brush: b, even_odd: true });
                    return;
                }
                "art" => {
                    if let Some(m) = br.get("symbol").and_then(|v| v.as_str()).and_then(|s| symbol_master(self.doc, s))
                        && let Some(mb) = self.doc.bounds(m)
                        && mb.width() > 0.0
                    {
                        let spine = Spine::new(path);
                        let across = n("width", 100.0) / 100.0 * k;
                        let flip = br.get("flip").and_then(|v| v.as_bool()).unwrap_or(false);
                        for leaf in self.doc.walk(&[m]) {
                            let Some(li) = self.doc.item(leaf) else { continue };
                            if matches!(li.kind, ItemKind::Group { .. } | ItemKind::States { .. }) || !self.visible(li) {
                                continue;
                            }
                            let world = self.doc.world_xf(leaf) * formdes_model::effects::apply(&li.extra, li.shape.to_path(&li.corner));
                            let bent = spine.bend(&world, mb, across, flip);
                            if li.shape.closed()
                                && let Some(fb) = brush_of(self.doc, &li.fill, 1.0, kurbo::Shape::bounding_box(&bent), Affine::IDENTITY)
                            {
                                self.ops.push(Op::Fill { path: bent.clone(), brush: fb, even_odd: li.extra.get("fillRule").and_then(|v| v.as_str()) == Some("evenodd") });
                            }
                            if li.stroke.weight > 0.0
                                && let Some(sb) = brush_of(self.doc, &li.stroke.paint, 1.0, kurbo::Shape::bounding_box(&bent), Affine::IDENTITY)
                            {
                                self.ops.push(Op::Stroke { path: bent, brush: sb, style: StrokeStyle { width: li.stroke.weight * across, cap: li.stroke.cap.clone(), join: li.stroke.join.clone(), dash: Vec::new() } });
                            }
                        }
                    }
                    return;
                }
                "scatter" => {
                    if let Some(m) = br.get("symbol").and_then(|v| v.as_str()).and_then(|s| symbol_master(self.doc, s))
                        && let Some(mb) = self.doc.bounds(m)
                        && self.depth < 6
                    {
                        let size = mb.width().max(mb.height()).max(1e-6);
                        let s = n("scale", 100.0) / 100.0;
                        let step = n("spacing", size * s).max(0.5) * k;
                        let turn = br.get("rotate").and_then(|v| v.as_bool()).unwrap_or(true);
                        for (p, ang) in along(path, step) {
                            let t = Affine::translate(p.to_vec2()) * Affine::rotate(if turn { ang } else { 0.0 }) * Affine::scale(s * k) * Affine::translate(-mb.center().to_vec2());
                            self.depth += 1;
                            self.emit(m, t);
                            self.depth -= 1;
                        }
                    }
                    return;
                }
                _ => {}
            }
        }
        if let Some(mode) = it.extra.get("strokeGradient").and_then(|v| v.as_str()).filter(|m| *m == "along" || *m == "across")
            && let Some(g) = self.doc.gradient_of(&st.paint)
        {
            let prof = formdes_model::widths::profile_of(&it.extra);
            for (left, right, ts, closed) in formdes_model::widths::sides(path, prof.as_ref(), w, it.shape.closed()) {
                self.gradient_ribbon(&g, mode == "along", &left, &right, &ts, closed);
            }
            return;
        }
        if let Some(prof) = formdes_model::widths::profile_of(&it.extra) {
            let shape = formdes_model::widths::outline(path, &prof, w, it.shape.closed());
            self.ops.push(Op::Fill { path: shape, brush: b, even_odd: true });
            return;
        }
        let join = if st.join == "miter" && (st.miter - 4.0).abs() > 1e-6 { format!("miter {}", st.miter.max(1.0)) } else { st.join.clone() };
        let mut dash: Vec<f64> = st.dash.iter().map(|d| d * k).collect();
        if st.align_dashes && !dash.is_empty() {
            dash = aligned_dashes(path, &dash, it.shape.closed());
        }
        let style = |width: f64| StrokeStyle { width, cap: st.cap.clone(), join: join.clone(), dash: dash.clone() };
        let head = b.clone();
        match st.align {
            StrokeAlign::Inside if it.shape.closed() => {
                self.ops.push(Op::PushClip(path.clone()));
                self.ops.push(Op::Stroke { path: path.clone(), brush: b, style: style(w * 2.0) });
                self.ops.push(Op::Pop);
            }
            _ => self.ops.push(Op::Stroke { path: path.clone(), brush: b, style: style(w) }),
        }
        if !it.shape.closed() {
            for (kind, scale, at_start) in [(&st.start, st.start_scale, true), (&st.end, st.end_scale, false)] {
                if kind.is_empty() || kind == "none" {
                    continue;
                }
                if let Some((p, dir)) = path_end(path, at_start) {
                    let (shape, filled) = arrowhead(kind, p, dir, w * scale.max(1.0) / 100.0);
                    if filled {
                        self.ops.push(Op::Fill { path: shape, brush: head.clone(), even_odd: false });
                    } else {
                        self.ops.push(Op::Stroke { path: shape, brush: head.clone(), style: StrokeStyle { width: w, cap: "round".into(), join: "round".into(), dash: Vec::new() } });
                    }
                }
            }
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
        let xf0 = xf;
        let vertical = sl.specs.get(fi).is_some_and(|s| s.vertical);
        let xf = if vertical { xf * formdes_text::layout::composition_xf(&sl.specs[fi]) } else { xf };
        let outer_vert = self.vert;
        self.vert = vertical;
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
            let alt_rows = table.and_then(|tb| tb.style_attr(&self.doc.styles, "altRows"));
            let alt_cols = table.and_then(|tb| tb.style_attr(&self.doc.styles, "altCols"));
            let alt_row_strokes = table.and_then(|tb| tb.style_attr(&self.doc.styles, "altRowStrokes"));
            let alt_col_strokes = table.and_then(|tb| tb.style_attr(&self.doc.styles, "altColStrokes"));
            let eff_all: Vec<formdes_model::attrs::Attrs> = table.map(|tb| tb.cells.iter().map(|cell| tb.cell_attrs(&self.doc.styles, cell)).collect()).unwrap_or_default();
            let radius = table.and_then(|tb| tb.attrs.get("cornerRadius")).and_then(|v| v.as_f64()).filter(|r| *r > 0.0);
            let round = radius.and_then(|r| {
                let area = t.cells.iter().map(|c| c.rect).reduce(|a, b| a.union(b))?;
                Some(kurbo::RoundedRect::from_rect(area, r.min(area.width() / 2.0).min(area.height() / 2.0)).to_path(0.1))
            });
            if let Some(rp) = &round {
                self.ops.push(Op::PushClip(xf * rp.clone()));
            }
            for c in &t.cells {
                let eff: Option<&formdes_model::attrs::Attrs> = table.and_then(|tb| tb.cells.iter().position(|cell| c.row >= cell.row && c.row < cell.row + cell.rows && c.col >= cell.col && c.col < cell.col + cell.cols)).and_then(|i| eff_all.get(i));
                if let Some(tb) = table
                    && let Some(ea) = eff
                {
                    let tint_of = |v: Option<&serde_json::Value>| v.and_then(|x| x.as_f64()).unwrap_or(1.0);
                    let explicit = ea.get("fill").and_then(Paint::from_value).filter(|p| !p.is_none()).map(|p| (p, tint_of(ea.get("fillTint"))));
                    let pattern = |spec: &Option<serde_json::Value>, line: usize, count: usize, head: usize, foot: usize| {
                        let a = spec.as_ref()?;
                        let first = formdes_model::story::Table::alt_phase(line, count, head, foot, a)?;
                        let (col, tint) = if first { (a.get("firstColor"), a.get("firstTint")) } else { (a.get("nextColor"), a.get("nextTint")) };
                        col.and_then(Paint::from_value).filter(|p| !p.is_none()).map(|p| (p, tint_of(tint)))
                    };
                    let alt = pattern(&alt_rows, c.row, tb.rows(), tb.header_rows, tb.footer_rows).or_else(|| pattern(&alt_cols, c.col, tb.cols(), 0, 0));
                    if let Some((paint, tint)) = explicit.or(alt)
                        && let Some(col) = self.doc.paint_color(&paint)
                    {
                        let col = if (tint - 1.0).abs() > 1e-6 { col.tinted(tint) } else { col };
                        let alpha = ea.get("fillOpacity").and_then(|v| v.as_f64()).map_or(1.0, |o| (o / 100.0).clamp(0.0, 1.0));
                        self.ops.push(Op::Fill { path: xf * c.rect.to_path(0.1), brush: Brush::Solid(col, alpha), even_odd: false });
                    }
                }
                let diagonal = eff.and_then(|ea| ea.get("diagonal")).and_then(|d| d.as_object());
                if let Some(d) = diagonal
                    && !d.get("front").and_then(|f| f.as_bool()).unwrap_or(false)
                {
                    self.diagonals(d, c.rect, xf, bw, bcol);
                }
                let graphic_cell = table.and_then(|tb| tb.cell(c.row, c.col)).filter(|cell| crate::cellgfx::cell_graphic(cell).is_some());
                if let Some(cell) = graphic_cell {
                    let ops = crate::cellgfx::cell_image_ops(self.doc, cell, c.rect, xf);
                    self.ops.extend(ops);
                }
                let edges: [Option<(f64, Color, String)>; 4] = match table.and_then(|tb| tb.cell(c.row, c.col).map(|cell| (tb, cell))) {
                    Some((tb, cell)) => ["top", "bottom", "left", "right"].map(|s| {
                        cell_edge(self.doc, tb, &eff_all, cell, s, bw, bcol).or_else(|| alt_edge(self.doc, tb, [&alt_row_strokes, &alt_col_strokes], cell, s))
                    }),
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
                let cell_xf = xf * formdes_text::flow::cell_xf(c.rect, c.rotate);
                for l in c.lines.iter().filter(|_| graphic_cell.is_none()) {
                    self.line(&l.line, l.baseline, cell_xf, cell_texts.get(l.para).map(String::as_str).unwrap_or(""));
                }
                if let Some(d) = diagonal
                    && d.get("front").and_then(|f| f.as_bool()).unwrap_or(false)
                {
                    self.diagonals(d, c.rect, xf, bw, bcol);
                }
            }
            if let Some(rp) = round {
                self.ops.push(Op::Pop);
                if bw > 0.0 {
                    self.ops.push(Op::Stroke { path: xf * rp, brush: Brush::Solid(bcol, 1.0), style: StrokeStyle { width: bw, cap: "butt".into(), join: "round".into(), dash: Vec::new() } });
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
        let mut ruled: Vec<usize> = Vec::new();
        for n in flow.notes.iter().filter(|n| n.frame == fi) {
            let rule = !ruled.contains(&n.column);
            ruled.push(n.column);
            if rule && let Some(first) = n.lines.first() {
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
        self.vert = outer_vert;
        let anchors: Vec<_> = flow.anchors.iter().filter(|a| a.frame == fi).cloned().collect();
        for a in anchors {
            let Some(it) = self.doc.item(a.item) else { continue };
            let b = it.xf.transform_rect_bbox(it.shape.bbox());
            let corner_y = if a.inline { b.y1 } else { b.y0 };
            let desired = xf0 * Affine::translate((a.x - b.x0, a.y - corner_y)) * it.xf;
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
        for r in &line.adorn {
            self.run(r, baseline, xf, "");
        }
        for m in &line.marks {
            self.kenten(m, baseline, xf);
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

    /// A table cell's diagonal lines: `down` runs from the top left corner to the bottom right, `up` from the
    /// bottom left to the top right; each is {weight, color, tint, type}.
    fn diagonals(&mut self, d: &serde_json::Map<String, serde_json::Value>, r: Rect, xf: Affine, bw: f64, bcol: Color) {
        for (key, a, b) in [("down", (r.x0, r.y0), (r.x1, r.y1)), ("up", (r.x0, r.y1), (r.x1, r.y0))] {
            let Some(o) = d.get(key).and_then(|v| v.as_object()) else { continue };
            let w = o.get("weight").and_then(|v| v.as_f64()).unwrap_or(bw.max(0.5));
            let kind = o.get("type").and_then(|v| v.as_str()).unwrap_or("solid");
            if w <= 0.0 || kind == "none" {
                continue;
            }
            let tint = o.get("tint").and_then(|v| v.as_f64()).unwrap_or(1.0);
            let col = o.get("color").and_then(Paint::from_value).filter(|p| !p.is_none()).and_then(|p| self.doc.paint_color(&p)).unwrap_or(bcol);
            let col = if (tint - 1.0).abs() > 1e-6 { col.tinted(tint) } else { col };
            let (cap, dash) = match kind {
                "dashed" => ("butt", vec![w * 3.0, w * 2.0]),
                "dotted" => ("round", vec![0.01, w * 2.0]),
                _ => ("butt", Vec::new()),
            };
            let mut line = BezPath::new();
            line.move_to(a);
            line.line_to(b);
            self.ops.push(Op::Stroke { path: xf * line, brush: Brush::Solid(col, 1.0), style: StrokeStyle { width: w, cap: cap.into(), join: "miter".into(), dash } });
        }
    }

    fn kenten(&mut self, m: &formdes_text::compose::EmphasisMark, baseline: f64, xf: Affine) {
        let s = m.size;
        let p = &m.props;
        let c = kurbo::Vec2::new(m.x, baseline + m.y);
        let local = if self.vert { Affine::translate(c) * Affine::rotate(-std::f64::consts::FRAC_PI_2) } else { Affine::translate(c) };
        let place = xf * local;
        let Some(brush) = brush_of(self.doc, &p.fill, p.fill_tint, Rect::new(-s, -s, s, s), Affine::IDENTITY) else { return };
        let ring = |this: &mut Self, path: BezPath, w: f64| {
            this.ops.push(Op::Stroke { path: place * path, brush: brush.clone(), style: StrokeStyle { width: w, cap: "butt".into(), join: "miter".into(), dash: Vec::new() } });
        };
        let solid = |this: &mut Self, path: BezPath| {
            this.ops.push(Op::Fill { path: place * path, brush: brush.clone(), even_odd: false });
        };
        let tri = || {
            let mut t = BezPath::new();
            t.move_to((0.0, -s * 0.5));
            t.line_to((s * 0.45, s * 0.4));
            t.line_to((-s * 0.45, s * 0.4));
            t.close_path();
            t
        };
        let sesame = || kurbo::Ellipse::new((0.0, 0.0), (s * 0.25, s * 0.5), 0.5).to_path(0.05);
        match m.kind {
            '\u{FE45}' => solid(self, sesame()),
            '\u{FE46}' => ring(self, sesame(), s * 0.1),
            '\u{2022}' => solid(self, kurbo::Circle::new((0.0, 0.0), s * 0.4).to_path(0.05)),
            '\u{25CB}' => ring(self, kurbo::Circle::new((0.0, 0.0), s * 0.42).to_path(0.05), s * 0.1),
            '\u{25C9}' => {
                ring(self, kurbo::Circle::new((0.0, 0.0), s * 0.5).to_path(0.05), s * 0.1);
                solid(self, kurbo::Circle::new((0.0, 0.0), s * 0.28).to_path(0.05));
            }
            '\u{25B2}' => solid(self, tri()),
            _ => ring(self, tri(), s * 0.1),
        }
    }

    fn run(&mut self, r: &GlyphRun, baseline: f64, xf: Affine, text: &str) {
        if r.glyphs.is_empty() {
            return;
        }
        let text = if r.text.is_empty() { text } else { r.text.as_str() };
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
        let mut per_glyph = None;
        if self.vert && r.orient != Orient::Side {
            // Upright CJK and tate-chu-yoko: the frame transform turned the whole line a quarter
            // turn, so each glyph (or a tate-chu-yoko group) turns back about the centre of its em box.
            let em = r.size;
            let group_cx = (r.x0 + r.x1) / 2.0;
            per_glyph = Some(
                r.glyphs
                    .iter()
                    .map(|g| {
                        let cx = if r.orient == Orient::Tcy { group_cx } else { g.x + 0.5 * em * r.hscale };
                        let c = kurbo::Vec2::new(cx, g.y - 0.38 * em);
                        line_xf * Affine::translate(c) * Affine::rotate(-std::f64::consts::FRAC_PI_2) * Affine::translate(-c) * Affine::translate((g.x, g.y))
                    })
                    .collect::<Vec<Affine>>(),
            );
            for g in &mut glyphs {
                g.1 = 0.0;
                g.2 = 0.0;
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
            per_glyph,
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
    let mut ctx = Ctx { doc, lay, opts, ops: Vec::new(), page: None, vert: false, depth: 0 };
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
    let mut ctx = Ctx { doc, lay, opts, ops: Vec::new(), page: None, vert: false, depth: 0 };
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
