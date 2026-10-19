//! Pixel edits of a placed raster image (crop, quarter turns, flips and tone adjustments), returning a new encoded
//! file so the caller can write it beside the original and relink: the original stays untouched.

use image::{DynamicImage, GenericImageView, RgbaImage};

/// What to do to the picture, in order: crop, turn, flip, then tones.
#[derive(Clone, Debug, Default)]
pub struct Edit {
    /// The part to keep, as shares of the width and height: [x0, y0, x1, y1].
    pub crop: Option<[f64; 4]>,
    /// Clockwise quarter turns: 0, 90, 180 or 270.
    pub rotate: u32,
    pub flip_h: bool,
    pub flip_v: bool,
    /// Stops, -3 to 3.
    pub exposure: f64,
    /// -100 to 100.
    pub contrast: f64,
    /// Black point and white point (0 to 255) and gamma (0.1 to 10).
    pub levels: Option<(f64, f64, f64)>,
    /// -100 (grey) to 100.
    pub saturation: f64,
    /// -100 (cooler) to 100 (warmer).
    pub temperature: f64,
}

impl Edit {
    pub fn is_tonal(&self) -> bool {
        self.exposure != 0.0 || self.contrast != 0.0 || self.levels.is_some() || self.saturation != 0.0 || self.temperature != 0.0
    }
}

/// The edited picture: encoded bytes, the file extension they need, the pixel size, and the crop actually applied
/// in pixels of the original ([x, y, w, h]) so the caller can keep the kept pixels where they were on the page.
pub struct Edited {
    pub bytes: Vec<u8>,
    pub ext: &'static str,
    pub pixels: (u32, u32),
    pub crop_px: [u32; 4],
    pub source_px: (u32, u32),
}

fn tone(v: f64, e: &Edit) -> f64 {
    let mut v = v;
    if let Some((black, white, gamma)) = e.levels {
        let (b, w) = (black / 255.0, (white / 255.0).max(black / 255.0 + 1e-3));
        v = ((v - b) / (w - b)).clamp(0.0, 1.0).powf(1.0 / gamma.clamp(0.1, 10.0));
    }
    if e.exposure != 0.0 {
        v *= 2f64.powf(e.exposure.clamp(-3.0, 3.0));
    }
    if e.contrast != 0.0 {
        let c = (e.contrast / 100.0).clamp(-1.0, 1.0);
        let k = if c >= 0.0 { 1.0 / (1.0 - c * 0.95) } else { 1.0 + c };
        v = (v - 0.5) * k + 0.5;
    }
    v
}

fn adjust(img: &mut RgbaImage, e: &Edit) {
    let sat = 1.0 + (e.saturation / 100.0).clamp(-1.0, 1.0);
    let warm = (e.temperature / 100.0).clamp(-1.0, 1.0) * 0.12;
    for p in img.pixels_mut() {
        let mut c = [p[0] as f64 / 255.0, p[1] as f64 / 255.0, p[2] as f64 / 255.0];
        for v in c.iter_mut() {
            *v = tone(*v, e);
        }
        if sat != 1.0 {
            let l = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
            for v in c.iter_mut() {
                *v = l + (*v - l) * sat;
            }
        }
        if warm != 0.0 {
            c[0] += warm;
            c[2] -= warm;
        }
        for (i, v) in c.iter().enumerate() {
            p[i] = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
}

/// Adjustments kept live on a placed picture: the frame stores these and a view
/// picture made from them, and the original stays as placed. Applied in order: the tones of `Edit` (levels,
/// exposure, contrast, saturation, temperature), then the curve, hue, lightness and black and white.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Adjust {
    pub exposure: f64,
    pub contrast: f64,
    pub levels: Option<(f64, f64, f64)>,
    pub saturation: f64,
    pub temperature: f64,
    /// Degrees, -180 to 180.
    pub hue: f64,
    /// -100 (black) to 100 (white).
    pub lightness: f64,
    pub black_white: bool,
    /// Curve points in 0..255 input and output, applied to each channel; fewer than two points is no curve.
    pub curve: Vec<(f64, f64)>,
}

impl Adjust {
    pub fn is_empty(&self) -> bool {
        !self.tones().is_tonal() && self.hue == 0.0 && self.lightness == 0.0 && !self.black_white && self.curve.len() < 2
    }

    fn tones(&self) -> Edit {
        Edit { exposure: self.exposure, contrast: self.contrast, levels: self.levels, saturation: self.saturation, temperature: self.temperature, ..Edit::default() }
    }
}

/// A 256-entry table through the curve's points, monotone between them (Fritsch-Carlson), so a curve that only
/// rises never folds back on itself.
fn curve_lut(points: &[(f64, f64)]) -> [u8; 256] {
    let mut p: Vec<(f64, f64)> = points.iter().map(|(x, y)| (x.clamp(0.0, 255.0), y.clamp(0.0, 255.0))).collect();
    p.sort_by(|a, b| a.0.total_cmp(&b.0));
    p.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-6);
    let mut lut = [0u8; 256];
    if p.len() < 2 {
        for (i, v) in lut.iter_mut().enumerate() {
            *v = i as u8;
        }
        return lut;
    }
    let n = p.len();
    let d: Vec<f64> = (0..n - 1).map(|i| (p[i + 1].1 - p[i].1) / (p[i + 1].0 - p[i].0)).collect();
    let mut m = vec![0.0; n];
    m[0] = d[0];
    m[n - 1] = d[n - 2];
    for i in 1..n - 1 {
        m[i] = if d[i - 1] * d[i] <= 0.0 { 0.0 } else { (d[i - 1] + d[i]) / 2.0 };
    }
    for i in 0..n - 1 {
        if d[i] == 0.0 {
            m[i] = 0.0;
            m[i + 1] = 0.0;
            continue;
        }
        let (a, b) = (m[i] / d[i], m[i + 1] / d[i]);
        let s = a * a + b * b;
        if s > 9.0 {
            let t = 3.0 / s.sqrt();
            m[i] = t * a * d[i];
            m[i + 1] = t * b * d[i];
        }
    }
    for (x, out) in lut.iter_mut().enumerate() {
        let xf = x as f64;
        let y = if xf <= p[0].0 {
            p[0].1
        } else if xf >= p[n - 1].0 {
            p[n - 1].1
        } else {
            let i = (0..n - 1).find(|&i| xf <= p[i + 1].0).unwrap_or(n - 2);
            let h = p[i + 1].0 - p[i].0;
            let t = (xf - p[i].0) / h;
            let (t2, t3) = (t * t, t * t * t);
            (2.0 * t3 - 3.0 * t2 + 1.0) * p[i].1 + (t3 - 2.0 * t2 + t) * h * m[i] + (-2.0 * t3 + 3.0 * t2) * p[i + 1].1 + (t3 - t2) * h * m[i + 1]
        };
        *out = y.round().clamp(0.0, 255.0) as u8;
    }
    lut
}

fn rgb_to_hsl(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
    let l = (mx + mn) / 2.0;
    if mx == mn {
        return (0.0, 0.0, l);
    }
    let d = mx - mn;
    let s = if l > 0.5 { d / (2.0 - mx - mn) } else { d / (mx + mn) };
    let h = if mx == r { (g - b) / d + if g < b { 6.0 } else { 0.0 } } else if mx == g { (b - r) / d + 2.0 } else { (r - g) / d + 4.0 };
    (h * 60.0, s, l)
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
    if s == 0.0 {
        return (l, l, l);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let f = |t: f64| {
        let t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    let h = h / 360.0;
    (f(h + 1.0 / 3.0), f(h), f(h - 1.0 / 3.0))
}

/// Applies `a` to an RGBA picture in place; alpha is kept.
pub fn apply_adjust(img: &mut RgbaImage, a: &Adjust) {
    let e = a.tones();
    if e.is_tonal() {
        adjust(img, &e);
    }
    let lut = (a.curve.len() >= 2).then(|| curve_lut(&a.curve));
    let hue = a.hue.clamp(-180.0, 180.0);
    let light = (a.lightness / 100.0).clamp(-1.0, 1.0);
    if lut.is_none() && hue == 0.0 && light == 0.0 && !a.black_white {
        return;
    }
    for p in img.pixels_mut() {
        if let Some(l) = &lut {
            for k in 0..3 {
                p[k] = l[p[k] as usize];
            }
        }
        let mut c = [p[0] as f64 / 255.0, p[1] as f64 / 255.0, p[2] as f64 / 255.0];
        if hue != 0.0 {
            let (h, s, l) = rgb_to_hsl(c[0], c[1], c[2]);
            let (r, g, b) = hsl_to_rgb(h + hue, s, l);
            c = [r, g, b];
        }
        if light != 0.0 {
            for v in c.iter_mut() {
                *v = if light > 0.0 { *v + (1.0 - *v) * light } else { *v * (1.0 + light) };
            }
        }
        if a.black_white {
            let y = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
            c = [y, y, y];
        }
        for (k, v) in c.iter().enumerate() {
            p[k] = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
}

/// The adjusted picture as a file for a view asset: JPEG (quality 92) when the source is a JPEG without
/// transparency, PNG otherwise, with `ppi` kept. Also returns the extension and the pixel size.
pub fn adjusted(src: &[u8], a: &Adjust, ppi: Option<f64>) -> Result<(Vec<u8>, &'static str, (u32, u32)), String> {
    let img = image::load_from_memory(src).map_err(|err| format!("cannot read the picture to adjust: {err}"))?;
    let alpha = img.color().has_alpha();
    let mut rgba = img.to_rgba8();
    apply_adjust(&mut rgba, a);
    let size = rgba.dimensions();
    let jpeg = src.starts_with(&[0xFF, 0xD8]) && !alpha;
    let bytes = encode(&DynamicImage::ImageRgba8(rgba), jpeg, ppi, 92)?;
    Ok((bytes, if jpeg { "jpg" } else { "png" }, size))
}

/// The adjusted picture no larger than `max_side`, as a JPEG (PNG with transparency), for a dialog's preview.
pub fn adjusted_preview(src: &[u8], a: &Adjust, max_side: u32) -> Result<(Vec<u8>, &'static str, (u32, u32)), String> {
    let img = image::load_from_memory(src).map_err(|err| format!("cannot read the picture to adjust: {err}"))?;
    let (w, h) = img.dimensions();
    let alpha = img.color().has_alpha();
    let k = (max_side.max(16) as f64 / w.max(h) as f64).min(1.0);
    let small = if k < 1.0 { img.resize((w as f64 * k).round().max(1.0) as u32, (h as f64 * k).round().max(1.0) as u32, image::imageops::FilterType::Triangle) } else { img };
    let mut rgba = small.to_rgba8();
    apply_adjust(&mut rgba, a);
    let size = rgba.dimensions();
    let bytes = encode(&DynamicImage::ImageRgba8(rgba), !alpha, None, 85)?;
    Ok((bytes, if alpha { "image/png" } else { "image/jpeg" }, size))
}

/// A PNG with a pHYs chunk after IHDR carrying `ppi`, so the edited copy places at the original's size.
fn png_with_density(png: Vec<u8>, ppi: f64) -> Vec<u8> {
    if png.len() < 33 || &png[12..16] != b"IHDR" {
        return png;
    }
    let ppm = (ppi / 0.0254).round() as u32;
    let mut chunk = Vec::with_capacity(21);
    chunk.extend_from_slice(&9u32.to_be_bytes());
    let mut body = b"pHYs".to_vec();
    body.extend_from_slice(&ppm.to_be_bytes());
    body.extend_from_slice(&ppm.to_be_bytes());
    body.push(1);
    let crc = crc32(&body);
    chunk.extend_from_slice(&body);
    chunk.extend_from_slice(&crc.to_be_bytes());
    let mut out = png[..33].to_vec();
    out.extend_from_slice(&chunk);
    out.extend_from_slice(&png[33..]);
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for b in data {
        crc ^= *b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

/// Applies `e` to the encoded image `src`. JPEG sources without transparency stay JPEG; everything else becomes PNG.
/// `ppi` is written into the new file when known.
pub fn edit(src: &[u8], e: &Edit, ppi: Option<f64>) -> Result<Edited, String> {
    let img = image::load_from_memory(src).map_err(|err| format!("cannot read the picture for editing: {err}"))?;
    let (sw, sh) = img.dimensions();
    let mut crop_px = [0, 0, sw, sh];
    let mut img: DynamicImage = match e.crop {
        Some([x0, y0, x1, y1]) => {
            let (x0, x1) = (x0.clamp(0.0, 1.0).min(x1.clamp(0.0, 1.0)), x1.clamp(0.0, 1.0).max(x0.clamp(0.0, 1.0)));
            let (y0, y1) = (y0.clamp(0.0, 1.0).min(y1.clamp(0.0, 1.0)), y1.clamp(0.0, 1.0).max(y0.clamp(0.0, 1.0)));
            let (px, py) = ((x0 * sw as f64).round() as u32, (y0 * sh as f64).round() as u32);
            let (pw, ph) = (((x1 - x0) * sw as f64).round().max(1.0) as u32, ((y1 - y0) * sh as f64).round().max(1.0) as u32);
            let (pw, ph) = (pw.min(sw - px.min(sw - 1)), ph.min(sh - py.min(sh - 1)));
            crop_px = [px.min(sw - 1), py.min(sh - 1), pw, ph];
            img.crop_imm(crop_px[0], crop_px[1], pw, ph)
        }
        None => img,
    };
    img = match e.rotate % 360 {
        90 => img.rotate90(),
        180 => img.rotate180(),
        270 => img.rotate270(),
        _ => img,
    };
    if e.flip_h {
        img = img.fliph();
    }
    if e.flip_v {
        img = img.flipv();
    }
    let alpha = img.color().has_alpha();
    if e.is_tonal() {
        let mut rgba = img.to_rgba8();
        adjust(&mut rgba, e);
        img = DynamicImage::ImageRgba8(rgba);
    }
    let pixels = img.dimensions();
    let jpeg = src.starts_with(&[0xFF, 0xD8]) && !alpha;
    let bytes = encode(&img, jpeg, ppi, 92)?;
    Ok(Edited { bytes, ext: if jpeg { "jpg" } else { "png" }, pixels, crop_px, source_px: (sw, sh) })
}

/// JPEG at `quality` (no transparency) or PNG, with `ppi` written into the file when known.
pub(crate) fn encode(img: &DynamicImage, jpeg: bool, ppi: Option<f64>, quality: u8) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    if jpeg {
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, quality);
        if let Some(p) = ppi {
            enc.set_pixel_density(image::codecs::jpeg::PixelDensity::dpi(p.round().clamp(1.0, 65535.0) as u16));
        }
        enc.encode_image(&DynamicImage::ImageRgb8(img.to_rgb8())).map_err(|err| format!("cannot write the edited picture: {err}"))?;
    } else {
        img.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).map_err(|err| format!("cannot write the edited picture: {err}"))?;
        if let Some(p) = ppi {
            bytes = png_with_density(bytes, p);
        }
    }
    Ok(bytes)
}
