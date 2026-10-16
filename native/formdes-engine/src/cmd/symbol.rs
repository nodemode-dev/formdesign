use formdes_model::doc::{Doc, Home};
use formdes_model::geom::{Affine, Rect, Vec2};
use formdes_model::ids::ItemId;
use formdes_model::item::{Item, ItemKind, Shape};
use formdes_render::scene::symbol_master;
use serde_json::{Map, Value, json};

use crate::registry::{Needs, Registry, cmd};
use crate::session::{Ctx, Selection};
use crate::target::items;
use crate::{EngineError, Result, args, ops};

fn library(doc: &Doc) -> Map<String, Value> {
    doc.prefs.get("symbols").and_then(Value::as_object).cloned().unwrap_or_default()
}

fn instances(doc: &Doc, name: &str) -> Vec<ItemId> {
    doc.items.values().filter(|i| i.extra.get("symbol").and_then(Value::as_str) == Some(name) && doc.home(i.id).is_some()).map(|i| i.id).collect()
}

fn uses_symbol(doc: &Doc, root: ItemId, name: &str) -> bool {
    doc.walk(&[root]).iter().any(|i| doc.item(*i).and_then(|it| it.extra.get("symbol")).and_then(Value::as_str) == Some(name))
}

fn make_master(doc: &mut Doc, ids: &[ItemId]) -> Result<ItemId> {
    let order = super::art::paint_order(doc, ids);
    let master = match order.len() {
        0 => return Err(EngineError::new("select the art to make into a symbol")),
        1 => order[0],
        _ => ops::group(doc, &order)?,
    };
    let world = doc.world_xf(master);
    doc.detach_from_home(master);
    if let Some(it) = doc.item_mut(master) {
        it.xf = world;
    }
    Ok(master)
}

fn instance(doc: &mut Doc, name: &str, rect: Rect, xf: Affine, si: usize, layer: formdes_model::ids::LayerId) -> ItemId {
    let mut it = Item::new(ItemId(0), ItemKind::Plain, Shape::Rect { rect }, layer);
    it.xf = xf;
    it.fill = formdes_model::color::Paint::None;
    it.stroke = ops::default_stroke(0.0);
    it.extra.insert("symbol".into(), json!(name));
    doc.insert_item(it, Home::Spread(si))
}

fn create(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let name = args::str_(p, "name", "a name for the symbol")?.trim().to_string();
    let doc = c.doc_mut()?;
    let mut lib = library(doc);
    if name.is_empty() || lib.contains_key(&name) {
        return Err(EngineError::new(format!("a symbol named {name} already exists (or the name is empty): choose another")));
    }
    let si = doc.spread_of(ids[0]).ok_or_else(|| EngineError::new("the art must be on a spread"))?;
    let layer = doc.item(ids[0]).map(|i| i.layer).unwrap_or(ops::active_layer(doc));
    let master = make_master(doc, &ids)?;
    let rect = doc.bounds(master).unwrap_or(Rect::ZERO);
    lib.insert(name.clone(), json!(master.0));
    doc.prefs.insert("symbols".into(), Value::Object(lib));
    let inst = instance(doc, &name, rect, Affine::IDENTITY, si, layer);
    c.set_selection(Selection::items(vec![inst]));
    c.label("New Symbol");
    Ok(json!({"name": name, "instance": inst.0}))
}

fn place(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = args::str_(p, "name", "the symbol's name")?.to_string();
    let home = super::object::spread_home(c.doc()?, p.get("spread"))?;
    let Home::Spread(si) = home else { return Err(EngineError::new("symbols are placed on document spreads")) };
    let doc = c.doc_mut()?;
    let master = symbol_master(doc, &name).ok_or_else(|| EngineError::new(format!("no symbol named {name}; symbol.list shows them")))?;
    let rect = doc.bounds(master).unwrap_or(Rect::ZERO);
    let at = p.get("at").and_then(Value::as_array).and_then(|a| Some((a.first()?.as_f64()?, a.get(1)?.as_f64()?)));
    let s = args::opt_f64(p, "scale").unwrap_or(100.0) / 100.0;
    let c0 = rect.center();
    let target = at.map(|(x, y)| kurbo::Point::new(x, y)).unwrap_or(c0 + Vec2::new(20.0, 20.0));
    let xf = Affine::translate(target.to_vec2()) * Affine::scale(s) * Affine::translate(-c0.to_vec2());
    let layer = ops::active_layer(doc);
    let inst = instance(doc, &name, rect, xf, si, layer);
    c.set_selection(Selection::items(vec![inst]));
    c.label("Place Symbol");
    Ok(json!({"id": inst.0}))
}

fn break_link(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let doc = c.doc_mut()?;
    let mut made = Vec::new();
    for id in &ids {
        let Some(name) = doc.item(*id).and_then(|i| i.extra.get("symbol")).and_then(Value::as_str).map(str::to_string) else { continue };
        let Some(master) = symbol_master(doc, &name) else { continue };
        let Some(home) = doc.home(*id) else { continue };
        let world = doc.world_xf(*id);
        let Some(copy) = ops::duplicate(doc, master, Vec2::ZERO, Some(home)) else { continue };
        let parent = doc.item(copy).and_then(|i| i.group).map(|g| doc.world_xf(g)).unwrap_or(Affine::IDENTITY);
        if let Some(it) = doc.item_mut(copy) {
            it.xf = parent.inverse() * world * it.xf;
        }
        doc.remove_item(*id);
        made.push(copy);
    }
    if made.is_empty() {
        return Err(EngineError::new("select one or more symbol instances"));
    }
    c.set_selection(Selection::items(made.clone()));
    c.label("Break Link to Symbol");
    Ok(json!({"ids": made}))
}

fn redefine(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let name = args::str_(p, "name", "the symbol to redefine")?.to_string();
    let doc = c.doc_mut()?;
    let old = symbol_master(doc, &name).ok_or_else(|| EngineError::new(format!("no symbol named {name}")))?;
    if ids.iter().any(|i| uses_symbol(doc, *i, &name)) {
        return Err(EngineError::new(format!("the new art contains an instance of {name} itself")));
    }
    let old_box = doc.bounds(old).unwrap_or(Rect::ZERO);
    let master = make_master(doc, &ids)?;
    let new_box = doc.bounds(master).unwrap_or(Rect::ZERO);
    if let Some(it) = doc.item_mut(master) {
        it.xf = Affine::translate(old_box.center() - new_box.center()) * it.xf;
    }
    let moved = doc.bounds(master).unwrap_or(Rect::ZERO);
    doc.remove_item(old);
    let mut lib = library(doc);
    lib.insert(name.clone(), json!(master.0));
    doc.prefs.insert("symbols".into(), Value::Object(lib));
    let insts = instances(doc, &name);
    for i in &insts {
        if let Some(it) = doc.item_mut(*i) {
            it.shape = Shape::Rect { rect: moved };
        }
    }
    c.set_selection(Selection::items(Vec::new()));
    c.label("Redefine Symbol");
    Ok(json!({"name": name, "instances": insts.len()}))
}

fn list(c: &mut Ctx, _p: &Value) -> Result<Value> {
    let doc = c.doc()?;
    let out: Vec<Value> = library(doc).keys().map(|n| json!({"name": n, "instances": instances(doc, n).len()})).collect();
    Ok(json!({"symbols": out}))
}

fn delete(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = args::str_(p, "name", "the symbol to delete")?.to_string();
    let doc = c.doc_mut()?;
    let master = symbol_master(doc, &name).ok_or_else(|| EngineError::new(format!("no symbol named {name}")))?;
    let n = instances(doc, &name).len();
    if n > 0 {
        return Err(EngineError::new(format!("{name} has {n} instance(s): break their links (symbol.break) or delete them first")));
    }
    doc.remove_item(master);
    let mut lib = library(doc);
    lib.remove(&name);
    doc.prefs.insert("symbols".into(), Value::Object(lib));
    c.label("Delete Symbol");
    Ok(json!({"deleted": name}))
}

pub fn register(r: &mut Registry) {
    cmd("symbol.create", "New Symbol").menu(&["Object", "Symbols", "New Symbol..."]).needs(Needs::Items).sig("{name, ids?}")
        .help("Turns the selected art into a symbol kept in the document and puts an instance in its place; every instance draws the symbol's art, so redefining it changes them all, as Illustrator's symbols do.").reg(r, create);
    cmd("symbol.place", "Place Symbol Instance").menu(&["Object", "Symbols", "Place Instance..."]).sig("{name, at?: [x, y] (centre, spread points), scale?: percent, spread?}")
        .help("Places another instance of a symbol, centred at the given point and scaled; instances move, scale, rotate and take effects like any object.").reg(r, place);
    cmd("symbol.break", "Break Link to Symbol").menu(&["Object", "Symbols", "Break Link to Symbol"]).needs(Needs::Items).sig("{ids?}")
        .help("Replaces the selected instances with editable copies of the symbol's art in the same place and size.").reg(r, break_link);
    cmd("symbol.redefine", "Redefine Symbol").menu(&["Object", "Symbols", "Redefine Symbol..."]).needs(Needs::Items).sig("{name, ids?}")
        .help("Makes the selected art the symbol's new definition, centred where the old art was; every instance updates.").reg(r, redefine);
    cmd("symbol.list", "Symbols").sig("{}").help("Lists the document's symbols with how many instances each has.").reg(r, list);
    cmd("symbol.delete", "Delete Symbol").menu(&["Object", "Symbols", "Delete Symbol..."]).sig("{name}")
        .help("Deletes a symbol that has no instances left.").reg(r, delete);
}
