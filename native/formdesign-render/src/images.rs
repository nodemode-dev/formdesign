use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use formdesign_model::ids::AssetId;
use formdesign_model::item::Asset;

pub struct Decoded {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Decoded {
    pub fn premultiplied(&self) -> Vec<u8> {
        let mut out = self.rgba.clone();
        for px in out.chunks_exact_mut(4) {
            let a = px[3] as u32;
            if a < 255 {
                for c in &mut px[..3] {
                    *c = ((*c as u32 * a + 127) / 255) as u8;
                }
            }
        }
        out
    }
}

pub fn is_pdf(a: &Asset) -> bool {
    a.mime == "application/pdf" || a.data.starts_with(b"%PDF")
}

type Cache = HashMap<(AssetId, u32), Option<Arc<Decoded>>>;
type PixCache = HashMap<(AssetId, u32), Option<Arc<vello_cpu::Pixmap>>>;

#[derive(Default)]
pub struct Images {
    cache: RwLock<Cache>,
    pix: RwLock<PixCache>,
}

impl Images {
    pub fn pixmap(&self, asset: &Asset, scale: f64) -> Option<Arc<vello_cpu::Pixmap>> {
        let key = (asset.id, if is_pdf(asset) { (scale * 4.0).ceil().clamp(1.0, 64.0) as u32 } else { 0 });
        if let Some(hit) = self.pix.read().ok().and_then(|m| m.get(&key).cloned()) {
            return hit;
        }
        let made = self.get(asset, scale).filter(|d| d.width > 0 && d.height > 0 && d.width <= 65535 && d.height <= 65535).map(|d| {
            let pre = d.premultiplied();
            let px: Vec<vello_cpu::color::PremulRgba8> = pre.chunks_exact(4).map(|c| vello_cpu::color::PremulRgba8 { r: c[0], g: c[1], b: c[2], a: c[3] }).collect();
            Arc::new(vello_cpu::Pixmap::from_parts(px, d.width as u16, d.height as u16))
        });
        if let Ok(mut m) = self.pix.write() {
            m.insert(key, made.clone());
        }
        made
    }

    pub fn forget(&self, asset: AssetId) {
        if let Ok(mut m) = self.cache.write() {
            m.retain(|k, _| k.0 != asset);
        }
        if let Ok(mut m) = self.pix.write() {
            m.retain(|k, _| k.0 != asset);
        }
    }

    pub fn get(&self, asset: &Asset, scale: f64) -> Option<Arc<Decoded>> {
        let key = (asset.id, if is_pdf(asset) { (scale * 4.0).ceil().clamp(1.0, 64.0) as u32 } else { 0 });
        if let Some(hit) = self.cache.read().ok().and_then(|m| m.get(&key).cloned()) {
            return hit;
        }
        let decoded = if is_pdf(asset) { render_pdf(asset, key.1 as f64 / 4.0) } else { decode(&asset.data) }.map(Arc::new);
        if let Ok(mut m) = self.cache.write() {
            m.insert(key, decoded.clone());
        }
        decoded
    }
}

pub fn image_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if crate::psd::is_psd(bytes) {
        return crate::psd::header_size(bytes);
    }
    image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?.into_dimensions().ok()
}

pub fn pdf_page_size(data: Arc<Vec<u8>>, page: usize) -> Option<(f64, f64, usize)> {
    let pdf = hayro::hayro_syntax::Pdf::new(data).ok()?;
    let pages = pdf.pages();
    let count = pages.len();
    let p = pages.get(page).or_else(|| pages.first())?;
    let (w, h) = p.render_dimensions();
    Some((w as f64, h as f64, count))
}

pub fn decode(bytes: &[u8]) -> Option<Decoded> {
    if crate::psd::is_psd(bytes) {
        let (width, height, rgba) = crate::psd::decode(bytes)?;
        return Some(Decoded { width, height, rgba });
    }
    let img = image::load_from_memory(bytes).ok()?;
    let rgba = img.to_rgba8();
    Some(Decoded { width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw() })
}

fn render_pdf(asset: &Asset, scale: f64) -> Option<Decoded> {
    let pdf = hayro::hayro_syntax::Pdf::new(asset.data.clone()).ok()?;
    let pages = pdf.pages();
    let page = pages.get(asset.page as usize).or_else(|| pages.first())?;
    let (w, h) = page.render_dimensions();
    let s = scale.clamp(0.25, 16.0).min(8000.0 / (w.max(h) as f64).max(1.0)) as f32;
    let settings = hayro::RenderSettings { x_scale: s, y_scale: s, ..Default::default() };
    let cache = hayro::RenderCache::new();
    let pix = hayro::render(page, &cache, &hayro::hayro_interpret::InterpreterSettings::default(), &settings);
    let mut rgba = pix.data_as_u8_slice().to_vec();
    for px in rgba.chunks_exact_mut(4) {
        let a = px[3] as u32;
        if a > 0 && a < 255 {
            for c in &mut px[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    Some(Decoded { width: pix.width() as u32, height: pix.height() as u32, rgba })
}
