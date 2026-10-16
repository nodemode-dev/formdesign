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

fn turn_deg(prev: Point, at: Point, next: Point) -> f64 {
    let a = at - prev;
    let b = next - at;
    if a.hypot() < 1e-9 || b.hypot() < 1e-9 {
        return 0.0;
    }
    (a.normalize().dot(b.normalize())).clamp(-1.0, 1.0).acos().to_degrees()
}

fn simplify(c: &mut Ctx, p: &Value) -> Result<Value> {
    use crate::tools::{draw, geo};
    let ids = shapes(c, p)?;
    let tol = args::opt_f64(p, "tolerance").unwrap_or(1.0).max(0.01);
    let corner = args::opt_f64(p, "cornerAngle").unwrap_or(50.0).clamp(1.0, 179.0);
    let straight = args::opt_bool(p, "straightLines").unwrap_or(false);
    let doc = c.doc_mut()?;
    let (mut before, mut after) = (0usize, 0usize);
    for id in &ids {
        let Some((path, _)) = path_of(doc, *id) else { continue };
        before += geo::anchors_of(&path).iter().map(|s| s.0.len()).sum::<usize>();
        if !straight {
            let opts = kurbo::simplify::SimplifyOptions::default().angle_thresh((corner.to_radians() / 2.0).tan());
            let mut out = BezPath::new();
            for sp in subpaths(&path) {
                let s = kurbo::simplify::simplify_bezpath(sp.iter(), tol, &opts);
                out.extend(s.iter());
            }
            let n_after: usize = geo::anchors_of(&out).iter().map(|s| s.0.len()).sum();
            let n_before: usize = geo::anchors_of(&path).iter().map(|s| s.0.len()).sum();
            if n_after == 0 || n_after >= n_before {
                after += n_before;
                continue;
            }
            after += n_after;
            let closed = is_closed(&out) || subpaths(&out).iter().all(is_closed);
            set_path(doc, *id, out, closed);
            continue;
        }
        let mut subs: Vec<(Vec<geo::Anchor>, bool)> = Vec::new();
        for sp in subpaths(&path) {
            let closed = is_closed(&sp);
            let mut pts: Vec<Point> = Vec::new();
            kurbo::flatten(sp.iter(), 0.05, |el| match el {
                PathEl::MoveTo(q) | PathEl::LineTo(q) => pts.push(q),
                _ => {}
            });
            if closed && pts.len() > 2 && (pts[0] - pts[pts.len() - 1]).hypot() < 1e-6 {
                pts.pop();
            }
            let mut kept = draw::rdp(&pts, tol);
            if closed && kept.len() > 2 && (kept[0] - kept[kept.len() - 1]).hypot() < 1e-6 {
                kept.pop();
            }
            if kept.len() < 2 {
                continue;
            }
            let mut anchors = if straight { kept.iter().map(|q| (*q, *q, *q)).collect() } else { draw::smooth_anchors(&kept, closed) };
            let n = kept.len();
            for i in 0..n {
                if !closed && (i == 0 || i == n - 1) {
                    continue;
                }
                let (pv, nx) = (kept[(i + n - 1) % n], kept[(i + 1) % n]);
                if turn_deg(pv, kept[i], nx) > corner {
                    anchors[i] = (kept[i], kept[i], kept[i]);
                }
            }
            if !closed {
                anchors[0].1 = anchors[0].0;
                let last = anchors.len() - 1;
                anchors[last].2 = anchors[last].0;
            }
            after += anchors.len();
            subs.push((anchors, closed));
        }
        if subs.is_empty() {
            continue;
        }
        let closed = subs.iter().all(|s| s.1);
        set_path(doc, *id, geo::path_of(&subs), closed);
    }
    Ok(json!({"anchorsBefore": before, "anchorsAfter": after}))
}

fn smooth(c: &mut Ctx, p: &Value) -> Result<Value> {
    use crate::tools::geo;
    let ids = shapes(c, p)?;
    let amount = args::opt_f64(p, "amount").unwrap_or(100.0).clamp(0.0, 100.0) / 100.0;
    let picked = c.selection().anchors;
    let doc = c.doc_mut()?;
    let mut changed = 0usize;
    for id in &ids {
        let Some((mut subs, _)) = geo::editable_anchors(doc, *id) else { continue };
        let mine: Vec<(usize, usize)> = picked.iter().filter(|a| a.0 == *id).map(|a| (a.1, a.2)).collect();
        for (si, (anchors, closed)) in subs.iter_mut().enumerate() {
            let n = anchors.len();
            let pts: Vec<Point> = anchors.iter().map(|a| a.0).collect();
            for i in 0..n {
                if !mine.is_empty() && !mine.contains(&(si, i)) {
                    continue;
                }
                if !*closed && (i == 0 || i + 1 == n) {
                    continue;
                }
                let (pv, nx) = (pts[(i + n - 1) % n], pts[(i + 1) % n]);
                let t = (nx - pv) / 6.0 * amount;
                anchors[i] = (pts[i], pts[i] - t, pts[i] + t);
                changed += 1;
            }
        }
        geo::set_anchors(doc, *id, &subs);
    }
    Ok(json!({"smoothed": changed}))
}

fn average(c: &mut Ctx, p: &Value) -> Result<Value> {
    use crate::tools::geo;
    let ids = shapes(c, p)?;
    let axis = args::opt_str(p, "axis").unwrap_or("both").to_string();
    let picked = c.selection().anchors;
    let doc = c.doc_mut()?;
    let mut chosen: Vec<(ItemId, usize, usize, Point)> = Vec::new();
    for id in &ids {
        let Some((subs, xf)) = geo::editable_anchors(doc, *id) else { continue };
        let mine: Vec<(usize, usize)> = picked.iter().filter(|a| a.0 == *id).map(|a| (a.1, a.2)).collect();
        for (si, (anchors, _)) in subs.iter().enumerate() {
            for (ai, a) in anchors.iter().enumerate() {
                if mine.is_empty() || mine.contains(&(si, ai)) {
                    chosen.push((*id, si, ai, xf * a.0));
                }
            }
        }
    }
    if chosen.len() < 2 {
        return Err(EngineError::new("Average needs two or more anchor points: pick them with Direct Selection, or select paths to average all their points"));
    }
    let k = chosen.len() as f64;
    let mean = Point::new(chosen.iter().map(|c| c.3.x).sum::<f64>() / k, chosen.iter().map(|c| c.3.y).sum::<f64>() / k);
    for id in &ids {
        let Some((mut subs, xf)) = geo::editable_anchors(doc, *id) else { continue };
        let inv = xf.inverse();
        for (cid, si, ai, w) in &chosen {
            if cid != id {
                continue;
            }
            let target = Point::new(if axis == "horizontal" { w.x } else { mean.x }, if axis == "vertical" { w.y } else { mean.y });
            if let Some(a) = subs.get_mut(*si).and_then(|s| s.0.get_mut(*ai)) {
                let d = inv * target - a.0;
                *a = (a.0 + d, a.1 + d, a.2 + d);
            }
        }
        geo::set_anchors(doc, *id, &subs);
    }
    Ok(json!({"averaged": chosen.len(), "at": [mean.x, mean.y]}))
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
    cmd("path.simplify", "Simplify").menu(&["Object", "Paths", "Simplify..."]).needs(Needs::Items)
        .sig("{tolerance?: pt (default 1), cornerAngle?: deg (default 50), straightLines?: bool, ids?}")
        .help("Reduces the anchor points of the selected paths while keeping their shape within the tolerance; turns sharper than the corner angle stay corners. straightLines makes every segment straight.").reg(r, simplify);
    cmd("path.smooth", "Smooth Points").menu(&["Object", "Paths", "Smooth Points"]).needs(Needs::Items).sig("{amount?: 0..100, ids?}")
        .help("Turns the anchors picked with Direct Selection (or every anchor of the selected paths) into smooth points with handles along the path.").reg(r, smooth);
    cmd("path.average", "Average").menu(&["Object", "Paths", "Average..."]).needs(Needs::Items).sig("{axis?: horizontal|vertical|both, ids?}")
        .help("Moves the anchors picked with Direct Selection (or every anchor of the selected paths) to their average position: horizontal lines them up on one y, vertical on one x, both stacks them.").reg(r, average);
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
