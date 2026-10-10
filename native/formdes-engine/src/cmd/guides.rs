use formdes_model::doc::Doc;
use formdes_model::page::{Guide, Orientation};
use serde_json::{Value, json};

use crate::registry::{Registry, cmd};
use crate::session::Ctx;
use crate::{EngineError, Result, args};

#[derive(Clone, Copy)]
enum Where {
    Spread(usize),
    Parent(usize),
}

fn where_of(c: &Ctx, p: &Value) -> Result<Where> {
    let doc = c.doc()?;
    let w = match p.get("spread") {
        Some(Value::Object(o)) => {
            let i = o.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
            if o.get("kind").and_then(Value::as_str) == Some("parent") { Where::Parent(i) } else { Where::Spread(i) }
        }
        Some(v) if !v.is_null() => Where::Spread(v.as_u64().ok_or_else(|| EngineError::new("`spread` expected a spread index or {kind: \"parent\", index}"))? as usize),
        _ => {
            let sel = c.selection();
            Where::Spread(sel.items.first().and_then(|i| doc.spread_of(*i)).unwrap_or(0))
        }
    };
    match w {
        Where::Spread(i) if i >= doc.spreads.len() => Err(EngineError::new(format!("no spread {i} (the document has {})", doc.spreads.len()))),
        Where::Parent(i) if i >= doc.parents.len() => Err(EngineError::new(format!("no parent spread {i}"))),
        _ => Ok(w),
    }
}

fn list_of(doc: &Doc, w: Where) -> &Vec<Guide> {
    match w {
        Where::Spread(i) => &doc.spreads[i].guides,
        Where::Parent(i) => &doc.parents[i].guides,
    }
}

fn list_mut(doc: &mut Doc, w: Where) -> &mut Vec<Guide> {
    match w {
        Where::Spread(i) => &mut doc.spreads[i].guides,
        Where::Parent(i) => &mut doc.parents[i].guides,
    }
}

fn pages_of(doc: &Doc, w: Where) -> Vec<(f64, f64)> {
    match w {
        Where::Spread(i) => doc.spreads[i].pages.iter().map(|p| (p.width, p.height)).collect(),
        Where::Parent(i) => doc.parents[i].pages.iter().map(|p| (p.width, p.height)).collect(),
    }
}

fn guide_json(i: usize, g: &Guide) -> Value {
    json!({"index": i, "orientation": g.orientation, "position": g.pos, "page": g.page, "color": g.color, "locked": g.locked})
}

fn list(c: &mut Ctx, p: &Value) -> Result<Value> {
    let w = where_of(c, p)?;
    let doc = c.doc()?;
    let out: Vec<Value> = list_of(doc, w).iter().enumerate().map(|(i, g)| guide_json(i, g)).collect();
    Ok(json!({"guides": out}))
}

fn index_of(c: &Ctx, p: &Value, w: Where) -> Result<usize> {
    let i = args::u64_(p, "index", "a guide index from guide.list")? as usize;
    let n = list_of(c.doc()?, w).len();
    if i >= n {
        return Err(EngineError::new(format!("no guide {i} on this spread (it has {n})")));
    }
    Ok(i)
}

fn delete(c: &mut Ctx, p: &Value) -> Result<Value> {
    let w = where_of(c, p)?;
    if args::opt_bool(p, "all").unwrap_or(false) || p.get("index").is_none() {
        let n = list_of(c.doc()?, w).len();
        list_mut(c.doc_mut()?, w).clear();
        c.label("Delete All Guides on Spread");
        return Ok(json!({"deleted": n}));
    }
    let i = index_of(c, p, w)?;
    if list_of(c.doc()?, w)[i].locked {
        return Err(EngineError::new("the guide is locked: unlock it first"));
    }
    list_mut(c.doc_mut()?, w).remove(i);
    c.label("Delete Guide");
    Ok(json!({"deleted": 1}))
}

fn set(c: &mut Ctx, p: &Value) -> Result<Value> {
    let w = where_of(c, p)?;
    let i = index_of(c, p, w)?;
    let pos = args::opt_f64(p, "position");
    let color = args::opt_str(p, "color").map(str::to_string);
    let locked = args::opt_bool(p, "locked");
    if pos.is_some() && list_of(c.doc()?, w)[i].locked && locked != Some(false) {
        return Err(EngineError::new("the guide is locked: unlock it first"));
    }
    let doc = c.doc_mut()?;
    let g = &mut list_mut(doc, w)[i];
    if let Some(v) = pos {
        g.pos = v;
    }
    if let Some(v) = color {
        g.color = v;
    }
    if let Some(v) = locked {
        g.locked = v;
    }
    let out = guide_json(i, g);
    c.label(if pos.is_some() { "Move Guide" } else { "Ruler Guides" });
    Ok(out)
}

fn create(c: &mut Ctx, p: &Value) -> Result<Value> {
    let w = where_of(c, p)?;
    let rows = args::opt_u64(p, "rows").unwrap_or(0).min(100) as usize;
    let cols = args::opt_u64(p, "columns").unwrap_or(0).min(100) as usize;
    let rg = args::opt_f64(p, "rowGutter").unwrap_or(12.0).max(0.0);
    let cg = args::opt_f64(p, "columnGutter").unwrap_or(12.0).max(0.0);
    let fit = args::opt_str(p, "fitTo").unwrap_or("margins").to_string();
    let color = args::opt_str(p, "color").unwrap_or("#4ae3ff").to_string();
    let remove = args::opt_bool(p, "removeExisting").unwrap_or(false);
    let doc = c.doc()?;
    let sizes = pages_of(doc, w);
    let first = match w {
        Where::Spread(i) => Some(doc.first_page_of_spread(i)),
        Where::Parent(_) => None,
    };
    let mut made = Vec::new();
    let mut x = 0.0;
    for (k, (pw, ph)) in sizes.iter().enumerate() {
        let r = match (fit.as_str(), first) {
            ("page", _) | (_, None) => (x, 0.0, x + pw, *ph),
            (_, Some(f)) => match crate::cmd::object::margin_rect(doc, f + k) {
                Some(m) => (m.x0, m.y0, m.x1, m.y1),
                None => (x, 0.0, x + pw, *ph),
            },
        };
        let (x0, y0, x1, y1) = r;
        if cols > 1 {
            let cw = ((x1 - x0) - cg * (cols as f64 - 1.0)) / cols as f64;
            for j in 1..cols {
                let left = x0 + j as f64 * cw + (j as f64 - 1.0) * cg;
                for v in [left, left + cg] {
                    made.push(Guide { orientation: Orientation::Vertical, pos: v, page: Some(k), color: color.clone(), locked: false });
                    if cg == 0.0 {
                        break;
                    }
                }
            }
        }
        if rows > 1 {
            let rh = ((y1 - y0) - rg * (rows as f64 - 1.0)) / rows as f64;
            for j in 1..rows {
                let top = y0 + j as f64 * rh + (j as f64 - 1.0) * rg;
                for v in [top, top + rg] {
                    made.push(Guide { orientation: Orientation::Horizontal, pos: v, page: Some(k), color: color.clone(), locked: false });
                    if rg == 0.0 {
                        break;
                    }
                }
            }
        }
        x += pw;
    }
    let n = made.len();
    let doc = c.doc_mut()?;
    let l = list_mut(doc, w);
    if remove {
        l.clear();
    }
    l.extend(made);
    c.label("Create Guides");
    Ok(json!({"created": n}))
}

pub fn register(r: &mut Registry) {
    cmd("guide.list", "Guides").query().sig("{spread?}").reg(r, list);
    cmd("guide.delete", "Delete All Guides on Spread").menu(&["View", "Grids & Guides", "Delete All Guides on Spread"]).sig("{spread?, index?: one guide, all?}").reg(r, delete);
    cmd("guide.set", "Ruler Guides").sig("{spread?, index, position?, color?, locked?}").reg(r, set);
    cmd("layout.createGuides", "Create Guides").menu(&["Layout", "Create Guides..."]).sig("{rows?, columns?, rowGutter?, columnGutter?, fitTo?: margins|page, removeExisting?, color?, spread?}").reg(r, create);
}
