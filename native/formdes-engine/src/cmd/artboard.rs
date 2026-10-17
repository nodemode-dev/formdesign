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

pub fn register(r: &mut Registry) {
    cmd("artboard.create", "New Artboard").menu(&["Object", "Artboards", "New Artboard"]).sig("{name?, rect?: [x0, y0, x1, y1] in spread coordinates, spread?: index (with rect; default 0)} (without rect it fits the selection)").reg(r, create);
    cmd("artboard.list", "Artboards").query().sig("{}").reg(r, list);
    cmd("artboard.set", "Artboard Options").sig("{artboard: index or name, name?, rect?}").reg(r, set);
    cmd("artboard.delete", "Delete Artboard").menu(&["Object", "Artboards", "Delete Artboard"]).sig("{artboard: index or name}").reg(r, delete);
}
