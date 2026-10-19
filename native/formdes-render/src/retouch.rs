//! Retouching a placed raster picture with brush strokes: clone, heal and red-eye. Strokes are in shares of the picture, so the same
//! strokes give the same result on the full picture and on a downscaled preview. Like pixedit, the result is a new
//! encoded file; the original is never written.

use image::{DynamicImage, GenericImageView, RgbaImage, imageops::FilterType};

use crate::pixedit::{Edited, encode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    /// Copies the pixels at `offset` from each dab.
    Clone,
    /// Copies texture from `offset` but keeps the colour and light around the dab, so a blemish takes on its surroundings.
    Heal,
    /// Takes the red out of pupils inside each dab; needs no source.
    RedEye,
}

#[derive(Clone, Debug)]
pub struct Stroke {
    pub tool: Tool,
    /// The brush path as shares of the picture's width and height.
    pub points: Vec<(f64, f64)>,
    /// The brush radius as a share of the picture's longer side.
    pub radius: f64,
    /// Where the source sits relative to the brush, as shares of the width and height (clone and heal).
    pub offset: (f64, f64),
}

/// A round brush: full strength to `hard` of the radius, easing to nothing at the radius.
fn falloff(d: f64, r: f64, hard: f64) -> f32 {
    if d <= r * hard {
        1.0
    } else if d >= r {
        0.0
    } else {
        let t = 1.0 - (d - r * hard) / (r * (1.0 - hard));
        (t * t * (3.0 - 2.0 * t)) as f32
    }
}

/// Clone and red-eye fade over the outer half of the brush. Heal matches the light around the stroke, so its
/// edge needs no long fade, and a long fade left a ring of the blemish wherever the brush was only partly on.
fn hardness(tool: Tool) -> f64 {
    if tool == Tool::Heal { 0.8 } else { 0.5 }
}

/// The stroke's coverage over a box of the picture: dabs every quarter radius along the path, the strongest wins.
struct Mask {
    x0: i64,
    y0: i64,
    w: usize,
    h: usize,
    a: Vec<f32>,
}

impl Mask {
    fn at(&self, x: i64, y: i64) -> f32 {
        if x < self.x0 || y < self.y0 || x >= self.x0 + self.w as i64 || y >= self.y0 + self.h as i64 {
            return 0.0;
        }
        self.a[(y - self.y0) as usize * self.w + (x - self.x0) as usize]
    }
}

fn dabs(pts: &[(f64, f64)], r: f64) -> Vec<(f64, f64)> {
    let step = (r * 0.25).max(0.5);
    let mut out = vec![pts[0]];
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        let n = (len / step).ceil() as usize;
        for k in 1..=n {
            let t = k as f64 / n as f64;
            out.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
        }
    }
    out
}

fn mask(pts: &[(f64, f64)], r: f64, hard: f64, pad: f64, w: u32, h: u32) -> Option<Mask> {
    let (mut lx, mut ly, mut hx, mut hy) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in pts {
        lx = lx.min(p.0);
        ly = ly.min(p.1);
        hx = hx.max(p.0);
        hy = hy.max(p.1);
    }
    let x0 = ((lx - r - pad).floor() as i64).max(0);
    let y0 = ((ly - r - pad).floor() as i64).max(0);
    let x1 = ((hx + r + pad).ceil() as i64).min(w as i64);
    let y1 = ((hy + r + pad).ceil() as i64).min(h as i64);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let (mw, mh) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let mut a = vec![0f32; mw * mh];
    for d in dabs(pts, r) {
        let (bx0, by0) = (((d.0 - r).floor() as i64).max(x0), ((d.1 - r).floor() as i64).max(y0));
        let (bx1, by1) = (((d.0 + r).ceil() as i64).min(x1), ((d.1 + r).ceil() as i64).min(y1));
        for y in by0..by1 {
            for x in bx0..bx1 {
                let dist = ((x as f64 + 0.5 - d.0).powi(2) + (y as f64 + 0.5 - d.1).powi(2)).sqrt();
                let v = falloff(dist, r, hard);
                let i = (y - y0) as usize * mw + (x - x0) as usize;
                if v > a[i] {
                    a[i] = v;
                }
            }
        }
    }
    Some(Mask { x0, y0, w: mw, h: mh, a })
}

fn px(img: &RgbaImage, x: i64, y: i64) -> [f32; 4] {
    let (w, h) = img.dimensions();
    let p = img.get_pixel(x.clamp(0, w as i64 - 1) as u32, y.clamp(0, h as i64 - 1) as u32);
    [p[0] as f32, p[1] as f32, p[2] as f32, p[3] as f32]
}

/// Box sums over a w by h grid of `n` channels, read back as a mean over a square of radius `r` around a cell.
struct Sums {
    w: usize,
    n: usize,
    s: Vec<f64>,
}

impl Sums {
    fn new(w: usize, h: usize, n: usize, f: impl Fn(usize, usize, &mut [f64])) -> Sums {
        let mut s = vec![0f64; (w + 1) * (h + 1) * n];
        let mut v = vec![0f64; n];
        for y in 0..h {
            for x in 0..w {
                f(x, y, &mut v);
                for c in 0..n {
                    let i = ((y + 1) * (w + 1) + x + 1) * n + c;
                    s[i] = v[c] + s[i - n] + s[i - (w + 1) * n] - s[i - (w + 2) * n];
                }
            }
        }
        Sums { w, n, s }
    }

    fn sum(&self, x: usize, y: usize, r: usize, h: usize, c: usize) -> f64 {
        let (x0, y0) = (x.saturating_sub(r), y.saturating_sub(r));
        let (x1, y1) = ((x + r + 1).min(self.w), (y + r + 1).min(h));
        let at = |xx: usize, yy: usize| self.s[(yy * (self.w + 1) + xx) * self.n + c];
        at(x1, y1) - at(x0, y1) - at(x1, y0) + at(x0, y0)
    }
}

fn clone_or_heal(img: &mut RgbaImage, s: &Stroke, pts: &[(f64, f64)], r: f64, off: (f64, f64)) {
    let (w, h) = img.dimensions();
    let heal = s.tool == Tool::Heal;
    // Healing reads the light around the stroke from a ring twice the brush wide.
    let reach = if heal { (r * 1.5).ceil() } else { 0.0 };
    let Some(m) = mask(pts, r, hardness(s.tool), reach, w, h) else { return };
    let snap = img.clone();
    let (ox, oy) = (off.0.round() as i64, off.1.round() as i64);
    let src = |x: i64, y: i64| px(&snap, x + ox, y + oy);
    if !heal {
        for y in 0..m.h as i64 {
            for x in 0..m.w as i64 {
                let (gx, gy) = (m.x0 + x, m.y0 + y);
                let a = m.at(gx, gy);
                if a <= 0.0 {
                    continue;
                }
                let (d, c) = (px(&snap, gx, gy), src(gx, gy));
                let o: Vec<u8> = (0..4).map(|k| (d[k] + (c[k] - d[k]) * a).round().clamp(0.0, 255.0) as u8).collect();
                img.put_pixel(gx as u32, gy as u32, image::Rgba([o[0], o[1], o[2], o[3]]));
            }
        }
        return;
    }
    // Heal: the cloned texture plus the difference between the light around the stroke and the light around the
    // source, both read over the same ring (weight 1 - coverage), so the blemish stays out of the colour the patch
    // takes on and a gradient across the window cancels. An unweighted source mean reached the blemish through
    // the offset and lifted the patch into a light disc.
    let rr = reach as usize;
    let dst = Sums::new(m.w, m.h, 4, |x, y, v| {
        let (gx, gy) = (m.x0 + x as i64, m.y0 + y as i64);
        let wgt = 1.0 - m.at(gx, gy) as f64;
        let p = px(&snap, gx, gy);
        v[0] = p[0] as f64 * wgt;
        v[1] = p[1] as f64 * wgt;
        v[2] = p[2] as f64 * wgt;
        v[3] = wgt;
    });
    let srcs = Sums::new(m.w, m.h, 3, |x, y, v| {
        let (gx, gy) = (m.x0 + x as i64, m.y0 + y as i64);
        let wgt = 1.0 - m.at(gx, gy) as f64;
        let p = src(gx, gy);
        v[0] = p[0] as f64 * wgt;
        v[1] = p[1] as f64 * wgt;
        v[2] = p[2] as f64 * wgt;
    });
    for y in 0..m.h {
        for x in 0..m.w {
            let (gx, gy) = (m.x0 + x as i64, m.y0 + y as i64);
            let a = m.at(gx, gy);
            if a <= 0.0 {
                continue;
            }
            let (d, c) = (px(&snap, gx, gy), src(gx, gy));
            let wsum = dst.sum(x, y, rr, m.h, 3);
            let mut o = [0u8; 4];
            for k in 0..3 {
                // No ring in reach (the middle of a very wide stroke): no correction, the texture as cloned.
                let shift = if wsum > 1e-3 { (dst.sum(x, y, rr, m.h, k) - srcs.sum(x, y, rr, m.h, k)) / wsum } else { 0.0 };
                let healed = c[k] as f64 + shift;
                o[k] = (d[k] as f64 + (healed - d[k] as f64) * a as f64).round().clamp(0.0, 255.0) as u8;
            }
            o[3] = d[3] as u8;
            img.put_pixel(gx as u32, gy as u32, image::Rgba(o));
        }
    }
}

fn red_eye(img: &mut RgbaImage, pts: &[(f64, f64)], r: f64) {
    let (w, h) = img.dimensions();
    let Some(m) = mask(pts, r, hardness(Tool::RedEye), 0.0, w, h) else { return };
    for y in 0..m.h as i64 {
        for x in 0..m.w as i64 {
            let (gx, gy) = (m.x0 + x, m.y0 + y);
            let a = m.at(gx, gy);
            if a <= 0.0 {
                continue;
            }
            let p = img.get_pixel_mut(gx as u32, gy as u32);
            let (rd, gb) = (p[0] as f32, (p[1] as f32 + p[2] as f32) / 2.0);
            // Only clearly red pixels change, and the change eases in so the edge of the pupil stays soft.
            let red = ((rd / gb.max(1.0) - 1.5) / 0.5).clamp(0.0, 1.0) * if rd > 60.0 { 1.0 } else { 0.0 };
            let k = a * red;
            p[0] = (rd + (gb - rd) * k).round() as u8;
        }
    }
}

/// Applies the strokes in order to `img`.
pub fn apply(img: &mut RgbaImage, strokes: &[Stroke]) {
    let (w, h) = img.dimensions();
    let long = w.max(h) as f64;
    for s in strokes {
        if s.points.is_empty() {
            continue;
        }
        let pts: Vec<(f64, f64)> = s.points.iter().map(|p| (p.0 * w as f64, p.1 * h as f64)).collect();
        let r = (s.radius * long).max(0.75);
        match s.tool {
            Tool::RedEye => red_eye(img, &pts, r),
            _ => clone_or_heal(img, s, &pts, r, (s.offset.0 * w as f64, s.offset.1 * h as f64)),
        }
    }
}

fn decode(src: &[u8]) -> Result<DynamicImage, String> {
    image::load_from_memory(src).map_err(|err| format!("cannot read the picture for retouching: {err}"))
}

/// The retouched picture as a new file: JPEG sources without transparency stay JPEG (quality 92), others PNG.
pub fn retouch(src: &[u8], strokes: &[Stroke], ppi: Option<f64>) -> Result<Edited, String> {
    let img = decode(src)?;
    let (sw, sh) = img.dimensions();
    let alpha = img.color().has_alpha();
    let mut rgba = img.to_rgba8();
    apply(&mut rgba, strokes);
    let jpeg = src.starts_with(&[0xFF, 0xD8]) && !alpha;
    let bytes = encode(&DynamicImage::ImageRgba8(rgba), jpeg, ppi, 92)?;
    Ok(Edited { bytes, ext: if jpeg { "jpg" } else { "png" }, pixels: (sw, sh), crop_px: [0, 0, sw, sh], source_px: (sw, sh) })
}

/// The strokes on a copy no larger than `max_side` on its longer side, as a JPEG (PNG when the picture has
/// transparency), for a dialog's preview. Returns the bytes, their MIME type and the pixel size.
pub fn preview(src: &[u8], strokes: &[Stroke], max_side: u32) -> Result<(Vec<u8>, &'static str, (u32, u32)), String> {
    let img = decode(src)?;
    let (w, h) = img.dimensions();
    let alpha = img.color().has_alpha();
    let k = (max_side.max(16) as f64 / w.max(h) as f64).min(1.0);
    let small = if k < 1.0 { img.resize((w as f64 * k).round().max(1.0) as u32, (h as f64 * k).round().max(1.0) as u32, FilterType::Triangle) } else { img };
    let mut rgba = small.to_rgba8();
    apply(&mut rgba, strokes);
    let size = rgba.dimensions();
    let bytes = encode(&DynamicImage::ImageRgba8(rgba), !alpha, None, 85)?;
    Ok((bytes, if alpha { "image/png" } else { "image/jpeg" }, size))
}
