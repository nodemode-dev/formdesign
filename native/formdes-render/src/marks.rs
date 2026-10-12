//! Printer's marks drawn in the slug area around a page: crop marks, bleed marks, registration targets, colour bars
//! and page information. Everything is plain vector ops in page coordinates (the trim box's top left is 0,0), so the
//! PDF writer and the rasteriser draw them the same way and a separation turns them into each plate's density.

use std::sync::Arc;

use formdes_model::color::Color;
use formdes_model::doc::Doc;
use formdes_model::geom::Edges;
use formdes_text::fonts::Face;
use kurbo::{Affine, BezPath, Circle, Rect, Shape};

use crate::scene::{Brush, GlyphOp, Op, StrokeStyle};

/// How far a crop or bleed mark reaches away from the page.
pub const LEN: f64 = 18.0;

#[derive(Clone, Debug)]
pub struct Marks {
    pub crop: bool,
    pub bleed: bool,
    pub registration: bool,
    pub color_bars: bool,
    pub page_info: bool,
    /// Japanese style crop marks: a second line at the bleed edge beside each trim line.
    pub japanese: bool,
    pub weight: f64,
    pub offset: f64,
}

impl Default for Marks {
    fn default() -> Self {
        Marks { crop: false, bleed: false, registration: false, color_bars: false, page_info: false, japanese: false, weight: 0.25, offset: 9.0 }
    }
}

fn reg() -> Color {
    Color::Cmyk([1.0, 1.0, 1.0, 1.0])
}

fn line(a: (f64, f64), b: (f64, f64)) -> BezPath {
    let mut p = BezPath::new();
    p.move_to(a);
    p.line_to(b);
    p
}

/// The bleed of one page as the scene computes it (inside is the left side), whether or not the export draws it.
pub fn page_bleed(doc: &Doc, abs: usize) -> Edges {
    let b = &doc.settings.bleed;
    let (l, r) = b.left_right(!doc.is_right_page(abs));
    Edges { top: b.top, bottom: b.bottom, inside: l, outside: r }
}

/// `YYYY-MM-DD HH:MM` in UTC.
pub fn timestamp() -> String {
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
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", rem / 3600, (rem % 3600) / 60)
}

impl Marks {
    pub fn any(&self) -> bool {
        self.crop || self.bleed || self.registration || self.color_bars || self.page_info
    }

    /// The width of the slug on every side: the offset (raised to the bleed, so a mark never enters it) plus the
    /// reach of a mark.
    pub fn slug(&self, bleed: &Edges) -> f64 {
        self.offset.max(bleed.max()).max(0.0) + LEN + 2.0
    }

    /// A copy of the page scene with the marks added and the bleed grown by the slug, so a raster of it shows the
    /// marks around the page the way the PDF does. The marks are in registration colour; a plate render separates them.
    pub fn framed(&self, doc: &Doc, scene: &crate::scene::PageScene, title: &str, plate_name: Option<&str>, face: Option<&Arc<Face>>) -> crate::scene::PageScene {
        let doc_bleed = page_bleed(doc, scene.page);
        let slug = self.slug(&doc_bleed);
        let name = doc.page_name(scene.page);
        let left = format!("{}  {}", if title.is_empty() { "Untitled" } else { title }, timestamp());
        let right = match plate_name {
            Some(pn) => format!("{pn}  Page {name}"),
            None => format!("Page {name}"),
        };
        let mut ops = scene.ops.clone();
        ops.extend(self.ops(scene.width, scene.height, &doc_bleed, &left, &right, face));
        let b = &scene.bleed;
        crate::scene::PageScene { page: scene.page, width: scene.width, height: scene.height, bleed: Edges { top: b.top + slug, bottom: b.bottom + slug, inside: b.inside + slug, outside: b.outside + slug }, ops }
    }

    fn text(&self, face: &Arc<Face>, s: &str, x: f64, baseline: f64, size: f64, max_w: f64) -> Option<Op> {
        let k = size / face.metrics.upem.max(1.0);
        let mut glyphs = Vec::new();
        let mut advances = Vec::new();
        let mut ranges = Vec::new();
        let mut text = String::new();
        let mut pen = 0.0;
        for c in s.chars() {
            let Some(g) = face.glyph(c) else { continue };
            let adv = face.advance(g) * k;
            if pen + adv > max_w {
                break;
            }
            ranges.push(text.len()..text.len() + c.len_utf8());
            text.push(c);
            glyphs.push((g, pen, 0.0));
            advances.push(adv);
            pen += adv;
        }
        if glyphs.is_empty() {
            return None;
        }
        Some(Op::Glyphs(GlyphOp {
            face: face.clone(),
            size,
            hscale: 1.0,
            vscale: 1.0,
            skew: 0.0,
            xf: Affine::translate((x, baseline)),
            glyphs,
            advances,
            per_glyph: None,
            text,
            ranges,
            fill: Some(Brush::Solid(reg(), 1.0)),
            stroke: None,
        }))
    }

    /// The marks for one page of `w` by `h` points whose bleed is `bleed` (inside is the left). `left` and `right`
    /// are the page information lines; `face` is the font they are set in.
    pub fn ops(&self, w: f64, h: f64, bleed: &Edges, left: &str, right: &str, face: Option<&Arc<Face>>) -> Vec<Op> {
        let mut out = Vec::new();
        if !self.any() {
            return out;
        }
        let ink = Brush::Solid(reg(), 1.0);
        let style = StrokeStyle { width: self.weight.max(0.01), cap: "butt".into(), join: "miter".into(), dash: Vec::new() };
        let stroke = |p: BezPath, out: &mut Vec<Op>| out.push(Op::Stroke { path: p, brush: ink.clone(), style: style.clone() });
        let (ot, ob, ol, or) = (self.offset.max(bleed.top), self.offset.max(bleed.bottom), self.offset.max(bleed.inside), self.offset.max(bleed.outside));
        // Each corner: a vertical and a horizontal line reaching away from the page, at the trim (crop) and at the
        // bleed edge (bleed marks, or the second line of a Japanese mark).
        let corner = |out: &mut Vec<Op>, stroke: &dyn Fn(BezPath, &mut Vec<Op>), x: f64, y: f64, sx: f64, sy: f64, dx: f64, dy: f64, ox: f64, oy: f64| {
            // sx, sy are -1 or 1: the direction away from the page along x and y.
            stroke(line((x + sx * dx, y + sy * oy), (x + sx * dx, y + sy * (oy + LEN))), out);
            stroke(line((x + sx * ox, y + sy * dy), (x + sx * (ox + LEN), y + sy * dy)), out);
        };
        let (jt, jb, jl, jr) = if self.japanese { (bleed.top.max(3.0), bleed.bottom.max(3.0), bleed.inside.max(3.0), bleed.outside.max(3.0)) } else { (0.0, 0.0, 0.0, 0.0) };
        let corners = [(0.0, 0.0, -1.0, -1.0, bleed.inside, bleed.top, ol, ot, jl, jt), (w, 0.0, 1.0, -1.0, bleed.outside, bleed.top, or, ot, jr, jt), (0.0, h, -1.0, 1.0, bleed.inside, bleed.bottom, ol, ob, jl, jb), (w, h, 1.0, 1.0, bleed.outside, bleed.bottom, or, ob, jr, jb)];
        for (x, y, sx, sy, bx, by, ox, oy, jx, jy) in corners {
            if self.crop {
                corner(&mut out, &stroke, x, y, sx, sy, 0.0, 0.0, ox, oy);
                if self.japanese {
                    corner(&mut out, &stroke, x, y, sx, sy, jx, jy, ox, oy);
                }
            }
            if self.bleed && (bx > 0.0 || by > 0.0) {
                corner(&mut out, &stroke, x, y, sx, sy, bx, by, ox, oy);
            }
        }
        if self.registration {
            let target = |cx: f64, cy: f64, out: &mut Vec<Op>| {
                stroke(Circle::new((cx, cy), 4.0).to_path(0.05), out);
                stroke(line((cx - 7.0, cy), (cx + 7.0, cy)), out);
                stroke(line((cx, cy - 7.0), (cx, cy + 7.0)), out);
                out.push(Op::Fill { path: Circle::new((cx, cy), 1.4).to_path(0.05), brush: Brush::Solid(reg(), 1.0), even_odd: false });
            };
            target(w / 2.0, -(ot + 9.0), &mut out);
            target(w / 2.0, h + ob + 9.0, &mut out);
            target(-(ol + 9.0), h / 2.0, &mut out);
            target(w + or + 9.0, h / 2.0, &mut out);
        }
        if self.color_bars {
            let sq = 8.0;
            let c = |v: [f64; 4]| Color::Cmyk(v);
            let solids = [
                c([1.0, 0.0, 0.0, 0.0]),
                c([0.0, 1.0, 0.0, 0.0]),
                c([0.0, 0.0, 1.0, 0.0]),
                c([0.0, 0.0, 0.0, 1.0]),
                c([0.0, 1.0, 1.0, 0.0]),
                c([1.0, 0.0, 1.0, 0.0]),
                c([1.0, 1.0, 0.0, 0.0]),
                c([1.0, 1.0, 1.0, 0.0]),
                c([0.0, 0.0, 0.0, 0.8]),
                c([0.0, 0.0, 0.0, 0.6]),
                c([0.0, 0.0, 0.0, 0.4]),
                c([0.0, 0.0, 0.0, 0.2]),
            ];
            let room = ((w / 2.0 - 28.0) / sq).floor().max(0.0) as usize;
            for (i, col) in solids.iter().take(room).enumerate() {
                let x = 14.0 + i as f64 * sq;
                out.push(Op::Fill { path: Rect::new(x, h + ob, x + sq, h + ob + sq).to_path(0.1), brush: Brush::Solid(*col, 1.0), even_odd: false });
            }
            let mut ramp = Vec::new();
            for ink in 0..4 {
                for t in [1.0, 0.75, 0.5, 0.25] {
                    let mut v = [0.0; 4];
                    v[ink] = t;
                    ramp.push(c(v));
                }
            }
            for (i, col) in ramp.iter().take(room).enumerate() {
                let x = w / 2.0 + 14.0 + i as f64 * sq;
                out.push(Op::Fill { path: Rect::new(x, -(ot + sq), x + sq, -ot).to_path(0.1), brush: Brush::Solid(*col, 1.0), even_odd: false });
            }
        }
        if self.page_info
            && let Some(face) = face
        {
            let room = (w / 2.0 - 24.0).max(0.0);
            if let Some(t) = self.text(face, left, 10.0, -(ot + 4.0), 6.0, room) {
                out.push(t);
            }
            if let Some(t) = self.text(face, right, w / 2.0 + 14.0, h + ob + 7.0, 6.0, room) {
                out.push(t);
            }
        }
        out
    }
}
