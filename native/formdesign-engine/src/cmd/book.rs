use std::path::PathBuf;

use formdesign_model::doc::Doc;
use formdesign_model::ids::AssetId;
use formdesign_model::style::StyleKind;
use formdesign_render::scene::{Op, SceneOpts, page_scene};
use serde_json::{Value, json};

use crate::registry::{Needs, Registry, cmd};
use crate::session::Ctx;
use crate::{EngineError, Result, args};

pub struct Book {
    pub path: PathBuf,
    pub chapters: Vec<(PathBuf, Doc)>,
    pub source: usize,
}

fn book<'a>(c: &'a mut Ctx<'_>) -> Result<&'a mut Book> {
    c.session.book.as_mut().ok_or_else(|| EngineError::new("no book is open: run book.new first"))
}

fn load(path: &PathBuf) -> Result<Doc> {
    let bytes = std::fs::read(path).map_err(|e| EngineError::new(format!("book: cannot read {}: {e}", path.display())))?;
    formdesign_model::format::load(&bytes).map_err(|e| EngineError::new(format!("book: {} is not a FormDesign document: {}", path.display(), e.0)))
}

fn save(path: &PathBuf, doc: &Doc) -> Result<()> {
    let bytes = formdesign_model::format::save(doc)?;
    std::fs::write(path, bytes).map_err(|e| EngineError::new(format!("book: cannot write {}: {e}", path.display())))
}

fn new(c: &mut Ctx, p: &Value) -> Result<Value> {
    let path = PathBuf::from(args::str_(p, "path", "where the book file goes")?);
    c.session.book = Some(Book { path, chapters: Vec::new(), source: 0 });
    Ok(json!({}))
}

fn add(c: &mut Ctx, p: &Value) -> Result<Value> {
    let path = PathBuf::from(args::str_(p, "path", "a chapter document")?);
    let doc = load(&path)?;
    book(c)?.chapters.push((path, doc));
    Ok(json!({}))
}

fn style_source(c: &mut Ctx, p: &Value) -> Result<Value> {
    let i = args::u64_(p, "index", "the chapter whose styles lead, from 0")? as usize;
    let b = book(c)?;
    if i >= b.chapters.len().max(1) {
        return Err(EngineError::new(format!("no chapter {i}")));
    }
    b.source = i;
    Ok(json!({}))
}

fn paginate(c: &mut Ctx, _: &Value) -> Result<Value> {
    let b = book(c)?;
    let mut next = 1u32;
    for (path, doc) in b.chapters.iter_mut() {
        let first = doc.sections.iter().find(|s| s.start == 0).and_then(|s| s.start_at);
        if first != Some(next) {
            match doc.sections.iter_mut().find(|s| s.start == 0) {
                Some(s) => s.start_at = Some(next),
                None => doc.sections.insert(0, formdesign_model::page::Section { start: 0, start_at: Some(next), style: "arabic".into(), prefix: String::new(), marker: String::new(), include_prefix: false }),
            }
            save(path, doc)?;
        }
        let count = doc.page_count();
        next = doc.page_number(count.saturating_sub(1)) + 1;
    }
    Ok(json!({}))
}

fn sync_styles(c: &mut Ctx, _: &Value) -> Result<Value> {
    let b = book(c)?;
    let Some((_, src)) = b.chapters.get(b.source) else { return Err(EngineError::new("the book has no chapters")) };
    let src = src.clone();
    let source = b.source;
    for (i, (path, doc)) in b.chapters.iter_mut().enumerate() {
        if i == source {
            continue;
        }
        for kind in [StyleKind::Paragraph, StyleKind::Character] {
            for s in src.styles.list(kind) {
                let list = doc.styles.list_mut(kind);
                match list.iter_mut().find(|x| x.name == s.name) {
                    Some(x) => *x = s.clone(),
                    None => list.push(s.clone()),
                }
            }
        }
        for sw in &src.swatches {
            match doc.swatches.iter_mut().find(|x| x.name == sw.name) {
                Some(x) => *x = sw.clone(),
                None => doc.swatches.push(sw.clone()),
            }
        }
        save(path, doc)?;
    }
    Ok(json!({}))
}

fn list(c: &mut Ctx, _: &Value) -> Result<Value> {
    let b = book(c)?;
    let docs: Vec<Value> = b.chapters.iter().map(|(path, d)| json!({"path": path.display().to_string(), "title": d.title, "pages": d.page_count(), "firstPage": d.page_number(0)})).collect();
    Ok(json!({"path": b.path.display().to_string(), "documents": docs}))
}

fn export_pdf(c: &mut Ctx, p: &Value) -> Result<Value> {
    let out = PathBuf::from(args::str_(p, "path", "where the PDF goes")?);
    let renderer = c.session.renderer.clone();
    let b = book(c)?;
    if b.chapters.is_empty() {
        return Err(EngineError::new("the book has no chapters"));
    }
    let mut merged = b.chapters[0].1.clone();
    merged.assets.clear();
    let mut scenes = Vec::new();
    for (k, (_, doc)) in b.chapters.iter().enumerate() {
        let base = (k as u64 + 1) << 40;
        for (id, a) in &doc.assets {
            let mut a = a.clone();
            a.id = AssetId(base + id.0);
            merged.assets.insert(a.id, a);
        }
        let lay = renderer.layout(doc);
        for page in 0..doc.page_count() {
            if let Some(mut s) = page_scene(doc, &lay, page, &SceneOpts { printing: true, bleed: false, overprint: false }) {
                for op in s.ops.iter_mut() {
                    if let Op::Image { asset, .. } = op {
                        *asset = AssetId(base + asset.0);
                    }
                }
                s.page = scenes.len();
                scenes.push(s);
            }
        }
    }
    merged.title = p.get("title").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| b.path.file_stem().and_then(|s| s.to_str()).unwrap_or("Book").to_string());
    let bytes = formdesign_render::pdf::export(&scenes, &merged, &renderer.images, &formdesign_render::pdf::PdfOpts { title: merged.title.clone(), ..Default::default() }).map_err(|e| EngineError::new(format!("book PDF: {e}")))?;
    std::fs::write(&out, &bytes).map_err(|e| EngineError::new(format!("book PDF: cannot write {}: {e}", out.display())))?;
    Ok(json!({"path": out.display().to_string(), "pages": scenes.len(), "bytes": bytes.len()}))
}

fn close(c: &mut Ctx, _: &Value) -> Result<Value> {
    c.session.book = None;
    Ok(json!({}))
}

pub fn register(r: &mut Registry) {
    cmd("book.new", "New Book").needs(Needs::Nothing).query().sig("{path}").reg(r, new);
    cmd("book.add", "Add Document").needs(Needs::Nothing).query().sig("{path}").reg(r, add);
    cmd("book.styleSource", "Style Source").needs(Needs::Nothing).query().sig("{index}").reg(r, style_source);
    cmd("book.paginate", "Update Numbering").needs(Needs::Nothing).query().sig("{}").reg(r, paginate);
    cmd("book.syncStyles", "Synchronize Book").needs(Needs::Nothing).query().sig("{}").reg(r, sync_styles);
    cmd("book.list", "Book Documents").needs(Needs::Nothing).query().sig("{}").reg(r, list);
    cmd("book.exportPdf", "Export Book to PDF").needs(Needs::Nothing).query().sig("{path, title?}").reg(r, export_pdf);
    cmd("book.close", "Close Book").needs(Needs::Nothing).query().sig("{}").reg(r, close);
}
