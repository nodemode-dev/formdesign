pub use kurbo::{Affine, BezPath, Point, Rect, Shape as KShape, Size, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Edges {
    pub top: f64,
    pub bottom: f64,
    pub inside: f64,
    pub outside: f64,
}

impl Edges {
    pub fn all(v: f64) -> Self {
        Edges { top: v, bottom: v, inside: v, outside: v }
    }

    pub fn left_right(&self, left_page: bool) -> (f64, f64) {
        if left_page { (self.outside, self.inside) } else { (self.inside, self.outside) }
    }

    pub fn max(&self) -> f64 {
        self.top.max(self.bottom).max(self.inside).max(self.outside)
    }
}

pub fn rect_xywh(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect::new(x, y, x + w, y + h)
}

pub fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

pub fn rect_json(r: Rect) -> [f64; 4] {
    [round2(r.x0), round2(r.y0), round2(r.x1), round2(r.y1)]
}

pub mod units {
    pub const PT: f64 = 1.0;
    pub const INCH: f64 = 72.0;
    pub const MM: f64 = 72.0 / 25.4;
    pub const CM: f64 = 72.0 / 2.54;
    pub const PICA: f64 = 12.0;
    pub const PX: f64 = 1.0;

    pub fn per_unit(name: &str) -> Option<f64> {
        Some(match name.to_ascii_lowercase().as_str() {
            "pt" | "points" => PT,
            "in" | "inch" | "inches" => INCH,
            "mm" | "millimeters" | "millimetres" => MM,
            "cm" | "centimeters" | "centimetres" => CM,
            "p" | "pc" | "pica" | "picas" => PICA,
            "px" | "pixels" => PX,
            _ => return None,
        })
    }

    pub fn parse_length(s: &str, default_unit: &str) -> Option<f64> {
        let t = s.trim();
        if let Some((p, rest)) = t.split_once('p')
            && let (Ok(picas), Ok(points)) = (p.trim().parse::<f64>(), rest.trim().parse::<f64>().or_else(|_| if rest.trim().is_empty() { Ok(0.0) } else { Err(()) }))
        {
            return Some(picas * PICA + points);
        }
        let split = t.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(t.len());
        let (num, unit) = t.split_at(split);
        let v: f64 = num.trim().parse().ok()?;
        let u = if unit.trim().is_empty() { default_unit } else { unit.trim() };
        Some(v * per_unit(u)?)
    }

    pub fn page_size(name: &str) -> Option<(f64, f64)> {
        Some(match name.to_ascii_lowercase().replace([' ', '-', '_'], "").as_str() {
            "letter" | "usletter" => (612.0, 792.0),
            "legal" | "uslegal" => (612.0, 1008.0),
            "tabloid" => (792.0, 1224.0),
            "a3" => (841.89, 1190.55),
            "a4" => (595.28, 841.89),
            "a5" => (419.53, 595.28),
            "a6" => (297.64, 419.53),
            "b5" => (498.9, 708.66),
            "halfletter" => (396.0, 612.0),
            "businesscard" => (252.0, 144.0),
            "postcard" => (432.0, 288.0),
            "instagram" | "square1080" => (1080.0, 1080.0),
            "story" | "1080x1920" => (1080.0, 1920.0),
            "web" | "desktop" | "1440" => (1440.0, 900.0),
            "slide" | "16:9" | "1920x1080" => (1920.0, 1080.0),
            _ => return None,
        })
    }
}
