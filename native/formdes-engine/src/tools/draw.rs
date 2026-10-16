use formdes_model::doc::{Doc, Home};
use formdes_model::geom::{Affine, BezPath, Point, Rect};
use formdes_model::ids::{ItemId, LayerId};
use formdes_model::item::{Graphic, Item, ItemKind, Shape, TextFrame};
use kurbo::{CubicBez, ParamCurve, ParamCurveNearest};

use super::geo::{self, Anchor};
use super::select::{SEL_COLOR, here, start};
use super::{Ev, G, Key, Kind, Overlay, View};
use crate::canvas::SpreadRef;
use crate::ops;
use crate::session::{Selection, Session, TextSel};
use crate::Result;

pub fn home(slot: SpreadRef) -> Home {
    match slot {
        SpreadRef::Doc(i) => Home::Spread(i),
        SpreadRef::Parent(i) => Home::Parent(i),
    }
}

pub fn layer_of(ses: &Session) -> LayerId {
    let Some(st) = ses.active() else { return LayerId(0) };
    st.active_layer.filter(|l| st.doc.layer(*l).is_some()).unwrap_or_else(|| ops::active_layer(&st.doc))
}

fn shape_of(tool: &str, r: Rect, a: Point, b: Point) -> Shape {
    match tool {
        "ellipseFrame" | "ellipse" => Shape::Ellipse { rect: r },
        "polygonFrame" | "polygon" => Shape::Polygon { rect: r, sides: 6, inset: 0.0 },
        "line" => Shape::Line { from: (a.x, a.y), to: (b.x, b.y) },
        _ => Shape::Rect { rect: r },
    }
}

pub fn make_item(doc: &mut Doc, layer: LayerId, tool: &str, shape: Shape, slot: SpreadRef) -> (ItemId, Option<formdes_model::ids::StoryId>) {
    let (kind, story, style) = match tool {
        "rectangleFrame" | "ellipseFrame" | "polygonFrame" => (ItemKind::Graphic(Graphic { asset: None, content: Affine::IDENTITY, fitting: None }), None, Some(formdes_model::style::BASIC_GRAPHIC)),
        "type" | "typeVertical" => {
            let sid = doc.new_story(formdes_model::style::BASIC_PARA);
            let mut tf = TextFrame::new(sid);
            tf.vertical = tool == "typeVertical";
            (ItemKind::Text(tf), Some(sid), Some(formdes_model::style::BASIC_TEXT))
        }
        _ => (ItemKind::Plain, None, None),
    };
    let mut it = Item::new(ItemId(0), kind, shape, layer);
    if story.is_none() && style.is_none() {
        it.stroke = ops::default_stroke(1.0);
    }
    it.style = style.map(str::to_string);
    let id = doc.insert_item(it, home(slot));
    if let Some(sid) = story
        && let Some(st) = doc.story_mut(sid)
    {
        st.frames.push(id);
    }
    (id, story)
}

fn snap_pt(ses: &Session, slot: SpreadRef, p: Point, view: View) -> Point {
    if !view.snap {
        return p;
    }
    let Some(st) = ses.active() else { return p };
    geo::snaps(&st.doc, slot, &[]).point(p, view.tol(6.0))
}

pub fn shape_tool(ses: &mut Session, ev: &Ev, view: View, tool: &str) -> Result<()> {
    match ev.kind {
        Kind::Down => {
            if let Some(mut h) = here(ses, ev.pos) {
                h.p = snap_pt(ses, h.slot, h.p, view);
                start(ses, ev, &h, G::Create { tool: tool.to_string() }, false);
            }
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

fn create_geom(g: &super::Gesture, ev: &Ev, ses: &Session, view: View, tool: &str) -> (Rect, Point, Point) {
    let mut p = snap_pt(ses, g.slot, g.slot_xf.inverse() * ev.pos, view);
    let a = g.start_sp;
    if tool == "line" {
        if ev.mods.shift {
            p = geo::constrain(a, p);
        }
        return (Rect::from_points(a, p), a, p);
    }
    (geo::drag_rect(a, p, ev.mods.shift, ev.mods.alt), a, p)
}

pub fn drag(ses: &mut Session, ev: &Ev, view: View) {
    let Some(mut g) = ses.tools.gesture.clone() else { return };
    if (ev.pos - g.start).hypot() > view.tol(2.0) {
        g.moved = true;
    }
    let p = g.slot_xf.inverse() * ev.pos;
    match &mut g.kind {
        G::Create { tool } => {
            let tool = tool.clone();
            let (r, a, b) = create_geom(&g, ev, ses, view, &tool);
            let path = g.slot_xf * shape_of(&tool, r, a, b).to_path(&Default::default());
            ses.tools.overlays = vec![Overlay::Path { path, color: SEL_COLOR.into(), dashed: false }];
            ses.tools.overlays.push(Overlay::Measure { p: ev.pos, text: format!("W: {:.1} pt  H: {:.1} pt", r.width(), r.height()) });
        }
        G::Pen { idx } => {
            if g.moved {
                let i = *idx;
                if let Some(an) = ses.tools.pen.anchors.get_mut(i) {
                    an.2 = p;
                    an.1 = an.0 + (an.0 - p);
                }
            }
        }
        G::Pencil { points } => {
            if points.last().is_none_or(|l| (*l - p).hypot() > view.tol(2.0)) {
                points.push(p);
            }
            let mut path = BezPath::new();
            for (i, q) in points.iter().enumerate() {
                if i == 0 { path.move_to(g.slot_xf * *q) } else { path.line_to(g.slot_xf * *q) }
            }
            ses.tools.overlays = vec![Overlay::Path { path, color: SEL_COLOR.into(), dashed: false }];
        }
        _ => {
            ses.tools.gesture = Some(g);
            return super::misc::drag(ses, ev, view);
        }
    }
    ses.tools.gesture = Some(g);
}

pub fn finish(ses: &mut Session, ev: &Ev, view: View) {
    let Some(g) = ses.tools.gesture.take() else { return };
    ses.tools.overlays.clear();
    let tool_now = ses.tool.clone();
    match &g.kind {
        G::Create { tool } => {
            let layer = layer_of(ses);
            if !g.moved {
                let p = g.start_sp;
                let content = match tool.as_str() {
                    "rectangleFrame" | "ellipseFrame" | "polygonFrame" => "graphic",
                    "type" | "typeVertical" => "text",
                    _ => "unassigned",
                };
                let shape = match tool.as_str() {
                    "ellipseFrame" | "ellipse" => "ellipse",
                    "polygonFrame" | "polygon" => "polygon",
                    _ => "rectangle",
                };
                let spread = match g.slot {
                    SpreadRef::Doc(i) => serde_json::json!(i),
                    SpreadRef::Parent(i) => serde_json::json!({"kind": "parent", "index": i}),
                };
                if tool != "type" {
                    ses.requests.push(serde_json::json!({"kind": "dialog", "id": "frame.create", "params": {"spread": spread, "rect": [p.x, p.y, p.x + 100.0, p.y + 100.0], "content": content, "shape": shape, "orientation": if tool == "typeVertical" { "vertical" } else { "horizontal" }}}));
                }
                return;
            }
            let (r, a, b) = create_geom(&g, ev, ses, view, tool);
            let shape = shape_of(tool, r, a, b);
            let tool = tool.clone();
            let slot = g.slot;
            let label = match tool.as_str() {
                "type" | "typeVertical" => "New Text Frame",
                "line" => "Line",
                "rectangleFrame" | "ellipseFrame" | "polygonFrame" => "New Frame",
                _ => "New Shape",
            };
            ses.edit_now(label, |doc, sel| {
                let (id, story) = make_item(doc, layer, &tool, shape, slot);
                *sel = match story {
                    Some(sid) => Selection::text(TextSel::caret(sid, 0)),
                    None => Selection::items(vec![id]),
                };
            });
        }
        G::Pen { .. } => {}
        G::Pencil { points } => {
            let pts = points.clone();
            if pts.len() < 2 {
                return;
            }
            let simple = rdp(&pts, view.tol(1.5));
            let anchors = smooth_anchors(&simple, false);
            let closed = ev.mods.alt;
            let slot = g.slot;
            let layer = layer_of(ses);
            if tool_now == "pencil" {
                ses.edit_now("Pencil", |doc, sel| {
                    let path = geo::path_of(&[(anchors, closed)]);
                    let (id, _) = make_item(doc, layer, "pencil", Shape::Path { path, closed }, slot);
                    *sel = Selection::items(vec![id]);
                });
            } else {
                edit_touched(ses, &pts, view, tool_now == "erase");
            }
        }
        _ => {
            ses.tools.gesture = Some(g);
            super::misc::finish(ses, ev, view);
        }
    }
}

pub fn rdp(pts: &[Point], tol: f64) -> Vec<Point> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let (a, b) = (pts[0], pts[pts.len() - 1]);
    let line = kurbo::Line::new(a, b);
    let (mut idx, mut dmax) = (0, 0.0);
    for (i, p) in pts.iter().enumerate().take(pts.len() - 1).skip(1) {
        let d = line.nearest(*p, 1e-6).distance_sq.sqrt();
        if d > dmax {
            dmax = d;
            idx = i;
        }
    }
    if dmax > tol {
        let mut l = rdp(&pts[..=idx], tol);
        let r = rdp(&pts[idx..], tol);
        l.pop();
        l.extend(r);
        l
    } else {
        vec![a, b]
    }
}

pub fn smooth_anchors(pts: &[Point], closed: bool) -> Vec<Anchor> {
    let n = pts.len();
    (0..n)
        .map(|i| {
            let prev = if i == 0 { if closed { pts[n - 1] } else { pts[0] } } else { pts[i - 1] };
            let next = if i + 1 == n { if closed { pts[0] } else { pts[n - 1] } } else { pts[i + 1] };
            let t = (next - prev) / 6.0;
            (pts[i], pts[i] - t, pts[i] + t)
        })
        .collect()
}

fn edit_touched(ses: &mut Session, stroke: &[Point], view: View, erase: bool) {
    let Some(st) = ses.active() else { return };
    let ids = st.selection.items.clone();
    let doc = st.doc.clone();
    let r = view.tol(8.0);
    let near = |p: Point| stroke.iter().any(|q| (*q - p).hypot() <= r);
    for id in ids {
        let Some((subs, xf)) = geo::editable_anchors(&doc, id) else { continue };
        let mut out: Vec<(Vec<Anchor>, bool)> = Vec::new();
        let mut touched = false;
        for (a, closed) in subs {
            if erase {
                let mut cur: Vec<Anchor> = Vec::new();
                let mut pieces = Vec::new();
                for an in a.iter() {
                    if near(xf * an.0) {
                        touched = true;
                        if cur.len() >= 2 {
                            pieces.push(std::mem::take(&mut cur));
                        }
                        cur.clear();
                    } else {
                        cur.push(*an);
                    }
                }
                if cur.len() >= 2 {
                    pieces.push(cur);
                }
                if touched {
                    out.extend(pieces.into_iter().map(|p| (p, false)));
                } else {
                    out.push((a, closed));
                }
            } else {
                let hit: Vec<bool> = a.iter().map(|an| near(xf * an.0)).collect();
                if !hit.iter().any(|h| *h) {
                    out.push((a, closed));
                    continue;
                }
                touched = true;
                let kept: Vec<Point> = a.iter().enumerate().filter(|(i, _)| !hit[*i] || i % 2 == 0 || *i == 0 || *i + 1 == a.len()).map(|(_, an)| an.0).collect();
                out.push((smooth_anchors(&kept, closed), closed));
            }
        }
        if touched {
            let label = if erase { "Erase" } else { "Smooth" };
            ses.edit_now(label, |d, sel| {
                if out.is_empty() {
                    d.remove_item(id);
                    sel.items.retain(|x| *x != id);
                } else {
                    geo::set_anchors(d, id, &out);
                }
            });
        }
    }
}

pub fn pencil(ses: &mut Session, ev: &Ev, view: View, _tool: &str) -> Result<()> {
    match ev.kind {
        Kind::Down => {
            if let Some(h) = here(ses, ev.pos) {
                let p = h.p;
                start(ses, ev, &h, G::Pencil { points: vec![p] }, false);
            }
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

pub fn finish_pen(ses: &mut Session, closed: bool) {
    let pen = std::mem::take(&mut ses.tools.pen);
    let Some(slot) = pen.slot else { return };
    if pen.anchors.len() < 2 {
        return;
    }
    let layer = layer_of(ses);
    ses.edit_now("Pen", |doc, sel| {
        let path = geo::path_of(&[(pen.anchors.clone(), closed)]);
        let (id, _) = make_item(doc, layer, "pen", Shape::Path { path, closed }, slot);
        *sel = Selection::items(vec![id]);
    });
}

pub fn pen(ses: &mut Session, ev: &Ev, view: View) -> Result<()> {
    match ev.kind {
        Kind::Down => {
            let Some(mut h) = here(ses, ev.pos) else { return Ok(()) };
            if ses.tools.pen.slot.is_some_and(|s| s != h.slot) {
                finish_pen(ses, false);
            }
            h.p = snap_pt(ses, h.slot, h.p, view);
            let anchors = &ses.tools.pen.anchors;
            if anchors.len() >= 2 && (h.xf * anchors[0].0 - ev.pos).hypot() <= view.tol(6.0) {
                finish_pen(ses, true);
                return Ok(());
            }
            ses.tools.pen.slot = Some(h.slot);
            ses.tools.pen.anchors.push((h.p, h.p, h.p));
            let idx = ses.tools.pen.anchors.len() - 1;
            start(ses, ev, &h, G::Pen { idx }, false);
        }
        Kind::Drag => drag(ses, ev, view),
        Kind::Up => {
            drag(ses, ev, view);
            ses.tools.gesture = None;
        }
        _ => {}
    }
    Ok(())
}

fn resmooth(pen: &mut super::Pen, closed: bool) {
    let pts: Vec<Point> = pen.anchors.iter().map(|a| a.0).collect();
    let n = pts.len();
    let mut out = smooth_anchors(&pts, closed);
    for (i, a) in out.iter_mut().enumerate() {
        if pen.corners.contains(&i) || (!closed && (i == 0 || i + 1 == n)) {
            *a = (a.0, a.0, a.0);
        }
    }
    pen.anchors = out;
}

pub fn curvature(ses: &mut Session, ev: &Ev, view: View) -> Result<()> {
    if ev.kind != Kind::Down {
        return Ok(());
    }
    let Some(mut h) = here(ses, ev.pos) else { return Ok(()) };
    if ses.tools.pen.slot.is_some_and(|s| s != h.slot) {
        finish_pen(ses, false);
    }
    h.p = snap_pt(ses, h.slot, h.p, view);
    let anchors = &ses.tools.pen.anchors;
    if anchors.len() >= 2 && (h.xf * anchors[0].0 - ev.pos).hypot() <= view.tol(6.0) {
        resmooth(&mut ses.tools.pen, true);
        finish_pen(ses, true);
        return Ok(());
    }
    ses.tools.pen.slot = Some(h.slot);
    ses.tools.pen.anchors.push((h.p, h.p, h.p));
    if ev.mods.alt {
        let i = ses.tools.pen.anchors.len() - 1;
        ses.tools.pen.corners.push(i);
    }
    resmooth(&mut ses.tools.pen, false);
    Ok(())
}

pub fn pen_key(ses: &mut Session, key: Key) -> bool {
    match key {
        Key::Enter | Key::Escape if !ses.tools.pen.anchors.is_empty() => {
            finish_pen(ses, false);
            true
        }
        _ => false,
    }
}

fn cubic(a: &Anchor, b: &Anchor) -> CubicBez {
    CubicBez::new(a.0, a.2, b.1, b.0)
}

pub fn anchor_tool(ses: &mut Session, ev: &Ev, view: View, tool: &str) -> Result<()> {
    if ev.kind != Kind::Down {
        return Ok(());
    }
    let Some(h) = here(ses, ev.pos) else { return Ok(()) };
    let Some(st) = ses.active() else { return Ok(()) };
    let doc = st.doc.clone();
    let tol = view.tol(5.0);
    let mut cands: Vec<ItemId> = st.selection.items.iter().copied().filter(|i| geo::spread_of_item(&doc, *i) == Some(h.slot)).collect();
    if let Some(hit) = geo::hit_deep(&doc, h.slot, h.p, tol)
        && !cands.contains(&hit)
    {
        cands.push(hit);
    }
    for id in cands {
        let Some((mut subs, xf)) = geo::editable_anchors(&doc, id) else { continue };
        let lp = xf.inverse() * h.p;
        let ltol = tol / xf.as_coeffs()[0].hypot(xf.as_coeffs()[1]).max(1e-6);
        let on_anchor = subs.iter().enumerate().find_map(|(si, (a, _))| a.iter().position(|an| (an.0 - lp).hypot() <= ltol).map(|ai| (si, ai)));
        let on_seg = || -> Option<(usize, usize, f64)> {
            for (si, (a, closed)) in subs.iter().enumerate() {
                let n = a.len();
                let segs = if *closed { n } else { n.saturating_sub(1) };
                for i in 0..segs {
                    let c = cubic(&a[i], &a[(i + 1) % n]);
                    let near = c.nearest(lp, 1e-4);
                    if near.distance_sq <= ltol * ltol {
                        return Some((si, i, near.t));
                    }
                }
            }
            None
        };
        let (label, ok) = match tool {
            "deleteAnchor" => match on_anchor {
                Some((si, ai)) if subs[si].0.len() > 2 => {
                    subs[si].0.remove(ai);
                    ("Delete Anchor Point", true)
                }
                _ => ("", false),
            },
            "convertDirection" => match on_anchor {
                Some((si, ai)) => {
                    let a = &mut subs[si].0;
                    let n = a.len();
                    let an = a[ai];
                    if (an.1 - an.0).hypot() > 1e-6 || (an.2 - an.0).hypot() > 1e-6 {
                        a[ai] = (an.0, an.0, an.0);
                    } else {
                        let prev = a[(ai + n - 1) % n].0;
                        let next = a[(ai + 1) % n].0;
                        let t = (next - prev) / 6.0;
                        a[ai] = (an.0, an.0 - t, an.0 + t);
                    }
                    ("Convert Direction Point", true)
                }
                None => ("", false),
            },
            "addAnchor" => match on_seg() {
                Some((si, i, t)) => {
                    let a = &mut subs[si].0;
                    let n = a.len();
                    let j = (i + 1) % n;
                    let c = cubic(&a[i], &a[j]);
                    let (l, r) = (c.subsegment(0.0..t), c.subsegment(t..1.0));
                    a[i].2 = l.p1;
                    a[j].1 = r.p2;
                    a.insert(i + 1, (l.p3, l.p2, r.p1));
                    ("Add Anchor Point", true)
                }
                None => ("", false),
            },
            _ => {
                let cut = on_anchor.map(|(si, ai)| (si, ai, None)).or_else(|| on_seg().map(|(si, i, t)| (si, i, Some(t))));
                match cut {
                    Some((si, i, t)) => {
                        let (mut a, closed) = subs[si].clone();
                        let mut k = i;
                        if let Some(t) = t {
                            let n = a.len();
                            let j = (i + 1) % n;
                            let c = cubic(&a[i], &a[j]);
                            let (l, r) = (c.subsegment(0.0..t), c.subsegment(t..1.0));
                            a[i].2 = l.p1;
                            a[j].1 = r.p2;
                            a.insert(i + 1, (l.p3, l.p2, r.p1));
                            k = i + 1;
                        }
                        let mut new_item: Option<Vec<Anchor>> = None;
                        if closed {
                            let mut rot: Vec<Anchor> = a[k..].to_vec();
                            rot.extend_from_slice(&a[..=k]);
                            subs[si] = (rot, false);
                        } else {
                            let tail = a[k..].to_vec();
                            subs[si] = (a[..=k].to_vec(), false);
                            if tail.len() >= 2 {
                                new_item = Some(tail);
                            }
                        }
                        let id2 = id;
                        let subs2 = subs.clone();
                        ses.edit_now("Cut Path", |d, sel| {
                            geo::set_anchors(d, id2, &subs2);
                            if let Some(tail) = new_item
                                && let Some(src) = d.item(id2).cloned()
                                && let Some(h) = d.home(id2)
                            {
                                let mut it = src.clone();
                                it.id = ItemId(0);
                                it.shape = Shape::Path { path: geo::path_of(&[(tail, false)]), closed: false };
                                it.kind = ItemKind::Plain;
                                let nid = d.insert_item(it, h);
                                *sel = Selection::items(vec![id2, nid]);
                            }
                        });
                        return Ok(());
                    }
                    None => ("", false),
                }
            }
        };
        if ok {
            ses.edit_now(label, |d, sel| {
                geo::set_anchors(d, id, &subs);
                *sel = Selection::items(vec![id]);
            });
            return Ok(());
        }
    }
    Ok(())
}
