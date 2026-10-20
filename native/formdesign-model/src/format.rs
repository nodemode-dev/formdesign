use std::io::{Cursor, Read, Write};
use std::sync::Arc;

use crate::doc::{Doc, FORMAT_VERSION};
use crate::{ModelError, Result, err};

pub const MIME: &str = "application/vnd.formdesign.document";

fn zerr(e: impl std::fmt::Display) -> ModelError {
    ModelError(format!("document file: {e}"))
}

pub fn save(doc: &Doc) -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let deflate = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        z.start_file("mimetype", stored).map_err(zerr)?;
        z.write_all(MIME.as_bytes()).map_err(zerr)?;
        z.start_file("doc.json", deflate).map_err(zerr)?;
        let json = serde_json::to_vec(doc).map_err(zerr)?;
        z.write_all(&json).map_err(zerr)?;
        for (id, a) in &doc.assets {
            if a.data.is_empty() {
                continue;
            }
            z.start_file(format!("assets/{id}"), stored).map_err(zerr)?;
            z.write_all(&a.data).map_err(zerr)?;
        }
        z.finish().map_err(zerr)?;
    }
    Ok(buf.into_inner())
}

pub fn load(bytes: &[u8]) -> Result<Doc> {
    if bytes.first() == Some(&b'{') {
        let mut d: Doc = serde_json::from_slice(bytes).map_err(zerr)?;
        d.bump_ids();
        d.check()?;
        return Ok(d);
    }
    let mut z = zip::ZipArchive::new(Cursor::new(bytes)).map_err(zerr)?;
    let mut json = Vec::new();
    match z.by_name("doc.json") {
        Ok(mut f) => {
            f.read_to_end(&mut json).map_err(zerr)?;
        }
        Err(_) => return err("not a FormDesign document (no doc.json)"),
    }
    let mut d: Doc = serde_json::from_slice(&json).map_err(zerr)?;
    if d.format > FORMAT_VERSION {
        return err(format!("this document was saved by a newer FormDesign (format {})", d.format));
    }
    let ids: Vec<_> = d.assets.keys().copied().collect();
    for id in ids {
        if let Ok(mut f) = z.by_name(&format!("assets/{id}")) {
            let mut data = Vec::new();
            f.read_to_end(&mut data).map_err(zerr)?;
            if let Some(a) = d.assets.get_mut(&id) {
                a.data = Arc::new(data);
            }
        }
    }
    d.bump_ids();
    d.check()?;
    Ok(d)
}
