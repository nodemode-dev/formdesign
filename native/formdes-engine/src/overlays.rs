use formdes_model::doc::Doc;
use formdes_model::geom::{Affine, BezPath, Point, Rect};
use formdes_model::ids::{ItemId, ParentId};
use formdes_model::item::{Graphic, ItemKind};
use formdes_model::page::Orientation;
use formdes_text::layout::DocLayout;
use kurbo::PathEl;
use serde_json::{Value, json};

use crate::canvas::{Canvas, SpreadRef};
use crate::session::Session;
use crate::tools::{Overlay, View, geo};

const EDGE: &str = "#4f99ff";
const GUIDE: &str = "#4ae3ff";
const MARGIN: &str = "#d946ef";
const COLUMN: &str = "#8b5cf6";
const BLEED: &str = "#ff3b30";
const GRID_ZOOM: f64 = 0.75;
const DIRECT_TOOLS: [&str; 5] = ["directSelection", "pen", "addAnchor", "deleteAnchor", "convertDirection"];

fn q(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn xy(p: Point) -> Value {
    json!([q(p.x), q(p.y)])
}

fn seg(a: Point, b: Point) -> Value {
    json!([q(a.x), q(a.y), q(b.x), q(b.y)])
}

fn boxed(r: Rect) -> Value {
    json!([q(r.x0), q(r.y0), q(r.x1), q(r.y1)])
}

fn trace(path: &BezPath, m: Affine, tol: f64) -> Vec<Vec<f64>> {
    let mut done = Vec::new();
    let mut open: Vec<f64> = Vec::new();
    let flush = |open: &mut Vec<f64>, done: &mut Vec<Vec<f64>>| {
        if open.len() > 2 {
            done.push(std::mem::take(open));
        } else {
            open.clear();
        }
    };
    kurbo::flatten(path, tol, |el| match el {
        PathEl::MoveTo(p) => {
            flush(&mut open, &mut done);
            let p = m * p;
            open.extend([q(p.x), q(p.y)]);
        }
        PathEl::LineTo(p) => {
            let p = m * p;
            open.extend([q(p.x), q(p.y)]);
        }
        PathEl::ClosePath => {
            if let [x, y, ..] = open[..] {
                open.extend([x, y]);
            }
        }
        _ => {}
    });
    flush(&mut open, &mut done);
    done
}

fn layer_color(doc: &Doc, id: ItemId) -> String {
    doc.item(id).and_then(|it| doc.layer(it.layer)).map(|l| l.color.clone()).filter(|c| c.starts_with('#')).unwrap_or_else(|| EDGE.into())
}

fn layer_shown(doc: &Doc, id: ItemId) -> bool {
    doc.item(id).is_some_and(|it| !it.hidden && doc.layer(it.layer).is_none_or(|l| l.visible))
}

fn outline(doc: &Doc, id: ItemId, m: Affine, tol: f64) -> Vec<Vec<f64>> {
    let Some(it) = doc.item(id) else { return Vec::new() };
    trace(&it.shape.to_path(&it.corner), m * doc.world_xf(id), tol)
}

fn in_view(r: Rect, view: Option<Rect>) -> bool {
    view.is_none_or(|v| !(r.x1 < v.x0 || r.x0 > v.x1 || r.y1 < v.y0 || r.y0 > v.y1))
}

fn frame_edges(doc: &Doc, top: &[ItemId], m: Affine, tol: f64, out: &mut Vec<Value>) {
    let mut todo: Vec<ItemId> = top.iter().rev().copied().collect();
    while let Some(id) = todo.pop() {
        let Some(it) = doc.item(id) else { continue };
        if !layer_shown(doc, id) || it.anchored_in.is_some() {
            continue;
        }
        if let ItemKind::Group { .. } | ItemKind::States { .. } = it.kind {
            todo.extend(it.children().into_iter().rev());
            continue;
        }
        let empty_graphic = matches!(it.kind, ItemKind::Graphic(Graphic { asset: None, .. }));
        let edged = it.stroke.weight <= 0.0 || it.stroke.paint.is_none() || matches!(it.kind, ItemKind::Text(_) | ItemKind::Graphic(_));
        if !edged {
            continue;
        }
        let mut v = json!({"id": id.0, "color": layer_color(doc, id), "polys": outline(doc, id, m, tol)});
        if empty_graphic {
            let w = m * doc.world_xf(id);
            let b = it.shape.bbox();
            v["cross"] = json!([xy(w * Point::new(b.x0, b.y0)), xy(w * Point::new(b.x1, b.y1)), xy(w * Point::new(b.x1, b.y0)), xy(w * Point::new(b.x0, b.y1))]);
        }
        out.push(v);
    }
}

fn parent_chain(doc: &Doc, pid: ParentId) -> Vec<usize> {
    let mut out = Vec::new();
    let mut cur = Some(pid);
    while let Some(id) = cur {
        let Some(i) = doc.parents.iter().position(|p| p.id == id) else { break };
        if out.contains(&i) || out.len() > 8 {
            break;
        }
        out.insert(0, i);
        cur = doc.parents[i].based_on;
    }
    out
}

#[derive(Default)]
struct Furniture {
    margins: Vec<Value>,
    columns: Vec<Value>,
    guides: Vec<Value>,
    bleeds: Vec<Value>,
    baselines: Vec<Value>,
    frames: Vec<Value>,
    parent_frames: Vec<Value>,
}

fn furniture(doc: &Doc, cv: &Canvas, zoom: f64, tol: f64, view: Option<Rect>) -> Furniture {
    let s = &doc.settings;
    let mut f = Furniture::default();
    let grid = &s.baseline;
    for slot in cv.slots.iter().filter(|sl| in_view(sl.bounds.inflate(400.0, 400.0), view)) {
        let m = slot.xf;
        let pages = geo::pages_of(doc, slot.spread);
        let Some(span) = pages.iter().map(|p| p.0).reduce(|a, b| a.union(b)) else { continue };
        let b = &s.bleed;
        if b.max() > 0.0
            && let (Some(first), Some(last)) = (pages.first(), pages.last())
        {
            let left = b.left_right(!first.2).0;
            let right = b.left_right(!last.2).1;
            f.bleeds.push(boxed(m.transform_rect_bbox(Rect::new(span.x0 - left, span.y0 - b.top, span.x1 + right, span.y1 + b.bottom))));
        }
        for (pr, page, right) in &pages {
            if zoom >= GRID_ZOOM && grid.increment > 0.0 {
                let origin = if grid.from_margin { pr.y0 + page.margins.top } else { pr.y0 };
                let mut y = origin + grid.start;
                while y < pr.y1 {
                    f.baselines.push(seg(m * Point::new(pr.x0, y), m * Point::new(pr.x1, y)));
                    y += grid.increment.max(1.0);
                }
            }
            f.margins.push(boxed(m.transform_rect_bbox(geo::margin_rect(*pr, page, *right))));
            for pair in geo::column_rects(*pr, page, *right).windows(2) {
                let (a, c) = (pair[0], pair[1]);
                f.columns.push(seg(m * Point::new(a.x1, a.y0), m * Point::new(a.x1, a.y1)));
                f.columns.push(seg(m * Point::new(c.x0, c.y0), m * Point::new(c.x0, c.y1)));
            }
        }
        let board = m.inverse().transform_rect_bbox(cv.pasteboard);
        for g in geo::guides_of(doc, slot.spread) {
            let vertical = g.orientation == Orientation::Vertical;
            let along = match g.page.and_then(|i| pages.get(i)) {
                Some(p) => p.0,
                None if vertical => span.inflate(0.0, 36.0),
                None => board,
            };
            let (a, c) = if vertical { (Point::new(g.pos, along.y0), Point::new(g.pos, along.y1)) } else { (Point::new(along.x0, g.pos), Point::new(along.x1, g.pos)) };
            f.guides.push(json!({"a": xy(m * a), "b": xy(m * c), "liquid": false, "vertical": vertical, "color": g.color}));
        }
        if let SpreadRef::Doc(si) = slot.spread {
            let first = doc.first_page_of_spread(si);
            for (pi, (pr, page, _)) in pages.iter().enumerate() {
                let Some(pid) = page.parent else { continue };
                let right = doc.settings.facing && doc.is_right_page(first + pi);
                for i in parent_chain(doc, pid) {
                    let parent = &doc.parents[i];
                    let at = if right { parent.pages.len().saturating_sub(1) } else { 0 };
                    let Some(pp) = parent.pages.get(at) else { continue };
                    let px = parent.page_x(at);
                    let prect = Rect::new(px, 0.0, px + pp.width, pp.height);
                    let shift = m * Affine::translate((pr.x0 - px, 0.0));
                    for id in &parent.items {
                        if page.detached.contains(id) || !layer_shown(doc, *id) || doc.bounds(*id).is_none_or(|b| b.intersect(prect).area() <= 0.0) {
                            continue;
                        }
                        f.parent_frames.push(json!({"color": layer_color(doc, *id), "polys": outline(doc, *id, shift, tol)}));
                    }
                }
            }
        }
        frame_edges(doc, &geo::top_items(doc, slot.spread), m, tol, &mut f.frames);
    }
    f
}

fn slot_of(doc: &Doc, cv: &Canvas, id: ItemId) -> Option<Affine> {
    let r = geo::spread_of_item(doc, doc.top_of(id))?;
    Some(cv.slot(r)?.xf)
}

fn thread_end(doc: &Doc, cv: &Canvas, id: ItemId, leaving: bool) -> Option<Point> {
    let it = doc.item(id)?;
    let b = it.shape.bbox();
    let local = if leaving { Point::new(b.x1 - 10.0, b.y1) } else { Point::new(b.x0 + 10.0, b.y0) };
    Some(slot_of(doc, cv, id)? * doc.world_xf(id) * local)
}

fn selection_json(ses: &Session, doc: &Doc, cv: &Canvas, lay: &DocLayout, tol: f64) -> Value {
    let Some(st) = ses.active() else { return Value::Null };
    let sel = &st.selection;
    let direct = DIRECT_TOOLS.contains(&ses.tool.as_str());
    let mut color = EDGE.to_string();
    let mut bounds: Option<Rect> = None;
    let (mut outlines, mut ports, mut threads, mut anchors) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut corners: Vec<Value> = Vec::new();
    let mut content_box = Value::Null;
    for &id in &sel.items {
        let (Some(it), Some(m)) = (doc.item(id), slot_of(doc, cv, id)) else { continue };
        color = layer_color(doc, id);
        outlines.extend(outline(doc, id, m, tol));
        if let Some(b) = doc.bounds(id) {
            let bb = m.transform_rect_bbox(b);
            bounds = Some(bounds.map_or(bb, |u| u.union(bb)));
        }
        let w = m * doc.world_xf(id);
        match &it.kind {
            ItemKind::Text(tf) => {
                if let Some(story) = doc.story(tf.story) {
                    let at = story.frames.iter().position(|f| *f == id).unwrap_or(0);
                    let last = at + 1 == story.frames.len();
                    let b = it.shape.bbox();
                    ports.push(json!({
                        "in": xy(w * Point::new(b.x0, b.y0)), "out": xy(w * Point::new(b.x1, b.y1)), "inLinked": at > 0, "outLinked": !last,
                        "overset": last && lay.overset(tf.story), "color": color, "story": tf.story.0,
                    }));
                    for pair in story.frames.windows(2) {
                        if let (Some(a), Some(b)) = (thread_end(doc, cv, pair[0], true), thread_end(doc, cv, pair[1], false)) {
                            threads.push(seg(a, b));
                        }
                    }
                }
            }
            ItemKind::Graphic(g) if sel.content => {
                if let Some(a) = g.asset.and_then(|a| doc.assets.get(&a)) {
                    let (aw, ah) = a.natural_size();
                    content_box = boxed((w * g.content).transform_rect_bbox(Rect::new(0.0, 0.0, aw, ah)));
                }
            }
            _ => {}
        }
        if direct && let Some((subs, _)) = geo::editable_anchors(doc, id) {
            for (si, (list, _)) in subs.iter().enumerate() {
                for (ai, an) in list.iter().enumerate() {
                    let handle = |h: Point| if (h - an.0).hypot() > 1e-6 { xy(w * h) } else { Value::Null };
                    anchors.push(json!({"p": xy(w * an.0), "in": handle(an.1), "out": handle(an.2), "picked": sel.anchors.contains(&(id, si, ai))}));
                }
            }
            for hd in geo::corner_handles(doc, id).into_iter().take(64) {
                corners.push(json!({"p": xy(w * hd.widget), "v": xy(w * hd.vertex), "r": hd.radius, "picked": sel.anchors.contains(&(id, hd.sub, hd.idx))}));
            }
        }
    }
    let pen = &ses.tools.pen;
    if let Some(m) = pen.slot.and_then(|r| cv.slot(r)).map(|s| s.xf) {
        let n = pen.anchors.len();
        for (i, an) in pen.anchors.iter().enumerate() {
            let handle = |h: Point| if (h - an.0).hypot() > 1e-6 { xy(m * h) } else { Value::Null };
            anchors.push(json!({"p": xy(m * an.0), "in": handle(an.1), "out": handle(an.2), "picked": i + 1 == n}));
        }
    }
    let only = if sel.items.len() == 1 { doc.item(sel.items[0]) } else { None };
    json!({
        "count": sel.items.len(), "content": sel.content, "color": color, "outlines": outlines,
        "bounds": bounds.map_or(Value::Null, boxed), "ports": ports, "threads": threads, "anchors": anchors, "corners": corners,
        "direct": direct || !pen.anchors.is_empty(), "contentBox": content_box,
        "liveCorner": only.is_some_and(|i| matches!(i.shape, formdes_model::item::Shape::Rect { .. }) && i.children().is_empty()),
        "grabber": only.is_some_and(|i| matches!(i.kind, ItemKind::Graphic(_))),
    })
}

fn quad(m: Affine, x0: f64, x1: f64, top: f64, foot: f64) -> Value {
    let pts = [Point::new(x0, top), Point::new(x1, top), Point::new(x1, foot), Point::new(x0, foot)].map(|p| m * p);
    Value::from(pts.iter().flat_map(|p| [q(p.x), q(p.y)]).collect::<Vec<f64>>())
}

fn caret_x(carets: &[(usize, f64)], byte: usize, left: f64) -> f64 {
    match carets.binary_search_by_key(&byte, |c| c.0) {
        Ok(i) => carets[i].1,
        Err(0) => carets.first().map(|c| c.1).unwrap_or(left),
        Err(i) => carets[i - 1].1,
    }
}

fn text_json(ses: &Session, doc: &Doc, cv: &Canvas, lay: &DocLayout, tol: f64) -> Value {
    let mut out = json!({"quads": [], "caret": null, "frames": []});
    let Some(st) = ses.active() else { return out };
    if let Some(ct) = st.selection.cell_text {
        if ct.range().is_empty() {
            if let Some((fid, x, top, foot)) = crate::celltext::caret_line(doc, lay, &ct, ct.focus)
                && let Some(m) = slot_of(doc, cv, fid).map(|s| s * doc.world_xf(fid))
            {
                out["caret"] = seg(m * Point::new(x, top), m * Point::new(x, foot));
            }
        } else if let Some((fid, qs)) = crate::celltext::quads(doc, lay, &ct)
            && let Some(m) = slot_of(doc, cv, fid).map(|s| s * doc.world_xf(fid))
        {
            out["quads"] = json!(qs.into_iter().map(|(x0, x1, t, f)| quad(m, x0, x1, t, f)).collect::<Vec<_>>());
        }
        return out;
    }
    if let Some(c) = st.selection.cells
        && let Some(sl) = lay.stories.get(&c.story)
        && let Some((para, _)) = sl.flow.para_byte(c.at)
    {
        let mut quads = Vec::new();
        for frag in sl.flow.tables.iter().filter(|t| t.para == para) {
            let Some(&fid) = sl.frames.get(frag.frame) else { continue };
            let Some(m) = slot_of(doc, cv, fid).map(|s| s * doc.world_xf(fid)) else { continue };
            let ranges: Vec<_> = std::iter::once(c).chain(st.selection.more_cells.iter().copied().filter(|m| m.story == c.story && m.at == c.at)).collect();
            let inside = |r: &crate::session::CellSel, row: usize, col: usize| (r.r0.min(r.r1)..=r.r0.max(r.r1)).contains(&row) && (r.c0.min(r.c1)..=r.c0.max(r.c1)).contains(&col);
            for cell in frag.cells.iter().filter(|x| ranges.iter().any(|r| inside(r, x.row, x.col))) {
                quads.push(quad(m, cell.rect.x0, cell.rect.x1, cell.rect.y0, cell.rect.y1));
            }
        }
        out["quads"] = json!(quads);
        return out;
    }
    let Some(ts) = st.selection.text else { return out };
    let Some(sl) = lay.stories.get(&ts.story) else { return out };
    let flow = &sl.flow;
    let xfs: Vec<Option<Affine>> = sl.frames.iter().map(|f| slot_of(doc, cv, *f).map(|s| s * doc.world_xf(*f))).collect();
    let mut shapes = Vec::new();
    for (fi, f) in sl.frames.iter().enumerate() {
        if let Some(Some(s)) = xfs.get(fi).map(|m| m.map(|m| m * doc.world_xf(*f).inverse())) {
            shapes.extend(outline(doc, *f, s, tol));
        }
    }
    out["frames"] = json!(shapes);
    let range = ts.range();
    if !range.is_empty() {
        let (Some((p0, b0)), Some((p1, b1))) = (flow.para_byte(range.start), flow.para_byte(range.end)) else { return out };
        let mut quads = Vec::new();
        for l in &flow.lines {
            if l.para < p0 || l.para > p1 {
                continue;
            }
            let Some(Some(m)) = xfs.get(l.frame) else { continue };
            let s = if l.para == p0 { b0.max(l.line.start) } else { l.line.start };
            let e = if l.para == p1 { b1.min(l.line.end) } else { l.line.end };
            let runs_past = e == l.line.end && (l.para < p1 || b1 > l.line.end);
            if s > e || (s == e && !runs_past) {
                continue;
            }
            let x0 = caret_x(&l.line.carets, s, l.line.left);
            let end_x = l.line.carets.last().map(|c| c.1).unwrap_or(l.line.left + l.line.width);
            let x1 = if runs_past { end_x.max(x0 + 3.0) } else { caret_x(&l.line.carets, e, l.line.left) };
            quads.push(quad(*m, x0, x1, l.baseline - l.line.ascent, l.baseline + l.line.descent));
        }
        out["quads"] = json!(quads);
        return out;
    }
    if let Some(c) = flow.caret(ts.focus)
        && let Some(Some(m)) = xfs.get(c.frame)
    {
        out["caret"] = seg(*m * Point::new(c.x, c.baseline - c.ascent), *m * Point::new(c.x, c.baseline + c.descent));
    } else if let Some(Some(m)) = xfs.first()
        && let Some(it) = sl.frames.first().and_then(|f| doc.item(*f))
    {
        let b = it.shape.bbox();
        out["caret"] = seg(*m * Point::new(b.x0 + 4.0, b.y0 + 4.0), *m * Point::new(b.x0 + 4.0, b.y0 + 16.0));
    }
    out
}

fn tool_json(o: &Overlay, tol: f64) -> Value {
    match o {
        Overlay::Marquee(r) => json!({"kind": "marquee", "rect": boxed(*r)}),
        Overlay::Path { path, color, dashed } => json!({"kind": "path", "polys": trace(path, Affine::IDENTITY, tol), "color": color, "dashed": dashed}),
        Overlay::Line { a, b, color, dashed } => json!({"kind": "line", "a": xy(*a), "b": xy(*b), "color": color, "dashed": dashed}),
        Overlay::Measure { p, text } => json!({"kind": "measure", "p": xy(*p), "text": text}),
        Overlay::Guide { a, b } => json!({"kind": "guide", "a": xy(*a), "b": xy(*b)}),
        Overlay::Gap { a, b, label } => json!({"kind": "gap", "a": xy(*a), "b": xy(*b), "label": label}),
    }
}

fn pen_overlay(ses: &Session, cv: &Canvas) -> Option<Overlay> {
    let pen = &ses.tools.pen;
    let m = cv.slot(pen.slot?)?.xf;
    if pen.anchors.len() < 2 {
        return None;
    }
    Some(Overlay::Path { path: m * geo::path_of(&[(pen.anchors.clone(), false)]), color: crate::tools::select::SEL_COLOR.into(), dashed: false })
}

pub fn overlays(ses: &mut Session, p: &Value, view: View) -> Value {
    let zoom = view.zoom;
    let tol = 0.25 / zoom.max(0.01);
    let visible = p.get("rect").and_then(Value::as_array).map(|a| {
        let n = |i: usize| a.get(i).and_then(Value::as_f64).unwrap_or(0.0);
        Rect::new(n(0), n(1), n(2), n(3))
    });
    let r = ses.renderer.clone();
    let Some(st) = ses.active_mut() else { return json!({"empty": true}) };
    let lay = st.layout(&r);
    let doc = st.doc.clone();
    let cv = st.canvas();
    let mut tool: Vec<Value> = ses.tools.overlays.iter().map(|o| tool_json(o, tol)).collect();
    if let Some(o) = pen_overlay(ses, &cv) {
        tool.push(tool_json(&o, tol));
    }
    let f = furniture(&doc, &cv, zoom, tol, visible);
    let grid = &doc.settings.baseline;
    json!({
        "zoom": zoom,
        "colors": {"margin": MARGIN, "column": COLUMN, "bleed": BLEED, "baseline": grid.color, "guide": GUIDE},
        "margins": f.margins, "columns": f.columns, "guides": f.guides, "bleed": f.bleeds, "baseline": f.baselines,
        "frames": f.frames, "parentFrames": f.parent_frames,
        "selection": selection_json(ses, &doc, &cv, &lay, tol),
        "text": text_json(ses, &doc, &cv, &lay, tol), "tool": tool,
        "page": ses.tools.page,
    })
}
