use std::sync::Arc;

use formdesign_model::color::Paint;
use formdesign_model::doc::{Doc, Home};
use formdesign_model::geom::{Affine, Point, Rect, Vec2};
use formdesign_model::ids::{ItemId, LayerId, StoryId};
use formdesign_model::item::{Graphic, Item, ItemKind, Shape, Stroke, TextFrame};
use formdesign_model::story::Para;

use crate::{EngineError, Result};

pub fn active_layer(doc: &Doc) -> LayerId {
    doc.layers.iter().find(|l| l.visible && !l.locked).or(doc.layers.first()).map(|l| l.id).unwrap_or(LayerId(0))
}

pub fn page_origin(doc: &Doc, abs: usize) -> Result<(usize, f64)> {
    let (si, pi) = doc.page_slot(abs).ok_or_else(|| EngineError::new(format!("no page {}", abs + 1)))?;
    Ok((si, doc.spreads[si].page_x(pi)))
}

pub fn black() -> Paint {
    Paint::swatch("Black")
}

pub fn frame_kind(doc: &mut Doc, content: &str, text: Option<&str>, style: &str) -> (ItemKind, Option<StoryId>) {
    match content {
        "text" => {
            let sid = doc.new_story(style);
            if let Some(t) = text
                && let Some(st) = doc.story_mut(sid)
            {
                st.set_text(t);
            }
            (ItemKind::Text(TextFrame::new(sid)), Some(sid))
        }
        "graphic" | "image" => (ItemKind::Graphic(Graphic { asset: None, content: Affine::IDENTITY, fitting: None }), None),
        _ => (ItemKind::Plain, None),
    }
}

pub fn shape_for(kind: &str, r: Rect) -> Shape {
    match kind {
        "ellipse" | "oval" => Shape::Ellipse { rect: r },
        "polygon" => Shape::Polygon { rect: r, sides: 6, inset: 0.0 },
        "star" => Shape::Polygon { rect: r, sides: 5, inset: 0.5 },
        "line" => Shape::Line { from: (r.x0, r.y0), to: (r.x1, r.y1) },
        _ => Shape::Rect { rect: r },
    }
}

pub fn add_item(doc: &mut Doc, page: usize, local: Shape, kind: ItemKind, fill: Paint, stroke: Stroke) -> Result<ItemId> {
    let (si, x) = page_origin(doc, page)?;
    let layer = active_layer(doc);
    let mut it = Item::new(ItemId(0), kind, local, layer);
    it.xf = Affine::translate((x, 0.0));
    it.fill = fill;
    it.stroke = stroke;
    let story = it.story();
    let id = doc.insert_item(it, Home::Spread(si));
    if let Some(sid) = story
        && let Some(st) = doc.story_mut(sid)
    {
        st.frames.push(id);
    }
    Ok(id)
}

pub fn default_stroke(weight: f64) -> Stroke {
    Stroke { paint: if weight > 0.0 { black() } else { Paint::None }, weight, ..Stroke::default() }
}

pub fn text_para(style: &str, text: &str) -> Para {
    Para::from_text(style, text)
}

pub fn ref_point(b: Rect, name: &str) -> Point {
    let (x, y) = match name {
        "topLeft" | "tl" => (b.x0, b.y0),
        "top" | "tc" => (b.center().x, b.y0),
        "topRight" | "tr" => (b.x1, b.y0),
        "left" | "ml" => (b.x0, b.center().y),
        "right" | "mr" => (b.x1, b.center().y),
        "bottomLeft" | "bl" => (b.x0, b.y1),
        "bottom" | "bc" => (b.center().x, b.y1),
        "bottomRight" | "br" => (b.x1, b.y1),
        _ => (b.center().x, b.center().y),
    };
    Point::new(x, y)
}

pub fn union_bounds(doc: &Doc, ids: &[ItemId]) -> Option<Rect> {
    ids.iter().filter_map(|i| doc.bounds(*i)).reduce(|a, b| a.union(b))
}

pub fn transform_items(doc: &mut Doc, ids: &[ItemId], m: Affine) {
    for id in ids {
        let parent = doc.item(*id).and_then(|i| i.group).map(|g| doc.world_xf(g)).unwrap_or(Affine::IDENTITY);
        let local_m = parent.inverse() * m * parent;
        if let Some(it) = doc.item_mut(*id) {
            it.xf = local_m * it.xf;
        }
    }
}

pub fn move_items(doc: &mut Doc, ids: &[ItemId], d: Vec2) {
    transform_items(doc, ids, Affine::translate(d));
}

pub fn duplicate(doc: &mut Doc, id: ItemId, offset: Vec2, home: Option<Home>) -> Option<ItemId> {
    let src = doc.item(id)?.clone();
    let home = home.or_else(|| doc.home(id))?;
    let mut copy = src.clone();
    copy.id = ItemId(0);
    copy.group = None;
    if matches!(home, Home::Spread(_) | Home::Parent(_)) {
        copy.xf = Affine::translate(offset) * copy.xf;
    }
    match &mut copy.kind {
        ItemKind::Group { children } => children.clear(),
        ItemKind::States { states, .. } => {
            for s in states.iter_mut() {
                s.items.clear();
            }
        }
        ItemKind::Text(tf) => {
            let old = tf.story;
            let new = doc.new_story("Basic Paragraph");
            let paras = doc.story(old).map(|s| s.paras.clone()).unwrap_or_default();
            if let Some(st) = doc.story_mut(new) {
                st.paras = paras;
            }
            tf.story = new;
        }
        _ => {}
    }
    let story = copy.story();
    let nid = doc.insert_item(copy, home);
    if let Some(sid) = story
        && let Some(st) = doc.story_mut(sid)
    {
        st.frames = vec![nid];
    }
    match &src.kind {
        ItemKind::Group { children } => {
            for c in children {
                duplicate(doc, *c, offset, Some(Home::Group(nid)));
            }
        }
        ItemKind::States { states, .. } => {
            for (si, s) in states.iter().enumerate() {
                for c in &s.items {
                    if let Some(cid) = duplicate(doc, *c, offset, Some(Home::Group(nid)))
                        && let Some(ItemKind::States { states, .. }) = doc.item_mut(nid).map(|i| &mut i.kind)
                        && let Some(st) = states.get_mut(si)
                    {
                        st.items.push(cid);
                    }
                }
            }
        }
        _ => {}
    }
    Some(nid)
}

pub fn home_list<'a>(doc: &'a mut Doc, home: &Home) -> Option<&'a mut Vec<ItemId>> {
    match home {
        Home::Spread(si) => doc.spreads.get_mut(*si).map(|s| &mut s.items),
        Home::Parent(pi) => doc.parents.get_mut(*pi).map(|p| &mut p.items),
        Home::Group(g) => match doc.items.get_mut(g).map(Arc::make_mut).map(|i| &mut i.kind) {
            Some(ItemKind::Group { children }) => Some(children),
            _ => None,
        },
        Home::Anchored(_) => None,
    }
}

pub fn arrange(doc: &mut Doc, ids: &[ItemId], op: &str) {
    for id in ids {
        let Some(home) = doc.home(*id) else { continue };
        let Some(list) = home_list(doc, &home) else { continue };
        let Some(i) = list.iter().position(|x| x == id) else { continue };
        let it = list.remove(i);
        let j = match op {
            "front" | "bringToFront" => list.len(),
            "forward" | "bringForward" => (i + 1).min(list.len()),
            "backward" | "sendBackward" => i.saturating_sub(1),
            _ => 0,
        };
        list.insert(j, it);
    }
}

pub fn group(doc: &mut Doc, ids: &[ItemId]) -> Result<ItemId> {
    if ids.len() < 2 {
        return Err(EngineError::new("select two or more items to group"));
    }
    let home = doc.home(ids[0]).ok_or_else(|| EngineError::new("no such item"))?;
    let layer = doc.item(ids[0]).map(|i| i.layer).unwrap_or(active_layer(doc));
    let list = home_list(doc, &home).cloned().unwrap_or_default();
    let mut ordered: Vec<ItemId> = list.iter().copied().filter(|x| ids.contains(x)).collect();
    if ordered.len() != ids.len() {
        ordered = ids.to_vec();
    }
    let top_pos = list.iter().rposition(|x| ids.contains(x)).unwrap_or(list.len());
    let gxf = Affine::IDENTITY;
    let mut g = Item::new(ItemId(0), ItemKind::Group { children: Vec::new() }, Shape::Rect { rect: Rect::ZERO }, layer);
    g.xf = gxf;
    let gid = ItemId(doc.alloc());
    g.id = gid;
    for id in &ordered {
        doc.detach_from_home(*id);
    }
    doc.items.insert(gid, Arc::new(g));
    if let Some(l) = home_list(doc, &home) {
        let at = top_pos.saturating_sub(ordered.len() - 1).min(l.len());
        l.insert(at, gid);
    }
    for id in &ordered {
        if let Some(it) = doc.item_mut(*id) {
            it.group = Some(gid);
        }
    }
    if let Some(ItemKind::Group { children }) = doc.item_mut(gid).map(|i| &mut i.kind) {
        *children = ordered;
    }
    Ok(gid)
}

pub fn ungroup(doc: &mut Doc, gid: ItemId) -> Result<Vec<ItemId>> {
    let g = doc.item(gid).cloned().ok_or_else(|| EngineError::new("no such group"))?;
    let ItemKind::Group { children } = &g.kind else { return Err(EngineError::new("the selection is not a group")) };
    let home = doc.home(gid).ok_or_else(|| EngineError::new("no such group"))?;
    let pos = home_list(doc, &home).and_then(|l| l.iter().position(|x| *x == gid)).unwrap_or(0);
    for c in children {
        if let Some(it) = doc.item_mut(*c) {
            it.xf = g.xf * it.xf;
            it.group = None;
        }
    }
    if let Some(l) = home_list(doc, &home) {
        l.retain(|x| *x != gid);
        for (k, c) in children.iter().enumerate() {
            l.insert((pos + k).min(l.len()), *c);
        }
    }
    if let Home::Group(pg) = home {
        for c in children {
            if let Some(it) = doc.item_mut(*c) {
                it.group = Some(pg);
            }
        }
    }
    doc.items.remove(&gid);
    Ok(children.clone())
}

pub fn thread(doc: &mut Doc, from: ItemId, to: ItemId) -> Result<()> {
    let a = doc.item(from).and_then(|i| i.story()).ok_or_else(|| EngineError::new("the first frame is not a text frame"))?;
    let to_item = doc.item(to).cloned().ok_or_else(|| EngineError::new("no such frame"))?;
    if let Some(b) = to_item.story() {
        if a == b {
            return Err(EngineError::new("those frames are already in the same story"));
        }
        let (bframes, bparas) = doc.story(b).map(|s| (s.frames.clone(), s.paras.clone())).unwrap_or_default();
        if bframes.first() != Some(&to) {
            return Err(EngineError::new("the second frame is in the middle of another thread"));
        }
        let nonempty = bparas.iter().any(|p| !p.plain().is_empty());
        if let Some(sa) = doc.story_mut(a) {
            let pos = sa.frames.iter().position(|f| *f == from).map(|i| i + 1).unwrap_or(sa.frames.len());
            for (k, f) in bframes.iter().enumerate() {
                sa.frames.insert(pos + k, *f);
            }
            if nonempty {
                sa.paras.extend(bparas);
            }
        }
        for f in bframes {
            if let Some(ItemKind::Text(tf)) = doc.item_mut(f).map(|i| &mut i.kind) {
                tf.story = a;
            }
        }
        doc.stories.remove(&b);
    } else {
        if let Some(it) = doc.item_mut(to) {
            it.kind = ItemKind::Text(TextFrame::new(a));
        }
        if let Some(sa) = doc.story_mut(a) {
            let pos = sa.frames.iter().position(|f| *f == from).map(|i| i + 1).unwrap_or(sa.frames.len());
            sa.frames.insert(pos, to);
        }
    }
    Ok(())
}

pub fn unthread_after(doc: &mut Doc, id: ItemId) -> Result<()> {
    let sid = doc.item(id).and_then(|i| i.story()).ok_or_else(|| EngineError::new("not a text frame"))?;
    let frames = doc.story(sid).map(|s| s.frames.clone()).unwrap_or_default();
    let pos = frames.iter().position(|f| *f == id).ok_or_else(|| EngineError::new("frame not in its story"))?;
    if pos + 1 >= frames.len() {
        return Err(EngineError::new("there is no thread after this frame"));
    }
    if let Some(st) = doc.story_mut(sid) {
        st.frames.truncate(pos + 1);
    }
    for f in &frames[pos + 1..] {
        let nsid = doc.new_story("Basic Paragraph");
        if let Some(ItemKind::Text(tf)) = doc.item_mut(*f).map(|i| &mut i.kind) {
            tf.story = nsid;
        }
        if let Some(st) = doc.story_mut(nsid) {
            st.frames = vec![*f];
        }
    }
    Ok(())
}
