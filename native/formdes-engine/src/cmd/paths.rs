use formdes_model::doc::Doc;
use formdes_model::geom::{Point, Rect};
use formdes_model::ids::ItemId;
use formdes_model::item::{Corner, ItemKind, Shape};
use kurbo::{BezPath, PathEl};
use serde_json::{Value, json};

use crate::registry::{Needs, Registry, cmd};
use crate::session::{Ctx, Selection};
use crate::target::items;
use crate::{EngineError, Result, args, ops};

fn subpaths(p: &BezPath) -> Vec<BezPath> {
    let mut out: Vec<BezPath> = Vec::new();
    for el in p.elements() {
        if matches!(el, PathEl::MoveTo(_)) || out.is_empty() {
            out.push(BezPath::new());
        }
        if let Some(last) = out.last_mut() {
            last.push(*el);
        }
    }
    out.retain(|s| s.elements().len() > 1);
    out
}

fn is_closed(p: &BezPath) -> bool {
    matches!(p.elements().last(), Some(PathEl::ClosePath))
}

fn end_point(el: &PathEl) -> Option<Point> {
    match el {
        PathEl::MoveTo(p) | PathEl::LineTo(p) => Some(*p),
        PathEl::QuadTo(_, p) => Some(*p),
        PathEl::CurveTo(_, _, p) => Some(*p),
        PathEl::ClosePath => None,
    }
}

fn ends(p: &BezPath) -> Option<(Point, Point)> {
    let els = p.elements();
    let a = els.first().and_then(end_point)?;
    let b = els.iter().rev().find_map(end_point)?;
    Some((a, b))
}

fn path_of(doc: &Doc, id: ItemId) -> Option<(BezPath, bool)> {
    let it = doc.item(id)?;
    if matches!(it.kind, ItemKind::Group { .. } | ItemKind::States { .. }) {
        return None;
    }
    Some((it.shape.to_path(&it.corner), it.shape.closed()))
}

fn set_path(doc: &mut Doc, id: ItemId, path: BezPath, closed: bool) {
    if let Some(it) = doc.item_mut(id) {
        it.shape = Shape::Path { path, closed };
        it.corner = Corner::default();
    }
}

fn shapes(c: &Ctx, p: &Value) -> Result<Vec<ItemId>> {
    let ids = items(c, p)?;
    let doc = c.doc()?;
    let ids: Vec<ItemId> = ids.into_iter().filter(|i| path_of(doc, *i).is_some()).collect();
    if ids.is_empty() {
        return Err(EngineError::new("needs a path or frame selected (not a group)"));
    }
    Ok(ids)
}

fn close(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = shapes(c, p)?;
    let doc = c.doc_mut()?;
    for id in &ids {
        let Some((path, closed)) = path_of(doc, *id) else { continue };
        if closed {
            continue;
        }
        let mut out = BezPath::new();
        for s in subpaths(&path) {
            for el in s.elements() {
                out.push(*el);
            }
            if !is_closed(&s) {
                out.close_path();
            }
        }
        set_path(doc, *id, out, true);
    }
    c.label("Close Path");
    Ok(json!({"ids": ids}))
}

fn open(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = shapes(c, p)?;
    let doc = c.doc_mut()?;
    for id in &ids {
        let Some((path, closed)) = path_of(doc, *id) else { continue };
        if !closed {
            continue;
        }
        let mut out = BezPath::new();
        for s in subpaths(&path) {
            let first = s.elements().first().and_then(end_point);
            for el in s.elements() {
                if *el == PathEl::ClosePath {
                    if let (Some(f), Some(last)) = (first, out.elements().iter().rev().find_map(end_point))
                        && (f - last).hypot() > 1e-6
                    {
                        out.line_to(f);
                    }
                    continue;
                }
                out.push(*el);
            }
        }
        if let Some(it) = doc.item_mut(*id) {
            it.fill = formdes_model::color::Paint::None;
        }
        set_path(doc, *id, out, false);
    }
    c.label("Open Path");
    Ok(json!({"ids": ids}))
}

fn reverse(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = shapes(c, p)?;
    let doc = c.doc_mut()?;
    for id in &ids {
        let Some((path, closed)) = path_of(doc, *id) else { continue };
        set_path(doc, *id, path.reverse_subpaths(), closed);
    }
    c.label("Reverse Path");
    Ok(json!({"ids": ids}))
}

fn local_in(doc: &Doc, from: ItemId, to: ItemId, path: BezPath) -> BezPath {
    (doc.world_xf(to).inverse() * doc.world_xf(from)) * path
}

fn join(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = shapes(c, p)?;
    let [a, b] = ids.as_slice() else { return Err(EngineError::new("Join needs exactly two open paths selected")) };
    let (a, b) = (*a, *b);
    let doc = c.doc()?;
    let (pa, ca) = path_of(doc, a).ok_or_else(|| EngineError::new("no path"))?;
    let (pb, cb) = path_of(doc, b).ok_or_else(|| EngineError::new("no path"))?;
    if ca || cb || subpaths(&pa).len() != 1 || subpaths(&pb).len() != 1 {
        return Err(EngineError::new("Join works on two open paths with one segment run each: open a closed path first"));
    }
    let pb = local_in(doc, b, a, pb);
    let ((a0, a1), (b0, b1)) = (ends(&pa).ok_or_else(|| EngineError::new("empty path"))?, ends(&pb).ok_or_else(|| EngineError::new("empty path"))?);
    let d = |x: Point, y: Point| (x - y).hypot();
    let options = [(d(a1, b0), false, false), (d(a1, b1), false, true), (d(a0, b0), true, false), (d(a0, b1), true, true)];
    let (gap, rev_a, rev_b) = options.iter().copied().fold((f64::INFINITY, false, false), |m, o| if o.0 < m.0 { o } else { m });
    let first = if rev_a { pa.reverse_subpaths() } else { pa };
    let second = if rev_b { pb.reverse_subpaths() } else { pb };
    let mut out = first.clone();
    for (k, el) in second.elements().iter().enumerate() {
        match (k, el) {
            (0, PathEl::MoveTo(pt)) => {
                if gap > 0.5 {
                    out.line_to(*pt);
                }
            }
            _ => out.push(*el),
        }
    }
    let doc = c.doc_mut()?;
    set_path(doc, a, out, false);
    doc.remove_item(b);
    c.set_selection(Selection::items(vec![a]));
    c.label("Join Path");
    Ok(json!({"id": a}))
}

fn compound(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = shapes(c, p)?;
    if ids.len() < 2 {
        return Err(EngineError::new("Make Compound Path needs two or more paths or frames selected"));
    }
    let doc = c.doc()?;
    let keep = ids[0];
    let mut out = BezPath::new();
    for id in &ids {
        let Some((path, closed)) = path_of(doc, *id) else { continue };
        let local = local_in(doc, *id, keep, path);
        for s in subpaths(&local) {
            for el in s.elements() {
                out.push(*el);
            }
            if closed && !is_closed(&s) {
                out.close_path();
            }
        }
    }
    let doc = c.doc_mut()?;
    for id in &ids {
        if *id != keep {
            doc.remove_item(*id);
        }
    }
    set_path(doc, keep, out, true);
    if let Some(it) = doc.item_mut(keep) {
        it.extra.insert("fillRule".into(), json!("evenodd"));
        it.kind = match &it.kind {
            ItemKind::Text(_) => ItemKind::Plain,
            k => k.clone(),
        };
    }
    c.set_selection(Selection::items(vec![keep]));
    c.label("Make Compound Path");
    Ok(json!({"id": keep}))
}

fn release(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = shapes(c, p)?;
    let mut made = Vec::new();
    for id in ids {
        let doc = c.doc()?;
        let Some((path, closed)) = path_of(doc, id) else { continue };
        let parts = subpaths(&path);
        if parts.len() < 2 {
            continue;
        }
        let doc = c.doc_mut()?;
        for part in parts.iter().skip(1) {
            if let Some(n) = ops::duplicate(doc, id, kurbo::Vec2::ZERO, None) {
                let cl = closed || is_closed(part);
                set_path(doc, n, part.clone(), cl);
                if let Some(it) = doc.item_mut(n) {
                    it.extra.remove("fillRule");
                }
                made.push(n);
            }
        }
        set_path(doc, id, parts[0].clone(), closed || is_closed(&parts[0]));
        if let Some(it) = doc.item_mut(id) {
            it.extra.remove("fillRule");
        }
        made.insert(0, id);
    }
    if made.is_empty() {
        return Err(EngineError::new("the selection holds no compound path to release"));
    }
    c.set_selection(Selection::items(made.clone()));
    c.label("Release Compound Path");
    Ok(json!({"ids": made}))
}

fn convert(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = shapes(c, p)?;
    let to = args::str_(p, "to", "rectangle, roundedRectangle, bevelRectangle, insetRectangle, invertedRoundedRectangle, ellipse, triangle, polygon, line, orthogonalLine, openPath or closedPath")?.to_string();
    if to == "openPath" {
        return open(c, p);
    }
    if to == "closedPath" {
        return close(c, p);
    }
    let radius = args::opt_f64(p, "radius").unwrap_or(12.0).max(0.0);
    let sides = args::opt_u64(p, "sides").unwrap_or(6).clamp(3, 100) as u32;
    let inset = args::opt_f64(p, "starInset").unwrap_or(0.0).clamp(0.0, 100.0) / 100.0;
    let doc = c.doc_mut()?;
    for id in &ids {
        let Some(it) = doc.item_mut(*id) else { continue };
        let b: Rect = it.shape.bbox();
        let (shape, corner) = match to.as_str() {
            "rectangle" => (Shape::Rect { rect: b }, Corner::default()),
            "roundedRectangle" => (Shape::Rect { rect: b }, Corner { radius, kind: "rounded".into(), each: Vec::new() }),
            "bevelRectangle" => (Shape::Rect { rect: b }, Corner { radius, kind: "bevel".into(), each: Vec::new() }),
            "insetRectangle" => (Shape::Rect { rect: b }, Corner { radius, kind: "inset".into(), each: Vec::new() }),
            "invertedRoundedRectangle" => (Shape::Rect { rect: b }, Corner { radius, kind: "inverse".into(), each: Vec::new() }),
            "ellipse" => (Shape::Ellipse { rect: b }, Corner::default()),
            "triangle" => (Shape::Polygon { rect: b, sides: 3, inset: 0.0 }, Corner::default()),
            "polygon" => (Shape::Polygon { rect: b, sides, inset }, Corner::default()),
            "line" => (Shape::Line { from: (b.x0, b.y0), to: (b.x1, b.y1) }, Corner::default()),
            "orthogonalLine" => {
                let horizontal = b.width() >= b.height();
                let s = if horizontal { Shape::Line { from: (b.x0, b.center().y), to: (b.x1, b.center().y) } } else { Shape::Line { from: (b.center().x, b.y0), to: (b.center().x, b.y1) } };
                (s, Corner::default())
            }
            other => return Err(EngineError::new(format!("`to` expected rectangle, roundedRectangle, bevelRectangle, insetRectangle, invertedRoundedRectangle, ellipse, triangle, polygon, line, orthogonalLine, openPath or closedPath, not {other}"))),
        };
        let line = matches!(shape, Shape::Line { .. });
        it.shape = shape;
        it.corner = corner;
        if line {
            it.fill = formdes_model::color::Paint::None;
            if matches!(it.kind, ItemKind::Text(_)) {
                it.kind = ItemKind::Plain;
            }
            if it.stroke.weight <= 0.0 || it.stroke.paint.is_none() {
                it.stroke = ops::default_stroke(1.0);
            }
        }
        it.extra.remove("fillRule");
    }
    c.label("Convert Shape");
    Ok(json!({"ids": ids}))
}

fn corner_kind(p: &Value) -> Result<String> {
    Ok(match args::opt_str(p, "kind").unwrap_or("round") {
        "round" | "rounded" | "fancy" => "rounded",
        "invertedRound" | "inverseRounded" | "inverse" => "inverse",
        "chamfer" | "bevel" => "bevel",
        "inset" => "inset",
        other => return Err(EngineError::new(format!("kind is round, invertedRound, chamfer or inset, not {other}"))),
    }
    .to_string())
}

fn round_corners(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = shapes(c, p)?;
    let radius = args::opt_f64(p, "radius").unwrap_or(12.0).max(0.0);
    let kind = corner_kind(p)?;
    let picked = c.selection().anchors;
    let listed: Option<Vec<usize>> = p.get("anchors").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_u64().map(|n| n as usize)).collect());
    let doc = c.doc_mut()?;
    let mut changed = 0usize;
    for id in &ids {
        let Some(it) = doc.item_mut(*id) else { continue };
        if matches!(it.shape, Shape::Ellipse { .. } | Shape::Line { .. }) {
            continue;
        }
        let base = it.shape.to_path(&Corner::default());
        let subs = crate::tools::geo::anchors_of(&base);
        let total: usize = subs.iter().map(|s| s.0.len()).sum();
        let mut offsets = Vec::with_capacity(subs.len());
        let mut at = 0usize;
        for s in &subs {
            offsets.push(at);
            at += s.0.len();
        }
        let targets: Vec<usize> = if let Some(l) = &listed {
            l.iter().copied().filter(|i| *i < total).collect()
        } else {
            let mine: Vec<usize> = picked.iter().filter(|a| a.0 == *id).filter_map(|a| offsets.get(a.1).map(|o| o + a.2)).filter(|i| *i < total).collect();
            if mine.is_empty() { (0..total).collect() } else { mine }
        };
        let mut each = if it.corner.live() && it.corner.each.len() == total {
            it.corner.each.clone()
        } else {
            (0..total).map(|_| formdes_model::corners::CornerAt { radius: it.corner.radius, kind: it.corner.kind.clone() }).collect()
        };
        for i in targets {
            if let Some(e) = each.get_mut(i) {
                e.radius = radius;
                e.kind = kind.clone();
            }
        }
        it.corner = Corner { radius: 0.0, kind: kind.clone(), each };
        changed += 1;
    }
    if changed == 0 {
        return Err(EngineError::new("rounds paths, rectangles and polygons; ellipses and lines have no corners"));
    }
    Ok(json!({"changed": changed}))
}

fn expand_corners(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = shapes(c, p)?;
    let doc = c.doc_mut()?;
    for id in &ids {
        let Some((path, closed)) = path_of(doc, *id) else { continue };
        set_path(doc, *id, path, closed);
    }
    Ok(json!({"expanded": ids.len()}))
}

pub fn register(r: &mut Registry) {
    cmd("path.roundCorners", "Round Corners").menu(&["Object", "Paths", "Round Corners..."]).needs(Needs::Items)
        .sig("{radius: pt, kind?: round|invertedRound|chamfer|inset, anchors?: [index], ids?}")
        .help("Rounds corners at anchor points and keeps them live: the selected anchors (Direct Selection), the listed anchor indices, or every corner. Paths, rectangles, frames and polygons; editing the anchors keeps the corners.")
        .reg(r, round_corners);
    cmd("path.expandCorners", "Expand Corners").menu(&["Object", "Paths", "Expand Corners"]).needs(Needs::Items).sig("{ids?}")
        .help("Turns live corners (and Corner Options) into ordinary path geometry.").reg(r, expand_corners);
    cmd("path.join", "Join Path").menu(&["Object", "Paths", "Join"]).needs(Needs::Items).sig("{ids?: two open paths}").reg(r, join);
    cmd("path.open", "Open Path").menu(&["Object", "Paths", "Open Path"]).needs(Needs::Items).sig("{ids?}").reg(r, open);
    cmd("path.close", "Close Path").menu(&["Object", "Paths", "Close Path"]).needs(Needs::Items).sig("{ids?}").reg(r, close);
    cmd("path.reverse", "Reverse Path").menu(&["Object", "Paths", "Reverse Path"]).needs(Needs::Items).sig("{ids?}").reg(r, reverse);
    cmd("path.compound", "Make Compound Path").menu(&["Object", "Paths", "Make Compound Path"]).key("Mod+8").needs(Needs::Items).sig("{ids?}").reg(r, compound);
    cmd("path.release", "Release Compound Path").menu(&["Object", "Paths", "Release Compound Path"]).key("Mod+Alt+Shift+8").needs(Needs::Items).sig("{ids?}").reg(r, release);
    cmd("object.convertShape", "Convert Shape").menu(&["Object", "Convert Shape"]).needs(Needs::Items).sig("{to: rectangle|roundedRectangle|bevelRectangle|insetRectangle|invertedRoundedRectangle|ellipse|triangle|polygon|line|orthogonalLine|openPath|closedPath, radius?, sides?, starInset?: percent, ids?}").reg(r, convert);
}
