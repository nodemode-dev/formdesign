use formdes_model::doc::Doc;
use formdes_model::geom::Rect;
use serde_json::{Value, json};

use crate::canvas::SpreadRef;
use crate::registry::{Registry, cmd};
use crate::session::Ctx;
use crate::tools::geo;
use crate::{EngineError, Result, args};

pub fn artboards(doc: &Doc) -> Vec<Value> {
    doc.prefs.get("artboards").and_then(Value::as_array).cloned().unwrap_or_default()
}

pub fn artboard_rect(a: &Value) -> Option<(usize, Rect)> {
    let spread = a.get("spread").and_then(Value::as_u64)? as usize;
    let r = a.get("rect").and_then(args::rect)?;
    (r.width() > 0.0 && r.height() > 0.0).then_some((spread, r))
}

pub fn find(doc: &Doc, key: &Value) -> Option<usize> {
    let list = artboards(doc);
    match key {
        Value::Number(n) => n.as_u64().map(|i| i as usize).filter(|i| *i < list.len()),
        Value::String(s) => list.iter().position(|a| a.get("name").and_then(Value::as_str) == Some(s.as_str())),
        _ => None,
    }
}

fn rect_arg(p: &Value) -> Result<Option<Rect>> {
    match p.get("rect") {
        None => Ok(None),
        Some(v) => args::rect(v).filter(|r| r.width() > 0.0 && r.height() > 0.0).map(Some).ok_or_else(|| EngineError::new("rect is [x0, y0, x1, y1] in spread coordinates, wider and taller than 0")),
    }
}

fn create(c: &mut Ctx, p: &Value) -> Result<Value> {
    let st = c.state()?;
    let doc = &st.doc;
    let sel = st.selection.items.clone();
    let (spread, rect) = match rect_arg(p)? {
        Some(r) => (args::opt_u64(p, "spread").map(|s| s as usize).unwrap_or(0), r),
        None => {
            let r = geo::bounds(doc, &sel).filter(|r| r.width() > 0.0 && r.height() > 0.0).ok_or_else(|| EngineError::new("give a rect, or select the objects the artboard should fit"))?;
            let spread = sel.first().and_then(|i| geo::spread_of_item(doc, doc.top_of(*i))).and_then(|s| match s { SpreadRef::Doc(i) => Some(i), SpreadRef::Parent(_) => None }).ok_or_else(|| EngineError::new("artboards go on document spreads, not parent spreads"))?;
            (spread, r)
        }
    };
    if spread >= doc.spreads.len() {
        return Err(EngineError::new(format!("no spread {spread} (the document has {})", doc.spreads.len())));
    }
    let mut list = artboards(doc);
    let name = match args::opt_str(p, "name") {
        Some(n) if list.iter().any(|a| a.get("name").and_then(Value::as_str) == Some(n)) => return Err(EngineError::new(format!("there is already an artboard named {n}"))),
        Some(n) => n.to_string(),
        None => (1..).map(|i| format!("Artboard {i}")).find(|n| !list.iter().any(|a| a.get("name").and_then(Value::as_str) == Some(n.as_str()))).unwrap_or_default(),
    };
    list.push(json!({"name": name, "spread": spread, "rect": [rect.x0, rect.y0, rect.x1, rect.y1]}));
    let index = list.len() - 1;
    c.doc_mut()?.prefs.insert("artboards".into(), Value::Array(list));
    c.label("New Artboard");
    Ok(json!({"index": index, "name": name}))
}

fn list(c: &mut Ctx, _p: &Value) -> Result<Value> {
    let doc = c.doc()?;
    Ok(json!({"artboards": artboards(doc).into_iter().enumerate().map(|(i, mut a)| { a["index"] = json!(i); a }).collect::<Vec<_>>()}))
}

fn set(c: &mut Ctx, p: &Value) -> Result<Value> {
    let key = p.get("artboard").ok_or_else(|| EngineError::new("missing `artboard` (its index or name)"))?;
    let doc = c.doc()?;
    let i = find(doc, key).ok_or_else(|| EngineError::new(format!("no artboard {key}")))?;
    let mut list = artboards(doc);
    if let Some(n) = args::opt_str(p, "name") {
        if list.iter().enumerate().any(|(j, a)| j != i && a.get("name").and_then(Value::as_str) == Some(n)) {
            return Err(EngineError::new(format!("there is already an artboard named {n}")));
        }
        list[i]["name"] = json!(n);
    }
    if let Some(r) = rect_arg(p)? {
        list[i]["rect"] = json!([r.x0, r.y0, r.x1, r.y1]);
    }
    c.doc_mut()?.prefs.insert("artboards".into(), Value::Array(list.clone()));
    c.label("Artboard Options");
    Ok(list[i].clone())
}

fn delete(c: &mut Ctx, p: &Value) -> Result<Value> {
    let key = p.get("artboard").ok_or_else(|| EngineError::new("missing `artboard` (its index or name)"))?;
    let doc = c.doc()?;
    let i = find(doc, key).ok_or_else(|| EngineError::new(format!("no artboard {key}")))?;
    let mut list = artboards(doc);
    let gone = list.remove(i);
    let d = c.doc_mut()?;
    if list.is_empty() {
        d.prefs.remove("artboards");
    } else {
        d.prefs.insert("artboards".into(), Value::Array(list));
    }
    c.label("Delete Artboard");
    Ok(gone)
}

/// One PDF page per artboard, each the artboard's own size, in list order or the order `artboards` names them.
fn export_pdf(c: &mut Ctx, p: &Value) -> Result<Value> {
    let lay = c.layout()?;
    let renderer = c.session.renderer.clone();
    let doc = c.doc()?;
    let list = artboards(doc);
    if list.is_empty() {
        return Err(EngineError::new("the document has no artboards: artboard.create makes one"));
    }
    let picks: Vec<usize> = match p.get("artboards") {
        None | Some(Value::Null) => (0..list.len()).collect(),
        Some(Value::Array(keys)) if !keys.is_empty() => keys.iter().map(|k| find(doc, k).ok_or_else(|| EngineError::new(format!("no artboard {k} (artboard.list shows them)")))).collect::<Result<_>>()?,
        Some(v) => return Err(EngineError::new(format!("artboards is a list of indices or names, not {v}"))),
    };
    let opts = formdes_render::SceneOpts { printing: true, bleed: false, overprint: false };
    let mut scenes = Vec::new();
    let mut names = Vec::new();
    for i in picks {
        let (si, area) = artboard_rect(&list[i]).ok_or_else(|| EngineError::new(format!("artboard {i} has no area")))?;
        let scene = formdes_render::scene::artboard_scene(doc, &lay, si, area, &opts).ok_or_else(|| EngineError::new(format!("artboard {i} sits on spread {si}, which no longer exists")))?;
        scenes.push(scene);
        names.push(list[i].get("name").cloned().unwrap_or(Value::Null));
    }
    let popts = formdes_render::pdf::PdfOpts { title: args::opt_str(p, "title").map(str::to_string).unwrap_or_else(|| doc.title.clone()), ..Default::default() };
    let bytes = formdes_render::pdf::export(&scenes, doc, &renderer.images, &popts).map_err(|e| EngineError::new(format!("PDF export: {e}")))?;
    crate::cmd::file::deliver(p, bytes, json!({"pages": names.len(), "artboards": names}))
}

pub fn register(r: &mut Registry) {
    cmd("artboard.create", "New Artboard").menu(&["Object", "Artboards", "New Artboard"]).sig("{name?, rect?: [x0, y0, x1, y1] in spread coordinates, spread?: index (with rect; default 0)} (without rect it fits the selection)").reg(r, create);
    cmd("artboard.list", "Artboards").query().sig("{}").reg(r, list);
    cmd("artboard.set", "Artboard Options").sig("{artboard: index or name, name?, rect?}").reg(r, set);
    cmd("artboard.delete", "Delete Artboard").menu(&["Object", "Artboards", "Delete Artboard"]).sig("{artboard: index or name}").reg(r, delete);
    cmd("artboard.exportPdf", "Export Artboards as PDF...").query().sig("{artboards?: [index or name] (default all, in list order), title?, path?} (without path the PDF comes back as base64)").reg(r, export_pdf);
}
