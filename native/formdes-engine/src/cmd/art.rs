use formdes_model::attrs::Attrs;
use formdes_model::color::Paint;
use formdes_model::doc::{Doc, Home};
use formdes_model::geom::{Affine, Rect};
use formdes_model::ids::ItemId;
use formdes_model::item::{Item, ItemKind, Shape};
use formdes_model::story::Inline;
use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;
use kurbo::{BezPath, PathEl};
use serde_json::{Value, json};

use crate::registry::{Needs, Registry, cmd};
use crate::session::{Ctx, Selection, TextSel};
use crate::target::items;
use crate::{EngineError, Result, args, ops};

type Contours = Vec<Vec<[f64; 2]>>;
type Shapes = Vec<Contours>;
type Outlined = (ItemId, usize, Vec<(Paint, BezPath)>);

fn contours(path: &BezPath) -> Contours {
    let mut out: Contours = Vec::new();
    let mut cur: Vec<[f64; 2]> = Vec::new();
    kurbo::flatten(path.iter(), 0.05, |el| match el {
        PathEl::MoveTo(p) => {
            if cur.len() > 2 {
                out.push(std::mem::take(&mut cur));
            }
            cur.clear();
            cur.push([p.x, p.y]);
        }
        PathEl::LineTo(p) => cur.push([p.x, p.y]),
        PathEl::ClosePath => {
            if cur.len() > 2 {
                out.push(std::mem::take(&mut cur));
            }
            cur.clear();
        }
        _ => {}
    });
    if cur.len() > 2 {
        out.push(cur);
    }
    out
}

fn to_path(shapes: &Shapes) -> BezPath {
    let mut p = BezPath::new();
    for shape in shapes {
        for c in shape {
            for (i, pt) in c.iter().enumerate() {
                if i == 0 {
                    p.move_to((pt[0], pt[1]));
                } else {
                    p.line_to((pt[0], pt[1]));
                }
            }
            p.close_path();
        }
    }
    p
}

fn world_shape(doc: &Doc, id: ItemId) -> Option<Shapes> {
    let it = doc.item(id)?;
    if matches!(it.kind, ItemKind::Group { .. } | ItemKind::States { .. }) || !it.shape.closed() {
        return None;
    }
    let path = doc.world_xf(id) * it.shape.to_path(&it.corner);
    let c = contours(&path);
    let rule = if it.extra.get("fillRule").and_then(Value::as_str) == Some("evenodd") { FillRule::EvenOdd } else { FillRule::NonZero };
    let empty: Contours = Vec::new();
    Some(c.overlay(&empty, OverlayRule::Subject, rule))
}

fn paint_order(doc: &Doc, ids: &[ItemId]) -> Vec<ItemId> {
    let mut keyed: Vec<((usize, usize), ItemId)> = ids
        .iter()
        .map(|id| {
            let top = doc.top_of(*id);
            let k = match doc.home(top) {
                Some(Home::Spread(si)) => (si, doc.spreads[si].items.iter().position(|x| *x == top).unwrap_or(0)),
                Some(Home::Parent(pi)) => (10_000 + pi, doc.parents[pi].items.iter().position(|x| *x == top).unwrap_or(0)),
                _ => (usize::MAX, 0),
            };
            (k, *id)
        })
        .collect();
    keyed.sort();
    keyed.into_iter().map(|(_, i)| i).collect()
}

fn pathfinder(c: &mut Ctx, p: &Value) -> Result<Value> {
    let op = args::str_(p, "op", "add, subtract, intersect, excludeOverlap or minusBack")?.to_string();
    let ids = items(c, p)?;
    let doc = c.doc()?;
    let ids = paint_order(doc, &ids);
    if ids.len() < 2 {
        return Err(EngineError::new("Pathfinder needs two or more closed shapes selected"));
    }
    let mut shapes = Vec::new();
    for id in &ids {
        shapes.push(world_shape(doc, *id).ok_or_else(|| EngineError::new(format!("object {id} is a group, an open path or a line: Pathfinder works on closed shapes")))?);
    }
    let union_of = |list: &[Shapes]| -> Shapes {
        let mut acc = list[0].clone();
        for s in &list[1..] {
            acc = acc.overlay(s, OverlayRule::Union, FillRule::NonZero);
        }
        acc
    };
    let (result, keep) = match op.as_str() {
        "add" => (union_of(&shapes), *ids.last().unwrap_or(&ids[0])),
        "subtract" => (shapes[0].overlay(&union_of(&shapes[1..]), OverlayRule::Difference, FillRule::NonZero), ids[0]),
        "minusBack" => {
            let n = shapes.len();
            (shapes[n - 1].overlay(&union_of(&shapes[..n - 1]), OverlayRule::Difference, FillRule::NonZero), ids[n - 1])
        }
        "intersect" => {
            let mut acc = shapes[0].clone();
            for s in &shapes[1..] {
                acc = acc.overlay(s, OverlayRule::Intersect, FillRule::NonZero);
            }
            (acc, *ids.last().unwrap_or(&ids[0]))
        }
        "excludeOverlap" => {
            let mut acc = shapes[0].clone();
            for s in &shapes[1..] {
                acc = acc.overlay(s, OverlayRule::Xor, FillRule::NonZero);
            }
            (acc, *ids.last().unwrap_or(&ids[0]))
        }
        other => return Err(EngineError::new(format!("`op` expected add, subtract, intersect, excludeOverlap or minusBack, not {other}"))),
    };
    let path = to_path(&result);
    let doc = c.doc_mut()?;
    for id in &ids {
        if *id != keep {
            doc.remove_item(*id);
        }
    }
    if path.elements().is_empty() {
        doc.remove_item(keep);
        c.set_selection(Selection::default());
        c.label("Pathfinder");
        return Ok(json!({"id": null, "empty": true}));
    }
    let local = doc.world_xf(keep).inverse() * path;
    if let Some(it) = doc.item_mut(keep) {
        it.shape = Shape::Path { path: local, closed: true };
        it.corner = Default::default();
        it.extra.insert("fillRule".into(), json!("evenodd"));
        if matches!(it.kind, ItemKind::Text(_)) {
            it.kind = ItemKind::Plain;
        }
    }
    c.set_selection(Selection::items(vec![keep]));
    c.label("Pathfinder");
    Ok(json!({"id": keep}))
}

fn qr_path(text: &str, r: Rect, quiet: u32) -> Result<BezPath> {
    let qr = qrcodegen::QrCode::encode_text(text, qrcodegen::QrCodeEcc::Medium).map_err(|_| EngineError::new("the text is too long for a QR code"))?;
    let n = qr.size();
    let total = n as f64 + 2.0 * quiet as f64;
    let side = r.width().min(r.height());
    let m = side / total;
    let x0 = r.x0 + (r.width() - side) / 2.0 + quiet as f64 * m;
    let y0 = r.y0 + (r.height() - side) / 2.0 + quiet as f64 * m;
    let mut p = BezPath::new();
    for y in 0..n {
        let mut x = 0;
        while x < n {
            if qr.get_module(x, y) {
                let start = x;
                while x < n && qr.get_module(x, y) {
                    x += 1;
                }
                let rr = Rect::new(x0 + start as f64 * m, y0 + y as f64 * m, x0 + x as f64 * m, y0 + (y + 1) as f64 * m);
                p.move_to((rr.x0, rr.y0));
                p.line_to((rr.x1, rr.y0));
                p.line_to((rr.x1, rr.y1));
                p.line_to((rr.x0, rr.y1));
                p.close_path();
            } else {
                x += 1;
            }
        }
    }
    Ok(p)
}

fn qr_code(c: &mut Ctx, p: &Value) -> Result<Value> {
    let text = args::str_(p, "text", "what the QR code holds: a web address, plain text, an email address")?.to_string();
    if text.is_empty() {
        return Err(EngineError::new("`text` is empty"));
    }
    let color = args::opt_str(p, "swatch").unwrap_or("Black").to_string();
    let quiet = args::opt_u64(p, "quietZone").unwrap_or(1).min(8) as u32;
    let sel = if p.get("rect").is_some() && p.get("ids").is_none() { Vec::new() } else { crate::target::items_or_empty(c, p)? };
    let layer = c.target_layer()?;
    let doc = c.doc()?;
    let existing = sel.first().copied().filter(|i| doc.item(*i).is_some_and(|it| it.extra.contains_key("qr")));
    if let Some(id) = existing {
        let r = doc.item(id).map(|i| i.extra.get("qrBox").and_then(crate::args::rect).unwrap_or(i.shape.bbox())).unwrap_or(Rect::ZERO);
        let path = qr_path(&text, r, quiet)?;
        let doc = c.doc_mut()?;
        if let Some(it) = doc.item_mut(id) {
            it.shape = Shape::Path { path, closed: true };
            it.extra.insert("qr".into(), json!(text));
            it.fill = Paint::swatch(&color);
        }
        c.label("Edit QR Code");
        return Ok(json!({"id": id}));
    }
    let (home, r, xf) = match sel.first() {
        Some(f) => {
            let it = doc.item(*f).ok_or_else(|| EngineError::new("no such object"))?;
            let home = doc.home(*f).ok_or_else(|| EngineError::new("no such object"))?;
            (home, it.shape.bbox(), it.xf)
        }
        None => {
            let home = crate::cmd::object::spread_home(doc, p.get("spread"))?;
            let r = match p.get("rect").and_then(crate::args::rect) {
                Some(r) => r.abs(),
                None => {
                    let si = match home {
                        Home::Spread(s) => s,
                        _ => 0,
                    };
                    let m = crate::cmd::object::margin_rect(doc, doc.first_page_of_spread(si)).unwrap_or(Rect::new(36.0, 36.0, 180.0, 180.0));
                    Rect::new(m.x0, m.y0, m.x0 + 144.0, m.y0 + 144.0)
                }
            };
            (home, r, Affine::IDENTITY)
        }
    };
    let path = qr_path(&text, r, quiet)?;
    let empty_frame = sel.first().and_then(|f| doc.item(*f)).is_some_and(|it| matches!(it.kind, ItemKind::Plain) && it.fill.is_none());
    let doc = c.doc_mut()?;
    let mut it = Item::new(ItemId(0), ItemKind::Plain, Shape::Path { path, closed: true }, layer);
    it.xf = xf;
    it.fill = Paint::swatch(&color);
    it.stroke = formdes_model::item::Stroke { weight: 0.0, paint: Paint::None, ..Default::default() };
    it.name = "QR Code".into();
    it.extra.insert("qr".into(), json!(text));
    it.extra.insert("qrBox".into(), json!([r.x0, r.y0, r.x1, r.y1]));
    let id = doc.insert_item(it, home);
    if empty_frame && let Some(f) = sel.first() {
        doc.remove_item(*f);
    }
    c.set_selection(Selection::items(vec![id]));
    c.label("Generate QR Code");
    Ok(json!({"id": id}))
}

fn create_outlines(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let doc = c.doc()?;
    let frames: Vec<(ItemId, usize)> = ids.iter().filter(|i| doc.item(**i).is_some_and(|it| it.text().is_some())).filter_map(|i| match doc.home(*i) {
        Some(Home::Spread(si)) => Some((*i, si)),
        _ => None,
    }).collect();
    if frames.is_empty() {
        return Err(EngineError::new("Create Outlines needs a text frame on a spread selected"));
    }
    let renderer = c.session.renderer.clone();
    let layer_of = |d: &Doc, id: ItemId| d.item(id).map(|i| i.layer).unwrap_or_default();
    let mut made: Vec<Outlined> = Vec::new();
    for (id, si) in &frames {
        let mut tmp = doc.clone();
        for (k, s) in tmp.spreads.iter_mut().enumerate() {
            s.items = if k == *si { vec![*id] } else { Vec::new() };
            if k == *si {
                for pg in s.pages.iter_mut() {
                    pg.parent = None;
                }
            }
        }
        if let Some(it) = tmp.item_mut(*id) {
            it.fill = Paint::None;
            it.stroke.weight = 0.0;
            it.opacity = 1.0;
        }
        let lay = formdes_text::layout::layout_doc(&tmp, &renderer.fonts);
        let opsv = formdes_render::scene::spread_ops(&tmp, &lay, formdes_render::scene::Target::Spread(*si), &formdes_render::SceneOpts { printing: true, bleed: false, overprint: false });
        let mut by: Vec<(String, Paint, BezPath)> = Vec::new();
        for op in &opsv {
            let formdes_render::Op::Glyphs(g) = op else { continue };
            let paint = match &g.fill {
                Some(formdes_render::scene::Brush::Solid(col, _)) => Paint::Color { color: *col },
                Some(_) => Paint::swatch("Black"),
                None => continue,
            };
            let key = format!("{paint:?}");
            let idx = match by.iter().position(|b| b.0 == key) {
                Some(i) => i,
                None => {
                    by.push((key, paint, BezPath::new()));
                    by.len() - 1
                }
            };
            for i in 0..g.glyphs.len() {
                let outline = renderer.outlines.get(&g.face, g.glyphs[i].0);
                if outline.elements().is_empty() {
                    continue;
                }
                let gp = formdes_render::raster::glyph_xf(g, i) * outline.as_ref().clone();
                for el in gp.elements() {
                    by[idx].2.push(*el);
                }
            }
        }
        made.push((*id, *si, by.into_iter().map(|(_, p, b)| (p, b)).collect()));
    }
    let doc = c.doc_mut()?;
    let mut out = Vec::new();
    for (id, si, parts) in made {
        let layer = layer_of(doc, id);
        let mut new_ids = Vec::new();
        for (paint, path) in parts {
            if path.elements().is_empty() {
                continue;
            }
            let mut it = Item::new(ItemId(0), ItemKind::Plain, Shape::Path { path, closed: true }, layer);
            it.fill = paint;
            it.stroke = formdes_model::item::Stroke { weight: 0.0, paint: Paint::None, ..Default::default() };
            new_ids.push(doc.insert_item(it, Home::Spread(si)));
        }
        if new_ids.is_empty() {
            continue;
        }
        let pos = doc.spreads[si].items.iter().position(|x| *x == id);
        doc.remove_item(id);
        let res = if new_ids.len() > 1 { ops::group(doc, &new_ids)? } else { new_ids[0] };
        if let Some(pos) = pos {
            doc.spreads[si].items.retain(|x| *x != res);
            let at = pos.min(doc.spreads[si].items.len());
            doc.spreads[si].items.insert(at, res);
        }
        out.push(res);
    }
    if out.is_empty() {
        return Err(EngineError::new("the frame has no visible text to outline"));
    }
    c.set_selection(Selection::items(out.clone()));
    c.label("Create Outlines");
    Ok(json!({"ids": out}))
}

fn anchored_insert(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = crate::target::ids_param(p, "ids").ok_or_else(|| EngineError::new("missing `ids` (the objects to anchor at the text cursor)"))?;
    let t = c.selection().text.ok_or_else(|| EngineError::new("needs a text cursor: click into text with the Type tool where the objects go"))?;
    let doc = c.doc()?;
    let frames = doc.story(t.story).map(|s| s.frames.clone()).unwrap_or_default();
    for id in &ids {
        let it = doc.item(*id).ok_or_else(|| EngineError::new(format!("no object with id {id}")))?;
        if frames.contains(id) || it.group.is_some() || it.anchored_in.is_some() {
            return Err(EngineError::new(format!("object {id} cannot be anchored here: it is this story's frame, inside a group, or already anchored")));
        }
    }
    let position = args::opt_str(p, "position").unwrap_or("inline").to_string();
    let doc = c.doc_mut()?;
    let mut at = t.range().start;
    for id in &ids {
        doc.detach_from_home(*id);
        if let Some(it) = doc.item_mut(*id) {
            it.anchored_in = Some(t.story);
        }
        let mut attrs = Attrs::new();
        attrs.insert("position".into(), json!(position));
        if let Some(st) = doc.story_mut(t.story) {
            st.insert_inline(at, Inline::Anchored { item: *id, attrs });
        }
        at += 1;
    }
    c.set_selection(Selection::text(TextSel::caret(t.story, at)));
    c.label("Anchor Object");
    Ok(json!({"story": t.story, "at": at - ids.len()}))
}

fn inline_of(doc: &Doc, id: ItemId) -> Option<(formdes_model::ids::StoryId, usize)> {
    let sid = doc.item(id)?.anchored_in?;
    let st = doc.story(sid)?;
    st.inlines().into_iter().find_map(|(at, i)| matches!(i, Inline::Anchored { item, .. } if *item == id).then_some((sid, at)))
}

fn anchored_options(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let doc = c.doc()?;
    let targets: Vec<(ItemId, formdes_model::ids::StoryId, usize)> = ids.iter().filter_map(|i| inline_of(doc, *i).map(|(s, a)| (*i, s, a))).collect();
    if targets.is_empty() {
        return Err(EngineError::new("needs an anchored object selected"));
    }
    let doc = c.doc_mut()?;
    for (_, sid, at) in &targets {
        if let Some(Inline::Anchored { attrs, .. }) = doc.story_mut(*sid).and_then(|s| s.inline_at_mut(*at)) {
            for k in ["position", "xOffset", "yOffset"] {
                if let Some(v) = p.get(k).filter(|v| !v.is_null()) {
                    attrs.insert(k.into(), v.clone());
                }
            }
        }
    }
    c.label("Anchored Object Options");
    Ok(json!({"ids": targets.iter().map(|t| t.0).collect::<Vec<_>>()}))
}

fn anchored_release(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let lay = c.layout()?;
    let doc = c.doc()?;
    let mut plan = Vec::new();
    for id in &ids {
        let Some((sid, at)) = inline_of(doc, *id) else { continue };
        let Some(sl) = lay.stories.get(&sid) else { continue };
        let it = doc.item(*id).ok_or_else(|| EngineError::new("no such object"))?;
        let placed = sl.flow.anchors.iter().find(|a| a.item == *id).and_then(|a| {
            let frame = *sl.frames.get(a.frame)?;
            let b = it.xf.transform_rect_bbox(it.shape.bbox());
            let corner_y = if a.inline { b.y1 } else { b.y0 };
            let si = doc.spread_of(frame)?;
            Some((si, doc.world_xf(frame) * Affine::translate((a.x - b.x0, a.y - corner_y)) * it.xf))
        });
        let (si, xf) = placed.unwrap_or_else(|| (doc.spread_of(*id).unwrap_or(0), it.xf));
        plan.push((*id, sid, at, si, xf));
    }
    if plan.is_empty() {
        return Err(EngineError::new("needs an anchored object selected"));
    }
    let doc = c.doc_mut()?;
    let mut out = Vec::new();
    for (id, sid, at, si, xf) in plan.into_iter().rev() {
        if let Some(st) = doc.story_mut(sid) {
            st.delete(at..at + 1);
        }
        if let Some(it) = doc.item_mut(id) {
            it.anchored_in = None;
            it.xf = xf;
        }
        if let Some(s) = doc.spreads.get_mut(si) {
            s.items.push(id);
        }
        out.push(id);
    }
    c.set_selection(Selection::items(out.clone()));
    c.label("Release Anchored Object");
    Ok(json!({"ids": out}))
}

pub fn register(r: &mut Registry) {
    cmd("object.pathfinder", "Pathfinder").menu(&["Object", "Pathfinder"]).needs(Needs::Items).sig("{op: add|subtract|intersect|excludeOverlap|minusBack, ids?}").help("Combines closed shapes: Add unites them, Subtract cuts the front shapes out of the back one, Intersect keeps their overlap, Exclude Overlap keeps the parts that do not overlap, Minus Back cuts the back shapes out of the front one. Curves come out as fine straight segments.").reg(r, pathfinder);
    cmd("object.qrCode", "Generate QR Code").menu(&["Object", "Generate QR Code..."]).sig("{text, swatch?, quietZone?: modules, ids?: a frame to fill or a QR code to edit, spread?, rect?}").reg(r, qr_code);
    cmd("type.createOutlines", "Create Outlines").menu(&["Type", "Create Outlines"]).key("Mod+Shift+O").needs(Needs::Items).sig("{ids?}").help("Turns the selected text frames into filled paths, one per text colour, grouped when there are several. Anchored objects in the text are not kept.").reg(r, create_outlines);
    cmd("anchored.insert", "Anchor Object").sig("{ids, position?: inline|aboveLine|custom}").help("Moves the objects into the text at the text cursor as anchored objects.").reg(r, anchored_insert);
    cmd("anchored.options", "Anchored Object Options").menu(&["Object", "Anchored Object", "Options..."]).needs(Needs::Items).sig("{position?: inline|aboveLine|custom, xOffset?, yOffset?, ids?}").reg(r, anchored_options);
    cmd("anchored.release", "Release Anchored Object").menu(&["Object", "Anchored Object", "Release"]).needs(Needs::Items).sig("{ids?}").reg(r, anchored_release);
}
