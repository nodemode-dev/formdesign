use std::collections::BTreeSet;

use formdes_model::doc::{Doc, Home, Intent};
use formdes_model::geom::{Affine, Edges, Rect, Vec2};
use formdes_model::ids::{ItemId, LayerId, ParentId};
use formdes_model::item::{Item, ItemKind};
use formdes_model::page::{Guide, LAYER_COLORS, Layer, Orientation, Parent, Section};
use formdes_model::style::StyleKind;
use serde_json::{Map, Value, json};

use crate::cmd::object::item_page;
use crate::registry::{Registry, cmd};
use crate::session::{Ctx, Selection};
use crate::{EngineError, Result, args, ops, wire};

fn ok() -> Result<Value> {
    Ok(json!({}))
}

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn rect(r: Rect) -> Value {
    json!([r2(r.x0), r2(r.y0), r2(r.x1), r2(r.y1)])
}

fn kind_label(it: &Item) -> String {
    use formdes_model::item::Shape;
    match &it.kind {
        ItemKind::Text(t) if t.on_path.is_some() => "type on a path".into(),
        ItemKind::Text(_) => "text frame".into(),
        ItemKind::Graphic(_) => "graphic frame".into(),
        ItemKind::Group { .. } => "group".into(),
        ItemKind::States { .. } => "multi-state object".into(),
        ItemKind::Plain => match it.shape {
            Shape::Rect { .. } => "<rectangle>".into(),
            Shape::Ellipse { .. } => "<ellipse>".into(),
            Shape::Polygon { .. } => "<polygon>".into(),
            Shape::Line { .. } => "<graphic line>".into(),
            Shape::Path { .. } => "<path>".into(),
        },
    }
}

fn item_json(doc: &Doc, id: ItemId, overset: &dyn Fn(formdes_model::ids::StoryId) -> bool) -> Option<Value> {
    let it = doc.item(id)?;
    let mut o = Map::new();
    o.insert("id".into(), json!(id));
    o.insert("kind".into(), json!(kind_label(it)));
    o.insert("name".into(), json!(it.name));
    o.insert("label".into(), json!(it.label));
    o.insert("layer".into(), json!(it.layer));
    o.insert("hidden".into(), json!(it.hidden));
    o.insert("locked".into(), json!(it.locked));
    o.insert("bounds".into(), rect(doc.bounds(id).unwrap_or(Rect::ZERO)));
    o.insert("fill".into(), json!(wire::paint_name(&it.fill)));
    o.insert("stroke".into(), json!({"swatch": wire::paint_name(&it.stroke.paint), "weight": it.stroke.weight}));
    if let Some(s) = &it.style {
        o.insert("objectStyle".into(), json!(wire::style_name(StyleKind::Object, s)));
    }
    // The appearance stack (extra fills and strokes) and live effects, for the Appearance panel; left out when empty.
    for key in ["appearance", "effects"] {
        if let Some(list) = it.extra.get(key).and_then(Value::as_array).filter(|a| !a.is_empty()) {
            let mut list = list.clone();
            // Each extra fill or stroke also carries its paint's swatch name, as the main fill and stroke do.
            for e in list.iter_mut() {
                if let Some(name) = e.get("paint").and_then(|p| serde_json::from_value::<formdes_model::color::Paint>(p.clone()).ok()).map(|p| wire::paint_name(&p)) {
                    e["swatch"] = json!(name);
                }
            }
            o.insert(key.into(), Value::Array(list));
        }
    }
    match &it.kind {
        ItemKind::Text(t) => {
            o.insert("story".into(), json!(t.story));
            let bb = it.shape.bbox();
            o.insert("columns".into(), json!(t.column_plan((bb.width() - t.insets.inside - t.insets.outside).max(1.0)).0));
            o.insert("columnRule".into(), json!(t.column_rule));
            if let Some(w) = t.fixed_column_width {
                o.insert("columnWidth".into(), json!(w));
            }
            if t.max_column_width > 0.0 {
                o.insert("maxColumnWidth".into(), json!(t.max_column_width));
            }
            o.insert("overset".into(), json!(overset(t.story)));
        }
        ItemKind::Graphic(g) => {
            if let Some(a) = g.asset {
                o.insert("asset".into(), json!(a));
            }
        }
        _ => {}
    }
    let kids = it.children();
    if matches!(it.kind, ItemKind::Group { .. } | ItemKind::States { .. }) {
        o.insert("children".into(), Value::Array(kids.iter().filter_map(|k| item_json(doc, *k, overset)).collect()));
    }
    Some(Value::Object(o))
}

fn inspect(c: &mut Ctx, _: &Value) -> Result<Value> {
    let lay = c.layout()?;
    let active_layer = c.target_layer()?;
    let st = c.state()?;
    let doc = &st.doc;
    let over = |s| lay.overset(s);
    let mut spreads = Vec::new();
    let mut abs = 0;
    for (si, s) in doc.spreads.iter().enumerate() {
        let mut pages = Vec::new();
        for (pi, pg) in s.pages.iter().enumerate() {
            let x = s.page_x(pi);
            let side = if !doc.settings.facing { "single" } else if doc.is_right_page(abs) { "right" } else { "left" };
            let parent = pg.parent.and_then(|p| doc.parent(p)).map(|p| p.prefix.clone());
            pages.push(json!({
                "index": pi, "abs": abs, "name": doc.page_name(abs), "bounds": rect(Rect::new(x, 0.0, x + pg.width, pg.height)),
                "parent": parent, "side": side, "columns": pg.columns, "gutter": pg.gutter,
                "margins": {"top": pg.margins.top, "bottom": pg.margins.bottom, "inside": pg.margins.inside, "outside": pg.margins.outside},
            }));
            abs += 1;
        }
        let items: Vec<Value> = s.items.iter().filter_map(|i| item_json(doc, *i, &over)).collect();
        spreads.push(json!({"index": si, "pages": pages, "items": items}));
    }
    let parents: Vec<Value> = doc
        .parents
        .iter()
        .enumerate()
        .map(|(i, p)| json!({"id": i, "index": i, "items": p.items.len(), "label": p.label(), "prefix": p.prefix, "name": p.name, "pages": p.pages.len(), "basedOn": p.based_on.and_then(|b| doc.parent(b)).map(|b| b.prefix.clone())}))
        .collect();
    let stories: Vec<Value> = doc
        .stories
        .values()
        .map(|s| {
            let text = s.text();
            let preview: String = s.plain().chars().filter(|c| *c != '\u{FFFC}').take(80).collect();
            json!({"id": s.id, "preview": preview, "frames": s.frames, "length": text.len(), "paragraphs": s.paras.len(), "overset": lay.overset(s.id)})
        })
        .collect();
    let layers: Vec<Value> = doc.layers.iter().map(|l| json!({"id": l.id, "name": l.name, "color": wire::rgb255(&l.color), "visible": l.visible, "locked": l.locked, "printable": l.printable})).collect();
    let names = |k: StyleKind| doc.styles.list(k).iter().map(|s| wire::style_name(k, &s.name)).collect::<Vec<_>>();
    let mut swatches = vec!["[None]".to_string()];
    swatches.extend(doc.swatches.iter().map(|s| wire::swatch_name(&s.name)));
    let s = &doc.settings;
    Ok(json!({
        "title": st.title(), "dirty": st.is_dirty(), "pageCount": doc.page_count(),
        "settings": {"facingPages": s.facing, "pageWidth": s.page_width, "pageHeight": s.page_height, "intent": s.intent, "columns": s.columns, "gutter": s.gutter,
                     "margins": {"top": s.margins.top, "bottom": s.margins.bottom, "inside": s.margins.inside, "outside": s.margins.outside},
                     "bleed": [s.bleed.top, s.bleed.bottom, s.bleed.inside, s.bleed.outside]},
        "activeLayer": active_layer, "layers": layers, "parents": parents, "spreads": spreads, "stories": stories,
        "paragraphStyles": names(StyleKind::Paragraph), "characterStyles": names(StyleKind::Character), "objectStyles": names(StyleKind::Object),
        "tableStyles": names(StyleKind::Table), "cellStyles": names(StyleKind::Cell),
        "swatches": swatches, "selection": st.selection,
    }))
}

const UNITS: &[&str] = &["points", "picas", "inches", "inchesDecimal", "millimeters", "centimeters", "ciceros", "agates", "pixels", "q", "ha"];

fn preferences(c: &mut Ctx, p: &Value) -> Result<Value> {
    let h = args::opt_str(p, "horizontalUnits");
    let v = args::opt_str(p, "verticalUnits");
    let grid = p.get("baselineGrid").and_then(Value::as_object);
    let smart = args::opt_bool(p, "smartTextReflow");
    for u in [h, v].into_iter().flatten() {
        if !UNITS.contains(&u) {
            return Err(EngineError::new(format!("units expected one of {}", UNITS.join(", "))));
        }
    }
    if h.is_none() && v.is_none() && grid.is_none() && smart.is_none() {
        let doc = c.doc()?;
        let pr = |k: &str| doc.prefs.get(k).cloned().unwrap_or(json!("points"));
        let g = &doc.settings.baseline;
        return Ok(json!({"horizontalUnits": pr("horizontalUnits"), "verticalUnits": pr("verticalUnits"), "baselineGrid": {"start": g.start, "increment": g.increment, "color": g.color, "relativeToMargin": g.from_margin}, "smartTextReflow": doc.settings.smart_reflow}));
    }
    let doc = c.doc_mut()?;
    if let Some(u) = h {
        doc.prefs.insert("horizontalUnits".into(), json!(u));
    }
    if let Some(u) = v {
        doc.prefs.insert("verticalUnits".into(), json!(u));
    }
    if let Some(g) = grid {
        if let Some(s) = g.get("start").and_then(Value::as_f64) {
            doc.settings.baseline.start = s;
        }
        if let Some(i) = g.get("increment").and_then(Value::as_f64) {
            doc.settings.baseline.increment = i.max(0.5);
        }
        if let Some(col) = g.get("color").and_then(wire::hex_of) {
            doc.settings.baseline.color = col;
        }
        if let Some(m) = g.get("relativeToMargin").and_then(Value::as_bool) {
            doc.settings.baseline.from_margin = m;
        }
    }
    if let Some(s) = smart {
        doc.settings.smart_reflow = s;
    }
    c.label("Preferences");
    ok()
}

fn edges(v: &Value) -> Option<Edges> {
    match v {
        Value::Number(n) => n.as_f64().map(Edges::all),
        Value::Array(a) => {
            let n: Vec<f64> = a.iter().filter_map(Value::as_f64).collect();
            (n.len() == 4).then(|| Edges { top: n[0], bottom: n[1], inside: n[2], outside: n[3] })
        }
        Value::Object(o) => {
            let g = |k: &str| o.get(k).and_then(Value::as_f64).unwrap_or(0.0);
            Some(Edges { top: g("top"), bottom: g("bottom"), inside: g("inside").max(g("left")), outside: g("outside").max(g("right")) })
        }
        _ => None,
    }
}

fn parent_id(doc: &Doc, v: Option<&Value>, fallback: Option<ParentId>) -> Result<Option<ParentId>> {
    match v {
        None => Ok(fallback),
        Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.eq_ignore_ascii_case("none") || s == "[None]" || s.is_empty() => Ok(None),
        Some(Value::String(s)) => doc.parent_by_label(s).map(|p| Some(p.id)).ok_or_else(|| EngineError::new(format!("no parent named {s}"))),
        Some(v) => {
            let i = v.as_u64().ok_or_else(|| EngineError::new("`parent` expected a parent prefix such as \"A\", or null"))? as usize;
            doc.parents.get(i).map(|p| Some(p.id)).ok_or_else(|| EngineError::new(format!("no parent {i}")))
        }
    }
}

pub fn insert_pages(doc: &mut Doc, at: usize, count: usize, parent: Option<ParentId>) {
    doc.repaginate(|pages, carried, d| {
        let at = at.min(pages.len());
        let mut fresh = Vec::new();
        for _ in 0..count {
            let mut pg = d.blank_page();
            pg.parent = parent;
            fresh.push(pg);
        }
        pages.splice(at..at, fresh);
        for c in carried.iter_mut() {
            if c.1 >= at {
                c.1 += count;
            }
        }
    });
    for s in doc.sections.iter_mut() {
        if s.start >= at && s.start > 0 {
            s.start += count;
        }
    }
}

fn delete_pages(doc: &mut Doc, set: &BTreeSet<usize>) {
    let mut doomed = Vec::new();
    doc.repaginate(|pages, carried, _| {
        let old = std::mem::take(pages);
        for (i, p) in old.into_iter().enumerate() {
            if !set.contains(&i) {
                pages.push(p);
            }
        }
        carried.retain(|c| {
            if set.contains(&c.1) {
                doomed.push(c.0);
                false
            } else {
                true
            }
        });
        for c in carried.iter_mut() {
            c.1 -= set.range(..c.1).count();
        }
    });
    for id in doomed {
        if let Some(it) = doc.items.get(&id).cloned() {
            for k in it.all_children() {
                doc.remove_item(k);
            }
            if let Some(sid) = it.story() {
                let mut drop = false;
                if let Some(st) = doc.story_mut(sid) {
                    st.frames.retain(|f| *f != id);
                    drop = st.frames.is_empty();
                }
                if drop {
                    doc.stories.remove(&sid);
                }
            }
            doc.items.remove(&id);
        }
    }
    let n = doc.page_count();
    doc.sections.retain(|s| s.start == 0 || !set.contains(&s.start));
    for s in doc.sections.iter_mut() {
        s.start -= set.range(..s.start).count();
        s.start = s.start.min(n.saturating_sub(1));
    }
    doc.sections.dedup_by_key(|s| s.start);
}

fn document_setup(c: &mut Ctx, p: &Value) -> Result<Value> {
    let keys = ["width", "height", "pages", "startPage", "facingPages", "intent", "bleed", "slug", "primaryTextFrame"];
    if !keys.iter().any(|k| p.get(*k).is_some()) {
        let doc = c.doc()?;
        let s = &doc.settings;
        return Ok(json!({
            "width": s.page_width, "height": s.page_height, "pages": doc.page_count(), "startPage": doc.page_number(0), "facingPages": s.facing,
            "intent": s.intent, "bleed": [s.bleed.top, s.bleed.bottom, s.bleed.inside, s.bleed.outside], "slug": [s.slug.top, s.slug.bottom, s.slug.inside, s.slug.outside],
            "binding": "leftToRight", "primaryTextFrame": s.primary_text_frame,
        }));
    }
    let width = args::opt_f64(p, "width");
    let height = args::opt_f64(p, "height");
    if width.is_some_and(|w| w < 1.0) || height.is_some_and(|h| h < 1.0) {
        return Err(EngineError::new("`width` and `height` must be at least 1 pt"));
    }
    let adjust = args::opt_bool(p, "adjustLayout").unwrap_or(false);
    let doc = c.doc_mut()?;
    if let Some(i) = args::opt_str(p, "intent") {
        doc.settings.intent = match i {
            "web" => Intent::Web,
            "mobile" | "digital" => Intent::Mobile,
            "print" => Intent::Print,
            other => return Err(EngineError::new(format!("`intent` expected print, web or mobile, not {other}"))),
        };
    }
    if let Some(e) = p.get("bleed").and_then(edges) {
        doc.settings.bleed = e;
    }
    if let Some(e) = p.get("slug").and_then(edges) {
        doc.settings.slug = e;
    }
    if let Some(b) = args::opt_bool(p, "primaryTextFrame") {
        doc.settings.primary_text_frame = b;
    }
    if width.is_some() || height.is_some() {
        let (ow, oh) = (doc.settings.page_width, doc.settings.page_height);
        let (nw, nh) = (width.unwrap_or(ow), height.unwrap_or(oh));
        let (sx, sy) = (nw / ow, nh / oh);
        doc.settings.page_width = nw;
        doc.settings.page_height = nh;
        for par in doc.parents.iter_mut() {
            for pg in par.pages.iter_mut() {
                pg.width = nw;
                pg.height = nh;
            }
        }
        if adjust {
            let pids: Vec<ItemId> = doc.parents.iter().flat_map(|p| p.items.clone()).collect();
            for id in pids {
                if let Some(it) = doc.item_mut(id) {
                    it.xf = Affine::scale_non_uniform(sx, sy) * it.xf;
                }
            }
        }
        doc.repaginate(|pages, carried, _| {
            for pg in pages.iter_mut() {
                pg.width = nw;
                pg.height = nh;
                if adjust {
                    pg.margins = Edges { top: pg.margins.top * sy, bottom: pg.margins.bottom * sy, inside: pg.margins.inside * sx, outside: pg.margins.outside * sx };
                }
            }
            let _ = carried;
        });
        if adjust {
            for abs in 0..doc.page_count() {
                crate::cmd::prod::adapt_page(doc, abs, (ow, oh), "guide");
            }
        }
    }
    if let Some(f) = args::opt_bool(p, "facingPages")
        && f != doc.settings.facing
    {
        doc.settings.facing = f;
        doc.repaginate(|_, _, _| {});
    }
    if let Some(n) = args::opt_u64(p, "pages") {
        let n = (n as usize).clamp(1, 9999);
        let have = doc.page_count();
        if n > have {
            let parent = doc.page(have - 1).and_then(|pg| pg.parent);
            insert_pages(doc, have, n - have, parent);
        } else if n < have {
            delete_pages(doc, &(n..have).collect());
        }
    }
    if let Some(sp) = args::opt_u64(p, "startPage") {
        let sp = sp.max(1) as u32;
        match doc.sections.iter_mut().find(|s| s.start == 0) {
            Some(s) => s.start_at = Some(sp),
            None => doc.sections.insert(0, Section { start: 0, start_at: Some(sp), style: "arabic".into(), prefix: String::new(), marker: String::new(), include_prefix: false }),
        }
        let left = sp.is_multiple_of(2);
        if doc.settings.facing && doc.settings.start_left != left {
            doc.settings.start_left = left;
            doc.repaginate(|_, _, _| {});
        }
    }
    c.label("Document Setup");
    ok()
}

fn margins_columns(c: &mut Ctx, p: &Value) -> Result<Value> {
    let margins = p.get("margins").and_then(edges);
    let columns = args::opt_u64(p, "columns").map(|n| n.clamp(1, 216) as u32);
    let gutter = args::opt_f64(p, "gutter").map(|g| g.max(0.0));
    if margins.is_none() && columns.is_none() && gutter.is_none() {
        let s = &c.doc()?.settings;
        return Err(EngineError::new(format!(
            "missing `margins`, `columns` or `gutter` (now margins {{top: {}, bottom: {}, inside: {}, outside: {}}}, columns {}, gutter {})",
            s.margins.top, s.margins.bottom, s.margins.inside, s.margins.outside, s.columns, s.gutter
        )));
    }
    let only: Option<BTreeSet<usize>> = p.get("pages").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).map(|n| n as usize).collect());
    let adjust = args::opt_bool(p, "adjustLayout").unwrap_or(false) && margins.is_some();
    let doc = c.doc_mut()?;
    let before: Vec<Option<formdes_model::geom::Rect>> = (0..doc.page_count()).map(|abs| crate::cmd::object::margin_rect(doc, abs)).collect();
    if only.is_none() {
        if let Some(m) = margins {
            doc.settings.margins = m;
        }
        if let Some(n) = columns {
            doc.settings.columns = n;
        }
        if let Some(g) = gutter {
            doc.settings.gutter = g;
        }
    }
    for abs in 0..doc.page_count() {
        if only.as_ref().is_some_and(|o| !o.contains(&abs)) {
            continue;
        }
        if let Some(pg) = doc.page_mut(abs) {
            if let Some(m) = margins {
                pg.margins = m;
            }
            if let Some(n) = columns {
                pg.columns = n;
            }
            if let Some(g) = gutter {
                pg.gutter = g;
            }
        }
    }
    if only.is_none() {
        for par in doc.parents.iter_mut() {
            for pg in par.pages.iter_mut() {
                if let Some(m) = margins {
                    pg.margins = m;
                }
                if let Some(n) = columns {
                    pg.columns = n;
                }
                if let Some(g) = gutter {
                    pg.gutter = g;
                }
            }
        }
    }
    if adjust {
        for (abs, old) in before.iter().enumerate() {
            let (Some(old), Some(new)) = (*old, crate::cmd::object::margin_rect(doc, abs)) else { continue };
            if old == new {
                continue;
            }
            let Some((si, _)) = doc.page_slot(abs) else { continue };
            let ids: Vec<ItemId> = doc.spreads[si].items.iter().copied().filter(|i| doc.page_of(*i) == Some(abs)).collect();
            let m = formdes_model::item::map_rect(old, new);
            for id in ids {
                let Some(b) = doc.bounds(id) else { continue };
                let nb = m.transform_rect_bbox(b);
                crate::cmd::object::resize_items(doc, &[id], formdes_model::item::map_rect(b, nb), true);
            }
        }
    }
    ok()
}

fn page_size(c: &mut Ctx, p: &Value) -> Result<Value> {
    let pages: Vec<usize> = p.get("pages").and_then(Value::as_array).ok_or_else(|| EngineError::new("missing `pages` (1-based page numbers)"))?.iter().filter_map(Value::as_u64).map(|n| n as usize).collect();
    let w = args::f64_(p, "width", "page width in points")?.max(1.0);
    let h = args::f64_(p, "height", "page height in points")?.max(1.0);
    let doc = c.doc_mut()?;
    let n = doc.page_count();
    if let Some(bad) = pages.iter().find(|x| **x == 0 || **x > n) {
        return Err(EngineError::new(format!("no page {bad} (pages are numbered 1 to {n})")));
    }
    let set: BTreeSet<usize> = pages.iter().map(|x| x - 1).collect();
    doc.repaginate(|pgs, _, _| {
        for (i, pg) in pgs.iter_mut().enumerate() {
            if set.contains(&i) {
                pg.width = w;
                pg.height = h;
            }
        }
    });
    ok()
}

fn pages_insert(c: &mut Ctx, p: &Value) -> Result<Value> {
    let count = args::opt_u64(p, "count").unwrap_or(1).clamp(1, 500) as usize;
    let doc = c.doc()?;
    let n = doc.page_count();
    let after = match p.get("after").and_then(Value::as_f64) {
        Some(a) if a < 0.0 => -1i64,
        Some(a) => (a as i64).min(n as i64 - 1),
        None => n as i64 - 1,
    };
    let at = (after + 1) as usize;
    let fallback = doc.page(after.max(0) as usize).and_then(|pg| pg.parent);
    let parent = parent_id(doc, p.get("parent"), fallback)?;
    insert_pages(c.doc_mut()?, at, count, parent);
    c.label(if count == 1 { "Insert Page" } else { "Insert Pages" });
    Ok(json!({"pages": (at..at + count).collect::<Vec<_>>()}))
}

fn pages_delete(c: &mut Ctx, p: &Value) -> Result<Value> {
    let set: BTreeSet<usize> = p.get("pages").and_then(Value::as_array).ok_or_else(|| EngineError::new("missing `pages` (0-based page indices)"))?.iter().filter_map(Value::as_u64).map(|n| n as usize).collect();
    let doc = c.doc()?;
    let n = doc.page_count();
    if let Some(bad) = set.iter().find(|x| **x >= n) {
        return Err(EngineError::new(format!("no page index {bad} (the document has {n} pages, indices are 0-based)")));
    }
    if set.len() >= n {
        return Err(EngineError::new("a document keeps at least one page"));
    }
    delete_pages(c.doc_mut()?, &set);
    c.set_selection(Selection::default());
    c.label(if set.len() == 1 { "Delete Page" } else { "Delete Pages" });
    ok()
}

fn pages_move(c: &mut Ctx, p: &Value) -> Result<Value> {
    let from = args::u64_(p, "from", "0-based index of the page to move")? as usize;
    let to = args::u64_(p, "to", "0-based index after removal")? as usize;
    let n = c.doc()?.page_count();
    if from >= n {
        return Err(EngineError::new(format!("no page index {from}")));
    }
    let to = to.min(n - 1);
    if from == to {
        return ok();
    }
    let map = |i: usize| -> usize {
        if i == from {
            to
        } else {
            let j = if i > from { i - 1 } else { i };
            if j >= to { j + 1 } else { j }
        }
    };
    c.doc_mut()?.repaginate(|pages, carried, _| {
        let pg = pages.remove(from);
        pages.insert(to, pg);
        for c in carried.iter_mut() {
            c.1 = map(c.1);
        }
    });
    c.label("Move Pages");
    ok()
}

fn duplicate_spread(c: &mut Ctx, p: &Value) -> Result<Value> {
    let si = args::u64_(p, "spread", "the spread index, from 0")? as usize;
    let doc = c.doc_mut()?;
    let spread = doc.spreads.get(si).cloned().ok_or_else(|| EngineError::new(format!("no spread {si}")))?;
    let first = doc.first_page_of_spread(si);
    let np = spread.pages.len();
    let copies: Vec<ItemId> = spread.items.iter().filter_map(|id| ops::duplicate(doc, *id, Vec2::ZERO, Some(Home::Spread(si)))).collect();
    doc.repaginate(|pages, carried, d| {
        let fresh: Vec<_> = spread.pages.iter().map(|pg| {
            let mut x = d.blank_page();
            let id = x.id;
            x = pg.clone();
            x.id = id;
            x
        }).collect();
        let at = first + np;
        pages.splice(at..at, fresh);
        for c in carried.iter_mut() {
            if copies.contains(&c.0) || c.1 >= at {
                c.1 += np;
            }
        }
    });
    ok()
}

/// A document to take pages from: an open one by its index (as `docs` lists them) or a .fdes, .fdoc or .idml file.
fn source_doc(c: &Ctx, from: &Value) -> Result<(std::sync::Arc<Doc>, Option<usize>)> {
    if let Some(i) = from.as_u64() {
        let st = c.session.docs.get(i as usize).ok_or_else(|| EngineError::new(format!("no open document {i} (docs lists them from 0)")))?;
        return Ok((st.doc.clone(), Some(i as usize)));
    }
    let path = from.as_str().ok_or_else(|| EngineError::new("`from` is an open document's index or the path of a .fdes, .fdoc or .idml file"))?;
    let data = std::fs::read(path).map_err(|e| EngineError::new(format!("cannot read {path}: {e}")))?;
    let doc = if crate::cmd::file::is_idml(&data) {
        formdes_interchange::idml::read::read(&data).map_err(|e| EngineError::new(format!("{path}: {e}")))?
    } else if data.starts_with(b"PK") {
        // A .fdes is a zip holding the layout as document.fdoc beside the content it was made from.
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(&data)).map_err(|e| EngineError::new(format!("{path}: {e}")))?;
        let mut inner = Vec::new();
        let mut f = z.by_name("document.fdoc").map_err(|_| EngineError::new(format!("{path} is a zip without document.fdoc, so not a .fdes document")))?;
        std::io::Read::read_to_end(&mut f, &mut inner).map_err(|e| EngineError::new(format!("{path}: {e}")))?;
        formdes_model::format::load(&inner).map_err(|e| EngineError::new(format!("{path}: {}", e.0)))?
    } else {
        formdes_model::format::load(&data).map_err(|e| EngineError::new(format!("{path} is not a FormDesign (.fdes, .fdoc) or IDML document ({})", e.0)))?
    };
    Ok((std::sync::Arc::new(doc), None))
}

/// The target's parent for source parent `pid`: one with the same label is reused; otherwise the parent is copied
/// (its base first, its items through the mapper) under its own prefix, or the next free letter when that is taken.
fn import_parent(src: &Doc, dst: &mut Doc, m: &mut crate::transplant::Mapper, map: &mut std::collections::HashMap<ParentId, ParentId>, added: &mut Vec<String>, pid: ParentId, depth: usize) -> Option<ParentId> {
    if let Some(t) = map.get(&pid) {
        return Some(*t);
    }
    let sp = src.parent(pid)?.clone();
    if let Some(d) = dst.parent_by_label(&sp.label()) {
        map.insert(pid, d.id);
        return Some(d.id);
    }
    let base = if depth < 16 { sp.based_on.and_then(|b| import_parent(src, dst, m, map, added, b, depth + 1)) } else { None };
    let prefix = if dst.parents.iter().any(|x| x.prefix == sp.prefix) {
        (b'A'..=b'Z').map(|b| (b as char).to_string()).find(|l| !dst.parents.iter().any(|x| x.prefix == *l)).unwrap_or_else(|| format!("P{}", dst.parents.len()))
    } else {
        sp.prefix.clone()
    };
    let pages = sp.pages.iter().map(|pg| {
        let mut x = pg.clone();
        x.id = formdes_model::ids::PageId(dst.alloc());
        x.parent = None;
        x.detached.clear();
        x
    }).collect();
    let id = ParentId(dst.alloc());
    dst.parents.push(Parent { id, prefix: prefix.clone(), name: sp.name.clone(), based_on: base, pages, items: Vec::new(), guides: sp.guides.clone() });
    let pi = dst.parents.len() - 1;
    map.insert(pid, id);
    added.push(format!("{prefix}-{}", sp.name));
    for it in &sp.items {
        m.item(dst, *it, Home::Parent(pi), Vec2::ZERO);
    }
    Some(id)
}

/// Layout > Pages > Insert Pages from Document (InDesign's Move Pages to another document, Affinity's Merge
/// Document): copies pages of an open document or a file in after a page of this one, with their items, stories,
/// pictures, styles, swatches, layers, parents and section starts; `move` also takes them out of an open source.
fn pages_import(c: &mut Ctx, p: &Value) -> Result<Value> {
    use std::collections::HashMap;
    let from = p.get("from").filter(|v| !v.is_null()).ok_or_else(|| EngineError::new("missing `from`: an open document's index (docs lists them from 0) or the path of a .fdes, .fdoc or .idml file"))?;
    let (src, src_index) = source_doc(c, from)?;
    if src_index.is_some() && src_index == c.session.active {
        return Err(EngineError::new("`from` is this document: Move Pages and Duplicate Spread rearrange its own pages"));
    }
    let picked = crate::cmd::file::page_list(&src, args::opt_str(p, "pages"))?;
    let moving = args::opt_bool(p, "move").unwrap_or(false);
    if moving && src_index.is_none() {
        return Err(EngineError::new("`move` takes pages out of an open document; a file is never changed, so leave move off to copy from it"));
    }
    if moving && picked.len() >= src.page_count() {
        return Err(EngineError::new("moving every page would leave the source document empty: copy them instead (move off) or leave it one page"));
    }
    let n = c.doc()?.page_count();
    let after = match p.get("after").and_then(Value::as_f64) {
        Some(a) if a < 0.0 => -1i64,
        Some(a) => (a as i64).min(n as i64 - 1),
        None => n as i64 - 1,
    };
    let at = (after + 1) as usize;
    // The source items on the picked pages, by the rule repaginate uses (the page under the item's centre), with the
    // shift that makes their transform page-local.
    let mut on: Vec<(ItemId, usize, Vec2)> = Vec::new();
    let mut abs = 0;
    for s in &src.spreads {
        for id in &s.items {
            let Some(it) = src.item(*id) else { continue };
            let pi = s.nearest_page(it.own_bounds().center().x);
            if let Some(k) = picked.iter().position(|q| *q == abs + pi) {
                on.push((*id, k, Vec2::new(-s.page_x(pi), 0.0)));
            }
        }
        abs += s.pages.len();
    }
    let mut mapper = crate::transplant::Mapper::new(&src);
    let dst = c.doc_mut()?;
    // Layers the carried items sit on are made by name when this document lacks them, so nothing lands on the wrong layer.
    let mut wanted: Vec<LayerId> = on.iter().filter_map(|(id, _, _)| src.item(*id).map(|i| i.layer)).collect();
    for pg in picked.iter().filter_map(|a| src.page(*a)) {
        if let Some(par) = pg.parent.and_then(|q| src.parent(q)) {
            wanted.extend(par.items.iter().filter_map(|i| src.item(*i).map(|x| x.layer)));
        }
    }
    for l in src.layers.iter().filter(|l| wanted.contains(&l.id)) {
        if !dst.layers.iter().any(|d| d.name == l.name) {
            let mut nl = l.clone();
            nl.id = LayerId(dst.alloc());
            dst.layers.push(nl);
        }
    }
    let mut pmap: HashMap<ParentId, ParentId> = HashMap::new();
    let mut added: Vec<String> = Vec::new();
    for a in &picked {
        if let Some(q) = src.page(*a).and_then(|pg| pg.parent) {
            import_parent(&src, dst, &mut mapper, &mut pmap, &mut added, q, 0);
        }
    }
    let k = picked.len();
    let mut carried_ids = Vec::new();
    dst.repaginate(|pages, carried, d| {
        let at = at.min(pages.len());
        let fresh: Vec<formdes_model::page::Page> = picked.iter().map(|a| {
            let mut x = src.page(*a).cloned().unwrap_or_else(|| d.blank_page());
            x.id = formdes_model::ids::PageId(d.alloc());
            x.parent = x.parent.and_then(|q| pmap.get(&q).copied());
            x.detached.clear();
            x.joined = false;
            x
        }).collect();
        pages.splice(at..at, fresh);
        for c in carried.iter_mut() {
            if c.1 >= at {
                c.1 += k;
            }
        }
        for (sid, ki, off) in &on {
            if let Some(nid) = mapper.item(d, *sid, Home::Spread(0), *off) {
                let xf = d.item(nid).map(|i| i.xf).unwrap_or(Affine::IDENTITY);
                carried.push((nid, at + ki, xf));
                carried_ids.push(nid);
            }
        }
    });
    for s in dst.sections.iter_mut() {
        if s.start >= at && s.start > 0 {
            s.start += k;
        }
    }
    for s in &src.sections {
        if let Some(i) = picked.iter().position(|a| *a == s.start) {
            let t = at + i;
            if !dst.sections.iter().any(|x| x.start == t) {
                let mut ns = s.clone();
                ns.start = t;
                dst.sections.push(ns);
            }
        }
    }
    dst.sections.sort_by_key(|s| s.start);
    // Frames of one source story were given a story each; thread them back into one, in the source's frame order,
    // and drop the copies (with any objects anchored in them).
    let mut by_story: HashMap<formdes_model::ids::StoryId, Vec<(usize, ItemId)>> = HashMap::new();
    for (sf, nf) in &mapper.frames {
        if let Some(sid) = src.item(*sf).and_then(|i| i.story()) {
            let ord = src.story(sid).and_then(|s| s.frames.iter().position(|f| f == sf)).unwrap_or(usize::MAX);
            by_story.entry(sid).or_default().push((ord, *nf));
        }
    }
    for (_, mut fs) in by_story {
        if fs.len() < 2 {
            continue;
        }
        fs.sort();
        let Some(keep) = dst.item(fs[0].1).and_then(|i| i.story()) else { continue };
        for (_, f) in &fs[1..] {
            let old = dst.item(*f).and_then(|i| i.story());
            if let Some(it) = dst.item_mut(*f)
                && let ItemKind::Text(tf) = &mut it.kind
            {
                tf.story = keep;
            }
            if let Some(o) = old.filter(|o| *o != keep) {
                let orphans: Vec<ItemId> = dst.items.values().filter(|i| i.anchored_in == Some(o)).map(|i| i.id).collect();
                for x in orphans {
                    dst.remove_item(x);
                }
                dst.stories.remove(&o);
            }
        }
        if let Some(st) = dst.story_mut(keep) {
            st.frames = fs.iter().map(|x| x.1).collect();
        }
    }
    if moving && let Some(si) = src_index {
        let set: BTreeSet<usize> = picked.iter().copied().collect();
        if let Some(st) = c.session.docs.get_mut(si) {
            let before = st.doc.clone();
            let sel = st.selection.clone();
            delete_pages(std::sync::Arc::make_mut(&mut st.doc), &set);
            st.selection = Selection::default();
            st.record("Move Pages".into(), before, sel, None);
            st.revision += 1;
        }
    }
    c.set_selection(Selection::items(carried_ids.clone()));
    c.label(if moving { "Move Pages" } else if k == 1 { "Insert Page" } else { "Insert Pages" });
    Ok(json!({"pages": (at..at + k).collect::<Vec<_>>(), "items": carried_ids.len(), "parentsAdded": added, "moved": moving}))
}

fn apply_parent(c: &mut Ctx, p: &Value) -> Result<Value> {
    let pages: Vec<usize> = p.get("pages").and_then(Value::as_array).ok_or_else(|| EngineError::new("missing `pages` (0-based page indices)"))?.iter().filter_map(Value::as_u64).map(|n| n as usize).collect();
    let doc = c.doc()?;
    let parent = parent_id(doc, Some(p.get("parent").unwrap_or(&Value::Null)), None)?;
    let n = doc.page_count();
    let doc = c.doc_mut()?;
    for abs in pages {
        if abs >= n {
            return Err(EngineError::new(format!("no page index {abs}")));
        }
        if let Some(pg) = doc.page_mut(abs) {
            pg.parent = parent;
            pg.detached.clear();
        }
    }
    ok()
}

fn parents_new(c: &mut Ctx, p: &Value) -> Result<Value> {
    let doc = c.doc_mut()?;
    let prefix = match args::opt_str(p, "prefix").map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => s.chars().take(4).collect::<String>(),
        None => (b'A'..=b'Z').map(|b| (b as char).to_string()).find(|l| !doc.parents.iter().any(|p| p.prefix == *l)).unwrap_or_else(|| format!("P{}", doc.parents.len())),
    };
    if doc.parents.iter().any(|x| x.prefix == prefix) {
        return Err(EngineError::new(format!("there is already a parent with prefix {prefix}")));
    }
    let name = args::opt_str(p, "name").unwrap_or("Parent").to_string();
    let based = match p.get("basedOn") {
        Some(v) if !v.is_null() => parent_id(doc, Some(v), None)?,
        _ => None,
    };
    let n = if doc.settings.facing { 2 } else { 1 };
    let pages = (0..n).map(|_| doc.blank_page()).collect();
    let id = ParentId(doc.alloc());
    doc.parents.push(Parent { id, prefix: prefix.clone(), name: name.clone(), based_on: based, pages, items: Vec::new(), guides: Vec::new() });
    Ok(json!({"index": doc.parents.len() - 1, "label": format!("{prefix}-{name}")}))
}

fn parents_options(c: &mut Ctx, p: &Value) -> Result<Value> {
    let doc = c.doc_mut()?;
    let id = parent_id(doc, p.get("parent"), None)?.ok_or_else(|| EngineError::new("missing `parent` (a parent prefix such as \"A\")"))?;
    let based = match p.get("basedOn") {
        None => doc.parent(id).and_then(|x| x.based_on),
        Some(v) => parent_id(doc, Some(v), None)?,
    };
    let mut cur = based;
    let mut guard = 0;
    while let Some(b) = cur {
        if b == id {
            return Err(EngineError::new("a parent cannot be based on itself, directly or through another parent"));
        }
        guard += 1;
        cur = if guard < 16 { doc.parent(b).and_then(|x| x.based_on) } else { None };
    }
    let prefix = args::opt_str(p, "prefix").map(str::trim).filter(|s| !s.is_empty()).map(|s| s.chars().take(4).collect::<String>());
    if let Some(pf) = &prefix
        && doc.parents.iter().any(|x| x.prefix == *pf && x.id != id)
    {
        return Err(EngineError::new(format!("there is already a parent with prefix {pf}")));
    }
    let par = doc.parents.iter_mut().find(|x| x.id == id).ok_or_else(|| EngineError::new("no such parent"))?;
    if let Some(pf) = prefix {
        par.prefix = pf;
    }
    if let Some(n) = args::opt_str(p, "name").map(str::trim).filter(|s| !s.is_empty()) {
        par.name = n.to_string();
    }
    par.based_on = based;
    let label = format!("{}-{}", par.prefix, par.name);
    let base = based.and_then(|b| doc.parent(b)).map(|b| b.prefix.clone());
    c.label("Parent Page Options");
    Ok(json!({"label": label, "basedOn": base}))
}

fn parents_edit(c: &mut Ctx, p: &Value) -> Result<Value> {
    let st = c.state_mut()?;
    st.editing_parents = args::opt_bool(p, "on").unwrap_or(!st.editing_parents);
    let on = st.editing_parents;
    st.revision += 1;
    c.set_selection(Selection::default());
    Ok(json!({"on": on}))
}

fn section(c: &mut Ctx, p: &Value) -> Result<Value> {
    let page = args::u64_(p, "page", "a 1-based page number")? as usize;
    let n = c.doc()?.page_count();
    if page == 0 || page > n {
        return Err(EngineError::new(format!("no page {page} (pages are numbered 1 to {n})")));
    }
    let abs = page - 1;
    let doc = c.doc_mut()?;
    if args::opt_bool(p, "remove") == Some(true) {
        if abs == 0 {
            if let Some(s) = doc.sections.iter_mut().find(|s| s.start == 0) {
                *s = Section { start: 0, start_at: Some(1), style: "arabic".into(), prefix: String::new(), marker: String::new(), include_prefix: false };
            }
        } else {
            doc.sections.retain(|s| s.start != abs);
        }
        return ok();
    }
    let style = match args::opt_str(p, "style").unwrap_or("arabic") {
        "upperRoman" | "ROMAN" => "ROMAN",
        "lowerRoman" | "roman" => "roman",
        "upperLetters" | "ALPHA" => "ALPHA",
        "lowerLetters" | "alpha" => "alpha",
        _ => "arabic",
    };
    let start_at = match p.get("startNumber") {
        Some(Value::Null) => None,
        Some(v) => v.as_u64().map(|n| n.max(1) as u32),
        None => doc.sections.iter().find(|s| s.start == abs).and_then(|s| s.start_at),
    };
    let sec = Section {
        start: abs,
        start_at: if abs == 0 { start_at.or(Some(1)) } else { start_at },
        style: style.into(),
        prefix: args::opt_str(p, "prefix").unwrap_or("").to_string(),
        marker: args::opt_str(p, "marker").unwrap_or("").to_string(),
        include_prefix: args::opt_bool(p, "includePrefix").unwrap_or(false),
    };
    match doc.sections.iter_mut().find(|s| s.start == abs) {
        Some(s) => *s = sec,
        None => {
            doc.sections.push(sec);
            doc.sections.sort_by_key(|s| s.start);
        }
    }
    c.label("Numbering & Section Options");
    ok()
}

fn parent_items_on(doc: &Doc, abs: usize) -> Vec<(ItemId, f64)> {
    let Some(pg) = doc.page(abs) else { return Vec::new() };
    let Some(pid) = pg.parent else { return Vec::new() };
    let right = doc.settings.facing && doc.is_right_page(abs);
    let mut out = Vec::new();
    let mut cur = doc.parent(pid);
    let mut guard = 0;
    while let Some(par) = cur {
        let slot = if right { par.pages.len().saturating_sub(1) } else { 0 };
        let px = par.page_x(slot);
        if let Some(pp) = par.pages.get(slot) {
            let prect = Rect::new(px, 0.0, px + pp.width, pp.height);
            for id in &par.items {
                if doc.bounds(*id).is_some_and(|b| b.intersect(prect).area() > 0.0) {
                    out.push((*id, px));
                }
            }
        }
        guard += 1;
        cur = if guard < 8 { par.based_on.and_then(|b| doc.parent(b)) } else { None };
    }
    out
}

fn override_all(c: &mut Ctx, p: &Value) -> Result<Value> {
    let abs = args::u64_(p, "page", "a 0-based page index")? as usize;
    let doc = c.doc_mut()?;
    let (si, pi) = doc.page_slot(abs).ok_or_else(|| EngineError::new(format!("no page index {abs}")))?;
    let x = doc.spreads[si].page_x(pi);
    let detached = doc.page(abs).map(|pg| pg.detached.clone()).unwrap_or_default();
    let mut made = Vec::new();
    for (id, px) in parent_items_on(doc, abs) {
        if detached.contains(&id) {
            continue;
        }
        let Some(nid) = ops::duplicate(doc, id, Vec2::ZERO, Some(Home::Spread(si))) else { continue };
        if let Some(it) = doc.item_mut(nid) {
            it.xf = Affine::translate((x - px, 0.0)) * it.xf;
            it.extra.insert("parentItem".into(), json!(id));
        }
        if let Some(pg) = doc.page_mut(abs) {
            pg.detached.push(id);
        }
        made.push(nid);
    }
    c.label("Override All Parent Page Items");
    Ok(json!({"ids": made}))
}

fn local_overrides(doc: &Doc, abs: usize) -> Vec<(ItemId, u64)> {
    let Some((si, _)) = doc.page_slot(abs) else { return Vec::new() };
    doc.spreads[si]
        .items
        .iter()
        .filter_map(|id| {
            let it = doc.item(*id)?;
            let src = it.extra.get("parentItem")?.as_u64()?;
            (item_page(doc, *id) == Some(abs)).then_some((*id, src))
        })
        .collect()
}

fn remove_overrides(c: &mut Ctx, p: &Value) -> Result<Value> {
    let abs = args::u64_(p, "page", "a 0-based page index")? as usize;
    let doc = c.doc_mut()?;
    for (id, src) in local_overrides(doc, abs) {
        doc.remove_item(id);
        if let Some(pg) = doc.page_mut(abs) {
            pg.detached.retain(|d| d.0 != src);
        }
    }
    c.label("Remove All Local Overrides");
    ok()
}

fn detach_all(c: &mut Ctx, p: &Value) -> Result<Value> {
    let abs = args::u64_(p, "page", "a 0-based page index")? as usize;
    let doc = c.doc_mut()?;
    for (id, _) in local_overrides(doc, abs) {
        if let Some(it) = doc.item_mut(id) {
            it.extra.remove("parentItem");
        }
    }
    c.label("Detach All Objects from Parent");
    ok()
}

fn guide_add(c: &mut Ctx, p: &Value) -> Result<Value> {
    let vertical = match args::str_(p, "orientation", "horizontal or vertical")? {
        "vertical" => true,
        "horizontal" => false,
        other => return Err(EngineError::new(format!("`orientation` expected horizontal or vertical, not {other}"))),
    };
    let pos = args::f64_(p, "position", "points in spread coordinates")?;
    let (parent, idx) = match p.get("spread") {
        Some(Value::Object(o)) => (o.get("kind").and_then(Value::as_str) == Some("parent"), o.get("index").and_then(Value::as_u64).unwrap_or(0) as usize),
        Some(v) => (false, v.as_u64().unwrap_or(0) as usize),
        None => {
            let sel = c.selection();
            (false, sel.items.first().and_then(|i| c.doc().ok()?.spread_of(*i)).unwrap_or(0))
        }
    };
    let spread_guide = args::opt_bool(p, "spreadGuide").unwrap_or(false);
    let at = args::opt_f64(p, "at");
    let doc = c.doc_mut()?;
    let page_list: Vec<f64> = if parent {
        doc.parents.get(idx).ok_or_else(|| EngineError::new(format!("no parent spread {idx}")))?.pages.iter().map(|x| x.width).collect()
    } else {
        doc.spreads.get(idx).ok_or_else(|| EngineError::new(format!("no spread {idx}")))?.pages.iter().map(|x| x.width).collect()
    };
    let probe = if vertical { Some(pos) } else { at };
    let page = if spread_guide {
        None
    } else {
        let mut acc = 0.0;
        let mut hit = None;
        if let Some(x) = probe {
            for (i, w) in page_list.iter().enumerate() {
                if x >= acc && x < acc + w {
                    hit = Some(i);
                }
                acc += w;
            }
        }
        hit.or(if probe.is_none() { Some(0) } else { None })
    };
    let g = Guide { orientation: if vertical { Orientation::Vertical } else { Orientation::Horizontal }, pos, page, color: "#4ae3ff".into(), locked: false };
    if parent {
        doc.parents[idx].guides.push(g);
    } else {
        doc.spreads[idx].guides.push(g);
    }
    c.label("New Guide");
    ok()
}

fn layer_id(p: &Value) -> Result<LayerId> {
    args::opt_u64(p, "id").or_else(|| args::opt_u64(p, "layer")).map(LayerId).ok_or_else(|| EngineError::new("missing `id` (a layer id)"))
}

fn need_layer(c: &Ctx, id: LayerId) -> Result<()> {
    if c.doc()?.layer(id).is_none() {
        return Err(EngineError::new(format!("no layer with id {id}")));
    }
    Ok(())
}

fn layer_new(c: &mut Ctx, p: &Value) -> Result<Value> {
    let active = c.target_layer()?;
    let doc = c.doc_mut()?;
    let n = doc.layers.len();
    let mut k = n + 1;
    while doc.layers.iter().any(|l| l.name == format!("Layer {k}")) {
        k += 1;
    }
    let id = LayerId(doc.alloc());
    let name = args::opt_str(p, "name").map(str::to_string).unwrap_or_else(|| format!("Layer {k}"));
    let color = p.get("color").and_then(wire::hex_of).unwrap_or_else(|| LAYER_COLORS[n % LAYER_COLORS.len()].to_string());
    let at = doc.layers.iter().position(|l| l.id == active).unwrap_or(0);
    doc.layers.insert(at, Layer::new(id, &name, &color));
    c.state_mut()?.active_layer = Some(id);
    Ok(json!({"id": id, "name": name}))
}

fn layer_set(c: &mut Ctx, p: &Value) -> Result<Value> {
    let id = layer_id(p)?;
    need_layer(c, id)?;
    let doc = c.doc_mut()?;
    let l = doc.layers.iter_mut().find(|l| l.id == id).ok_or_else(|| EngineError::new("no such layer"))?;
    if let Some(n) = args::opt_str(p, "name") {
        l.name = n.to_string();
    }
    if let Some(col) = p.get("color").and_then(wire::hex_of) {
        l.color = col;
    }
    if let Some(v) = args::opt_bool(p, "visible") {
        l.visible = v;
    }
    if let Some(v) = args::opt_bool(p, "locked") {
        l.locked = v;
    }
    if let Some(v) = args::opt_bool(p, "printable") {
        l.printable = v;
    }
    c.label("Layer Options");
    ok()
}

fn items_on_layer(doc: &Doc, id: LayerId) -> Vec<ItemId> {
    let tops = doc.spreads.iter().flat_map(|s| s.items.iter()).chain(doc.parents.iter().flat_map(|p| p.items.iter()));
    tops.copied().filter(|i| doc.item(*i).is_some_and(|it| it.layer == id)).collect()
}

fn layer_delete(c: &mut Ctx, p: &Value) -> Result<Value> {
    let id = layer_id(p)?;
    need_layer(c, id)?;
    let doc = c.doc_mut()?;
    if doc.layers.len() < 2 {
        return Err(EngineError::new("a document keeps at least one layer"));
    }
    for it in items_on_layer(doc, id) {
        doc.remove_item(it);
    }
    doc.layers.retain(|l| l.id != id);
    c.set_selection(Selection::default());
    ok()
}

fn layer_merge(c: &mut Ctx, p: &Value) -> Result<Value> {
    let into = args::opt_u64(p, "into").map(LayerId).ok_or_else(|| EngineError::new("missing `into` (the layer to keep)"))?;
    need_layer(c, into)?;
    let ids: Vec<LayerId> = p.get("ids").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).map(LayerId).collect()).unwrap_or_default();
    let doc = c.doc_mut()?;
    let all: Vec<ItemId> = doc.items.keys().copied().collect();
    for i in all {
        if let Some(it) = doc.item(i)
            && ids.contains(&it.layer)
            && it.layer != into
            && let Some(m) = doc.item_mut(i)
        {
            m.layer = into;
        }
    }
    doc.layers.retain(|l| l.id == into || !ids.contains(&l.id));
    c.label("Merge Layers");
    ok()
}

fn layer_select(c: &mut Ctx, p: &Value) -> Result<Value> {
    let id = layer_id(p)?;
    need_layer(c, id)?;
    let st = c.state()?;
    let doc = &st.doc;
    let tops: Vec<ItemId> = if st.editing_parents { doc.parents.iter().flat_map(|p| p.items.clone()).collect() } else { doc.spreads.iter().flat_map(|s| s.items.clone()).collect() };
    let ids: Vec<ItemId> = tops.into_iter().filter(|i| doc.item(*i).is_some_and(|it| it.layer == id && !it.locked && !it.hidden)).collect();
    c.set_selection(Selection::items(ids.clone()));
    Ok(json!({"items": ids}))
}

fn layer_others(c: &mut Ctx, p: &Value) -> Result<Value> {
    let id = layer_id(p)?;
    need_layer(c, id)?;
    let (hide, lock, show, unlock) = (args::opt_bool(p, "hide") == Some(true), args::opt_bool(p, "lock") == Some(true), args::opt_bool(p, "show") == Some(true), args::opt_bool(p, "unlock") == Some(true));
    c.label(if hide { "Hide Others" } else if lock { "Lock Others" } else if show { "Show All Layers" } else { "Unlock All Layers" });
    let doc = c.doc_mut()?;
    for l in doc.layers.iter_mut() {
        if show {
            l.visible = true;
        } else if unlock {
            l.locked = false;
        } else if hide {
            l.visible = l.id == id;
        } else if lock {
            l.locked = l.id != id;
        }
    }
    ok()
}

fn layer_activate(c: &mut Ctx, p: &Value) -> Result<Value> {
    let id = layer_id(p)?;
    need_layer(c, id)?;
    let st = c.state_mut()?;
    st.active_layer = Some(id);
    c.session.sel_rev += 1;
    Ok(json!({"activeLayer": id}))
}

fn layer_move(c: &mut Ctx, p: &Value) -> Result<Value> {
    let id = layer_id(p)?;
    need_layer(c, id)?;
    let to = args::u64_(p, "to", "the new 0-based position, top layer first")? as usize;
    let doc = c.doc_mut()?;
    let i = doc.layers.iter().position(|l| l.id == id).unwrap_or(0);
    let l = doc.layers.remove(i);
    let to = to.min(doc.layers.len());
    doc.layers.insert(to, l);
    ok()
}

fn layer_delete_unused(c: &mut Ctx, _: &Value) -> Result<Value> {
    let doc = c.doc_mut()?;
    let used: BTreeSet<LayerId> = doc.items.values().map(|i| i.layer).collect();
    let keep_one = doc.layers.iter().find(|l| used.contains(&l.id)).or(doc.layers.first()).map(|l| l.id);
    doc.layers.retain(|l| used.contains(&l.id) || Some(l.id) == keep_one);
    c.label("Delete Unused Layers");
    ok()
}

const TRANSITIONS: &[&str] = &["none", "fade", "dissolve", "wipe", "push", "cover", "uncover", "split", "blinds", "box", "comb", "zoomIn", "zoomOut", "pageTurn"];

fn transition(c: &mut Ctx, p: &Value) -> Result<Value> {
    let kind = args::opt_str(p, "kind").unwrap_or("none");
    if !TRANSITIONS.contains(&kind) {
        return Err(EngineError::new(format!("`kind` expected one of {}", TRANSITIONS.join(", "))));
    }
    let t = (kind != "none").then(|| json!({"kind": kind, "duration": args::opt_f64(p, "duration").unwrap_or(1.0).max(0.01), "horizontal": args::opt_bool(p, "horizontal").unwrap_or(true)}));
    let all = args::opt_bool(p, "all") == Some(true);
    let si = args::opt_u64(p, "spread").map(|s| s as usize);
    if !all && si.is_none() {
        return Err(EngineError::new("missing `spread` (a spread index) or `all: true`"));
    }
    let doc = c.doc_mut()?;
    for (i, s) in doc.spreads.iter_mut().enumerate() {
        if all || Some(i) == si {
            s.transition = t.clone();
        }
    }
    c.label("Page Transition");
    ok()
}

fn transitions(c: &mut Ctx, _: &Value) -> Result<Value> {
    let doc = c.doc()?;
    Ok(Value::Array(
        doc.spreads
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                let t = s.transition.as_ref()?;
                Some(json!({"spread": i, "kind": t.get("kind").cloned().unwrap_or(json!("none")), "duration": t.get("duration").cloned().unwrap_or(json!(1.0)), "horizontal": t.get("horizontal").cloned().unwrap_or(json!(true))}))
            })
            .collect(),
    ))
}

pub fn register(r: &mut Registry) {
    cmd("document.inspect", "Inspect Document").query().sig("{}").reg(r, inspect);
    cmd("document.preferences", "Document Preferences").menu(&["Edit", "Preferences", "Units & Grids..."]).sig("{horizontalUnits?, verticalUnits?, baselineGrid?: {start, increment}}").reg(r, preferences);
    cmd("layout.documentSetup", "Document Setup...").menu(&["File", "Document Setup..."]).key("Mod+Alt+P").sig("{width?, height?, pages?, startPage?, facingPages?, intent?, bleed?, slug?, adjustLayout?}").reg(r, document_setup);
    cmd("layout.marginsAndColumns", "Margins and Columns...").menu(&["Layout", "Margins and Columns..."]).sig("{margins?: pt | {top, bottom, inside, outside}, columns?, gutter?, pages?: [0-based], adjustLayout?}").reg(r, margins_columns);
    cmd("layout.pageSize", "Page Size").sig("{pages: [1-based], width, height}").reg(r, page_size);
    cmd("layout.pages.insert", "Insert Pages...").menu(&["Layout", "Pages", "Insert Pages..."]).sig("{count, after?: 0-based, parent?: prefix | null}").reg(r, pages_insert);
    cmd("layout.pages.delete", "Delete Pages").menu(&["Layout", "Pages", "Delete Pages"]).sig("{pages: [0-based]}").reg(r, pages_delete);
    cmd("layout.pages.move", "Move Pages...").menu(&["Layout", "Pages", "Move Pages..."]).sig("{from, to} (0-based)").reg(r, pages_move);
    cmd("layout.pages.import", "Insert Pages from Document").menu(&["Layout", "Pages", "Insert Pages from Document..."]).sig("{from: an open document's index (docs, from 0) or the path of a .fdes, .fdoc or .idml file, pages?: \"all\" or a range such as \"2-4, 7\" (page names, or +n for the nth page), after?: 0-based page of this document to insert after (-1 for the start; default the end), move?: also take the pages out of an open source document}").reg(r, pages_import);
    cmd("layout.pages.duplicateSpread", "Duplicate Spread").menu(&["Layout", "Pages", "Duplicate Spread"]).sig("{spread}").reg(r, duplicate_spread);
    cmd("layout.pages.applyParent", "Apply Parent to Pages...").menu(&["Layout", "Pages", "Apply Parent to Pages..."]).sig("{pages: [0-based], parent: prefix | null}").reg(r, apply_parent);
    cmd("layout.parents.new", "New Parent...").menu(&["Layout", "Pages", "New Parent..."]).sig("{prefix?, name?, basedOn?}").reg(r, parents_new);
    cmd("layout.parents.options", "Parent Page Options...").menu(&["Layout", "Pages", "Parent Page Options..."]).sig("{parent: prefix, prefix?, name?, basedOn?: prefix | null}").reg(r, parents_options);
    cmd("layout.parents.edit", "Edit Parents").menu(&["Layout", "Pages", "Edit Parents"]).query().sig("{on}").reg(r, parents_edit);
    cmd("layout.section", "Numbering & Section Options...").menu(&["Layout", "Numbering & Section Options..."]).sig("{page: 1-based, remove? | startNumber?, style?, prefix?, includePrefix?, marker?}").reg(r, section);
    cmd("layout.overrideParentItems", "Override All Parent Page Items").menu(&["Layout", "Pages", "Override All Parent Page Items"]).sig("{page: 0-based}").reg(r, override_all);
    cmd("layout.removeOverrides", "Remove All Local Overrides").menu(&["Layout", "Pages", "Remove All Local Overrides"]).sig("{page: 0-based}").reg(r, remove_overrides);
    cmd("layout.detachAll", "Detach All Objects from Parent").menu(&["Layout", "Pages", "Detach All Objects from Parent"]).sig("{page: 0-based}").reg(r, detach_all);
    cmd("guide.add", "New Guide").sig("{orientation: horizontal|vertical, position, at?, spread?, spreadGuide?}").reg(r, guide_add);
    cmd("layer.new", "New Layer").menu(&["Window", "Layers", "New Layer"]).sig("{name?, color?}").reg(r, layer_new);
    cmd("layer.set", "Layer Options").sig("{id, name?, color?: [r,g,b], visible?, locked?, printable?}").reg(r, layer_set);
    cmd("layer.delete", "Delete Layer").sig("{id}").reg(r, layer_delete);
    cmd("layer.merge", "Merge Layers").sig("{ids: [layer ids], into}").reg(r, layer_merge);
    cmd("layer.selectItems", "Select Items on Layer").query().sig("{id}").reg(r, layer_select);
    cmd("layer.others", "Layer Visibility").sig("{id, hide? | lock? | show? | unlock?}").reg(r, layer_others);
    cmd("layer.activate", "Activate Layer").query().sig("{id}").reg(r, layer_activate);
    cmd("layer.move", "Move Layer").sig("{id, to}").reg(r, layer_move);
    cmd("layer.deleteUnused", "Delete Unused Layers").menu(&["Window", "Layers", "Delete Unused Layers"]).sig("{}").reg(r, layer_delete_unused);
    cmd("page.transition", "Page Transition").sig("{spread | all: true, kind, duration?, horizontal?}").reg(r, transition);
    cmd("page.transitions", "Page Transitions").query().sig("{}").reg(r, transitions);
}
