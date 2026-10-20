use formdesign_model::doc::Doc;
use formdesign_model::geom::{Affine, Rect};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "index", rename_all = "camelCase")]
pub enum SpreadRef {
    Doc(usize),
    Parent(usize),
}

#[derive(Clone, Debug)]
pub struct Slot {
    pub spread: SpreadRef,
    pub bounds: Rect,
    pub xf: Affine,
    pub rotation: u32,
}

#[derive(Clone, Debug)]
pub struct Canvas {
    pub slots: Vec<Slot>,
    pub pasteboard: Rect,
}

pub const SPREAD_GAP: f64 = 48.0;

fn spread_size(doc: &Doc, r: SpreadRef) -> (f64, f64, usize) {
    match r {
        SpreadRef::Doc(i) => {
            let s = &doc.spreads[i];
            (s.width(), s.height(), s.pages.len())
        }
        SpreadRef::Parent(i) => {
            let p = &doc.parents[i];
            (p.pages.iter().map(|x| x.width).sum(), p.pages.iter().map(|x| x.height).fold(0.0, f64::max), p.pages.len())
        }
    }
}

pub fn canvas(doc: &Doc, parents: bool, rotations: &[(SpreadRef, u32)]) -> Canvas {
    let refs: Vec<SpreadRef> = if parents { (0..doc.parents.len()).map(SpreadRef::Parent).collect() } else { (0..doc.spreads.len()).map(SpreadRef::Doc).collect() };
    let facing = doc.settings.facing;
    let mut y = 0.0;
    let mut slots = Vec::new();
    let mut widest: f64 = doc.settings.page_width;
    for r in refs {
        let (w, h, n) = spread_size(doc, r);
        let first_page_w = match r {
            SpreadRef::Doc(i) => doc.spreads[i].pages.first().map(|p| p.width).unwrap_or(w),
            SpreadRef::Parent(i) => doc.parents[i].pages.first().map(|p| p.width).unwrap_or(w),
        };
        let starts_right = match r {
            SpreadRef::Doc(i) => facing && n == 1 && doc.is_right_page(doc.first_page_of_spread(i)),
            SpreadRef::Parent(_) => false,
        };
        let x = if !facing {
            -w / 2.0
        } else if starts_right {
            0.0
        } else {
            -first_page_w
        };
        let rot = rotations.iter().find(|(s, _)| *s == r).map(|(_, a)| *a).unwrap_or(0);
        let local = Rect::new(0.0, 0.0, w, h);
        let rot_xf = match rot {
            90 => Affine::translate((h, 0.0)) * Affine::rotate(std::f64::consts::FRAC_PI_2),
            180 => Affine::translate((w, h)) * Affine::rotate(std::f64::consts::PI),
            270 => Affine::translate((0.0, w)) * Affine::rotate(-std::f64::consts::FRAC_PI_2),
            _ => Affine::IDENTITY,
        };
        let shown = rot_xf.transform_rect_bbox(local);
        let xf = Affine::translate((x, y)) * rot_xf;
        slots.push(Slot { spread: r, bounds: Rect::new(x, y, x + shown.width(), y + shown.height()), xf, rotation: rot });
        widest = widest.max(shown.width());
        y += shown.height() + SPREAD_GAP;
    }
    let left = slots.iter().map(|s| s.bounds.x0).fold(0.0, f64::min);
    let right = slots.iter().map(|s| s.bounds.x1).fold(0.0, f64::max);
    let margin = widest.max(doc.settings.page_width);
    let pasteboard = Rect::new(left - margin, -SPREAD_GAP * 2.0, right + margin, (y - SPREAD_GAP).max(0.0) + SPREAD_GAP * 2.0);
    Canvas { slots, pasteboard }
}

impl Canvas {
    pub fn slot(&self, r: SpreadRef) -> Option<&Slot> {
        self.slots.iter().find(|s| s.spread == r)
    }

    pub fn xf(&self, r: SpreadRef) -> Affine {
        self.slot(r).map(|s| s.xf).unwrap_or(Affine::IDENTITY)
    }

    pub fn page_rect(&self, doc: &Doc, abs: usize) -> Option<Rect> {
        let (si, pi) = doc.page_slot(abs)?;
        let s = &doc.spreads[si];
        let x = s.page_x(pi);
        let p = &s.pages[pi];
        Some(self.xf(SpreadRef::Doc(si)).transform_rect_bbox(Rect::new(x, 0.0, x + p.width, p.height)))
    }

    pub fn hit_slot(&self, x: f64, y: f64) -> Option<&Slot> {
        self.slots
            .iter()
            .min_by(|a, b| dist(a.bounds, x, y).total_cmp(&dist(b.bounds, x, y)))
    }
}

fn dist(r: Rect, x: f64, y: f64) -> f64 {
    let dx = if x < r.x0 { r.x0 - x } else if x > r.x1 { x - r.x1 } else { 0.0 };
    let dy = if y < r.y0 { r.y0 - y } else if y > r.y1 { y - r.y1 } else { 0.0 };
    dx + dy
}
