use std::path::Path;
use std::sync::Arc;

use formdesign_model::ids::AssetId;
use formdesign_model::item::Asset;

use crate::{EngineError, Result};

pub fn mime_of(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" | "jpe" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "svg" => "image/svg+xml",
        "pdf" | "ai" => "application/pdf",
        "eps" => "application/postscript",
        "psd" => "image/vnd.adobe.photoshop",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "m4a" | "aac" => "audio/mp4",
        "ogg" | "oga" => "audio/ogg",
        _ => "application/octet-stream",
    }
}

fn be16(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*b.get(i)?, *b.get(i + 1)?]))
}

fn be32(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_be_bytes([*b.get(i)?, *b.get(i + 1)?, *b.get(i + 2)?, *b.get(i + 3)?]))
}

pub fn density(b: &[u8]) -> Option<(f64, f64)> {
    if b.starts_with(&[0xFF, 0xD8]) {
        let mut i = 2;
        while i + 4 < b.len() && b[i] == 0xFF {
            let marker = b[i + 1];
            let len = be16(b, i + 2)? as usize;
            if marker == 0xE0 && b.get(i + 4..i + 9) == Some(b"JFIF\0") {
                let units = *b.get(i + 11)?;
                let (x, y) = (be16(b, i + 12)? as f64, be16(b, i + 14)? as f64);
                return match units {
                    1 if x > 0.0 && y > 0.0 => Some((x, y)),
                    2 if x > 0.0 && y > 0.0 => Some((x * 2.54, y * 2.54)),
                    _ => None,
                };
            }
            if marker == 0xDA {
                break;
            }
            i += 2 + len;
        }
        return None;
    }
    if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        let mut i = 8;
        while i + 8 <= b.len() {
            let len = be32(b, i)? as usize;
            let kind = b.get(i + 4..i + 8)?;
            if kind == b"pHYs" {
                let (x, y, unit) = (be32(b, i + 8)? as f64, be32(b, i + 12)? as f64, *b.get(i + 16)?);
                return (unit == 1 && x > 0.0 && y > 0.0).then_some((x * 0.0254, y * 0.0254));
            }
            if kind == b"IDAT" {
                break;
            }
            i += 12 + len;
        }
    }
    None
}

pub fn mtime(path: &Path) -> u64 {
    std::fs::metadata(path).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn make(name: &str, data: Vec<u8>, link: Option<String>, page: u32) -> Result<Asset> {
    let mut mime = mime_of(name).to_string();
    if data.starts_with(b"%PDF") {
        mime = "application/pdf".into();
    }
    let data = Arc::new(data);
    let (pixels, ppi) = if mime == "application/pdf" {
        let (w, h, count) = formdesign_render::images::pdf_page_size(data.clone(), page as usize).ok_or_else(|| EngineError::new(format!("{name}: cannot read this PDF")))?;
        if page as usize >= count {
            return Err(EngineError::new(format!("{name} has {count} pages (page indices are 0-based)")));
        }
        (Some((w.round() as u32, h.round() as u32)), None)
    } else if mime.starts_with("image/") && formdesign_render::psd::is_psd(&data) {
        let dims = formdesign_render::psd::check(&data).map_err(|why| EngineError::new(format!("{name}: cannot read this PSD file: {why}")))?;
        (Some(dims), density(&data).or_else(|| formdesign_render::psd::density(&data)))
    } else if mime.starts_with("image/") {
        let dims = formdesign_render::images::image_size(&data).ok_or_else(|| EngineError::new(format!("{name}: cannot read this image (supported: JPEG, PNG, GIF, WebP, BMP, TIFF and PDF)")))?;
        (Some(dims), density(&data).or_else(|| formdesign_render::psd::density(&data)))
    } else if mime.starts_with("video/") || mime.starts_with("audio/") {
        (Some((640, 360)), None)
    } else {
        return Err(EngineError::new(format!("cannot place {name}: unsupported file type")));
    };
    let modified = link.as_deref().map(|p| mtime(Path::new(p))).unwrap_or(0);
    Ok(Asset { id: AssetId(0), name: name.to_string(), mime, embedded: link.is_none(), link, data, pixels, ppi, page, modified })
}

pub fn load(path: &str, page: u32) -> Result<Asset> {
    let data = std::fs::read(path).map_err(|e| EngineError::new(format!("cannot read {path}: {e}")))?;
    let name = Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or(path).to_string();
    make(&name, data, Some(path.to_string()), page)
}
