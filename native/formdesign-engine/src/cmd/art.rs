use formdesign_model::attrs::Attrs;
use formdesign_model::color::Paint;
use formdesign_model::doc::{Doc, Home};
use formdesign_model::geom::{Affine, Rect};
use formdesign_model::ids::ItemId;
use formdesign_model::item::{Item, ItemKind, Shape};
use formdesign_model::story::Inline;
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

pub(crate) fn paint_order(doc: &Doc, ids: &[ItemId]) -> Vec<ItemId> {
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
    let op = args::str_(p, "op", "add, subtract, intersect, excludeOverlap, minusBack, divide, trim, merge, crop or outline")?.to_string();
    if matches!(op.as_str(), "divide" | "trim" | "merge" | "crop" | "outline") {
        return pathfinder_pieces(c, p, &op);
    }
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
    it.stroke = formdesign_model::item::Stroke { weight: 0.0, paint: Paint::None, ..Default::default() };
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
        let lay = formdesign_text::layout::layout_doc(&tmp, &renderer.fonts);
        let opsv = formdesign_render::scene::spread_ops(&tmp, &lay, formdesign_render::scene::Target::Spread(*si), &formdesign_render::SceneOpts { printing: true, bleed: false, overprint: false });
        let mut by: Vec<(String, Paint, BezPath)> = Vec::new();
        for op in &opsv {
            let formdesign_render::Op::Glyphs(g) = op else { continue };
            let paint = match &g.fill {
                Some(formdesign_render::scene::Brush::Solid(col, _)) => Paint::Color { color: *col },
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
                let gp = formdesign_render::raster::glyph_xf(g, i) * outline.as_ref().clone();
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
            it.stroke = formdesign_model::item::Stroke { weight: 0.0, paint: Paint::None, ..Default::default() };
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

fn inline_of(doc: &Doc, id: ItemId) -> Option<(formdesign_model::ids::StoryId, usize)> {
    let sid = doc.item(id)?.anchored_in?;
    let st = doc.story(sid)?;
    st.inlines().into_iter().find_map(|(at, i)| matches!(i, Inline::Anchored { item, .. } if *item == id).then_some((sid, at)))
}

fn anchored_options(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let doc = c.doc()?;
    let targets: Vec<(ItemId, formdesign_model::ids::StoryId, usize)> = ids.iter().filter_map(|i| inline_of(doc, *i).map(|(s, a)| (*i, s, a))).collect();
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

fn nonempty(s: &Shapes) -> bool {
    s.iter().any(|shape| shape.iter().any(|c| c.len() > 2))
}

fn pathfinder_pieces(c: &mut Ctx, p: &Value, op: &str) -> Result<Value> {
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
    let fills: Vec<Paint> = ids.iter().map(|id| doc.item(*id).map(|i| i.fill.clone()).unwrap_or(Paint::None)).collect();
    let n = shapes.len();
    let mut pieces: Vec<(Shapes, usize)> = Vec::new();
    match op {
        "divide" | "outline" => {
            for (i, s) in shapes.iter().enumerate() {
                let mut next = Vec::new();
                let mut rest = s.clone();
                for (piece, owner) in pieces.drain(..) {
                    let inside = piece.overlay(s, OverlayRule::Intersect, FillRule::NonZero);
                    let outside = piece.overlay(s, OverlayRule::Difference, FillRule::NonZero);
                    rest = rest.overlay(&piece, OverlayRule::Difference, FillRule::NonZero);
                    if nonempty(&inside) { next.push((inside, i)); }
                    if nonempty(&outside) { next.push((outside, owner)); }
                }
                if nonempty(&rest) { next.push((rest, i)); }
                pieces = next;
            }
        }
        "trim" | "merge" => {
            for i in 0..n {
                let mut visible = shapes[i].clone();
                for above in &shapes[i + 1..] {
                    visible = visible.overlay(above, OverlayRule::Difference, FillRule::NonZero);
                }
                if nonempty(&visible) { pieces.push((visible, i)); }
            }
            if op == "merge" {
                let mut merged: Vec<(Shapes, usize)> = Vec::new();
                for (s, owner) in pieces.drain(..) {
                    if let Some(m) = merged.iter_mut().find(|m| fills[m.1] == fills[owner]) {
                        m.0 = m.0.overlay(&s, OverlayRule::Union, FillRule::NonZero);
                    } else {
                        merged.push((s, owner));
                    }
                }
                pieces = merged;
            }
        }
        _ => {
            let top = &shapes[n - 1];
            for i in 0..n - 1 {
                let kept = shapes[i].overlay(top, OverlayRule::Intersect, FillRule::NonZero);
                if nonempty(&kept) { pieces.push((kept, i)); }
            }
        }
    }
    let doc = c.doc_mut()?;
    let mut made = Vec::new();
    for (s, owner) in &pieces {
        let src = ids[*owner];
        let Some(nid) = ops::duplicate(doc, src, kurbo::Vec2::ZERO, None) else { continue };
        let local = doc.world_xf(nid).inverse() * to_path(s);
        if let Some(it) = doc.item_mut(nid) {
            it.shape = Shape::Path { path: local, closed: true };
            it.corner = Default::default();
            it.extra.insert("fillRule".into(), json!("nonzero"));
            it.kind = ItemKind::Plain;
            if op == "outline" {
                let paint = std::mem::replace(&mut it.fill, Paint::None);
                it.stroke.paint = paint;
                it.stroke.weight = 0.5;
            } else if op != "crop" {
                it.stroke.paint = Paint::None;
                it.stroke.weight = 0.0;
            }
        }
        made.push(nid);
    }
    for id in &ids {
        doc.remove_item(*id);
    }
    let group = made.clone();
    c.set_selection(Selection::items(group.clone()));
    c.label("Pathfinder");
    Ok(json!({"ids": group, "pieces": group.len()}))
}

fn inside(shape: &Contours, x: f64, y: f64) -> bool {
    let mut odd = false;
    for c in shape {
        let n = c.len();
        for i in 0..n {
            let (a, b) = (c[i], c[(i + 1) % n]);
            if (a[1] > y) != (b[1] > y) && x < (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]) + a[0] {
                odd = !odd;
            }
        }
    }
    odd
}

/// The regions of overlapping closed shapes in paint order (each overlap owned by the upper shape), cut along
/// `cuts` when given, as single faces with their owner's index.
fn faces_of(shapes: &[Shapes], cuts: Option<&Shapes>) -> Vec<(Shapes, usize)> {
    let mut pieces: Vec<(Shapes, usize)> = Vec::new();
    for (i, s) in shapes.iter().enumerate() {
        let mut next = Vec::new();
        let mut rest = s.clone();
        for (piece, owner) in pieces.drain(..) {
            let a = piece.overlay(s, OverlayRule::Intersect, FillRule::NonZero);
            let b = piece.overlay(s, OverlayRule::Difference, FillRule::NonZero);
            rest = rest.overlay(&piece, OverlayRule::Difference, FillRule::NonZero);
            if nonempty(&a) { next.push((a, i)); }
            if nonempty(&b) { next.push((b, owner)); }
        }
        if nonempty(&rest) { next.push((rest, i)); }
        pieces = next;
    }
    let mut faces: Vec<(Shapes, usize)> = Vec::new();
    for (s, owner) in pieces {
        let s = match cuts {
            Some(k) => s.overlay(k, OverlayRule::Difference, FillRule::NonZero),
            None => s,
        };
        for shape in s {
            if shape.iter().any(|c| c.len() > 2) {
                faces.push((vec![shape], owner));
            }
        }
    }
    faces
}

fn live_paint(c: &mut Ctx, p: &Value) -> Result<Value> {
    let pts: Vec<[f64; 2]> = p.get("points").and_then(Value::as_array).map(|a| a.iter().filter_map(|q| {
        let q = q.as_array()?;
        Some([q.first()?.as_f64()?, q.get(1)?.as_f64()?])
    }).collect()).unwrap_or_default();
    if pts.is_empty() {
        return Err(EngineError::new("`points` needs one or more [x, y] points in spread coordinates: the regions to fill"));
    }
    let name = args::str_(p, "swatch", "the fill colour (a swatch name or #rrggbb)")?;
    let paint = crate::wire::paint_in(name, args::opt_f64(p, "tint"));
    let ids = items(c, p)?;
    let doc = c.doc()?;
    if doc.paint_color(&paint).is_none() && doc.gradient_of(&paint).is_none() {
        return Err(EngineError::new(format!("no colour for {name}")));
    }
    if let Some(gid) = live_paint_group(doc, &ids) {
        let inv = doc.world_xf(gid).inverse();
        let sources = live_paint_sources(doc, gid);
        let faces = regions(doc, &sources)?;
        if !pts.iter().any(|q| faces.iter().any(|(s, _)| s.iter().any(|shape| inside(shape, q[0], q[1])))) {
            return Err(EngineError::new("none of the points falls inside a region of the live paint group"));
        }
        let ins: Vec<Vec<u64>> = pts.iter().map(|q| membership(doc, &sources, q[0], q[1])).collect();
        let doc = c.doc_mut()?;
        if let Some(g) = doc.item_mut(gid)
            && let Some(Value::Object(lp)) = g.extra.get_mut("livePaint")
        {
            let fills = lp.entry("fills").or_insert_with(|| json!([]));
            if let Value::Array(list) = fills {
                for (q, m) in pts.iter().zip(&ins) {
                    let local = inv * kurbo::Point::new(q[0], q[1]);
                    list.push(json!({"p": [local.x, local.y], "in": m, "swatch": name, "tint": args::opt_f64(p, "tint")}));
                }
            }
        }
        c.label("Fill Regions");
        return Ok(json!({"group": gid, "fills": pts.len()}));
    }
    let ids = paint_order(doc, &ids);
    let faces = regions(doc, &ids)?;
    let mut chosen: Vec<usize> = Vec::new();
    for q in &pts {
        if let Some(i) = faces.iter().position(|(s, _)| s.iter().any(|shape| inside(shape, q[0], q[1])))
            && !chosen.contains(&i)
        {
            chosen.push(i);
        }
    }
    if chosen.is_empty() {
        return Err(EngineError::new("none of the points falls inside a region of the selection"));
    }
    let bottom = ids[0];
    let doc = c.doc_mut()?;
    let mut made = Vec::new();
    for i in chosen {
        let Some(nid) = ops::duplicate(doc, bottom, kurbo::Vec2::ZERO, None) else { continue };
        let local = doc.world_xf(nid).inverse() * to_path(&faces[i].0);
        if let Some(it) = doc.item_mut(nid) {
            it.kind = ItemKind::Plain;
            it.shape = Shape::Path { path: local, closed: true };
            it.corner = Default::default();
            it.fill = paint.clone();
            it.stroke = ops::default_stroke(0.0);
            it.extra.clear();
        }
        made.push(nid);
    }
    if let Some(home) = doc.home(bottom)
        && let Some(list) = ops::home_list(doc, &home)
    {
        list.retain(|x| !made.contains(x));
        let at = list.iter().position(|x| *x == bottom).unwrap_or(0);
        list.splice(at..at, made.iter().copied());
    }
    c.set_selection(Selection::items(made.clone()));
    c.label("Fill Regions");
    Ok(json!({"ids": made}))
}

/// The regions the closed shapes among `ids` make by overlapping, divided further by any open paths among them.
fn regions(doc: &Doc, ids: &[ItemId]) -> Result<Vec<(Shapes, usize)>> {
    let mut shapes = Vec::new();
    let mut blades: Vec<BezPath> = Vec::new();
    for id in ids {
        match world_shape(doc, *id) {
            Some(s) => shapes.push(s),
            None => {
                if let Some(it) = doc.item(*id)
                    && !matches!(it.kind, ItemKind::Group { .. } | ItemKind::States { .. })
                {
                    blades.push(doc.world_xf(*id) * it.shape.to_path(&it.corner));
                }
            }
        }
    }
    if shapes.is_empty() {
        return Err(EngineError::new("Fill Regions needs at least one closed shape among the selection; open paths then divide its regions"));
    }
    let mut cuts: Shapes = Vec::new();
    for b in &blades {
        let slit = outline_of(b, &stroke_style(0.05, "miter", "butt", 4.0, &[]));
        cuts = if cuts.is_empty() { slit } else { cuts.overlay(&slit, OverlayRule::Union, FillRule::NonZero) };
    }
    Ok(faces_of(&shapes, if cuts.is_empty() { None } else { Some(&cuts) }))
}

/// A point inside a region: the centre of the first triangle fanned from a ring's first vertex that lies inside.
fn face_point(face: &Shapes) -> Option<[f64; 2]> {
    for shape in face {
        let Some(ring) = shape.first() else { continue };
        for i in 2..ring.len() {
            let (a, b, c) = (ring[0], ring[i - 1], ring[i]);
            let q = [(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0];
            if inside(shape, q[0], q[1]) {
                return Some(q);
            }
        }
    }
    None
}

/// Which of a live paint group's closed shapes hold a point: the identity a painted region keeps while the
/// shapes move, so its fill stays with it rather than with the spot that was clicked.
fn membership(doc: &Doc, sources: &[ItemId], x: f64, y: f64) -> Vec<u64> {
    sources.iter().filter(|id| world_shape(doc, **id).is_some_and(|sh| sh.iter().any(|c| inside(c, x, y)))).map(|id| id.0).collect()
}

/// The live paint group the selection is, or is inside.
fn live_paint_group(doc: &Doc, ids: &[ItemId]) -> Option<ItemId> {
    ids.iter().find_map(|id| {
        let it = doc.item(*id)?;
        if it.extra.contains_key("livePaint") {
            return Some(*id);
        }
        it.group.filter(|g| doc.item(*g).is_some_and(|gi| gi.extra.contains_key("livePaint")))
    })
}

/// A live paint group's own paths (not the regions it painted), in paint order.
fn live_paint_sources(doc: &Doc, gid: ItemId) -> Vec<ItemId> {
    let kids: Vec<ItemId> = doc.item(gid).map(|g| g.children()).unwrap_or_default().into_iter().filter(|k| doc.item(*k).is_some_and(|it| !it.extra.contains_key("lpFace"))).collect();
    paint_order(doc, &kids)
}

fn live_paint_sig(doc: &Doc, gid: ItemId) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    if let Some(g) = doc.item(gid) {
        g.extra.get("livePaint").and_then(|v| v.get("fills")).map(|f| f.to_string()).unwrap_or_default().hash(&mut h);
    }
    for id in live_paint_sources(doc, gid) {
        if let Some(it) = doc.item(id) {
            serde_json::to_string(it).unwrap_or_default().hash(&mut h);
            format!("{:?}", doc.world_xf(id)).hash(&mut h);
        }
    }
    format!("{:016x}", h.finish())
}

/// Rebuilds the painted regions of every live paint group whose paths or fills changed since it was last built:
/// each fill keeps the point it was clicked at (in the group's space), and paints whichever region holds that
/// point now. Called by the core after every call.
pub fn refresh_live_paint(doc: &mut std::sync::Arc<Doc>) {
    let stale: Vec<ItemId> = doc.items.iter().filter_map(|(id, it)| {
        let lp = it.extra.get("livePaint")?;
        (lp.get("sig").and_then(Value::as_str) != Some(live_paint_sig(doc, *id).as_str())).then_some(*id)
    }).collect();
    if stale.is_empty() {
        return;
    }
    let d = std::sync::Arc::make_mut(doc);
    for gid in stale {
        let old: Vec<ItemId> = d.item(gid).map(|g| g.children()).unwrap_or_default().into_iter().filter(|k| d.item(*k).is_some_and(|it| it.extra.contains_key("lpFace"))).collect();
        for id in old {
            d.remove_item(id);
        }
        let fills: Vec<Value> = d.item(gid).and_then(|g| g.extra.get("livePaint")).and_then(|v| v.get("fills")).and_then(Value::as_array).cloned().unwrap_or_default();
        let gx = d.world_xf(gid);
        let mut painted: Vec<(usize, formdesign_model::color::Paint)> = Vec::new();
        let sources = live_paint_sources(d, gid);
        if let Ok(faces) = regions(d, &sources) {
            let mem: Vec<Option<Vec<u64>>> = faces.iter().map(|(s, _)| face_point(s).map(|q| membership(d, &sources, q[0], q[1]))).collect();
            for f in &fills {
                let (Some(x), Some(y)) = (f.pointer("/p/0").and_then(Value::as_f64), f.pointer("/p/1").and_then(Value::as_f64)) else { continue };
                let w = gx * kurbo::Point::new(x, y);
                let paint = crate::wire::paint_in(f.get("swatch").and_then(Value::as_str).unwrap_or("None"), f.get("tint").and_then(Value::as_f64));
                let by_point = faces.iter().position(|(s, _)| s.iter().any(|shape| inside(shape, w.x, w.y)));
                let want: Option<Vec<u64>> = f.get("in").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).collect());
                let hit = match want {
                    Some(want) => {
                        let cands: Vec<usize> = mem.iter().enumerate().filter(|(_, m)| m.as_ref() == Some(&want)).map(|(i, _)| i).collect();
                        by_point.filter(|b| cands.contains(b)).or(cands.first().copied())
                    }
                    None => by_point,
                };
                if let Some(i) = hit {
                    painted.retain(|(j, _)| *j != i);
                    painted.push((i, paint));
                }
            }
            let layer = d.item(gid).map(|g| g.layer).unwrap_or(ops::active_layer(d));
            let mut made = Vec::new();
            for (i, paint) in painted {
                let mut it = formdesign_model::item::Item::new(ItemId(0), ItemKind::Plain, Shape::Path { path: gx.inverse() * to_path(&faces[i].0), closed: true }, layer);
                it.fill = paint;
                it.stroke = ops::default_stroke(0.0);
                it.locked = true;
                it.extra.insert("lpFace".into(), json!(true));
                made.push(d.insert_item(it, formdesign_model::doc::Home::Group(gid)));
            }
            if let Some(ItemKind::Group { children }) = d.item_mut(gid).map(|g| &mut g.kind) {
                children.retain(|c| !made.contains(c));
                children.splice(0..0, made);
            }
        }
        let sig = live_paint_sig(d, gid);
        if let Some(g) = d.item_mut(gid)
            && let Some(Value::Object(lp)) = g.extra.get_mut("livePaint")
        {
            lp.insert("sig".into(), json!(sig));
        }
    }
}

fn live_paint_make(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let doc = c.doc()?;
    let ids = paint_order(doc, &ids);
    regions(doc, &ids)?;
    let doc = c.doc_mut()?;
    let gid = ops::group(doc, &ids)?;
    if let Some(g) = doc.item_mut(gid) {
        g.extra.insert("livePaint".into(), json!({"fills": []}));
    }
    c.set_selection(Selection::items(vec![gid]));
    c.label("Make Fill Regions");
    Ok(json!({"group": gid}))
}

fn live_paint_end(c: &mut Ctx, p: &Value, release: bool) -> Result<Value> {
    let ids = items(c, p)?;
    let doc = c.doc()?;
    let groups: Vec<ItemId> = ids.iter().filter_map(|id| live_paint_group(doc, &[*id])).collect();
    if groups.is_empty() {
        return Err(EngineError::new("select a fill regions group (made with Make Fill Regions)"));
    }
    let doc = c.doc_mut()?;
    let mut out = Vec::new();
    for gid in groups {
        let kids = doc.item(gid).map(|g| g.children()).unwrap_or_default();
        for k in kids {
            let face = doc.item(k).is_some_and(|it| it.extra.contains_key("lpFace"));
            if face && release {
                doc.remove_item(k);
            } else if face && let Some(it) = doc.item_mut(k) {
                it.extra.remove("lpFace");
                it.locked = false;
            }
        }
        if let Some(g) = doc.item_mut(gid) {
            g.extra.remove("livePaint");
        }
        if release {
            out.extend(ops::ungroup(doc, gid)?);
        } else {
            out.push(gid);
        }
    }
    c.set_selection(Selection::items(out.clone()));
    c.label(if release { "Release Fill Regions" } else { "Expand Fill Regions" });
    Ok(json!({"ids": out}))
}

fn live_paint_expand(c: &mut Ctx, p: &Value) -> Result<Value> {
    live_paint_end(c, p, false)
}

fn live_paint_release(c: &mut Ctx, p: &Value) -> Result<Value> {
    live_paint_end(c, p, true)
}

fn shape_builder(c: &mut Ctx, p: &Value) -> Result<Value> {
    let pts: Vec<[f64; 2]> = p.get("points").and_then(Value::as_array).map(|a| a.iter().filter_map(|q| {
        let q = q.as_array()?;
        Some([q.first()?.as_f64()?, q.get(1)?.as_f64()?])
    }).collect()).unwrap_or_default();
    if pts.is_empty() {
        return Err(EngineError::new("`points` needs one or more [x, y] points in spread coordinates: the regions to merge or erase"));
    }
    let erase = args::opt_str(p, "mode") == Some("erase");
    let ids = items(c, p)?;
    let doc = c.doc()?;
    let ids = paint_order(doc, &ids);
    if ids.len() < 2 {
        return Err(EngineError::new("Shape Crafter needs two or more overlapping closed shapes selected"));
    }
    let mut shapes = Vec::new();
    for id in &ids {
        shapes.push(world_shape(doc, *id).ok_or_else(|| EngineError::new(format!("object {id} is a group, an open path or a line: Shape Crafter works on closed shapes")))?);
    }
    let faces = faces_of(&shapes, None);
    let hit: Vec<bool> = faces.iter().map(|(s, _)| pts.iter().any(|q| s.iter().any(|shape| inside(shape, q[0], q[1])))).collect();
    if !hit.iter().any(|h| *h) {
        return Err(EngineError::new("none of the points falls inside the selected shapes"));
    }
    let mut out: Vec<(Shapes, usize)> = Vec::new();
    let mut merged: Option<(Shapes, usize)> = None;
    let first = faces.iter().find(|(s, _)| s.iter().any(|shape| inside(shape, pts[0][0], pts[0][1]))).map(|f| f.1);
    for ((s, owner), h) in faces.into_iter().zip(hit) {
        if !h {
            out.push((s, owner));
        } else if !erase {
            merged = Some(match merged.take() {
                None => (s, owner),
                Some((m, o)) => (m.overlay(&s, OverlayRule::Union, FillRule::NonZero), o),
            });
        }
    }
    if let Some((m, o)) = merged {
        out.push((m, first.unwrap_or(o)));
    }
    let doc = c.doc_mut()?;
    let mut made = Vec::new();
    for (s, owner) in &out {
        let Some(nid) = ops::duplicate(doc, ids[*owner], kurbo::Vec2::ZERO, None) else { continue };
        let local = doc.world_xf(nid).inverse() * to_path(s);
        if let Some(it) = doc.item_mut(nid) {
            it.shape = Shape::Path { path: local, closed: true };
            it.corner = Default::default();
            it.extra.insert("fillRule".into(), json!("nonzero"));
            it.kind = ItemKind::Plain;
        }
        made.push(nid);
    }
    for id in &ids {
        doc.remove_item(*id);
    }
    c.set_selection(Selection::items(made.clone()));
    c.label("Shape Crafter");
    Ok(json!({"ids": made, "shapes": made.len()}))
}

fn knife(c: &mut Ctx, p: &Value) -> Result<Value> {
    let pts: Vec<kurbo::Point> = p.get("points").and_then(Value::as_array).map(|a| a.iter().filter_map(|q| {
        let q = q.as_array()?;
        Some(kurbo::Point::new(q.first()?.as_f64()?, q.get(1)?.as_f64()?))
    }).collect()).unwrap_or_default();
    if pts.len() < 2 {
        return Err(EngineError::new("`points` needs two or more [x, y] points in spread coordinates: the line to cut along"));
    }
    let mut blade = BezPath::new();
    blade.move_to(pts[0]);
    for q in &pts[1..] {
        blade.line_to(*q);
    }
    let slit = outline_of(&blade, &stroke_style(0.05, "miter", "butt", 4.0, &[]));
    let ids = items(c, p)?;
    let doc = c.doc()?;
    let mut cut: Vec<(ItemId, Shapes)> = Vec::new();
    for id in &ids {
        let Some(s) = world_shape(doc, *id) else { continue };
        let parts = s.overlay(&slit, OverlayRule::Difference, FillRule::NonZero);
        let pieces: Shapes = parts.into_iter().filter(|sh| sh.iter().any(|c| c.len() > 2)).collect();
        if pieces.len() > 1 {
            cut.push((*id, pieces));
        }
    }
    if cut.is_empty() {
        return Err(EngineError::new("the line does not cut through any selected closed shape"));
    }
    let doc = c.doc_mut()?;
    let mut made = Vec::new();
    for (id, pieces) in &cut {
        for piece in pieces {
            let Some(nid) = ops::duplicate(doc, *id, kurbo::Vec2::ZERO, None) else { continue };
            let local = doc.world_xf(nid).inverse() * to_path(&vec![piece.clone()]);
            if let Some(it) = doc.item_mut(nid) {
                it.shape = Shape::Path { path: local, closed: true };
                it.corner = Default::default();
                it.extra.insert("fillRule".into(), json!("nonzero"));
                if matches!(it.kind, ItemKind::Text(_)) {
                    it.kind = ItemKind::Plain;
                }
            }
            made.push(nid);
        }
        doc.remove_item(*id);
    }
    c.set_selection(Selection::items(made.clone()));
    c.label("Knife");
    Ok(json!({"ids": made, "pieces": made.len()}))
}

fn stroke_style(width: f64, join: &str, cap: &str, miter: f64, dash: &[f64]) -> kurbo::Stroke {
    let join = match join { "round" => kurbo::Join::Round, "bevel" => kurbo::Join::Bevel, _ => kurbo::Join::Miter };
    let cap = match cap { "round" => kurbo::Cap::Round, "projecting" | "square" => kurbo::Cap::Square, _ => kurbo::Cap::Butt };
    let s = kurbo::Stroke::new(width).with_join(join).with_caps(cap).with_miter_limit(miter.max(1.0));
    if dash.iter().any(|d| *d > 0.0) { s.with_dashes(0.0, dash.iter().copied()) } else { s }
}

fn seam_mid(path: &BezPath) -> BezPath {
    use kurbo::{ParamCurve, PathSeg};
    let mut out = BezPath::new();
    let mut sub: Vec<PathEl> = Vec::new();
    let flush = |sub: &mut Vec<PathEl>, out: &mut BezPath, closed: bool| {
        let els = std::mem::take(sub);
        let Some(PathEl::MoveTo(p0)) = els.first().copied() else { for e in els { out.push(e) } return };
        if !closed || els.len() < 2 {
            for e in els { out.push(e) }
            if closed { out.close_path() }
            return;
        }
        let seg = match els[1] {
            PathEl::LineTo(p) => PathSeg::Line(kurbo::Line::new(p0, p)),
            PathEl::QuadTo(c, p) => PathSeg::Quad(kurbo::QuadBez::new(p0, c, p)),
            PathEl::CurveTo(a, b, p) => PathSeg::Cubic(kurbo::CubicBez::new(p0, a, b, p)),
            _ => { for e in els { out.push(e) } out.close_path(); return }
        };
        let (h1, h2) = seg.subdivide();
        let push = |out: &mut BezPath, s: PathSeg| match s {
            PathSeg::Line(l) => out.line_to(l.p1),
            PathSeg::Quad(q) => out.quad_to(q.p1, q.p2),
            PathSeg::Cubic(c) => out.curve_to(c.p1, c.p2, c.p3),
        };
        out.move_to(h2.start());
        push(out, h2);
        let mut last = seg.end();
        for e in &els[2..] {
            out.push(*e);
            if let PathEl::LineTo(p) | PathEl::QuadTo(_, p) | PathEl::CurveTo(_, _, p) = *e { last = p; }
        }
        if (last - p0).hypot() > 1e-9 { out.line_to(p0); }
        push(out, h1);
        out.close_path();
    };
    for el in path.elements() {
        match el {
            PathEl::MoveTo(_) => { if !sub.is_empty() { flush(&mut sub, &mut out, false); } sub.push(*el); }
            PathEl::ClosePath => flush(&mut sub, &mut out, true),
            e => sub.push(*e),
        }
    }
    if !sub.is_empty() { flush(&mut sub, &mut out, false); }
    out
}

fn outline_of(path: &BezPath, style: &kurbo::Stroke) -> Shapes {
    let out = kurbo::stroke(seam_mid(path).iter(), style, &kurbo::StrokeOpts::default(), 0.02);
    let empty: Contours = Vec::new();
    contours(&out).overlay(&empty, OverlayRule::Subject, FillRule::NonZero)
}

fn outline_stroke(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let doc = c.doc_mut()?;
    let mut done = Vec::new();
    for id in &ids {
        let Some(it) = doc.item(*id) else { continue };
        if matches!(it.kind, ItemKind::Group { .. } | ItemKind::States { .. } | ItemKind::Text(_)) || it.stroke.weight <= 0.0 || matches!(it.stroke.paint, Paint::None) {
            continue;
        }
        let path = it.shape.to_path(&it.corner);
        let style = stroke_style(it.stroke.weight, &it.stroke.join, &it.stroke.cap, args::opt_f64(p, "miterLimit").unwrap_or(it.stroke.miter), &it.stroke.dash);
        let prof = formdesign_model::widths::profile_of(&it.extra);
        let nib = it.extra.get("brush").filter(|b| b.get("kind").and_then(Value::as_str) == Some("calligraphic")).map(|b| {
            let n = |k: &str, d: f64| b.get(k).and_then(Value::as_f64).unwrap_or(d);
            formdesign_model::widths::nib(&path, n("size", it.stroke.weight), n("angle", 45.0), n("roundness", 30.0), it.shape.closed())
        });
        let even_odd = nib.is_some() || prof.is_some();
        let outline = match (nib, &prof) {
            (Some(o), _) => o,
            (None, Some(pr)) => formdesign_model::widths::outline(&path, pr, it.stroke.weight, it.shape.closed()),
            (None, None) => to_path(&outline_of(&path, &style)),
        };
        if outline.elements().is_empty() {
            continue;
        }
        let paint = it.stroke.paint.clone();
        let keep = !matches!(it.fill, Paint::None) || !matches!(it.kind, ItemKind::Plain);
        let target = if keep { ops::duplicate(doc, *id, kurbo::Vec2::ZERO, None).unwrap_or(*id) } else { *id };
        if target != *id
            && let Some(orig) = doc.item_mut(*id)
        {
            orig.stroke.paint = Paint::None;
            orig.stroke.weight = 0.0;
        }
        if let Some(it) = doc.item_mut(target) {
            it.shape = Shape::Path { path: outline, closed: true };
            it.corner = Default::default();
            it.fill = paint;
            it.stroke.paint = Paint::None;
            it.stroke.weight = 0.0;
            it.extra.insert("fillRule".into(), json!(if even_odd { "evenodd" } else { "nonzero" }));
            it.extra.remove("widthProfile");
            it.extra.remove("brush");
            it.kind = ItemKind::Plain;
        }
        if target != *id
            && let Some(orig) = doc.item_mut(*id)
        {
            orig.extra.remove("widthProfile");
            orig.extra.remove("brush");
        }
        done.push(target);
    }
    if done.is_empty() {
        return Err(EngineError::new("Outline Stroke needs a selected path or frame with a visible stroke"));
    }
    c.set_selection(Selection::items(done.clone()));
    c.label("Outline Stroke");
    Ok(json!({"ids": done}))
}

fn offset_path(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let distance = args::opt_f64(p, "offset").or_else(|| args::opt_f64(p, "distance")).unwrap_or(6.0);
    if distance.abs() < 1e-6 {
        return Err(EngineError::new("`offset` must not be 0"));
    }
    let join = args::opt_str(p, "join").unwrap_or("miter").to_string();
    let miter = args::opt_f64(p, "miterLimit").unwrap_or(4.0);
    let copy = args::opt_bool(p, "copy").unwrap_or(true);
    let doc = c.doc_mut()?;
    let mut made = Vec::new();
    for id in &ids {
        let Some(it) = doc.item(*id) else { continue };
        if matches!(it.kind, ItemKind::Group { .. } | ItemKind::States { .. }) {
            continue;
        }
        let path = it.shape.to_path(&it.corner);
        let ring = outline_of(&path, &stroke_style(distance.abs() * 2.0, &join, "butt", miter, &[]));
        let result = if it.shape.closed() {
            let empty: Contours = Vec::new();
            let base = contours(&path).overlay(&empty, OverlayRule::Subject, FillRule::NonZero);
            base.overlay(&ring, if distance > 0.0 { OverlayRule::Union } else { OverlayRule::Difference }, FillRule::NonZero)
        } else {
            ring
        };
        let out = to_path(&result);
        if out.elements().is_empty() {
            continue;
        }
        let target = if copy { ops::duplicate(doc, *id, kurbo::Vec2::ZERO, None).unwrap_or(*id) } else { *id };
        if let Some(t) = doc.item_mut(target) {
            t.shape = Shape::Path { path: out, closed: true };
            t.corner = Default::default();
            t.extra.insert("fillRule".into(), json!("nonzero"));
            if matches!(t.kind, ItemKind::Text(_) | ItemKind::Graphic { .. }) {
                t.kind = ItemKind::Plain;
            }
        }
        made.push(target);
    }
    if made.is_empty() {
        return Err(EngineError::new("Offset Path needs a selected path, frame or shape; an inset larger than the shape leaves nothing"));
    }
    c.set_selection(Selection::items(made.clone()));
    c.label("Offset Path");
    Ok(json!({"ids": made}))
}

pub fn register(r: &mut Registry) {
    cmd("path.livePaint", "Fill Regions Bucket").menu(&["Object", "Fill Regions", "Fill Regions..."]).needs(Needs::Items).sig("{points: [[x, y], ...] (inside the regions to fill), swatch, tint?, ids? (a live paint group, or a member of one, keeps the fills live: they follow the regions as the paths change)}")
        .help("Fills the regions under the given points with a colour, where regions are formed by the selected closed shapes overlapping and by open paths crossing them; each fill is a new shape placed behind the selection, so the lines stay on top.").reg(r, live_paint);
    cmd("path.livePaintMake", "Make Fill Regions").menu(&["Object", "Fill Regions", "Make"]).needs(Needs::Items).sig("{ids?: closed shapes and open paths}").reg(r, live_paint_make);
    cmd("path.livePaintExpand", "Expand Fill Regions").menu(&["Object", "Fill Regions", "Expand"]).needs(Needs::Items).sig("{ids?: live paint groups}").reg(r, live_paint_expand);
    cmd("path.livePaintRelease", "Release Fill Regions").menu(&["Object", "Fill Regions", "Release"]).needs(Needs::Items).sig("{ids?: live paint groups}").reg(r, live_paint_release);
    cmd("path.shapeBuilder", "Shape Crafter").menu(&["Object", "Paths", "Shape Crafter..."]).needs(Needs::Items).sig("{points: [[x, y], ...] (spread points inside the regions), mode?: merge|erase, ids?}")
        .help("Splits the selected overlapping shapes into their regions, then merges the regions under the given points into one shape (or erases them with mode erase), keeping every other region as its own shape.").reg(r, shape_builder);
    cmd("path.knife", "Knife").menu(&["Object", "Paths", "Knife..."]).needs(Needs::Items).sig("{points: [[x, y], ...] (a line or polyline in spread points), ids?}")
        .help("Cuts every selected closed shape that the line crosses into separate shapes along it, each keeping its fill and stroke.").reg(r, knife);
    cmd("path.offset", "Offset Path").menu(&["Object", "Paths", "Offset Path..."]).needs(Needs::Items)
        .sig("{offset: pt (negative insets), join?: miter|round|bevel, miterLimit?, copy?: bool (default true), ids?}")
        .help("Makes a new path a set distance outside (or, with a negative offset, inside) each selected shape, with mitred, round or bevelled corners. An open path gets the closed outline around it.").reg(r, offset_path);
    cmd("path.outlineStroke", "Outline Stroke").menu(&["Object", "Paths", "Outline Stroke"]).needs(Needs::Items)
        .sig("{miterLimit?, ids?}")
        .help("Turns each selected object's stroke into a filled shape in the stroke's colour (caps, joins and dashes included), so it can be edited and combined with Pathfinder. The fill is replaced.").reg(r, outline_stroke);
    cmd("object.pathfinder", "Pathfinder").menu(&["Object", "Pathfinder"]).needs(Needs::Items).sig("{op: add|subtract|intersect|excludeOverlap|minusBack|divide|trim|merge|crop|outline, ids?}").help("Combines closed shapes: Add unites them, Subtract cuts the front shapes out of the back one, Intersect keeps their overlap, Exclude Overlap keeps the parts that do not overlap, Minus Back cuts the back shapes out of the front one. Divide splits them into every separate face, each in the colour of the shape on top there; Trim removes the hidden parts of each shape; Merge trims and joins touching shapes of the same fill; Crop keeps what lies inside the front shape; Outline gives each face's edges as strokes in its fill colour. Curves come out as fine straight segments.").reg(r, pathfinder);
    cmd("object.qrCode", "Generate QR Code").menu(&["Object", "Generate QR Code..."]).sig("{text, swatch?, quietZone?: modules, ids?: a frame to fill or a QR code to edit, spread?, rect?}").reg(r, qr_code);
    cmd("type.createOutlines", "Create Outlines").menu(&["Type", "Create Outlines"]).key("Mod+Shift+O").needs(Needs::Items).sig("{ids?}").help("Turns the selected text frames into filled paths, one per text colour, grouped when there are several. Anchored objects in the text are not kept.").reg(r, create_outlines);
    cmd("anchored.insert", "Anchor Object").sig("{ids, position?: inline|aboveLine|custom}").help("Moves the objects into the text at the text cursor as anchored objects.").reg(r, anchored_insert);
    cmd("anchored.options", "Anchored Object Options").menu(&["Object", "Anchored Object", "Options..."]).needs(Needs::Items).sig("{position?: inline|aboveLine|custom, xOffset?, yOffset?, ids?}").reg(r, anchored_options);
    cmd("anchored.release", "Release Anchored Object").menu(&["Object", "Anchored Object", "Release"]).needs(Needs::Items).sig("{ids?}").reg(r, anchored_release);
}
