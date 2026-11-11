use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "space", content = "v", rename_all = "lowercase")]
pub enum Color {
    Rgb([f64; 3]),
    Cmyk([f64; 4]),
    Lab([f64; 3]),
    Gray(f64),
    /// A spot or mixed ink swatch, kept as its ink so a separation can split it into plates. `sw` indexes the
    /// document's swatches (only valid for the document that produced it, so it is never stored); `rgb` is the
    /// display colour and `cmyk` the process equivalent when the ink has one.
    Ink { sw: u32, tint: f64, rgb: [f64; 3], cmyk: Option<[f64; 4]> },
}

/// One component of a mixed ink: a process ink (Cyan, Magenta, Yellow, Black) or a spot ink swatch, at a tint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MixPart {
    pub ink: String,
    pub tint: f64,
}

/// The plate index (0 to 3) of a process ink name, accepting the usual letters.
pub fn process_plate(name: &str) -> Option<usize> {
    match name.trim().to_ascii_lowercase().as_str() {
        "cyan" | "c" => Some(0),
        "magenta" | "m" => Some(1),
        "yellow" | "y" => Some(2),
        "black" | "k" => Some(3),
        _ => None,
    }
}

impl Color {
    pub fn black() -> Color {
        Color::Cmyk([0.0, 0.0, 0.0, 1.0])
    }

    pub fn white() -> Color {
        Color::Rgb([1.0, 1.0, 1.0])
    }

    pub fn from_hex(s: &str) -> Option<Color> {
        let h = s.trim().trim_start_matches('#');
        if !h.is_ascii() {
            return None;
        }
        let full: String = match h.len() {
            3 => h.chars().flat_map(|c| [c, c]).collect(),
            6 | 8 => h[..6].to_string(),
            _ => return None,
        };
        let v = u32::from_str_radix(&full, 16).ok()?;
        Some(Color::Rgb([((v >> 16) & 255) as f64 / 255.0, ((v >> 8) & 255) as f64 / 255.0, (v & 255) as f64 / 255.0]))
    }

    pub fn from_value(v: &Value) -> Option<Color> {
        match v {
            Value::String(s) => Color::from_hex(s),
            Value::Object(o) => {
                let arr = |k: &str| o.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_f64).collect::<Vec<_>>());
                let scale = |v: Vec<f64>, max: f64| v.into_iter().map(|x| if max > 1.0 && x > 1.0 { x / max } else { x }).collect::<Vec<_>>();
                if let Some(c) = arr("cmyk").filter(|a| a.len() == 4) {
                    let c = scale(c, 100.0);
                    return Some(Color::Cmyk([c[0], c[1], c[2], c[3]]));
                }
                if let Some(c) = arr("rgb").filter(|a| a.len() == 3) {
                    let c = scale(c, 255.0);
                    return Some(Color::Rgb([c[0], c[1], c[2]]));
                }
                if let Some(c) = arr("lab").filter(|a| a.len() == 3) {
                    return Some(Color::Lab([c[0], c[1], c[2]]));
                }
                if let Some(g) = o.get("gray").and_then(Value::as_f64) {
                    return Some(Color::Gray(if g > 1.0 { g / 100.0 } else { g }));
                }
                if let Some(h) = o.get("hex").and_then(Value::as_str) {
                    return Color::from_hex(h);
                }
                if let (Some(space), Some(vals)) = (o.get("space").and_then(Value::as_str), o.get("v")) {
                    return serde_json::from_value(json!({"space": space, "v": vals})).ok();
                }
                None
            }
            _ => None,
        }
    }

    pub fn to_rgb(self) -> [f64; 3] {
        match self {
            Color::Rgb(c) => c,
            Color::Gray(g) => [1.0 - g, 1.0 - g, 1.0 - g],
            Color::Cmyk([c, m, y, k]) => {
                let f = |x: f64| ((1.0 - x) * (1.0 - k)).clamp(0.0, 1.0);
                let mut rgb = [f(c), f(m), f(y)];
                if k > 0.9 && c + m + y < 0.01 {
                    let v = 0.137 + (1.0 - k) * 0.86;
                    rgb = [v, v, v];
                }
                rgb
            }
            Color::Lab([l, a, b]) => lab_to_srgb(l, a, b),
            Color::Ink { rgb, .. } => rgb,
        }
    }

    pub fn tinted(self, tint: f64) -> Color {
        let t = tint.clamp(0.0, 1.0);
        match self {
            Color::Ink { sw, tint: t0, rgb, cmyk } => Color::Ink { sw, tint: t0 * t, rgb: rgb.map(|v| 1.0 - (1.0 - v) * t), cmyk: cmyk.map(|c| c.map(|v| v * t)) },
            Color::Cmyk(c) => Color::Cmyk([c[0] * t, c[1] * t, c[2] * t, c[3] * t]),
            Color::Gray(g) => Color::Gray(g * t),
            other => {
                let [r, g, b] = other.to_rgb();
                Color::Rgb([1.0 - (1.0 - r) * t, 1.0 - (1.0 - g) * t, 1.0 - (1.0 - b) * t])
            }
        }
    }

    pub fn hex(self) -> String {
        let [r, g, b] = self.to_rgb();
        format!("#{:02x}{:02x}{:02x}", (r * 255.0).round() as u8, (g * 255.0).round() as u8, (b * 255.0).round() as u8)
    }

    pub fn rgba8(self, alpha: f64) -> [u8; 4] {
        let [r, g, b] = self.to_rgb();
        [(r * 255.0).round() as u8, (g * 255.0).round() as u8, (b * 255.0).round() as u8, (alpha.clamp(0.0, 1.0) * 255.0).round() as u8]
    }

    pub fn is_process_black(self) -> bool {
        matches!(self, Color::Cmyk([c, m, y, k]) if c == 0.0 && m == 0.0 && y == 0.0 && k >= 0.999)
    }
}

fn lab_to_srgb(l: f64, a: f64, b: f64) -> [f64; 3] {
    let fy = (l + 16.0) / 116.0;
    let fx = fy + a / 500.0;
    let fz = fy - b / 200.0;
    let f = |t: f64| if t.powi(3) > 0.008856 { t.powi(3) } else { (t - 16.0 / 116.0) / 7.787 };
    let (x, y, z) = (0.95047 * f(fx), f(fy), 1.08883 * f(fz));
    let lin = [3.2406 * x - 1.5372 * y - 0.4986 * z, -0.9689 * x + 1.8758 * y + 0.0415 * z, 0.0557 * x - 0.2040 * y + 1.0570 * z];
    lin.map(|c| {
        let c = c.clamp(0.0, 1.0);
        if c <= 0.0031308 { 12.92 * c } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stop {
    pub at: f64,
    pub color: Color,
    #[serde(default = "opaque")]
    pub opacity: f64,
}

fn opaque() -> f64 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Gradient {
    pub radial: bool,
    pub stops: Vec<Stop>,
    #[serde(default)]
    pub angle: f64,
}

impl Gradient {
    /// The colour and opacity at `t` (0..1), between the stops around it; the same space is kept when both
    /// stops are CMYK or both grey, otherwise the mix is in RGB.
    pub fn at(&self, t: f64) -> Option<(Color, f64)> {
        let mut stops: Vec<&Stop> = self.stops.iter().collect();
        stops.sort_by(|a, b| a.at.total_cmp(&b.at));
        let (first, last) = (*stops.first()?, *stops.last()?);
        if t <= first.at {
            return Some((first.color, first.opacity));
        }
        for w in stops.windows(2) {
            let (a, b) = (w[0], w[1]);
            if t <= b.at {
                let u = ((t - a.at) / (b.at - a.at).max(1e-9)).clamp(0.0, 1.0);
                let color = match (a.color, b.color) {
                    (Color::Cmyk(x), Color::Cmyk(y)) => Color::Cmyk([0, 1, 2, 3].map(|i| x[i] + (y[i] - x[i]) * u)),
                    (Color::Gray(x), Color::Gray(y)) => Color::Gray(x + (y - x) * u),
                    _ => {
                        let (x, y) = (a.color.to_rgb(), b.color.to_rgb());
                        Color::Rgb([0, 1, 2].map(|i| x[i] + (y[i] - x[i]) * u))
                    }
                };
                return Some((color, a.opacity + (b.opacity - a.opacity) * u));
            }
        }
        Some((last.color, last.opacity))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SwatchValue {
    Color { color: Color },
    Tint { base: String, tint: f64 },
    Gradient { gradient: Gradient },
    /// A mixed ink: process and spot inks at tints, printed together.
    Mixed { parts: Vec<MixPart> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Swatch {
    pub name: String,
    pub value: SwatchValue,
    #[serde(default)]
    pub spot: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default)]
    pub locked: bool,
}

impl Swatch {
    pub fn color(name: &str, color: Color, spot: bool) -> Swatch {
        Swatch { name: name.into(), value: SwatchValue::Color { color }, spot, group: None, locked: false }
    }

    pub fn defaults() -> Vec<Swatch> {
        let mut paper = Swatch::color("Paper", Color::white(), false);
        paper.locked = true;
        let mut black = Swatch::color("Black", Color::black(), false);
        black.locked = true;
        let mut reg = Swatch::color("Registration", Color::Cmyk([1.0, 1.0, 1.0, 1.0]), false);
        reg.locked = true;
        vec![
            paper,
            black,
            reg,
            Swatch::color("Cyan", Color::Cmyk([1.0, 0.0, 0.0, 0.0]), false),
            Swatch::color("Magenta", Color::Cmyk([0.0, 1.0, 0.0, 0.0]), false),
            Swatch::color("Yellow", Color::Cmyk([0.0, 0.0, 1.0, 0.0]), false),
            Swatch::color("Red", Color::Cmyk([0.0, 1.0, 1.0, 0.0]), false),
            Swatch::color("Green", Color::Cmyk([0.75, 0.0, 1.0, 0.0]), false),
            Swatch::color("Blue", Color::Cmyk([1.0, 0.9, 0.1, 0.0]), false),
        ]
    }
}

pub fn swatch_key(name: &str) -> String {
    let n = name.trim();
    let n = n.strip_prefix('[').and_then(|s| s.strip_suffix(']')).unwrap_or(n);
    n.to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Paint {
    None,
    Swatch { name: String, tint: f64 },
    Color { color: Color },
    Gradient { gradient: Gradient },
}

impl Paint {
    pub fn swatch(name: &str) -> Paint {
        Paint::Swatch { name: swatch_key(name), tint: 1.0 }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Paint::None)
    }

    pub fn from_value(v: &Value) -> Option<Paint> {
        match v {
            Value::Null => Some(Paint::None),
            Value::String(s) => {
                let k = swatch_key(s);
                if k.eq_ignore_ascii_case("none") || k.is_empty() {
                    Some(Paint::None)
                } else if k.starts_with('#') {
                    Color::from_hex(&k).map(|color| Paint::Color { color })
                } else {
                    Some(Paint::Swatch { name: k, tint: 1.0 })
                }
            }
            Value::Object(o) => {
                if let Some(name) = o.get("swatch").and_then(Value::as_str) {
                    let tint = o.get("tint").and_then(Value::as_f64).unwrap_or(1.0);
                    let k = swatch_key(name);
                    return Some(if k.eq_ignore_ascii_case("none") { Paint::None } else { Paint::Swatch { name: k, tint } });
                }
                if let Some(g) = o.get("gradient") {
                    return serde_json::from_value(g.clone()).ok().map(|gradient| Paint::Gradient { gradient });
                }
                if o.get("kind").is_some() {
                    return serde_json::from_value(v.clone()).ok();
                }
                Color::from_value(v).map(|color| Paint::Color { color })
            }
            _ => None,
        }
    }

    pub fn to_value(&self) -> Value {
        match self {
            Paint::None => json!("none"),
            Paint::Swatch { name, tint } if *tint >= 0.999 => json!(name),
            Paint::Swatch { name, tint } => json!({"swatch": name, "tint": tint}),
            Paint::Color { color } => json!(color.hex()),
            Paint::Gradient { gradient } => json!({"gradient": gradient}),
        }
    }
}
