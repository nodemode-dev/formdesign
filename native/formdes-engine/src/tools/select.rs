use formdes_model::geom::{Affine, Point, Rect, Vec2};
use formdes_model::ids::ItemId;
use formdes_model::item::{ItemKind, map_rect};

use super::geo::{self, bounds, handles};
use super::{Ev, G, Gesture, Key, Kind, Mods, Overlay, View};
use crate::canvas::SpreadRef;
use crate::cmd::object::resize_items;
use crate::ops;
use crate::session::{Selection, Session, TextSel};
use crate::Result;

pub const SEL_COLOR: &str = "#4f99ff";

fn about(c: Point, m: Affine) -> Affine {
    Affine::translate(c.to_vec2()) * m * Affine::translate(-c.to_vec2())
}

pub struct Here {
    pub slot: SpreadRef,
    pub xf: Affine,
    pub p: Point,
}

pub fn here(ses: &Session, at: Point) -> Option<Here> {
    let st = ses.active()?;
    let (slot, xf) = geo::slot_at(st, at)?;
    Some(Here { slot, xf, p: xf.inverse() * at })
}

/// The isolated group's direct children, when a group is isolated and still a group.
pub fn isolated_children(ses: &Session, doc: &formdes_model::doc::Doc) -> Option<Vec<ItemId>> {
    let g = doc.item(ses.tools.isolated?)?;
    matches!(g.kind, ItemKind::Group { .. } | ItemKind::States { .. }).then(|| g.children())
}

/// The object a click picks: a top-level object, or inside an isolated group one of its direct children.
fn pick(ses: &Session, doc: &formdes_model::doc::Doc, slot: SpreadRef, p: Point, tol: f64) -> Option<ItemId> {
    match isolated_children(ses, doc) {
        Some(kids) => kids.into_iter().rev().find(|c| geo::selectable(doc, *c) && geo::hit_item(doc, *c, p, tol).is_some()),
        None => geo::hit_top(doc, slot, p, tol),
    }
}

fn sel_in(ses: &Session, slot: SpreadRef) -> Vec<ItemId> {
    let Some(st) = ses.active() else { return Vec::new() };
    st.selection.items.iter().copied().filter(|i| geo::spread_of_item(&st.doc, *i) == Some(slot)).collect()
}

pub fn start(ses: &mut Session, ev: &Ev, h: &Here, kind: G, edit: bool) {
    let before = if edit { ses.begin_edit() } else { None };
    ses.tools.gesture = Some(Gesture { kind, start: ev.pos, start_sp: h.p, slot: h.slot, slot_xf: h.xf, before, moved: false, mods: ev.mods });
}

fn out_port(ses: &Session, id: ItemId) -> Option<Point> {
    let st = ses.active()?;
    let it = st.doc.item(id)?;
    it.text()?;
    let b = it.shape.bbox();
    Some(st.doc.world_xf(id) * Point::new(b.x1, b.y1))
}

pub fn content_box(ses: &Session, id: ItemId) -> Option<Rect> {
    let st = ses.active()?;
    let it = st.doc.item(id)?;
    let g = it.graphic()?;
    let a = st.doc.assets.get(&g.asset?)?;
    let (w, h) = a.natural_size();
    Some((st.doc.world_xf(id) * g.content).transform_rect_bbox(Rect::new(0.0, 0.0, w, h)))
}

pub fn selection(ses: &mut Session, ev: &Ev, view: View, free: bool) -> Result<()> {
    match ev.kind {
        Kind::Down => {
            let Some(h) = here(ses, ev.pos) else { return Ok(()) };
            let tol = view.tol(4.0);
            let sel = sel_in(ses, h.slot);
            let doc = ses.active().map(|s| s.doc.clone()).expect("checked");
            if let Some(from) = ses.tools.thread.take() {
                start(ses, ev, &h, G::Thread { from, loaded: true }, false);
                return Ok(());
            }
            if sel.len() == 1
                && let Some(port) = out_port(ses, sel[0])
                && (port - kurbo::Vec2::new(0.0, view.tol(14.0)) - h.p).hypot() <= view.tol(7.0)
            {
                start(ses, ev, &h, G::Thread { from: sel[0], loaded: false }, false);
                return Ok(());
            }
            if let Some(b) = bounds(&doc, &sel) {
                for (i, hp) in handles(b).iter().enumerate() {
                    if (*hp - h.p).hypot() <= view.tol(6.0) {
                        start(ses, ev, &h, G::Resize { ids: sel.clone(), handle: i, bounds: b }, true);
                        return Ok(());
                    }
                }
                let outside = !b.inflate(view.tol(2.0), view.tol(2.0)).contains(h.p);
                if outside && [0usize, 2, 4, 6].iter().any(|i| (handles(b)[*i] - h.p).hypot() <= view.tol(if free { 24.0 } else { 18.0 })) {
                    start(ses, ev, &h, G::Rotate { ids: sel.clone(), center: b.center() }, true);
                    return Ok(());
                }
            }
            let content = ses.active().is_some_and(|s| s.selection.content);
            match pick(ses, &doc, h.slot, h.p, tol) {
                Some(id) => {
                    if content && sel == [id] && content_box(ses, id).is_some_and(|r| r.contains(h.p)) {
                        start(ses, ev, &h, G::Content { item: id }, true);
                        return Ok(());
                    }
                    let mut items = ses.active().map(|s| s.selection.items.clone()).unwrap_or_default();
                    if ev.mods.shift {
                        if let Some(i) = items.iter().position(|x| *x == id) {
                            items.remove(i);
                        } else {
                            items.push(id);
                        }
                    } else if !items.contains(&id) {
                        items = vec![id];
                    }
                    let keep = items.contains(&id);
                    ses.select(Selection::items(items.clone()));
                    if keep {
                        start(ses, ev, &h, G::Move { ids: items, dup: ev.mods.alt, copies: Vec::new() }, true);
                    }
                }
                None => {
                    if !ev.mods.shift {
                        ses.select(Selection::default());
                    }
                    start(ses, ev, &h, G::Marquee { add: ev.mods.shift }, false);
                }
            }
        }
        Kind::Drag => drag(ses, ev, view),
        Kind::Up => {
            drag(ses, ev, view);
            finish(ses, ev, view);
        }
        Kind::Double => {
            let Some(h) = here(ses, ev.pos) else { return Ok(()) };
            let doc = ses.active().map(|s| s.doc.clone()).expect("checked");
            let tol = view.tol(4.0);
            let Some(id) = pick(ses, &doc, h.slot, h.p, tol) else {
                if let Some(g) = ses.tools.isolated.take() {
                    ses.select(Selection::items(vec![g]));
                    ses.sel_rev += 1;
                }
                return Ok(());
            };
            let it = doc.item(id).cloned();
            match it.as_ref().map(|i| &i.kind) {
                Some(ItemKind::Text(_)) => {
                    let r = ses.renderer.clone();
                    let lay = ses.active_mut().map(|s| s.layout(&r));
                    if let (Some(lay), Some(st)) = (lay, ses.active())
                        && let Some((sid, pos)) = geo::text_hit(st, &lay, id, h.p)
                    {
                        ses.tool = "type".into();
                        ses.select(Selection::text(TextSel::caret(sid, pos)));
                    }
                }
                Some(ItemKind::Group { .. } | ItemKind::States { .. }) => {
                    ses.tools.isolated = Some(id);
                    ses.sel_rev += 1;
                    let kids = it.as_ref().map(|i| i.children()).unwrap_or_default();
                    if let Some(c) = kids.into_iter().rev().find(|c| geo::hit_item(&doc, *c, h.p, tol).is_some()) {
                        ses.select(Selection::items(vec![c]));
                    }
                }
                Some(ItemKind::Graphic(g)) if g.asset.is_some() => {
                    let on = !ses.active().is_some_and(|s| s.selection.content);
                    ses.select(Selection { items: vec![id], content: on, ..Selection::default() });
                }
                _ => {}
            }
        }
        Kind::Move => {
            ses.tools.overlays.clear();
            if let Some(from) = ses.tools.thread
                && let Some(st) = ses.active()
                && let Some(r) = geo::spread_of_item(&st.doc, st.doc.top_of(from))
                && let Some(port) = out_port(ses, from)
            {
                let a = geo::slot_xf(st, r) * port;
                ses.tools.overlays.push(Overlay::Line { a, b: ev.pos, color: SEL_COLOR.into(), dashed: true });
            }
        }
    }
    Ok(())
}

pub fn drag(ses: &mut Session, ev: &Ev, view: View) {
    let Some(mut g) = ses.tools.gesture.clone() else { return };
    let p = g.slot_xf.inverse() * ev.pos;
    if (ev.pos - g.start).hypot() > view.tol(2.0) {
        g.moved = true;
    }
    let tol = view.tol(6.0);
    ses.tools.overlays.clear();
    match &mut g.kind {
        G::Move { ids, dup, copies } => {
            if !g.moved {
                ses.tools.gesture = Some(g);
                return;
            }
            let Some(before) = g.before.clone() else { return };
            let mut d = p - g.start_sp;
            if ev.mods.shift {
                if d.x.abs() > d.y.abs() { d.y = 0.0 } else { d.x = 0.0 }
            }
            if view.snap
                && let Some(b) = bounds(&before.doc, ids)
            {
                d = geo::snaps(&before.doc, g.slot, ids).delta(b, d, tol);
            }
            let ids2 = ids.clone();
            let dup2 = *dup;
            let mut made = Vec::new();
            ses.preview(&before, |doc| {
                let targets = if dup2 { ids2.iter().filter_map(|id| ops::duplicate(doc, *id, Vec2::ZERO, None)).collect() } else { ids2.clone() };
                ops::move_items(doc, &targets, d);
                made = targets;
            });
            if dup2 {
                *copies = made;
            }
        }
        G::Resize { ids, handle, bounds: b } => {
            let Some(before) = g.before.clone() else { return };
            let q = if view.snap { geo::snaps(&before.doc, g.slot, ids).point(p, tol) } else { p };
            let (mut x0, mut y0, mut x1, mut y1) = (b.x0, b.y0, b.x1, b.y1);
            let i = *handle;
            if matches!(i, 0 | 6 | 7) {
                x0 = q.x;
            }
            if matches!(i, 2..=4) {
                x1 = q.x;
            }
            if matches!(i, 0..=2) {
                y0 = q.y;
            }
            if matches!(i, 4..=6) {
                y1 = q.y;
            }
            if ev.mods.shift && matches!(i, 0 | 2 | 4 | 6) && b.width() > 1e-6 && b.height() > 1e-6 {
                let s = ((x1 - x0) / b.width()).abs().max(((y1 - y0) / b.height()).abs());
                let (ax, ay) = (if matches!(i, 0 | 6) { b.x1 } else { b.x0 }, if matches!(i, 0 | 2) { b.y1 } else { b.y0 });
                let (sx, sy) = (if matches!(i, 0 | 6) { -1.0 } else { 1.0 }, if matches!(i, 0 | 2) { -1.0 } else { 1.0 });
                let (nx, ny) = (ax + sx * b.width() * s, ay + sy * b.height() * s);
                (x0, x1) = (ax.min(nx), ax.max(nx));
                (y0, y1) = (ay.min(ny), ay.max(ny));
            }
            if ev.mods.alt {
                let c = b.center();
                let hw = if matches!(i, 1 | 5) { b.width() / 2.0 } else if matches!(i, 0 | 6 | 7) { c.x - x0 } else { x1 - c.x };
                let hh = if matches!(i, 3 | 7) { b.height() / 2.0 } else if matches!(i, 0..=2) { c.y - y0 } else { y1 - c.y };
                (x0, x1, y0, y1) = (c.x - hw.abs(), c.x + hw.abs(), c.y - hh.abs(), c.y + hh.abs());
            }
            let nb = Rect::new(x0, y0, x1, y1);
            if nb.width().abs() < 0.5 || nb.height().abs() < 0.5 {
                ses.tools.gesture = Some(g);
                return;
            }
            let m = map_rect(*b, nb);
            let ids2 = ids.clone();
            let content = ev.mods.cmd;
            ses.preview(&before, |doc| resize_items(doc, &ids2, m, content));
        }
        G::Rotate { ids, center } => {
            let Some(before) = g.before.clone() else { return };
            let a0 = (g.start_sp - *center).atan2();
            let mut da = (p - *center).atan2() - a0;
            if ev.mods.shift {
                let step = std::f64::consts::FRAC_PI_4;
                da = (da / step).round() * step;
            }
            let ids2 = ids.clone();
            let c = *center;
            ses.preview(&before, |doc| ops::transform_items(doc, &ids2, about(c, Affine::rotate(da))));
            ses.tools.overlays.push(Overlay::Measure { p: ev.pos, text: format!("{:.1}\u{00B0}", -da.to_degrees()) });
        }
        G::Scale { ids, center } => {
            let Some(before) = g.before.clone() else { return };
            let s0 = g.start_sp - *center;
            let s1 = p - *center;
            let f = |a: f64, b: f64| if a.abs() < 1e-6 { 1.0 } else { b / a };
            let (mut sx, mut sy) = (f(s0.x, s1.x), f(s0.y, s1.y));
            if ev.mods.shift {
                let k = if s0.hypot() < 1e-6 { 1.0 } else { s1.hypot() / s0.hypot() };
                (sx, sy) = (k, k);
            }
            let ids2 = ids.clone();
            let c = *center;
            ses.preview(&before, |doc| ops::transform_items(doc, &ids2, about(c, Affine::scale_non_uniform(sx, sy))));
            ses.tools.overlays.push(Overlay::Measure { p: ev.pos, text: format!("{:.0}% x {:.0}%", sx * 100.0, sy * 100.0) });
        }
        G::Shear { ids, center } => {
            let Some(before) = g.before.clone() else { return };
            let dy = g.start_sp.y - center.y;
            let k = if dy.abs() < 1e-6 { 0.0 } else { (p.x - g.start_sp.x) / dy };
            let ids2 = ids.clone();
            let c = *center;
            ses.preview(&before, |doc| ops::transform_items(doc, &ids2, about(c, Affine::new([1.0, 0.0, k, 1.0, 0.0, 0.0]))));
        }
        G::Content { item } => {
            let Some(before) = g.before.clone() else { return };
            let d = p - g.start_sp;
            let id = *item;
            ses.preview(&before, |doc| {
                let inv = doc.world_xf(id).inverse();
                let ld = inv * Point::new(d.x, d.y) - inv * Point::ZERO;
                if let Some(ItemKind::Graphic(gr)) = doc.item_mut(id).map(|i| &mut i.kind) {
                    gr.content = Affine::translate(ld) * gr.content;
                }
            });
        }
        G::Marquee { .. } => ses.tools.overlays.push(Overlay::Marquee(Rect::from_points(g.start, ev.pos))),
        G::Thread { from, .. } => {
            if let Some(port) = out_port(ses, *from) {
                ses.tools.overlays.push(Overlay::Line { a: g.slot_xf * port, b: ev.pos, color: SEL_COLOR.into(), dashed: true });
            }
        }
        _ => {
            ses.tools.gesture = Some(g);
            return super::draw::drag(ses, ev, view);
        }
    }
    ses.tools.gesture = Some(g);
}

pub fn finish(ses: &mut Session, ev: &Ev, view: View) {
    let Some(g) = ses.tools.gesture.take() else { return };
    ses.tools.overlays.clear();
    let p = g.slot_xf.inverse() * ev.pos;
    match g.kind {
        G::Move { ids, dup, copies } => {
            if let Some(b) = &g.before {
                if g.moved {
                    ses.commit(b, if dup { "Duplicate" } else { "Move" });
                    if dup && !copies.is_empty() {
                        ses.select(Selection::items(copies));
                    }
                } else {
                    ses.restore(b);
                    let _ = ids;
                }
            }
        }
        G::Resize { .. } | G::Rotate { .. } | G::Scale { .. } | G::Shear { .. } | G::Content { .. } => {
            let label = match g.kind {
                G::Resize { .. } => "Resize",
                G::Rotate { .. } => "Rotate",
                G::Scale { .. } => "Scale",
                G::Shear { .. } => "Shear",
                _ => "Move Content",
            };
            if let Some(b) = &g.before {
                if g.moved { ses.commit(b, label) } else { ses.restore(b) }
            }
        }
        G::Marquee { add } => {
            let Some(st) = ses.active() else { return };
            let r = Rect::from_points(g.start_sp, p);
            if r.width() < view.tol(2.0) && r.height() < view.tol(2.0) {
                return;
            }
            let doc = &st.doc;
            let hits: Vec<ItemId> = isolated_children(ses, doc).unwrap_or_else(|| geo::top_items(doc, g.slot)).into_iter().filter(|i| geo::selectable(doc, *i) && doc.bounds(*i).is_some_and(|b| b.intersect(r).area() > 0.0 || (b.width() == 0.0 || b.height() == 0.0) && r.contains(b.center()))).collect();
            let mut items = if add { st.selection.items.clone() } else { Vec::new() };
            for h in hits {
                if !items.contains(&h) {
                    items.push(h);
                }
            }
            ses.select(Selection::items(items));
        }
        G::Thread { from, loaded } => {
            if !loaded && !g.moved {
                ses.tools.thread = Some(from);
                return;
            }
            let Some(st) = ses.active() else { return };
            let doc = st.doc.clone();
            let target = geo::hit_top(&doc, g.slot, p, view.tol(4.0)).filter(|t| *t != from);
            let src = doc.item(from).map(|i| i.shape.bbox()).unwrap_or(Rect::new(0.0, 0.0, 200.0, 200.0));
            let rect = if g.moved && (p - g.start_sp).hypot() > view.tol(8.0) && target.is_none() { Rect::from_points(g.start_sp, p) } else { Rect::new(p.x, p.y, p.x + src.width(), p.y + src.height()) };
            let slot = g.slot;
            let layer = ses.active().and_then(|s| s.active_layer).filter(|l| doc.layer(*l).is_some()).unwrap_or_else(|| ops::active_layer(&doc));
            ses.edit_now("Thread Text Frames", |d, sel| {
                let to = match target {
                    Some(t) => Some(t),
                    None => {
                        let mut it = formdes_model::item::Item::new(ItemId(0), ItemKind::Plain, formdes_model::item::Shape::Rect { rect }, layer);
                        it.style = Some(formdes_model::style::BASIC_TEXT.into());
                        let home = match slot {
                            SpreadRef::Doc(i) => formdes_model::doc::Home::Spread(i),
                            SpreadRef::Parent(i) => formdes_model::doc::Home::Parent(i),
                        };
                        Some(d.insert_item(it, home))
                    }
                };
                if let Some(to) = to
                    && ops::thread(d, from, to).is_ok()
                {
                    *sel = Selection::items(vec![to]);
                }
            });
        }
        G::None => {}
        _ => {
            ses.tools.gesture = Some(g);
            super::draw::finish(ses, ev, view);
        }
    }
}

fn anchor_at(doc: &formdes_model::doc::Doc, ids: &[ItemId], p: Point, tol: f64) -> Option<(ItemId, usize, usize, u8)> {
    for id in ids {
        let Some((subs, xf)) = geo::editable_anchors(doc, *id) else { continue };
        for (si, (a, _)) in subs.iter().enumerate() {
            for (ai, an) in a.iter().enumerate() {
                for (part, pt) in [(0u8, an.0), (1, an.1), (2, an.2)] {
                    if (part == 0 || (pt - an.0).hypot() > 1e-6) && (xf * pt - p).hypot() <= tol {
                        return Some((*id, si, ai, part));
                    }
                }
            }
        }
    }
    None
}

pub fn direct(ses: &mut Session, ev: &Ev, view: View) -> Result<()> {
    match ev.kind {
        Kind::Down => {
            let Some(h) = here(ses, ev.pos) else { return Ok(()) };
            let doc = ses.active().map(|s| s.doc.clone()).expect("checked");
            let sel = sel_in(ses, h.slot);
            let tol = view.tol(5.0);
            let hit = geo::hit_deep(&doc, h.slot, h.p, view.tol(4.0));
            let picked_now = ses.active().map(|s| s.selection.anchors.clone()).unwrap_or_default();
            for id in &sel {
                let xf = doc.world_xf(*id);
                let handles = geo::corner_handles(&doc, *id);
                let shown = |hd: &geo::CornerHandle| hd.vertex + hd.dir * (hd.widget - hd.vertex).hypot().max(view.tol(14.0));
                if let Some(hd) = handles.iter().find(|hd| (xf * shown(hd) - h.p).hypot() <= view.tol(6.0)) {
                    let mine: Vec<usize> = handles.iter().filter(|o| picked_now.contains(&(*id, o.sub, o.idx))).map(|o| o.flat).collect();
                    let flats = if mine.contains(&hd.flat) { mine } else { vec![hd.flat] };
                    start(ses, ev, &h, G::Corner { item: *id, flat: hd.flat, flats }, true);
                    return Ok(());
                }
            }
            let found = anchor_at(&doc, &sel, h.p, tol).map(|a| (a, sel.clone())).or_else(|| {
                let id = hit.filter(|i| !sel.contains(i))?;
                anchor_at(&doc, &[id], h.p, tol).map(|a| (a, vec![id]))
            });
            if let Some(((id, si, ai, part), items)) = found {
                let mut anchors = if ev.mods.shift && items == sel { ses.active().map(|s| s.selection.anchors.clone()).unwrap_or_default() } else { Vec::new() };
                if !anchors.contains(&(id, si, ai)) {
                    anchors.push((id, si, ai));
                }
                ses.select(Selection { items, anchors, ..Selection::default() });
                start(ses, ev, &h, G::Anchor { item: id, sub: si, idx: ai, part }, true);
                return Ok(());
            }
            match hit {
                Some(id) => {
                    if content_box(ses, id).is_some_and(|r| r.contains(h.p)) && doc.item(id).is_some_and(|i| !i.shape.to_path(&i.corner).segments().any(|s| kurbo::ParamCurveNearest::nearest(&s, doc.world_xf(id).inverse() * h.p, 1e-3).distance_sq < tol * tol)) {
                        ses.select(Selection { items: vec![id], content: true, ..Selection::default() });
                        start(ses, ev, &h, G::Content { item: id }, true);
                    } else {
                        ses.select(Selection::items(vec![id]));
                        start(ses, ev, &h, G::Move { ids: vec![id], dup: ev.mods.alt, copies: Vec::new() }, true);
                    }
                }
                None => {
                    ses.select(Selection::default());
                    start(ses, ev, &h, G::Marquee { add: ev.mods.shift }, false);
                }
            }
        }
        Kind::Drag | Kind::Up => {
            if let Some(g) = ses.tools.gesture.clone()
                && let G::Corner { item, flat, ref flats } = g.kind
            {
                let Some(before) = g.before.clone() else { return Ok(()) };
                let moved = (ev.pos - g.start).hypot() > view.tol(2.0);
                if moved {
                    let p = g.slot_xf.inverse() * ev.pos;
                    let flats = flats.clone();
                    ses.preview(&before, |doc| {
                        let xf = doc.world_xf(item);
                        let Some(hd) = geo::corner_handles(doc, item).into_iter().find(|o| o.flat == flat) else { return };
                        let r = geo::radius_for(&hd, xf.inverse() * p);
                        geo::apply_corners(doc, item, &flats, Some(r), None);
                    });
                    if let Some(gg) = ses.tools.gesture.as_mut() {
                        gg.moved = true;
                    }
                }
                if ev.kind == Kind::Up {
                    let g = ses.tools.gesture.take();
                    let did_move = g.as_ref().is_some_and(|g| g.moved);
                    if did_move {
                        ses.commit(&before, "Round Corners");
                    } else if g.as_ref().is_some_and(|g| g.mods.alt) {
                        let flats = flats.clone();
                        ses.preview(&before, |doc| {
                            let cur = doc.item(item).and_then(|it| if it.corner.live() { it.corner.each.get(flat).map(|c| c.kind.clone()) } else { Some(it.corner.kind.clone()) }).unwrap_or_default();
                            let next = match cur.as_str() { "rounded" => "inverse", "inverse" => "bevel", _ => "rounded" };
                            let hd = geo::corner_handles(doc, item).into_iter().find(|o| o.flat == flat);
                            let r = hd.map(|h| if h.radius > 0.0 { h.radius } else { 12.0 });
                            geo::apply_corners(doc, item, &flats, r, Some(next));
                        });
                        ses.commit(&before, "Corner Type");
                    } else {
                        ses.restore(&before);
                    }
                }
                return Ok(());
            }
            if let Some(g) = ses.tools.gesture.clone()
                && let G::Anchor { item, sub, idx, part } = g.kind
            {
                if (ev.pos - g.start).hypot() > view.tol(2.0)
                    && let Some(before) = g.before.clone()
                {
                    let p = g.slot_xf.inverse() * ev.pos;
                    let alt = ev.mods.alt;
                    ses.preview(&before, |doc| {
                        let Some((mut subs, xf)) = geo::editable_anchors(doc, item) else { return };
                        let lp = xf.inverse() * p;
                        if let Some(an) = subs.get_mut(sub).and_then(|s| s.0.get_mut(idx)) {
                            match part {
                                0 => {
                                    let d = lp - an.0;
                                    *an = (an.0 + d, an.1 + d, an.2 + d);
                                }
                                1 => {
                                    an.1 = lp;
                                    if !alt && (an.2 - an.0).hypot() > 1e-6 {
                                        let len = (an.2 - an.0).hypot();
                                        let dir = an.0 - lp;
                                        let n = dir.hypot().max(1e-9);
                                        an.2 = an.0 + dir * (len / n);
                                    }
                                }
                                _ => {
                                    an.2 = lp;
                                    if !alt && (an.1 - an.0).hypot() > 1e-6 {
                                        let len = (an.1 - an.0).hypot();
                                        let dir = an.0 - lp;
                                        let n = dir.hypot().max(1e-9);
                                        an.1 = an.0 + dir * (len / n);
                                    }
                                }
                            }
                        }
                        geo::set_anchors(doc, item, &subs);
                    });
                    if let Some(gg) = ses.tools.gesture.as_mut() {
                        gg.moved = true;
                    }
                }
                if ev.kind == Kind::Up {
                    let g = ses.tools.gesture.take();
                    if let Some(b) = g.as_ref().and_then(|g| g.before.clone()) {
                        if g.is_some_and(|g| g.moved) { ses.commit(&b, "Edit Path") } else { ses.restore(&b) }
                    }
                }
                return Ok(());
            }
            if ev.kind == Kind::Drag {
                drag(ses, ev, view);
            } else {
                drag(ses, ev, view);
                finish(ses, ev, view);
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn transform_tool(ses: &mut Session, ev: &Ev, view: View, tool: &str) -> Result<()> {
    match ev.kind {
        Kind::Down => {
            let Some(h) = here(ses, ev.pos) else { return Ok(()) };
            let doc = ses.active().map(|s| s.doc.clone()).expect("checked");
            let mut sel = sel_in(ses, h.slot);
            if sel.is_empty() {
                if let Some(id) = geo::hit_top(&doc, h.slot, h.p, view.tol(4.0)) {
                    ses.select(Selection::items(vec![id]));
                    sel = vec![id];
                } else {
                    return Ok(());
                }
            }
            let center = bounds(&doc, &sel).map(|b| b.center()).unwrap_or(h.p);
            let kind = match tool {
                "rotate" => G::Rotate { ids: sel, center },
                "scale" => G::Scale { ids: sel, center },
                _ => G::Shear { ids: sel, center },
            };
            start(ses, ev, &h, kind, true);
        }
        Kind::Drag => drag(ses, ev, view),
        Kind::Up => {
            drag(ses, ev, view);
            finish(ses, ev, view);
        }
        _ => {}
    }
    Ok(())
}

pub fn key(ses: &mut Session, key: Key, mods: Mods) -> Result<bool> {
    let Some(st) = ses.active() else { return Ok(false) };
    let items = st.selection.items.clone();
    let content = st.selection.content;
    match key {
        Key::Left | Key::Right | Key::Up | Key::Down if !items.is_empty() => {
            let step = if mods.shift { 10.0 } else { 1.0 };
            let d = match key {
                Key::Left => Vec2::new(-step, 0.0),
                Key::Right => Vec2::new(step, 0.0),
                Key::Up => Vec2::new(0.0, -step),
                _ => Vec2::new(0.0, step),
            };
            ses.edit_now(if content { "Move Content" } else { "Move" }, |doc, _| {
                if content {
                    for id in &items {
                        let inv = doc.world_xf(*id).inverse();
                        let ld = inv * Point::new(d.x, d.y) - inv * Point::ZERO;
                        if let Some(ItemKind::Graphic(g)) = doc.item_mut(*id).map(|i| &mut i.kind) {
                            g.content = Affine::translate(ld) * g.content;
                        }
                    }
                } else {
                    ops::move_items(doc, &items, d);
                }
            });
            Ok(true)
        }
        Key::Delete | Key::Backspace if !items.is_empty() => {
            ses.edit_now("Clear", |doc, sel| {
                for id in &items {
                    doc.remove_item(*id);
                }
                *sel = Selection::default();
            });
            Ok(true)
        }
        Key::Escape if !items.is_empty() => {
            ses.select(Selection::default());
            Ok(true)
        }
        Key::Enter if items.len() == 1 => {
            let sid = st.doc.item(items[0]).and_then(|i| i.story());
            if let Some(sid) = sid {
                let len = st.doc.story(sid).map(|s| s.len()).unwrap_or(0);
                ses.tool = "type".into();
                ses.select(Selection::text(TextSel::caret(sid, len)));
                return Ok(true);
            }
            Ok(false)
        }
        _ => Ok(false),
    }
}

pub fn cursor(ses: &mut Session, at: Point, view: View) -> &'static str {
    let Some(h) = here(ses, at) else { return "arrow" };
    let Some(st) = ses.active() else { return "arrow" };
    let sel = sel_in(ses, h.slot);
    let doc = &st.doc;
    if let Some(b) = bounds(doc, &sel) {
        for (i, hp) in handles(b).iter().enumerate() {
            if (*hp - h.p).hypot() <= view.tol(6.0) {
                return geo::handle_cursor(i);
            }
        }
        if !b.contains(h.p) && [0usize, 2, 4, 6].iter().any(|i| (handles(b)[*i] - h.p).hypot() <= view.tol(18.0)) {
            return "rotate";
        }
    }
    match geo::hit_top(doc, h.slot, h.p, view.tol(4.0)) {
        Some(id) if sel.contains(&id) => "move",
        _ => "arrow",
    }
}
