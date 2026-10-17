use formdes_interchange::idml::unb64;
use formdes_model::color::{Gradient, Paint, Stop, Swatch};
use formdes_model::doc::Doc;
use formdes_model::geom::{Point, Rect, Vec2};
use formdes_model::ids::ItemId;
use formdes_model::item::{ItemKind, Shape};
use serde_json::{Value, json};

use super::draw::{home, layer_of, make_item};
use super::geo;
use super::select::{SEL_COLOR, here, start};
use super::{Ev, G, Kind, Overlay, View};
use crate::session::{Selection, Session, TextSel};
use crate::{Result, assets, ops};

fn sample_of(doc: &Doc, id: ItemId) -> Option<Value> {
    let it = doc.item(id)?;
    let mut s = json!({
        "fill": serde_json::to_value(&it.fill).ok()?, "stroke": serde_json::to_value(&it.stroke).ok()?,
        "opacity": it.opacity, "blend": it.blend, "effects": serde_json::to_value(&it.effects).ok()?,
    });
    if let Some(st) = it.story().and_then(|sid| doc.story(sid))
        && let Some(p) = st.paras.first()
    {
        let (cs, over) = p.format_at(1);
        s["paraStyle"] = json!(p.style);
        s["charStyle"] = json!(cs);
        s["chars"] = serde_json::to_value(over).ok()?;
    }
    Some(s)
}

fn apply_sample(doc: &mut Doc, ids: &[ItemId], s: &Value) {
    for id in ids {
        let story = doc.item(*id).and_then(|i| i.story());
        if let Some(it) = doc.item_mut(*id) {
            if let Some(f) = s.get("fill").and_then(|v| serde_json::from_value(v.clone()).ok()) {
                it.fill = f;
            }
            if let Some(st) = s.get("stroke").and_then(|v| serde_json::from_value(v.clone()).ok()) {
                it.stroke = st;
            }
            if let Some(o) = s.get("opacity").and_then(Value::as_f64) {
                it.opacity = o;
            }
            if let Some(b) = s.get("blend").and_then(Value::as_str) {
                it.blend = b.to_string();
            }
            if let Some(e) = s.get("effects").and_then(|v| serde_json::from_value(v.clone()).ok()) {
                it.effects = e;
            }
        }
        if let (Some(sid), Some(ps)) = (story, s.get("paraStyle").and_then(Value::as_str))
            && let Some(st) = doc.story_mut(sid)
        {
            let n = st.paras.len();
            let len = st.len();
            st.set_para_style(0..=n - 1, ps, false);
            if let Some(m) = s.get("chars").and_then(Value::as_object) {
                let _ = st.set_char_attrs(0..len, m);
            }
            let cs = s.get("charStyle").and_then(Value::as_str);
            st.set_char_style(0..len, cs, false);
        }
    }
}

pub fn eyedropper(ses: &mut Session, ev: &Ev, view: View) -> Result<()> {
    if ev.kind != Kind::Down {
        return Ok(());
    }
    let Some(h) = here(ses, ev.pos) else { return Ok(()) };
    let st = ses.active().expect("checked");
    let doc = st.doc.clone();
    let Some(id) = geo::hit_deep(&doc, h.slot, h.p, view.tol(4.0)) else { return Ok(()) };
    let sel = st.selection.items.clone();
    if ev.mods.alt || (ses.tools.sample.is_none() && sel.is_empty()) {
        ses.tools.sample = sample_of(&doc, id);
        return Ok(());
    }
    if !sel.is_empty() && !sel.contains(&id) {
        let s = sample_of(&doc, id);
        ses.tools.sample = s.clone();
        if let Some(s) = s {
            ses.edit_now("Eyedropper", |d, _| apply_sample(d, &sel, &s));
        }
        return Ok(());
    }
    if let Some(s) = ses.tools.sample.clone() {
        ses.edit_now("Eyedropper", |d, _| apply_sample(d, &[id], &s));
    }
    Ok(())
}

pub fn color_theme(ses: &mut Session, ev: &Ev, view: View) -> Result<()> {
    if ev.kind != Kind::Down {
        return Ok(());
    }
    let Some(h) = here(ses, ev.pos) else { return Ok(()) };
    let doc = ses.active().map(|s| s.doc.clone()).expect("checked");
    let Some(id) = geo::hit_deep(&doc, h.slot, h.p, view.tol(4.0)) else { return Ok(()) };
    let mut colors = Vec::new();
    for leaf in doc.walk(&[id]) {
        let Some(it) = doc.item(leaf) else { continue };
        for p in [&it.fill, &it.stroke.paint] {
            if let Some(c) = doc.paint_color(p)
                && !colors.iter().any(|x: &formdes_model::color::Color| x.hex() == c.hex())
            {
                colors.push(c);
            }
        }
    }
    if colors.is_empty() {
        return Ok(());
    }
    ses.edit_now("Color Theme", |d, _| {
        let mut n = 1;
        for c in colors {
            while d.swatch(&format!("Theme {n}")).is_some() {
                n += 1;
            }
            let mut sw = Swatch::color(&format!("Theme {n}"), c, false);
            sw.group = Some("Color Theme".into());
            d.swatches.push(sw);
        }
    });
    Ok(())
}

pub fn measure(ses: &mut Session, ev: &Ev, view: View) -> Result<()> {
    match ev.kind {
        Kind::Down => {
            if let Some(h) = here(ses, ev.pos) {
                start(ses, ev, &h, G::Measure, false);
            }
        }
        Kind::Drag => super::draw::drag(ses, ev, view),
        Kind::Up => {
            super::draw::drag(ses, ev, view);
            super::draw::finish(ses, ev, view);
        }
        _ => {}
    }
    Ok(())
}

pub fn gradient(ses: &mut Session, ev: &Ev, view: View, _tool: &str) -> Result<()> {
    match ev.kind {
        Kind::Down => {
            let Some(h) = here(ses, ev.pos) else { return Ok(()) };
            let doc = ses.active().map(|s| s.doc.clone()).expect("checked");
            let mut ids = ses.active().map(|s| s.selection.items.clone()).unwrap_or_default();
            if ids.is_empty() {
                if let Some(id) = geo::hit_top(&doc, h.slot, h.p, view.tol(4.0)) {
                    ids = vec![id];
                    ses.select(Selection::items(ids.clone()));
                } else {
                    return Ok(());
                }
            }
            start(ses, ev, &h, G::Gradient { ids }, false);
        }
        Kind::Drag => super::draw::drag(ses, ev, view),
        Kind::Up => {
            super::draw::drag(ses, ev, view);
            super::draw::finish(ses, ev, view);
        }
        _ => {}
    }
    Ok(())
}

pub fn note(ses: &mut Session, ev: &Ev, view: View) -> Result<()> {
    if ev.kind != Kind::Down {
        return Ok(());
    }
    let Some(h) = here(ses, ev.pos) else { return Ok(()) };
    let doc = ses.active().map(|s| s.doc.clone()).expect("checked");
    let Some(id) = geo::hit_deep(&doc, h.slot, h.p, view.tol(3.0)).filter(|i| doc.item(*i).is_some_and(|it| it.text().is_some())) else { return Ok(()) };
    let r = ses.renderer.clone();
    let lay = ses.active_mut().map(|s| s.layout(&r));
    if let (Some(lay), Some(st)) = (lay, ses.active())
        && let Some((sid, pos)) = geo::text_hit(st, &lay, id, h.p)
    {
        ses.select(Selection::text(TextSel::caret(sid, pos)));
        ses.requests.push(json!({"kind": "dialog", "id": "note.new", "params": {"text": "", "author": ""}}));
    }
    Ok(())
}

pub fn page(ses: &mut Session, ev: &Ev, _view: View) -> Result<()> {
    if ev.kind != Kind::Down {
        return Ok(());
    }
    let Some(h) = here(ses, ev.pos) else { return Ok(()) };
    if let crate::canvas::SpreadRef::Doc(si) = h.slot {
        let doc = &ses.active().expect("checked").doc;
        let first = doc.first_page_of_spread(si);
        let pi = doc.spreads[si].nearest_page(h.p.x);
        ses.tools.page = Some(first + pi);
        ses.select(Selection::default());
    }
    Ok(())
}

/// The local box mesh points are shares of: the item's path after effects, as the scene draws it.
fn mesh_box(doc: &Doc, id: ItemId) -> Option<Rect> {
    let it = doc.item(id)?;
    let bb = kurbo::Shape::bounding_box(&formdes_model::effects::apply(&it.extra, it.local_path()));
    (bb.width() > 0.0 && bb.height() > 0.0).then_some(bb)
}

/// The grid of mesh points as stored shares, filling the even grid when none were moved.
fn mesh_shares(m: &Value) -> Option<(usize, usize, Vec<[f64; 2]>)> {
    let cols = m.get("cols")?.as_u64()? as usize;
    let rows = m.get("rows")?.as_u64()? as usize;
    if cols == 0 || rows == 0 {
        return None;
    }
    let n = (cols + 1) * (rows + 1);
    let stored: Option<Vec<[f64; 2]>> = m.get("points").and_then(Value::as_array).map(|a| a.iter().filter_map(|q| Some([q.get(0)?.as_f64()?, q.get(1)?.as_f64()?])).collect()).filter(|v: &Vec<[f64; 2]>| v.len() == n);
    Some((cols, rows, stored.unwrap_or_else(|| (0..n).map(|i| [(i % (cols + 1)) as f64 / cols as f64, (i / (cols + 1)) as f64 / rows as f64]).collect())))
}

/// The mesh points of `id` in spread coordinates, row by row, or None when it has no mesh.
pub fn mesh_points(doc: &Doc, id: ItemId) -> Option<Vec<Point>> {
    let (_, _, shares) = mesh_shares(doc.item(id)?.extra.get("mesh")?)?;
    let bb = mesh_box(doc, id)?;
    let w = doc.world_xf(id);
    Some(shares.iter().map(|s| w * Point::new(bb.x0 + s[0] * bb.width(), bb.y0 + s[1] * bb.height())).collect())
}

/// Illustrator's Mesh tool, on formdes's stored meshes: a press on a point of the selected mesh drags that point
/// (one undo step); a press on a closed shape or frame without a mesh gives it a 3 by 3 mesh in its fill colour;
/// a press on a meshed object selects it so its points show.
pub fn mesh(ses: &mut Session, ev: &Ev, view: View) -> Result<()> {
    match ev.kind {
        Kind::Down => {
            let Some(h) = here(ses, ev.pos) else { return Ok(()) };
            let doc = ses.active().map(|s| s.doc.clone()).expect("checked");
            let selected = ses.active().map(|s| s.selection.items.clone()).unwrap_or_default();
            for id in &selected {
                if let Some(pts) = mesh_points(&doc, *id)
                    && let Some(idx) = pts.iter().position(|q| (*q - h.p).hypot() <= view.tol(6.0))
                {
                    start(ses, ev, &h, G::MeshPoint { item: *id, idx }, true);
                    return Ok(());
                }
            }
            let Some(id) = geo::hit_top(&doc, h.slot, h.p, view.tol(4.0)) else { return Ok(()) };
            let Some(it) = doc.item(id) else { return Ok(()) };
            ses.select(Selection::items(vec![id]));
            if it.extra.get("mesh").is_none() && it.shape.closed() && matches!(it.kind, ItemKind::Plain | ItemKind::Graphic(_)) {
                if let Err(e) = ses.execute("object.mesh", &json!({})) {
                    ses.requests.push(json!({"kind": "toast", "text": e.to_string()}));
                }
            }
        }
        Kind::Drag => super::draw::drag(ses, ev, view),
        Kind::Up => {
            super::draw::drag(ses, ev, view);
            super::draw::finish(ses, ev, view);
        }
        _ => {}
    }
    Ok(())
}

/// The topmost artboard on spread `si` whose rect holds `p`, with that rect.
fn artboard_at(doc: &Doc, si: usize, p: Point) -> Option<(usize, Rect)> {
    let list = crate::cmd::artboard::artboards(doc);
    list.iter().enumerate().rev().find_map(|(i, a)| {
        let (s, r) = crate::cmd::artboard::artboard_rect(a)?;
        (s == si && p.x >= r.x0 && p.x <= r.x1 && p.y >= r.y0 && p.y <= r.y1).then_some((i, r))
    })
}

fn corners(r: Rect) -> [Point; 4] {
    [Point::new(r.x0, r.y0), Point::new(r.x1, r.y0), Point::new(r.x1, r.y1), Point::new(r.x0, r.y1)]
}

/// The artboard corner within `tol` of `p` on spread `si`: (index, rect, corner 0 to 3), topmost first.
fn artboard_corner(doc: &Doc, si: usize, p: Point, tol: f64) -> Option<(usize, Rect, u8)> {
    let list = crate::cmd::artboard::artboards(doc);
    list.iter().enumerate().rev().find_map(|(i, a)| {
        let (s, r) = crate::cmd::artboard::artboard_rect(a)?;
        if s != si {
            return None;
        }
        corners(r).iter().position(|c| (*c - p).hypot() <= tol).map(|k| (i, r, k as u8))
    })
}

/// Removes the artboard the tool last touched; true when one went.
pub fn artboard_delete(ses: &mut Session) -> bool {
    let Some(i) = ses.tools.artboard.take() else { return false };
    let exists = ses.active().is_some_and(|s| i < crate::cmd::artboard::artboards(&s.doc).len());
    if exists {
        ses.edit_now("Delete Artboard", |doc, _| {
            let mut list = crate::cmd::artboard::artboards(doc);
            list.remove(i);
            if list.is_empty() {
                doc.prefs.remove("artboards");
            } else {
                doc.prefs.insert("artboards".into(), Value::Array(list));
            }
        });
    }
    exists
}

fn set_artboard_rect(doc: &mut Doc, i: usize, r: Rect) {
    if let Some(a) = doc.prefs.get_mut("artboards").and_then(Value::as_array_mut).and_then(|l| l.get_mut(i)) {
        a["rect"] = json!([r.x0, r.y0, r.x1, r.y1]);
    }
}

/// Illustrator's Artboard tool: a drag on empty canvas draws a new artboard (Shift keeps it square, Alt draws from
/// the centre), a drag that starts inside an artboard moves it. Artboards live on document spreads only.
pub fn artboard(ses: &mut Session, ev: &Ev, view: View) -> Result<()> {
    match ev.kind {
        Kind::Down => {
            let Some(h) = here(ses, ev.pos) else { return Ok(()) };
            let crate::canvas::SpreadRef::Doc(si) = h.slot else { return Ok(()) };
            let doc = ses.active().map(|s| s.doc.clone()).expect("checked");
            if let Some((i, r, k)) = artboard_corner(&doc, si, h.p, view.tol(6.0)) {
                ses.tools.artboard = Some(i);
                start(ses, ev, &h, G::Artboard { index: Some(i), rect: r, handle: Some(k) }, true);
                return Ok(());
            }
            match artboard_at(&doc, si, h.p) {
                Some((i, r)) => {
                    ses.tools.artboard = Some(i);
                    start(ses, ev, &h, G::Artboard { index: Some(i), rect: r, handle: None }, true);
                }
                None => {
                    ses.tools.artboard = None;
                    start(ses, ev, &h, G::Artboard { index: None, rect: Rect::new(h.p.x, h.p.y, h.p.x, h.p.y), handle: None }, false);
                }
            }
            ses.sel_rev += 1;
        }
        Kind::Drag => super::draw::drag(ses, ev, view),
        Kind::Up => {
            super::draw::drag(ses, ev, view);
            super::draw::finish(ses, ev, view);
        }
        _ => {}
    }
    Ok(())
}

type GapHit = (Vec<ItemId>, Vec<ItemId>, bool, f64, f64);

fn find_gap(doc: &Doc, slot: crate::canvas::SpreadRef, p: Point) -> Option<GapHit> {
    let items: Vec<(ItemId, Rect)> = geo::top_items(doc, slot).into_iter().filter(|i| geo::selectable(doc, *i)).filter_map(|i| Some((i, doc.bounds(i)?))).collect();
    let row: Vec<&(ItemId, Rect)> = items.iter().filter(|(_, b)| p.y >= b.y0 && p.y <= b.y1).collect();
    let left = row.iter().filter(|(_, b)| b.x1 <= p.x).map(|(_, b)| b.x1).fold(f64::NEG_INFINITY, f64::max);
    let right = row.iter().filter(|(_, b)| b.x0 >= p.x).map(|(_, b)| b.x0).fold(f64::INFINITY, f64::min);
    if left.is_finite() && right.is_finite() {
        let a = row.iter().filter(|(_, b)| (b.x1 - left).abs() < 0.5).map(|x| x.0).collect();
        let b = row.iter().filter(|(_, b)| (b.x0 - right).abs() < 0.5).map(|x| x.0).collect();
        return Some((a, b, false, left, right));
    }
    let col: Vec<&(ItemId, Rect)> = items.iter().filter(|(_, b)| p.x >= b.x0 && p.x <= b.x1).collect();
    let top = col.iter().filter(|(_, b)| b.y1 <= p.y).map(|(_, b)| b.y1).fold(f64::NEG_INFINITY, f64::max);
    let bottom = col.iter().filter(|(_, b)| b.y0 >= p.y).map(|(_, b)| b.y0).fold(f64::INFINITY, f64::min);
    if top.is_finite() && bottom.is_finite() {
        let a = col.iter().filter(|(_, b)| (b.y1 - top).abs() < 0.5).map(|x| x.0).collect();
        let b = col.iter().filter(|(_, b)| (b.y0 - bottom).abs() < 0.5).map(|x| x.0).collect();
        return Some((a, b, true, top, bottom));
    }
    None
}

pub fn gap(ses: &mut Session, ev: &Ev, view: View) -> Result<()> {
    match ev.kind {
        Kind::Down => {
            let Some(h) = here(ses, ev.pos) else { return Ok(()) };
            let doc = ses.active().map(|s| s.doc.clone()).expect("checked");
            if let Some((a, b, vertical, edge, _)) = find_gap(&doc, h.slot, h.p) {
                start(ses, ev, &h, G::Gap { a, b, vertical, edge }, true);
            }
        }
        Kind::Drag => super::draw::drag(ses, ev, view),
        Kind::Up => {
            super::draw::drag(ses, ev, view);
            super::draw::finish(ses, ev, view);
        }
        Kind::Move => {
            ses.tools.overlays.clear();
            if let Some(h) = here(ses, ev.pos) {
                let doc = ses.active().map(|s| s.doc.clone()).expect("checked");
                if let Some((_, _, vertical, e0, e1)) = find_gap(&doc, h.slot, h.p) {
                    let (a, b) = if vertical { (Point::new(h.p.x, e0), Point::new(h.p.x, e1)) } else { (Point::new(e0, h.p.y), Point::new(e1, h.p.y)) };
                    ses.tools.overlays.push(Overlay::Gap { a: h.xf * a, b: h.xf * b, label: format!("{:.1} pt", e1 - e0) });
                }
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn conveyor(ses: &mut Session, ev: &Ev, view: View, tool: &str) -> Result<()> {
    if ev.kind != Kind::Down {
        return Ok(());
    }
    let Some(h) = here(ses, ev.pos) else { return Ok(()) };
    let doc = ses.active().map(|s| s.doc.clone()).expect("checked");
    if tool == "contentCollector" {
        if let Some(id) = geo::hit_top(&doc, h.slot, h.p, view.tol(4.0))
            && !ses.tools.conveyor.contains(&id)
        {
            ses.tools.conveyor.push(id);
        }
        return Ok(());
    }
    if ses.tools.conveyor.is_empty() {
        return Ok(());
    }
    let src = ses.tools.conveyor.remove(0);
    let Some(b) = doc.bounds(src) else { return Ok(()) };
    let off = h.p - Point::new(b.x0, b.y0);
    let slot = h.slot;
    ses.edit_now("Place", |d, sel| {
        if let Some(nid) = ops::duplicate(d, src, Vec2::ZERO, Some(home(slot))) {
            ops::move_items(d, &[nid], off);
            *sel = Selection::items(vec![nid]);
        }
    });
    Ok(())
}

pub fn place(ses: &mut Session, ev: &Ev, view: View) -> Result<()> {
    match ev.kind {
        Kind::Down => {
            if let Some(h) = here(ses, ev.pos) {
                start(ses, ev, &h, G::Place, false);
            }
        }
        Kind::Drag => super::draw::drag(ses, ev, view),
        Kind::Up => {
            super::draw::drag(ses, ev, view);
            super::draw::finish(ses, ev, view);
        }
        _ => {}
    }
    Ok(())
}

pub fn drag(ses: &mut Session, ev: &Ev, view: View) {
    let Some(mut g) = ses.tools.gesture.clone() else { return };
    if (ev.pos - g.start).hypot() > view.tol(2.0) {
        g.moved = true;
    }
    let p = g.slot_xf.inverse() * ev.pos;
    ses.tools.overlays.clear();
    let mut grown: Option<Rect> = None;
    match &g.kind {
        G::Measure => {
            let mut b = p;
            if ev.mods.shift {
                b = geo::constrain(g.start_sp, p);
            }
            let d = b - g.start_sp;
            let ang = (-d.y).atan2(d.x).to_degrees();
            ses.tools.overlays.push(Overlay::Line { a: g.start, b: g.slot_xf * b, color: "#ff00ff".into(), dashed: true });
            ses.tools.overlays.push(Overlay::Measure { p: g.slot_xf * b, text: format!("D: {:.2} pt  A: {:.1}\u{00B0}  W: {:.2}  H: {:.2}", d.hypot(), ang, d.x.abs(), d.y.abs()) });
        }
        G::Artboard { index: Some(i), rect, handle } => {
            let d = p - g.start_sp;
            let r = match handle {
                Some(k) => {
                    let c = corners(*rect);
                    let r = Rect::from_points(c[(*k as usize + 2) % 4], c[*k as usize] + d);
                    if r.width() < 1.0 || r.height() < 1.0 { *rect } else { r }
                }
                None => Rect::new(rect.x0 + d.x, rect.y0 + d.y, rect.x1 + d.x, rect.y1 + d.y),
            };
            if g.moved
                && let Some(before) = g.before.clone()
            {
                let i = *i;
                ses.preview(&before, |doc| set_artboard_rect(doc, i, r));
            }
            let text = if handle.is_some() { format!("W: {:.1} pt  H: {:.1} pt", r.width(), r.height()) } else { format!("X: {:.1} pt  Y: {:.1} pt", r.x0, r.y0) };
            ses.tools.overlays.push(Overlay::Measure { p: ev.pos, text });
        }
        G::MeshPoint { item, idx } => {
            if g.moved
                && let Some(before) = g.before.clone()
                && let Some(bb) = mesh_box(&before.doc, *item)
            {
                let local = before.doc.world_xf(*item).inverse() * p;
                let share = [((local.x - bb.x0) / bb.width()).clamp(-0.5, 1.5), ((local.y - bb.y0) / bb.height()).clamp(-0.5, 1.5)];
                let (id, i) = (*item, *idx);
                ses.preview(&before, |doc| {
                    let Some(it) = doc.item_mut(id) else { return };
                    let Some((_, _, mut shares)) = it.extra.get("mesh").and_then(mesh_shares) else { return };
                    if i < shares.len() {
                        shares[i] = share;
                    }
                    if let Some(m) = it.extra.get_mut("mesh") {
                        m["points"] = json!(shares);
                    }
                });
            }
        }
        G::Artboard { index: None, .. } => {
            let r = geo::drag_rect(g.start_sp, p, ev.mods.shift, ev.mods.alt);
            ses.tools.overlays.push(Overlay::Marquee(g.slot_xf.transform_rect_bbox(r)));
            ses.tools.overlays.push(Overlay::Measure { p: ev.pos, text: format!("W: {:.1} pt  H: {:.1} pt", r.width(), r.height()) });
            grown = Some(r);
        }
        G::Gradient { .. } => ses.tools.overlays.push(Overlay::Line { a: g.start, b: ev.pos, color: SEL_COLOR.into(), dashed: false }),
        G::Place => {
            if g.moved {
                ses.tools.overlays.push(Overlay::Marquee(Rect::from_points(g.start, ev.pos)));
            }
        }
        G::Gap { a, b, vertical, .. } => {
            if let Some(before) = g.before.clone() {
                let d = if *vertical { p.y - g.start_sp.y } else { p.x - g.start_sp.x };
                let (a, b, v) = (a.clone(), b.clone(), *vertical);
                ses.preview(&before, |doc| {
                    for (ids, near) in [(&a, true), (&b, false)] {
                        for id in ids {
                            let Some(bb) = doc.bounds(*id) else { continue };
                            let nb = match (v, near) {
                                (false, true) => Rect::new(bb.x0, bb.y0, bb.x1 + d, bb.y1),
                                (false, false) => Rect::new(bb.x0 + d, bb.y0, bb.x1, bb.y1),
                                (true, true) => Rect::new(bb.x0, bb.y0, bb.x1, bb.y1 + d),
                                (true, false) => Rect::new(bb.x0, bb.y0 + d, bb.x1, bb.y1),
                            };
                            if nb.width() > 1.0 && nb.height() > 1.0 {
                                crate::cmd::object::resize_items(doc, &[*id], formdes_model::item::map_rect(bb, nb), false);
                            }
                        }
                    }
                });
            }
        }
        _ => {}
    }
    if let (Some(r), G::Artboard { rect, .. }) = (grown, &mut g.kind) {
        *rect = r;
    }
    ses.tools.gesture = Some(g);
}

pub fn finish(ses: &mut Session, ev: &Ev, view: View) {
    let Some(g) = ses.tools.gesture.take() else { return };
    let p = g.slot_xf.inverse() * ev.pos;
    match g.kind {
        G::Measure => {
            ses.tools.gesture = Some(g.clone());
            drag(ses, ev, view);
            ses.tools.gesture = None;
        }
        G::Gap { .. } => {
            if let Some(b) = &g.before {
                if g.moved { ses.commit(b, "Gap") } else { ses.restore(b) }
            }
        }
        G::MeshPoint { .. } => {
            if let Some(b) = &g.before {
                if g.moved { ses.commit(b, "Gradient Mesh") } else { ses.restore(b) }
            }
        }
        G::Artboard { index: Some(_), .. } => {
            ses.tools.overlays.clear();
            if let Some(b) = &g.before {
                if g.moved { ses.commit(b, "Artboard Options") } else { ses.restore(b) }
            }
        }
        G::Artboard { index: None, rect, .. } => {
            ses.tools.overlays.clear();
            let crate::canvas::SpreadRef::Doc(si) = g.slot else { return };
            if !g.moved || rect.width() < 1.0 || rect.height() < 1.0 {
                return;
            }
            let mut made = None;
            ses.edit_now("New Artboard", |doc, _| {
                let mut list = crate::cmd::artboard::artboards(doc);
                let taken = |n: &str| list.iter().any(|a| a.get("name").and_then(Value::as_str) == Some(n));
                let name = (list.len() + 1..).map(|k| format!("Artboard {k}")).find(|n| !taken(n)).expect("an unused name");
                list.push(json!({"name": name, "spread": si, "rect": [rect.x0, rect.y0, rect.x1, rect.y1]}));
                made = Some(list.len() - 1);
                doc.prefs.insert("artboards".into(), Value::Array(list));
            });
            ses.tools.artboard = made;
        }
        G::Gradient { ids } => {
            ses.tools.overlays.clear();
            if !g.moved {
                return;
            }
            let d = p - g.start_sp;
            let angle = (-d.y).atan2(d.x).to_degrees();
            let feather = ses.tool == "gradientFeather";
            ses.edit_now(if feather { "Gradient Feather" } else { "Gradient" }, |doc, _| {
                for id in &ids {
                    if feather {
                        if let Some(it) = doc.item_mut(*id) {
                            it.effects.insert("gradientFeather".into(), json!({"on": true, "angle": angle}));
                        }
                        continue;
                    }
                    let fill = doc.item(*id).map(|i| i.fill.clone());
                    let mut grad = fill.as_ref().and_then(|f| doc.gradient_of(f)).unwrap_or_else(|| {
                        let c = fill.as_ref().and_then(|f| doc.paint_color(f)).unwrap_or(formdes_model::color::Color::black());
                        Gradient { radial: false, stops: vec![Stop { at: 0.0, color: c, opacity: 1.0 }, Stop { at: 1.0, color: formdes_model::color::Color::white(), opacity: 1.0 }], angle: 0.0 }
                    });
                    grad.angle = angle;
                    if let Some(it) = doc.item_mut(*id) {
                        it.fill = Paint::Gradient { gradient: grad };
                    }
                }
            });
        }
        G::Place => {
            ses.tools.overlays.clear();
            let Some(entry) = ses.place_gun.first().cloned() else { return };
            let name = entry.get("name").and_then(Value::as_str).unwrap_or("placed").to_string();
            let path = entry.get("path").and_then(Value::as_str).map(str::to_string);
            let data = match (&path, entry.get("base64").and_then(Value::as_str)) {
                (_, Some(b)) => unb64(b),
                (Some(pth), None) => match std::fs::read(pth) {
                    Ok(d) => d,
                    Err(e) => {
                        ses.requests.push(json!({"kind": "toast", "text": format!("cannot read {pth}: {e}")}));
                        ses.place_gun.remove(0);
                        return;
                    }
                },
                _ => return,
            };
            let page = entry.get("page").and_then(Value::as_u64).unwrap_or(0) as u32;
            let doc = ses.active().map(|s| s.doc.clone()).expect("checked");
            let text = formdes_interchange::text::is_text_name(&name);
            let target = if g.moved { None } else { geo::hit_top(&doc, g.slot, p, view.tol(4.0)).filter(|i| doc.item(*i).is_some_and(|it| !matches!(it.kind, ItemKind::Group { .. } | ItemKind::States { .. }))) };
            let asset = if text { None } else { assets::make(&name, data.clone(), path.clone(), page).ok() };
            let rect = if g.moved {
                Rect::from_points(g.start_sp, p)
            } else if let Some(a) = &asset {
                let (w, h) = a.natural_size();
                Rect::new(p.x, p.y, p.x + w, p.y + h)
            } else {
                let cols: Vec<Rect> = geo::pages_of(&doc, g.slot).into_iter().flat_map(|(r, pg, right)| geo::column_rects(r, &pg, right)).collect();
                cols.into_iter().find(|c| p.x >= c.x0 && p.x <= c.x1).map(|c| Rect::new(c.x0, p.y.max(c.y0), c.x1, c.y1.max(p.y + 36.0))).unwrap_or(Rect::new(p.x, p.y, p.x + 200.0, p.y + 200.0))
            };
            let imported = if text { formdes_interchange::text::import(&name, &data).ok() } else { None };
            let layer = layer_of(ses);
            let slot = g.slot;
            let mode = super::draw::draw_mode(ses, slot);
            ses.edit_now("Place", |d, sel| {
                let frame = target.unwrap_or_else(|| make_item(d, layer, if text { "type" } else { "rectangleFrame" }, Shape::Rect { rect }, slot, mode).0);
                let ok = match (imported, asset) {
                    (Some(imp), _) => crate::cmd::file::place_text(d, frame, imp).is_ok(),
                    (_, Some(a)) => crate::cmd::file::place_image(d, frame, a).is_ok(),
                    _ => false,
                };
                if ok {
                    *sel = Selection::items(vec![frame]);
                }
            });
            ses.place_gun.remove(0);
            if ses.place_gun.is_empty() {
                ses.tool = "selection".into();
                ses.sel_rev += 1;
            }
        }
        _ => {}
    }
}
