use formdes_model::doc::Doc;
use formdes_model::geom::{Affine, BezPath, KShape, Point, Rect};
use formdes_model::ids::ItemId;
use formdes_model::item::{ItemKind, Shape};
use formdes_model::page::{Guide, Orientation, Page};
use kurbo::{ParamCurveNearest, PathEl};

use crate::canvas::{Canvas, SpreadRef};
use crate::session::DocState;

pub type Anchor = (Point, Point, Point);

pub fn slot_at(st: &DocState, p: Point) -> Option<(SpreadRef, Affine)> {
    let cv: Canvas = st.canvas();
    let s = cv.hit_slot(p.x, p.y)?;
    Some((s.spread, s.xf))
}

pub fn slot_xf(st: &DocState, r: SpreadRef) -> Affine {
    st.canvas().xf(r)
}

pub fn top_items(doc: &Doc, r: SpreadRef) -> Vec<ItemId> {
    match r {
        SpreadRef::Doc(i) => doc.spreads.get(i).map(|s| s.items.clone()).unwrap_or_default(),
        SpreadRef::Parent(i) => doc.parents.get(i).map(|s| s.items.clone()).unwrap_or_default(),
    }
}

pub fn spread_of_item(doc: &Doc, id: ItemId) -> Option<SpreadRef> {
    doc.spread_of(id).map(SpreadRef::Doc).or_else(|| doc.parent_of(id).map(SpreadRef::Parent))
}

pub fn pages_of(doc: &Doc, r: SpreadRef) -> Vec<(Rect, Page, bool)> {
    match r {
        SpreadRef::Doc(si) => {
            let Some(s) = doc.spreads.get(si) else { return Vec::new() };
            let first = doc.first_page_of_spread(si);
            s.pages.iter().enumerate().map(|(i, p)| (Rect::new(s.page_x(i), 0.0, s.page_x(i) + p.width, p.height), p.clone(), doc.is_right_page(first + i))).collect()
        }
        SpreadRef::Parent(pi) => {
            let Some(s) = doc.parents.get(pi) else { return Vec::new() };
            let n = s.pages.len();
            s.pages.iter().enumerate().map(|(i, p)| (Rect::new(s.page_x(i), 0.0, s.page_x(i) + p.width, p.height), p.clone(), !doc.settings.facing || i + 1 == n)).collect()
        }
    }
}

pub fn guides_of(doc: &Doc, r: SpreadRef) -> Vec<Guide> {
    match r {
        SpreadRef::Doc(i) => doc.spreads.get(i).map(|s| s.guides.clone()).unwrap_or_default(),
        SpreadRef::Parent(i) => doc.parents.get(i).map(|s| s.guides.clone()).unwrap_or_default(),
    }
}

pub fn margin_rect(r: Rect, p: &Page, right: bool) -> Rect {
    let (l, rt) = p.margins.left_right(!right);
    Rect::new(r.x0 + l, r.y0 + p.margins.top, r.x1 - rt, r.y1 - p.margins.bottom)
}

pub fn column_rects(r: Rect, p: &Page, right: bool) -> Vec<Rect> {
    let m = margin_rect(r, p, right);
    let n = p.columns.max(1) as f64;
    let w = (m.width() - p.gutter * (n - 1.0)) / n;
    (0..p.columns.max(1)).map(|i| {
        let x = m.x0 + i as f64 * (w + p.gutter);
        Rect::new(x, m.y0, x + w, m.y1)
    }).collect()
}

pub fn selectable(doc: &Doc, id: ItemId) -> bool {
    let Some(it) = doc.item(id) else { return false };
    let layer = doc.layer(it.layer);
    !it.hidden && !it.locked && layer.is_none_or(|l| l.visible && !l.locked)
}

pub fn item_path(doc: &Doc, id: ItemId) -> BezPath {
    let Some(it) = doc.item(id) else { return BezPath::new() };
    doc.world_xf(id) * it.shape.to_path(&it.corner)
}

pub fn near_path(path: &BezPath, p: Point, tol: f64) -> bool {
    path.segments().any(|s| s.nearest(p, 1e-3).distance_sq <= tol * tol)
}

pub fn hit_leaf(doc: &Doc, id: ItemId, p: Point, tol: f64) -> bool {
    let Some(it) = doc.item(id) else { return false };
    let path = item_path(doc, id);
    let inside_counts = matches!(it.kind, ItemKind::Text(_) | ItemKind::Graphic(_)) || (!it.fill.is_none() && it.shape.closed());
    (inside_counts && path.contains(p)) || near_path(&path, p, tol.max(it.stroke.weight / 2.0))
}

pub fn hit_item(doc: &Doc, id: ItemId, p: Point, tol: f64) -> Option<ItemId> {
    let it = doc.item(id)?;
    match &it.kind {
        ItemKind::Group { .. } | ItemKind::States { .. } => it.children().iter().rev().find_map(|c| hit_item(doc, *c, p, tol)),
        _ => hit_leaf(doc, id, p, tol).then_some(id),
    }
}

fn ordered(doc: &Doc, r: SpreadRef) -> Vec<ItemId> {
    let items = top_items(doc, r);
    let mut out = Vec::new();
    for l in &doc.layers {
        for id in items.iter().rev() {
            if doc.item(*id).is_some_and(|it| it.layer == l.id) {
                out.push(*id);
            }
        }
    }
    out
}

pub fn hit_top(doc: &Doc, r: SpreadRef, p: Point, tol: f64) -> Option<ItemId> {
    ordered(doc, r).into_iter().filter(|id| selectable(doc, *id)).find(|id| hit_item(doc, *id, p, tol).is_some())
}

pub fn hit_deep(doc: &Doc, r: SpreadRef, p: Point, tol: f64) -> Option<ItemId> {
    ordered(doc, r).into_iter().filter(|id| selectable(doc, *id)).find_map(|id| hit_item(doc, id, p, tol))
}

pub fn bounds(doc: &Doc, ids: &[ItemId]) -> Option<Rect> {
    ids.iter().filter_map(|i| doc.bounds(*i)).reduce(|a, b| a.union(b))
}

pub fn handles(b: Rect) -> [Point; 8] {
    let c = b.center();
    [Point::new(b.x0, b.y0), Point::new(c.x, b.y0), Point::new(b.x1, b.y0), Point::new(b.x1, c.y), Point::new(b.x1, b.y1), Point::new(c.x, b.y1), Point::new(b.x0, b.y1), Point::new(b.x0, c.y)]
}

pub fn handle_cursor(i: usize) -> &'static str {
    match i {
        0 | 4 => "resizeNwse",
        2 | 6 => "resizeNesw",
        1 | 5 => "resizeV",
        _ => "resizeH",
    }
}

pub fn anchors_of(path: &BezPath) -> Vec<(Vec<Anchor>, bool)> {
    let mut subs = Vec::new();
    let mut cur: Vec<Anchor> = Vec::new();
    let mut closed = false;
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(a) => {
                if !cur.is_empty() {
                    subs.push((std::mem::take(&mut cur), closed));
                }
                closed = false;
                cur.push((a, a, a));
            }
            PathEl::LineTo(a) => cur.push((a, a, a)),
            PathEl::QuadTo(c, a) => {
                let p0 = cur.last().map(|x| x.0).unwrap_or(a);
                if let Some(l) = cur.last_mut() {
                    l.2 = p0 + (c - p0) * (2.0 / 3.0);
                }
                cur.push((a, a + (c - a) * (2.0 / 3.0), a));
            }
            PathEl::CurveTo(c1, c2, a) => {
                if let Some(l) = cur.last_mut() {
                    l.2 = c1;
                }
                cur.push((a, c2, a));
            }
            PathEl::ClosePath => {
                closed = true;
                if cur.len() > 1 && (cur[0].0 - cur[cur.len() - 1].0).hypot() < 1e-6 {
                    let last = cur.pop().expect("len checked");
                    cur[0].1 = last.1;
                }
            }
        }
    }
    if !cur.is_empty() {
        subs.push((cur, closed));
    }
    subs
}

fn seg(p: &mut BezPath, a: &Anchor, b: &Anchor) {
    if (a.2 - a.0).hypot() < 1e-9 && (b.1 - b.0).hypot() < 1e-9 {
        p.line_to(b.0);
    } else {
        p.curve_to(a.2, b.1, b.0);
    }
}

pub fn path_of(subs: &[(Vec<Anchor>, bool)]) -> BezPath {
    let mut p = BezPath::new();
    for (a, closed) in subs {
        if a.is_empty() {
            continue;
        }
        p.move_to(a[0].0);
        for w in a.windows(2) {
            seg(&mut p, &w[0], &w[1]);
        }
        if *closed && a.len() > 1 {
            seg(&mut p, &a[a.len() - 1], &a[0]);
            p.close_path();
        }
    }
    p
}

pub type Subpaths = Vec<(Vec<Anchor>, bool)>;

pub fn editable_anchors(doc: &Doc, id: ItemId) -> Option<(Subpaths, Affine)> {
    let it = doc.item(id)?;
    if matches!(it.kind, ItemKind::Group { .. } | ItemKind::States { .. }) {
        return None;
    }
    let path = if it.corner.live() { it.shape.to_path(&Default::default()) } else { it.shape.to_path(&it.corner) };
    Some((anchors_of(&path), doc.world_xf(id)))
}

pub fn set_anchors(doc: &mut Doc, id: ItemId, subs: &[(Vec<Anchor>, bool)]) {
    let closed = subs.iter().all(|s| s.1);
    if let Some(it) = doc.item_mut(id) {
        it.shape = Shape::Path { path: path_of(subs), closed };
        let count: usize = subs.iter().map(|s| s.0.len()).sum();
        if !it.corner.live() || it.corner.each.len() != count {
            it.corner.each.clear();
            it.corner.radius = 0.0;
        }
    }
}

pub struct Snaps {
    pub xs: Vec<f64>,
    pub ys: Vec<f64>,
}

pub fn snaps(doc: &Doc, r: SpreadRef, exclude: &[ItemId]) -> Snaps {
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for (pr, p, right) in pages_of(doc, r) {
        xs.extend([pr.x0, pr.x1, pr.center().x]);
        ys.extend([pr.y0, pr.y1, pr.center().y]);
        let m = margin_rect(pr, &p, right);
        xs.extend([m.x0, m.x1]);
        ys.extend([m.y0, m.y1]);
        for c in column_rects(pr, &p, right) {
            xs.extend([c.x0, c.x1]);
        }
    }
    for g in guides_of(doc, r) {
        match g.orientation {
            Orientation::Vertical => xs.push(g.pos),
            Orientation::Horizontal => ys.push(g.pos),
        }
    }
    for id in top_items(doc, r) {
        if exclude.contains(&id) {
            continue;
        }
        if let Some(b) = doc.bounds(id) {
            xs.extend([b.x0, b.center().x, b.x1]);
            ys.extend([b.y0, b.center().y, b.y1]);
        }
    }
    Snaps { xs, ys }
}

fn best(cands: &[f64], vals: &[f64], tol: f64) -> Option<f64> {
    let mut out: Option<(f64, f64)> = None;
    for v in vals {
        for c in cands {
            let d = c - v;
            if d.abs() <= tol && out.is_none_or(|o| d.abs() < o.1.abs()) {
                out = Some((d, d));
            }
        }
    }
    out.map(|o| o.0)
}

impl Snaps {
    pub fn point(&self, p: Point, tol: f64) -> Point {
        Point::new(p.x + best(&self.xs, &[p.x], tol).unwrap_or(0.0), p.y + best(&self.ys, &[p.y], tol).unwrap_or(0.0))
    }

    pub fn delta(&self, b: Rect, d: kurbo::Vec2, tol: f64) -> kurbo::Vec2 {
        let m = Rect::new(b.x0 + d.x, b.y0 + d.y, b.x1 + d.x, b.y1 + d.y);
        let dx = best(&self.xs, &[m.x0, m.center().x, m.x1], tol).unwrap_or(0.0);
        let dy = best(&self.ys, &[m.y0, m.center().y, m.y1], tol).unwrap_or(0.0);
        kurbo::Vec2::new(d.x + dx, d.y + dy)
    }
}

pub fn constrain(a: Point, b: Point) -> Point {
    let d = b - a;
    let ang = d.y.atan2(d.x);
    let step = std::f64::consts::FRAC_PI_4;
    let snapped = (ang / step).round() * step;
    let len = d.hypot();
    Point::new(a.x + len * snapped.cos(), a.y + len * snapped.sin())
}

pub fn drag_rect(a: Point, b: Point, square: bool, center: bool) -> Rect {
    let mut d = b - a;
    if square {
        let s = d.x.abs().max(d.y.abs());
        d = kurbo::Vec2::new(s * d.x.signum(), s * d.y.signum());
    }
    if center { Rect::new(a.x - d.x.abs(), a.y - d.y.abs(), a.x + d.x.abs(), a.y + d.y.abs()) } else { Rect::from_points(a, a + d) }
}

pub fn frame_flow(st: &DocState, lay: &formdes_text::layout::DocLayout, id: ItemId) -> Option<(formdes_model::ids::StoryId, usize)> {
    let _ = st;
    lay.frame_of.get(&id).copied()
}

pub fn text_hit(st: &DocState, lay: &formdes_text::layout::DocLayout, id: ItemId, p_spread: Point) -> Option<(formdes_model::ids::StoryId, usize)> {
    let (sid, fi) = lay.frame_of.get(&id).copied()?;
    let sl = lay.stories.get(&sid)?;
    let local = st.doc.world_xf(id).inverse() * p_spread;
    let story = st.doc.story(sid)?;
    let pos = sl.flow.hit(fi, local.x, local.y).unwrap_or(if fi == 0 { 0 } else { story.len() });
    Some((sid, pos.min(story.len())))
}

pub fn rect_path(r: Rect) -> BezPath {
    r.to_path(0.1)
}
