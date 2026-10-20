use formdesign_model::geom::{Affine, Point, Rect, Vec2};
use formdesign_model::ids::ItemId;
use formdesign_model::item::{Item, ItemKind, Shape};
use kurbo::BezPath;
use serde_json::{Value, json};

use crate::registry::{Needs, Registry, cmd};
use crate::session::{Ctx, Selection};
use crate::target::items;
use crate::{EngineError, Result, args, ops, wire};

const K: f64 = 0.552_284_749_8;

fn pt(p: &Value, k: &str) -> Result<Point> {
    let a = p.get(k).and_then(Value::as_array).filter(|a| a.len() >= 2).ok_or_else(|| EngineError::new(format!("missing `{k}` (a point [x, y] in spread points)")))?;
    let n = |i: usize| a[i].as_f64().ok_or_else(|| EngineError::new(format!("`{k}` expected two numbers")));
    Ok(Point::new(n(0)?, n(1)?))
}

fn rect_of(p: &Value) -> Result<Rect> {
    let a = p.get("rect").and_then(Value::as_array).filter(|a| a.len() >= 4).ok_or_else(|| EngineError::new("missing `rect` ([x0, y0, x1, y1] in spread points)"))?;
    let n = |i: usize| a[i].as_f64().ok_or_else(|| EngineError::new("`rect` expected four numbers"));
    let r = Rect::new(n(0)?, n(1)?, n(2)?, n(3)?).abs();
    if r.width() < 1e-3 || r.height() < 1e-3 {
        return Err(EngineError::new("`rect` has no area"));
    }
    Ok(r)
}

fn place(c: &mut Ctx, p: &Value, path: BezPath, closed: bool, label: &str) -> Result<Value> {
    let home = super::object::spread_home(c.doc()?, p.get("spread"))?;
    let layer = c.target_layer()?;
    let weight = args::opt_f64(p, "weight").unwrap_or(1.0).max(0.0);
    let doc = c.doc_mut()?;
    let mut it = Item::new(ItemId(0), ItemKind::Plain, Shape::Path { path, closed }, layer);
    it.stroke = ops::default_stroke(weight);
    let id = doc.insert_item(it, home);
    c.set_selection(Selection::items(vec![id]));
    c.label(label);
    Ok(json!({"id": id}))
}

fn spiral(c: &mut Ctx, p: &Value) -> Result<Value> {
    let center = pt(p, "center")?;
    let radius = args::opt_f64(p, "radius").unwrap_or(60.0).max(0.5);
    let decay = (args::opt_f64(p, "decay").unwrap_or(80.0) / 100.0).clamp(0.05, 1.5);
    let segments = args::opt_u64(p, "segments").unwrap_or(10).clamp(2, 400) as usize;
    let cw = args::opt_bool(p, "clockwise").unwrap_or(true);
    let sign = if cw { 1.0 } else { -1.0 };
    let dir = |q: usize| -> Vec2 {
        let a = std::f64::consts::FRAC_PI_2 * q as f64 * sign;
        Vec2::new(a.cos(), a.sin())
    };
    let mut path = BezPath::new();
    let mut r = radius;
    let mut c0 = center;
    let mut start = c0 + dir(0) * r;
    path.move_to(start);
    for q in 0..segments {
        let (d0, d1) = (dir(q), dir(q + 1));
        let end = c0 + d1 * r;
        let t0 = Vec2::new(-d0.y, d0.x) * sign;
        let t1 = Vec2::new(-d1.y, d1.x) * sign;
        path.curve_to(start + t0 * (r * K), end - t1 * (r * K), end);
        let next = r * decay;
        c0 = end - d1 * next;
        r = next;
        start = end;
    }
    place(c, p, path, false, "Spiral")
}

fn arc(c: &mut Ctx, p: &Value) -> Result<Value> {
    let a = pt(p, "a")?;
    let b = pt(p, "b")?;
    let slope = args::opt_f64(p, "slope").unwrap_or(50.0).clamp(-100.0, 100.0);
    let closed = args::opt_bool(p, "closed").unwrap_or(false);
    let corner = if slope >= 0.0 { Point::new(b.x, a.y) } else { Point::new(a.x, b.y) };
    let base = if slope >= 0.0 { Point::new(a.x, b.y) } else { Point::new(b.x, a.y) };
    let k = K * slope.abs() / 50.0;
    let mut path = BezPath::new();
    path.move_to(a);
    path.curve_to(a + (corner - a) * k, b + (corner - b) * k, b);
    if closed {
        path.line_to(base);
        path.close_path();
    }
    place(c, p, path, closed, "Arc")
}

fn grid(c: &mut Ctx, p: &Value) -> Result<Value> {
    let r = rect_of(p)?;
    let rows = args::opt_u64(p, "rows").unwrap_or(5).clamp(0, 500) as usize;
    let cols = args::opt_u64(p, "columns").unwrap_or(5).clamp(0, 500) as usize;
    let mut path = BezPath::new();
    for i in 0..=rows + 1 {
        let y = r.y0 + r.height() * i as f64 / (rows + 1) as f64;
        path.move_to((r.x0, y));
        path.line_to((r.x1, y));
    }
    for j in 0..=cols + 1 {
        let x = r.x0 + r.width() * j as f64 / (cols + 1) as f64;
        path.move_to((x, r.y0));
        path.line_to((x, r.y1));
    }
    place(c, p, path, false, "Rectangular Grid")
}

fn polar_grid(c: &mut Ctx, p: &Value) -> Result<Value> {
    let r = rect_of(p)?;
    let rings = args::opt_u64(p, "rings").unwrap_or(5).clamp(0, 200) as usize;
    let dividers = args::opt_u64(p, "dividers").unwrap_or(5).clamp(0, 360) as usize;
    let c0 = r.center();
    let (rx, ry) = (r.width() / 2.0, r.height() / 2.0);
    let mut path = BezPath::new();
    for i in 1..=rings + 1 {
        let f = i as f64 / (rings + 1) as f64;
        let e = kurbo::Ellipse::new(c0, (rx * f, ry * f), 0.0);
        path.extend(kurbo::Shape::path_elements(&e, 0.1));
    }
    for j in 0..dividers {
        let a = -std::f64::consts::FRAC_PI_2 + std::f64::consts::TAU * j as f64 / dividers.max(1) as f64;
        path.move_to(c0);
        path.line_to((c0.x + rx * a.cos(), c0.y + ry * a.sin()));
    }
    place(c, p, path, false, "Polar Grid")
}

fn copies(c: &mut Ctx, ids: &[ItemId], copy: bool) -> Result<Vec<ItemId>> {
    if !copy {
        return Ok(ids.to_vec());
    }
    let doc = c.doc_mut()?;
    Ok(ids.iter().filter_map(|id| ops::duplicate(doc, *id, Vec2::ZERO, None)).collect())
}

fn about(at: Point, m: Affine) -> Affine {
    Affine::translate(at.to_vec2()) * m * Affine::translate(-at.to_vec2())
}

fn reflect(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let b = ops::union_bounds(c.doc()?, &ids).ok_or_else(|| EngineError::new("the selection has no bounds"))?;
    let angle = match args::opt_str(p, "axis").unwrap_or("vertical") {
        "horizontal" => 0.0,
        "vertical" => 90.0,
        _ => args::opt_f64(p, "angle").unwrap_or(90.0),
    };
    let t = (-angle).to_radians();
    let m = Affine::rotate(t) * Affine::scale_non_uniform(1.0, -1.0) * Affine::rotate(-t);
    let targets = copies(c, &ids, args::opt_bool(p, "copy").unwrap_or(false))?;
    ops::transform_items(c.doc_mut()?, &targets, about(b.center(), m));
    c.set_selection(Selection::items(targets.clone()));
    c.label("Reflect");
    Ok(json!({"ids": targets}))
}

fn shear(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let angle = args::f64_(p, "angle", "degrees, -89 to 89")?.clamp(-89.0, 89.0);
    let b = ops::union_bounds(c.doc()?, &ids).ok_or_else(|| EngineError::new("the selection has no bounds"))?;
    let k = angle.to_radians().tan();
    let m = if args::opt_str(p, "axis") == Some("vertical") { Affine::new([1.0, k, 0.0, 1.0, 0.0, 0.0]) } else { Affine::new([1.0, 0.0, -k, 1.0, 0.0, 0.0]) };
    let targets = copies(c, &ids, args::opt_bool(p, "copy").unwrap_or(false))?;
    ops::transform_items(c.doc_mut()?, &targets, about(b.center(), m));
    c.set_selection(Selection::items(targets.clone()));
    c.label("Shear");
    Ok(json!({"ids": targets}))
}

fn each(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let sx = args::opt_f64(p, "scaleX").unwrap_or(100.0) / 100.0;
    let sy = args::opt_f64(p, "scaleY").unwrap_or(sx * 100.0) / 100.0;
    if sx.abs() < 1e-6 || sy.abs() < 1e-6 {
        return Err(EngineError::new("`scaleX` and `scaleY` must not be 0"));
    }
    let angle = args::opt_f64(p, "angle").unwrap_or(0.0);
    let d = Vec2::new(args::opt_f64(p, "dx").unwrap_or(0.0), args::opt_f64(p, "dy").unwrap_or(0.0));
    let fx = if args::opt_bool(p, "reflectX").unwrap_or(false) { -1.0 } else { 1.0 };
    let fy = if args::opt_bool(p, "reflectY").unwrap_or(false) { -1.0 } else { 1.0 };
    let targets = copies(c, &ids, args::opt_bool(p, "copy").unwrap_or(false))?;
    let doc = c.doc_mut()?;
    for id in &targets {
        let Some(b) = ops::union_bounds(doc, &[*id]) else { continue };
        let m = Affine::translate(d) * about(b.center(), Affine::rotate(-angle.to_radians()) * Affine::scale_non_uniform(sx * fx, sy * fy));
        ops::transform_items(doc, &[*id], m);
    }
    c.set_selection(Selection::items(targets.clone()));
    c.label("Transform Each");
    Ok(json!({"ids": targets}))
}

fn mix(a: formdesign_model::color::Color, b: formdesign_model::color::Color, t: f64) -> formdesign_model::color::Color {
    use formdesign_model::color::Color;
    match (a, b) {
        (Color::Cmyk(x), Color::Cmyk(y)) => Color::Cmyk([0, 1, 2, 3].map(|i| x[i] + (y[i] - x[i]) * t)),
        (Color::Gray(x), Color::Gray(y)) => Color::Gray(x + (y - x) * t),
        _ => {
            let (x, y) = (a.to_rgb(), b.to_rgb());
            Color::Rgb([0, 1, 2].map(|i| x[i] + (y[i] - x[i]) * t))
        }
    }
}

fn mix_paint(doc: &formdesign_model::doc::Doc, a: &formdesign_model::color::Paint, b: &formdesign_model::color::Paint, t: f64) -> formdesign_model::color::Paint {
    use formdesign_model::color::Paint;
    match (doc.paint_color(a), doc.paint_color(b)) {
        (Some(x), Some(y)) => Paint::Color { color: mix(x, y, t) },
        _ => if t < 0.5 { a.clone() } else { b.clone() },
    }
}

fn cubics(path: &BezPath) -> Option<(Vec<kurbo::CubicBez>, bool)> {
    use kurbo::ParamCurveArclen;
    let closed = path.elements().iter().any(|e| matches!(e, kurbo::PathEl::ClosePath));
    if path.elements().iter().filter(|e| matches!(e, kurbo::PathEl::MoveTo(_))).count() != 1 {
        return None;
    }
    let segs: Vec<kurbo::CubicBez> = path.segments().filter(|s| s.arclen(0.1) > 1e-3).map(|s| s.to_cubic()).collect();
    let closed = closed || segs.first().zip(segs.last()).is_some_and(|(a, b)| a.p0.distance(b.p3) < 1e-3);
    (!segs.is_empty()).then_some((segs, closed))
}

fn spine_point(segs: &[kurbo::CubicBez], f: f64) -> Point {
    use kurbo::{ParamCurve, ParamCurveArclen};
    let lens: Vec<f64> = segs.iter().map(|s| s.arclen(0.01)).collect();
    let mut left = f.clamp(0.0, 1.0) * lens.iter().sum::<f64>();
    for (s, l) in segs.iter().zip(&lens) {
        if left <= *l {
            return s.eval(s.inv_arclen(left, 0.01));
        }
        left -= l;
    }
    segs.last().map(|s| s.p3).unwrap_or(Point::ZERO)
}

fn signed_area(segs: &[kurbo::CubicBez]) -> f64 {
    segs.iter().map(|s| kurbo::ParamCurveArea::signed_area(s)).sum()
}

fn fractions(segs: &[kurbo::CubicBez]) -> Vec<f64> {
    use kurbo::ParamCurveArclen;
    let lens: Vec<f64> = segs.iter().map(|s| s.arclen(0.01)).collect();
    let total: f64 = lens.iter().sum::<f64>().max(1e-9);
    let mut acc = 0.0;
    lens.iter().map(|l| { acc += l; acc / total }).collect()
}

fn cut_at(segs: &[kurbo::CubicBez], cuts: &[f64]) -> Vec<kurbo::CubicBez> {
    use kurbo::{ParamCurve, ParamCurveArclen};
    let lens: Vec<f64> = segs.iter().map(|s| s.arclen(0.01)).collect();
    let total: f64 = lens.iter().sum::<f64>().max(1e-9);
    let mut out = Vec::new();
    let mut start = 0.0;
    for (s, l) in segs.iter().zip(&lens) {
        let (f0, f1) = (start / total, (start + l) / total);
        let mut ts = vec![0.0];
        for c in cuts.iter().filter(|c| **c > f0 + 1e-6 && **c < f1 - 1e-6) {
            ts.push(s.inv_arclen((c - f0) * total, 0.01));
        }
        ts.push(1.0);
        for w in ts.windows(2) {
            out.push(s.subsegment(w[0]..w[1]));
        }
        start += l;
    }
    out
}

fn matched(a: &BezPath, b: &BezPath) -> Option<(Vec<kurbo::CubicBez>, Vec<kurbo::CubicBez>, bool)> {
    let (sa, ca) = cubics(a)?;
    let (mut sb, cb) = cubics(b)?;
    if ca != cb {
        return None;
    }
    if ca {
        if signed_area(&sa).signum() != signed_area(&sb).signum() {
            sb = sb.iter().rev().map(|s| kurbo::CubicBez::new(s.p3, s.p2, s.p1, s.p0)).collect();
        }
        let (ba, bb) = (kurbo::Shape::bounding_box(a), kurbo::Shape::bounding_box(b));
        let rel = sa[0].p0 - ba.center();
        let want = bb.center() + kurbo::Vec2::new(rel.x * bb.width() / ba.width().max(1e-9), rel.y * bb.height() / ba.height().max(1e-9));
        let (i, t) = sb.iter().enumerate().map(|(i, s)| (i, kurbo::ParamCurveNearest::nearest(s, want, 1e-3))).min_by(|x, y| x.1.distance_sq.total_cmp(&y.1.distance_sq)).map(|(i, n)| (i, n.t)).unwrap_or((0, 0.0));
        if t > 1e-3 && t < 1.0 - 1e-3 {
            let s = sb[i];
            sb.splice(i..=i, [kurbo::ParamCurve::subsegment(&s, 0.0..t), kurbo::ParamCurve::subsegment(&s, t..1.0)]);
            sb.rotate_left(i + 1);
        } else {
            let k = if t >= 1.0 - 1e-3 { (i + 1) % sb.len() } else { i };
            sb.rotate_left(k);
        }
    }
    let (fa, fb) = (fractions(&sa), fractions(&sb));
    let (sa, sb) = (cut_at(&sa, &fb), cut_at(&sb, &fa));
    (sa.len() == sb.len()).then_some((sa, sb, ca))
}

fn blend(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    if ids.len() != 2 {
        return Err(EngineError::new("Blend needs exactly two objects selected"));
    }
    let steps = args::opt_u64(p, "steps").unwrap_or(5).clamp(1, 500) as usize;
    let distance = args::opt_f64(p, "distance");
    if distance.is_some_and(|d| d.is_nan() || d <= 0.0) {
        return Err(EngineError::new("distance is the space between steps in points, more than 0"));
    }
    let spine = args::item(p, "spine");
    let no_morph = args::opt_bool(p, "noMorph").unwrap_or(false);
    let doc = c.doc_mut()?;
    let (made, morph) = blend_into(doc, [ids[0], ids[1]], steps, distance, spine, no_morph)?;
    if args::opt_bool(p, "live").unwrap_or(false) {
        let mut members = vec![ids[0]];
        members.extend(&made);
        members.push(ids[1]);
        members.extend(spine);
        let gid = ops::group(doc, &members)?;
        mark_steps(doc, &made);
        let mut settings = json!({"steps": steps, "distance": distance, "spine": spine, "noMorph": no_morph});
        settings["sig"] = json!(blend_sig(doc, gid, &settings));
        if let Some(g) = doc.item_mut(gid) {
            g.extra.insert("blend".into(), settings);
        }
        c.set_selection(Selection::items(vec![gid]));
        c.label("Blend");
        return Ok(json!({"ids": made, "group": gid, "morph": morph}));
    }
    c.set_selection(Selection::items([vec![ids[0]], made.clone(), vec![ids[1]]].concat()));
    c.label("Blend");
    Ok(json!({"ids": made, "morph": morph}))
}

fn draw_mode(c: &mut Ctx, p: &Value) -> Result<Value> {
    use crate::tools::DrawMode;
    let inside = || -> Result<ItemId> {
        let st = c.state()?;
        let [id] = st.selection.items.as_slice() else { return Err(EngineError::new("Draw Inside needs one closed shape or frame selected")) };
        let it = st.doc.item(*id).ok_or_else(|| EngineError::new("the selected object is gone"))?;
        if !it.shape.closed() || !matches!(it.kind, ItemKind::Plain | ItemKind::Graphic(_)) {
            return Err(EngineError::new("Draw Inside needs a closed shape or a graphic frame, not a line, text frame or group"));
        }
        Ok(*id)
    };
    let next = match args::opt_str(p, "mode") {
        Some("normal") => DrawMode::Normal,
        Some("behind") => DrawMode::Behind(None),
        Some("inside") => DrawMode::Inside(inside()?),
        Some(_) => return Err(EngineError::new("mode is normal, behind or inside")),
        None => match c.session.tools.draw {
            DrawMode::Normal => DrawMode::Behind(None),
            DrawMode::Behind(_) => inside().map(DrawMode::Inside).unwrap_or(DrawMode::Normal),
            DrawMode::Inside(_) => DrawMode::Normal,
        },
    };
    c.session.tools.draw = next;
    c.session.sel_rev += 1;
    let name = match next {
        DrawMode::Normal => "normal",
        DrawMode::Behind(_) => "behind",
        DrawMode::Inside(_) => "inside",
    };
    Ok(json!({"mode": name, "inside": match next { DrawMode::Inside(s) => json!(s), _ => Value::Null }}))
}

fn isolate(c: &mut Ctx, p: &Value) -> Result<Value> {
    if args::opt_bool(p, "exit").unwrap_or(false) {
        let was = c.session.tools.isolated.take();
        if let Some(g) = was
            && c.state()?.doc.item(g).is_some()
        {
            c.set_selection(Selection::items(vec![g]));
        }
        c.session.sel_rev += 1;
        return Ok(json!({"isolated": Value::Null, "was": was}));
    }
    let ids = items(c, p)?;
    let doc = c.doc()?;
    let [id] = ids.as_slice() else { return Err(EngineError::new("select one group, or one object inside a group, to isolate")) };
    let it = doc.item(*id).ok_or_else(|| EngineError::new("the selected object is gone"))?;
    let gid = match it.kind {
        ItemKind::Group { .. } | ItemKind::States { .. } => *id,
        _ => it.group.ok_or_else(|| EngineError::new("isolation works on a group: this object is not in one"))?,
    };
    c.session.tools.isolated = Some(gid);
    c.session.sel_rev += 1;
    Ok(json!({"isolated": gid}))
}

fn live_blends(c: &mut Ctx, p: &Value) -> Result<Vec<ItemId>> {
    let ids = items(c, p)?;
    let doc = c.doc()?;
    let out: Vec<ItemId> = ids.into_iter().filter(|id| doc.item(*id).is_some_and(|it| it.extra.contains_key("blend"))).collect();
    if out.is_empty() {
        return Err(EngineError::new("select a live blend (made with Make Blend and live: true)"));
    }
    Ok(out)
}

fn blend_expand(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = live_blends(c, p)?;
    let doc = c.doc_mut()?;
    for gid in &ids {
        let kids = doc.item(*gid).map(|g| g.children()).unwrap_or_default();
        for k in kids {
            if let Some(it) = doc.item_mut(k)
                && it.extra.remove("blendStep").is_some()
            {
                it.locked = false;
            }
        }
        if let Some(g) = doc.item_mut(*gid) {
            g.extra.remove("blend");
        }
    }
    c.label("Expand Blend");
    Ok(json!({"groups": ids}))
}

fn blend_release(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = live_blends(c, p)?;
    let doc = c.doc_mut()?;
    let mut out = Vec::new();
    for gid in &ids {
        let steps: Vec<ItemId> = doc.item(*gid).map(|g| g.children()).unwrap_or_default().into_iter().filter(|k| doc.item(*k).is_some_and(|it| it.extra.contains_key("blendStep"))).collect();
        for s in steps {
            doc.remove_item(s);
        }
        if let Some(g) = doc.item_mut(*gid) {
            g.extra.remove("blend");
        }
        out.extend(ops::ungroup(doc, *gid)?);
    }
    c.set_selection(Selection::items(out.clone()));
    c.label("Release Blend");
    Ok(json!({"ids": out}))
}

fn mark_steps(doc: &mut formdesign_model::doc::Doc, made: &[ItemId]) {
    for id in made {
        if let Some(it) = doc.item_mut(*id) {
            it.extra.insert("blendStep".into(), json!(true));
            it.locked = true;
        }
    }
}

fn blend_settings(v: &Value) -> (usize, Option<f64>, Option<ItemId>, bool) {
    let steps = v.get("steps").and_then(Value::as_u64).unwrap_or(5).clamp(1, 500) as usize;
    let spine = v.get("spine").and_then(|s| serde_json::from_value(s.clone()).ok());
    (steps, v.get("distance").and_then(Value::as_f64), spine, v.get("noMorph").and_then(Value::as_bool).unwrap_or(false))
}

fn blend_parts(doc: &formdesign_model::doc::Doc, gid: ItemId, spine: Option<ItemId>) -> Vec<ItemId> {
    let Some(g) = doc.item(gid) else { return Vec::new() };
    g.children().into_iter().filter(|c| Some(*c) != spine && doc.item(*c).is_some_and(|it| !it.extra.contains_key("blendStep"))).collect()
}

fn blend_sig(doc: &formdesign_model::doc::Doc, gid: ItemId, settings: &Value) -> String {
    use std::hash::{Hash, Hasher};
    let (steps, distance, spine, no_morph) = blend_settings(settings);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    format!("{steps} {distance:?} {spine:?} {no_morph}").hash(&mut h);
    for id in blend_parts(doc, gid, spine).into_iter().chain(spine) {
        if let Some(it) = doc.item(id) {
            serde_json::to_string(it).unwrap_or_default().hash(&mut h);
            format!("{:?}", doc.world_xf(id)).hash(&mut h);
        }
    }
    format!("{:016x}", h.finish())
}

/// Rebuilds the steps of every live blend whose ends, spine or settings changed since it was last built.
/// The core calls this after every call, so the steps follow an end while it is dragged and undo restores
/// the steps with the ends.
pub fn refresh_blends(doc: &mut std::sync::Arc<formdesign_model::doc::Doc>) {
    let stale: Vec<ItemId> = doc.items.iter().filter_map(|(id, it)| {
        let s = it.extra.get("blend")?;
        (s.get("sig").and_then(Value::as_str) != Some(blend_sig(doc, *id, s).as_str())).then_some(*id)
    }).collect();
    if stale.is_empty() {
        return;
    }
    let d = std::sync::Arc::make_mut(doc);
    for gid in stale {
        let Some(settings) = d.item(gid).and_then(|g| g.extra.get("blend").cloned()) else { continue };
        let (steps, distance, spine, no_morph) = blend_settings(&settings);
        let old: Vec<ItemId> = d.item(gid).map(|g| g.children()).unwrap_or_default().into_iter().filter(|c| d.item(*c).is_some_and(|it| it.extra.contains_key("blendStep"))).collect();
        for id in old {
            d.remove_item(id);
        }
        let parts = blend_parts(d, gid, spine);
        if parts.len() == 2
            && let Ok((made, _)) = blend_into(d, [parts[0], parts[1]], steps, distance, spine, no_morph)
        {
            mark_steps(d, &made);
        }
        let mut s = settings.clone();
        s["sig"] = json!(blend_sig(d, gid, &settings));
        if let Some(g) = d.item_mut(gid) {
            g.extra.insert("blend".into(), s);
        }
    }
}

fn blend_into(doc: &mut formdesign_model::doc::Doc, ids: [ItemId; 2], mut steps: usize, distance: Option<f64>, spine_id: Option<ItemId>, no_morph: bool) -> Result<(Vec<ItemId>, bool)> {
    use crate::tools::geo;
    let (ia, ib) = (doc.item(ids[0]).cloned(), doc.item(ids[1]).cloned());
    let (Some(ia), Some(ib)) = (ia, ib) else { return Err(EngineError::new("both objects must exist")) };
    if matches!(ia.kind, ItemKind::Group { .. } | ItemKind::States { .. }) || matches!(ib.kind, ItemKind::Group { .. } | ItemKind::States { .. }) {
        return Err(EngineError::new("Blend works on shapes and paths, not groups"));
    }
    let mut wa = doc.world_xf(ids[0]) * ia.shape.to_path(&ia.corner);
    let mut wb = doc.world_xf(ids[1]) * ib.shape.to_path(&ib.corner);
    let spine = match spine_id {
        None => None,
        Some(sid) if ids.contains(&sid) => return Err(EngineError::new("the spine is a third path, not one of the two blended objects")),
        Some(sid) => {
            let it = doc.item(sid).ok_or_else(|| EngineError::new(format!("no object {sid} for the spine")))?;
            let (segs, _) = cubics(&(doc.world_xf(sid) * it.shape.to_path(&it.corner))).ok_or_else(|| EngineError::new("the spine must be one path of one piece"))?;
            Some(segs)
        }
    };
    let (mut ends, mut length) = ((Vec2::ZERO, Vec2::ZERO), kurbo::Shape::bounding_box(&wa).center().distance(kurbo::Shape::bounding_box(&wb).center()));
    if let Some(segs) = &spine {
        let (start, end) = (spine_point(segs, 0.0), spine_point(segs, 1.0));
        ends = (start - kurbo::Shape::bounding_box(&wa).center(), end - kurbo::Shape::bounding_box(&wb).center());
        wa = Affine::translate(ends.0) * wa;
        wb = Affine::translate(ends.1) * wb;
        length = segs.iter().map(|s| kurbo::ParamCurveArclen::arclen(s, 0.01)).sum();
    }
    if let Some(d) = distance {
        steps = ((length / d).round() as usize).saturating_sub(1).clamp(1, 500);
    }
    let (sa, sb) = (geo::anchors_of(&wa), geo::anchors_of(&wb));
    let same = sa.len() == sb.len() && sa.iter().zip(&sb).all(|(x, y)| x.0.len() == y.0.len() && x.1 == y.1);
    let (ba, bb) = (kurbo::Shape::bounding_box(&wa), kurbo::Shape::bounding_box(&wb));
    let morph = !no_morph && same;
    let resampled = if no_morph || same { None } else { matched(&wa, &wb) };
    let mut made = Vec::new();
    for k in 1..=steps {
        let t = k as f64 / (steps + 1) as f64;
        let lp = |a: Point, b: Point| a + (b - a) * t;
        let world = if morph {
            let subs: Vec<(Vec<geo::Anchor>, bool)> = sa.iter().zip(&sb).map(|(x, y)| (x.0.iter().zip(&y.0).map(|(u, v)| (lp(u.0, v.0), lp(u.1, v.1), lp(u.2, v.2))).collect(), x.1)).collect();
            geo::path_of(&subs)
        } else if let Some((ca, cb, closed)) = &resampled {
            let mut bp = BezPath::new();
            bp.move_to(lp(ca[0].p0, cb[0].p0));
            for (x, y) in ca.iter().zip(cb) {
                bp.curve_to(lp(x.p1, y.p1), lp(x.p2, y.p2), lp(x.p3, y.p3));
            }
            if *closed {
                bp.close_path();
            }
            bp
        } else {
            let r = Rect::new(ba.x0 + (bb.x0 - ba.x0) * t, ba.y0 + (bb.y0 - ba.y0) * t, ba.x1 + (bb.x1 - ba.x1) * t, ba.y1 + (bb.y1 - ba.y1) * t);
            formdesign_model::item::map_rect(ba, r) * wa.clone()
        };
        let world = match &spine {
            Some(segs) => Affine::translate(spine_point(segs, t) - (ba.center() + (bb.center() - ba.center()) * t)) * world,
            None => world,
        };
        let fill = mix_paint(doc, &ia.fill, &ib.fill, t);
        let stroke_paint = mix_paint(doc, &ia.stroke.paint, &ib.stroke.paint, t);
        let Some(nid) = ops::duplicate(doc, ids[0], Vec2::ZERO, None) else { continue };
        let local = doc.world_xf(nid).inverse() * world;
        if let Some(it) = doc.item_mut(nid) {
            it.shape = Shape::Path { path: local, closed: ia.shape.closed() };
            it.corner = Default::default();
            it.kind = ItemKind::Plain;
            it.fill = fill;
            it.stroke.paint = stroke_paint;
            it.stroke.weight = ia.stroke.weight + (ib.stroke.weight - ia.stroke.weight) * t;
            it.opacity = ia.opacity + (ib.opacity - ia.opacity) * t;
        }
        made.push(nid);
    }
    if spine.is_some() {
        ops::move_items(doc, &[ids[0]], ends.0);
        ops::move_items(doc, &[ids[1]], ends.1);
    }
    if let Some(home) = doc.home(ids[0]).filter(|h| doc.home(ids[1]).as_ref() == Some(h)) {
        if let Some(list) = ops::home_list(doc, &home) {
            list.retain(|x| !made.contains(x));
            let (pa, pb) = (list.iter().position(|x| *x == ids[0]), list.iter().position(|x| *x == ids[1]));
            if let (Some(pa), Some(pb)) = (pa, pb) {
                let mut steps = made.clone();
                if pa > pb { steps.reverse(); }
                let at = pa.min(pb) + 1;
                list.splice(at..at, steps);
            }
        }
    }
    Ok((made, morph || resampled.is_some()))
}

fn width_profile(c: &mut Ctx, p: &Value) -> Result<Value> {
    use formdesign_model::widths;
    let ids = items(c, p)?;
    let clear = args::opt_bool(p, "clear").unwrap_or(false) || args::opt_str(p, "profile") == Some("uniform");
    let mut prof: Option<widths::Profile> = None;
    if !clear {
        if let Some(name) = args::opt_str(p, "profile") {
            prof = Some(widths::preset(name).ok_or_else(|| EngineError::new(format!("profile is one of {}", widths::PRESETS.join(", "))))?);
        } else if p.get("points").is_some() {
            let mut a = formdesign_model::Attrs::new();
            a.insert("widthProfile".into(), p["points"].clone());
            prof = Some(widths::profile_of(&a).ok_or_else(|| EngineError::new("`points` is a list of [position 0..1, width] or [position, left, right], widths as multiples of the stroke weight"))?);
        } else {
            return Err(EngineError::new("give a `profile` name, `points`, or clear: true"));
        }
    }
    if args::opt_bool(p, "flip").unwrap_or(false) {
        if let Some(pr) = prof.as_mut() {
            *pr = pr.iter().rev().map(|(t, l, r)| (1.0 - t, *r, *l)).collect();
        }
    }
    let doc = c.doc_mut()?;
    let mut n = 0;
    for id in &ids {
        let Some(it) = doc.item_mut(*id) else { continue };
        match &prof {
            None => { it.extra.remove("widthProfile"); }
            Some(pr) => { it.extra.insert("widthProfile".into(), json!(pr.iter().map(|(t, l, r)| json!([t, l, r])).collect::<Vec<_>>())); }
        }
        n += 1;
    }
    c.label("Width Profile");
    Ok(json!({"items": n}))
}

fn effect(c: &mut Ctx, p: &Value) -> Result<Value> {
    use formdesign_model::effects::KINDS;
    let ids = items(c, p)?;
    let clear = args::opt_bool(p, "clear").unwrap_or(false);
    let remove = args::opt_u64(p, "remove").map(|v| v as usize);
    let kind = args::opt_str(p, "kind");
    // index with on and no kind shows or hides one effect, keeping its settings.
    if kind.is_none()
        && let (Some(i), Some(on)) = (args::opt_u64(p, "index").map(|v| v as usize), args::opt_bool(p, "on"))
    {
        return toggle_entry(c, &ids, "effects", i, on, "Effect");
    }
    if !clear && remove.is_none() && !kind.is_some_and(|k| KINDS.contains(&k)) {
        return Err(EngineError::new(format!("kind is one of {} (or clear: true, or remove: index, or index with on)", KINDS.join(", "))));
    }
    let mut entry = p.as_object().cloned().unwrap_or_default();
    for k in ["ids", "index", "clear", "remove"] {
        entry.remove(k);
    }
    let index = args::opt_u64(p, "index").map(|v| v as usize);
    let doc = c.doc_mut()?;
    let mut n = 0;
    for id in &ids {
        let Some(it) = doc.item_mut(*id) else { continue };
        let mut list = it.extra.get("effects").and_then(Value::as_array).cloned().unwrap_or_default();
        if clear {
            list.clear();
        } else if let Some(i) = remove {
            if i < list.len() { list.remove(i); }
        } else if let Some(i) = index.filter(|i| *i < list.len()) {
            list[i] = Value::Object(entry.clone());
        } else {
            list.push(Value::Object(entry.clone()));
        }
        if list.is_empty() { it.extra.remove("effects"); } else { it.extra.insert("effects".into(), Value::Array(list)); }
        n += 1;
    }
    c.label(if clear { "Clear Effects" } else { "Effect" });
    Ok(json!({"items": n}))
}

/// Sets `on` for entry `i` of the list `key` in each item's extras (an appearance or effect entry hidden or shown).
fn toggle_entry(c: &mut Ctx, ids: &[ItemId], key: &str, i: usize, on: bool, label: &str) -> Result<Value> {
    let doc = c.doc_mut()?;
    let mut n = 0;
    for id in ids {
        if let Some(e) = doc.item_mut(*id).and_then(|it| it.extra.get_mut(key)).and_then(Value::as_array_mut).and_then(|a| a.get_mut(i)) {
            e["on"] = json!(on);
            n += 1;
        }
    }
    if n == 0 {
        return Err(EngineError::new(format!("no {key} entry {i} on the selection")));
    }
    c.label(label);
    Ok(json!({"items": n}))
}

fn appearance(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let clear = args::opt_bool(p, "clear").unwrap_or(false);
    let remove = args::opt_u64(p, "remove").map(|v| v as usize);
    let index = args::opt_u64(p, "index").map(|v| v as usize);
    if p.get("kind").is_none()
        && let (Some(i), Some(on)) = (index, args::opt_bool(p, "on"))
    {
        return toggle_entry(c, &ids, "appearance", i, on, "Appearance");
    }
    let entry = if clear || remove.is_some() {
        None
    } else {
        let kind = args::opt_str(p, "kind").filter(|k| *k == "fill" || *k == "stroke").ok_or_else(|| EngineError::new("kind is fill or stroke (or clear: true, or remove: index)"))?;
        let name = args::str_(p, "swatch", "a swatch name such as Accent or #rrggbb")?;
        if c.doc()?.swatch(name).is_none() && !name.starts_with('#') && !wire::LOCKED.iter().any(|l| l.eq_ignore_ascii_case(&formdesign_model::color::swatch_key(name))) {
            return Err(EngineError::new(format!("no swatch named {name}")));
        }
        let mut e = serde_json::Map::new();
        e.insert("kind".into(), json!(kind));
        e.insert("paint".into(), serde_json::to_value(wire::paint_in(name, args::opt_f64(p, "tint"))).unwrap_or(Value::Null));
        for k in ["weight", "opacity", "cap", "join", "dash", "effects", "above", "evenOdd", "on"] {
            if let Some(v) = p.get(k) {
                e.insert(k.into(), v.clone());
            }
        }
        Some(Value::Object(e))
    };
    let doc = c.doc_mut()?;
    let mut n = 0;
    for id in &ids {
        let Some(it) = doc.item_mut(*id) else { continue };
        let mut list = it.extra.get("appearance").and_then(Value::as_array).cloned().unwrap_or_default();
        match (&entry, remove) {
            (_, Some(i)) => { if i < list.len() { list.remove(i); } }
            (None, None) => list.clear(),
            (Some(e), None) => match index.filter(|i| *i < list.len()) {
                Some(i) => list[i] = e.clone(),
                None => list.push(e.clone()),
            },
        }
        if list.is_empty() { it.extra.remove("appearance"); } else { it.extra.insert("appearance".into(), Value::Array(list)); }
        n += 1;
    }
    c.label("Appearance");
    Ok(json!({"items": n}))
}

fn expand_appearance(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let mut made = Vec::new();
    for id in &ids {
        let doc = c.doc_mut()?;
        let Some(it) = doc.item(*id).cloned() else { continue };
        let Some(list) = it.extra.get("appearance").and_then(Value::as_array).cloned() else { continue };
        let base = formdesign_model::effects::apply(&it.extra, it.shape.to_path(&it.corner));
        let mut below = Vec::new();
        let mut above = Vec::new();
        for e in &list {
            let Some(paint) = e.get("paint").and_then(|v| serde_json::from_value::<formdesign_model::color::Paint>(v.clone()).ok()) else { continue };
            let Some(nid) = ops::duplicate(doc, *id, Vec2::ZERO, None) else { continue };
            let fx = e.get("effects").and_then(Value::as_array).cloned().unwrap_or_default();
            let path = fx.iter().fold(base.clone(), |q, f| formdesign_model::effects::one(f, &q));
            if let Some(d) = doc.item_mut(nid) {
                d.shape = Shape::Path { path, closed: it.shape.closed() };
                d.corner = Default::default();
                d.kind = ItemKind::Plain;
                d.extra.remove("effects");
                d.extra.remove("appearance");
                d.extra.remove("widthProfile");
                d.opacity = it.opacity * e.get("opacity").and_then(Value::as_f64).unwrap_or(100.0) / 100.0;
                if e.get("kind").and_then(Value::as_str) == Some("stroke") {
                    d.fill = formdesign_model::color::Paint::None;
                    d.stroke.paint = paint;
                    d.stroke.weight = e.get("weight").and_then(Value::as_f64).unwrap_or(1.0);
                    if let Some(s) = e.get("cap").and_then(Value::as_str) { d.stroke.cap = s.into(); }
                    if let Some(s) = e.get("join").and_then(Value::as_str) { d.stroke.join = s.into(); }
                    d.stroke.dash = e.get("dash").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
                } else {
                    d.fill = paint;
                    d.stroke.paint = formdesign_model::color::Paint::None;
                    d.stroke.weight = 0.0;
                }
            }
            if e.get("above").and_then(Value::as_bool).unwrap_or(false) { above.push(nid); } else { below.push(nid); }
        }
        if let Some(home) = doc.home(*id)
            && let Some(list) = ops::home_list(doc, &home)
        {
            list.retain(|x| !below.contains(x) && !above.contains(x));
            if let Some(at) = list.iter().position(|x| x == id) {
                list.splice(at + 1..at + 1, above.iter().copied());
                list.splice(at..at, below.iter().copied());
            }
        }
        if let Some(d) = doc.item_mut(*id) {
            d.extra.remove("appearance");
        }
        let all: Vec<ItemId> = below.iter().chain([id]).chain(above.iter()).copied().collect();
        made.push(if all.len() > 1 { ops::group(doc, &all).unwrap_or(*id) } else { *id });
    }
    let doc = c.doc_mut()?;
    let mut n = made.len();
    for id in made.clone().iter().chain(ids.iter()) {
        let Some(it) = doc.item(*id) else { continue };
        let targets: Vec<ItemId> = if matches!(it.kind, ItemKind::Group { .. }) { it.children() } else { vec![*id] };
        for t in targets {
            let Some(it) = doc.item_mut(t) else { continue };
            if !it.extra.contains_key("effects") || matches!(it.kind, ItemKind::Group { .. } | ItemKind::States { .. }) {
                continue;
            }
            let closed = it.shape.closed();
            let path = formdesign_model::effects::apply(&it.extra, it.shape.to_path(&it.corner));
            it.shape = Shape::Path { path, closed };
            it.corner = Default::default();
            it.extra.remove("effects");
            n += 1;
        }
    }
    if n == 0 {
        return Err(EngineError::new("none of the selected objects has live effects or extra fills and strokes to expand"));
    }
    if !made.is_empty() {
        c.set_selection(Selection::items(made.clone()));
    }
    c.label("Expand Appearance");
    Ok(json!({"items": n, "ids": made}))
}

fn mask_group(c: &mut Ctx, p: &Value, key: &str, label: &str) -> Result<Value> {
    let ids = items(c, p)?;
    let release = args::opt_bool(p, "release").unwrap_or(false);
    let doc = c.doc_mut()?;
    if release {
        let mut out = Vec::new();
        for id in &ids {
            let gid = if doc.item(*id).is_some_and(|i| i.extra.contains_key(key)) { *id } else { match doc.item(*id).and_then(|i| i.group) { Some(g) => g, None => continue } };
            let Some(g) = doc.item_mut(gid) else { continue };
            if g.extra.remove(key).is_none() {
                continue;
            }
            g.extra.remove("maskClip");
            out.push(gid);
        }
        if out.is_empty() {
            return Err(EngineError::new(format!("the selection has no {} to release", label.to_lowercase())));
        }
        c.label(&format!("Release {label}"));
        return Ok(json!({"groups": out}));
    }
    let order = super::art::paint_order(doc, &ids);
    if order.len() < 2 {
        return Err(EngineError::new(format!("{label} needs two or more objects: the topmost becomes the mask")));
    }
    let mask = order[order.len() - 1];
    let gid = ops::group(doc, &order)?;
    if let Some(g) = doc.item_mut(gid) {
        g.extra.insert(key.into(), json!(mask.0));
        if key == "opacityMask" {
            g.extra.insert("maskClip".into(), json!(args::opt_bool(p, "clip").unwrap_or(true)));
        }
    }
    c.set_selection(Selection::items(vec![gid]));
    c.label(&format!("Make {label}"));
    Ok(json!({"id": gid, "mask": mask.0}))
}

fn opacity_mask(c: &mut Ctx, p: &Value) -> Result<Value> {
    mask_group(c, p, "opacityMask", "Opacity Mask")
}

fn clipping_mask(c: &mut Ctx, p: &Value) -> Result<Value> {
    mask_group(c, p, "clipMask", "Clipping Mask")
}

fn isolate_blending(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let on = args::opt_bool(p, "on").unwrap_or(true);
    let doc = c.doc_mut()?;
    let mut n = 0;
    for id in &ids {
        let Some(it) = doc.item_mut(*id) else { continue };
        if on { it.extra.insert("isolate".into(), json!(true)); } else { it.extra.remove("isolate"); }
        n += 1;
    }
    c.label("Isolate Blending");
    Ok(json!({"items": n}))
}

fn mesh(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let clear = args::opt_bool(p, "clear").unwrap_or(false);
    let size = |k: &str| -> Result<Option<u64>> {
        match args::opt_u64(p, k) {
            Some(n) if !(1..=12).contains(&n) => Err(EngineError::new(format!("{k} is 1 to 12"))),
            v => Ok(v),
        }
    };
    let (cols, rows) = (size("cols")?, size("rows")?);
    let point: Option<[usize; 2]> = match p.get("point") {
        None => None,
        Some(v) => Some(v.as_array().and_then(|a| Some([a.first()?.as_u64()? as usize, a.get(1)?.as_u64()? as usize])).ok_or_else(|| EngineError::new("point is [col, row] on the grid of mesh points"))?),
    };
    let to: Option<[f64; 2]> = match p.get("to") {
        None => None,
        Some(v) => Some(v.as_array().and_then(|a| Some([a.first()?.as_f64()?.clamp(-0.5, 1.5), a.get(1)?.as_f64()?.clamp(-0.5, 1.5)])).ok_or_else(|| EngineError::new("to is [x, y] as shares of the object's bounds"))?),
    };
    let doc = c.doc()?;
    let color: Option<Vec<f64>> = match args::opt_str(p, "color") {
        None => None,
        Some(name) => Some(doc.paint_color(&wire::paint_in(name, None)).map(|c| c.to_rgb().to_vec()).ok_or_else(|| EngineError::new(format!("no colour for {name}: use a swatch name or #rrggbb")))?),
    };
    let highlight = args::opt_f64(p, "highlight").map(|h| h.clamp(0.0, 100.0) / 100.0);
    let mut plans: Vec<(ItemId, Option<Value>)> = Vec::new();
    for id in &ids {
        let it = doc.item(*id).ok_or_else(|| EngineError::new(format!("no object {id}")))?;
        if !it.shape.closed() || !matches!(it.kind, ItemKind::Plain | ItemKind::Graphic(_)) {
            return Err(EngineError::new("a gradient mesh goes on a closed shape or frame, not a line, text frame or group"));
        }
        if clear {
            plans.push((*id, None));
            continue;
        }
        let mut m = match it.extra.get("mesh").cloned() {
            Some(cur) if cols.is_none() && rows.is_none() => cur,
            _ => {
                let (cs, rs) = (cols.unwrap_or(3) as usize, rows.unwrap_or(3) as usize);
                let base = color.clone().or_else(|| doc.paint_color(&it.fill).map(|c| c.to_rgb().to_vec())).unwrap_or_else(|| vec![1.0, 1.0, 1.0]);
                let mut colors = Vec::new();
                for j in 0..=rs {
                    for i in 0..=cs {
                        let inner = i > 0 && i < cs && j > 0 && j < rs;
                        let h = if inner { highlight.unwrap_or(0.0) } else { 0.0 };
                        colors.push(json!(base.iter().map(|v| v + (1.0 - v) * h).collect::<Vec<f64>>()));
                    }
                }
                json!({"cols": cs, "rows": rs, "colors": colors})
            }
        };
        if let Some([pc, pr]) = point {
            let cs = m.get("cols").and_then(Value::as_u64).unwrap_or(0) as usize;
            let rs = m.get("rows").and_then(Value::as_u64).unwrap_or(0) as usize;
            if pc > cs || pr > rs {
                return Err(EngineError::new(format!("point is [col 0..{cs}, row 0..{rs}] on this mesh")));
            }
            let idx = pr * (cs + 1) + pc;
            if let Some(rgb) = &color
                && let Some(slot) = m.get_mut("colors").and_then(Value::as_array_mut).and_then(|a| a.get_mut(idx))
            {
                *slot = json!(rgb);
            }
            if let Some(q) = to {
                if m.get("points").is_none() {
                    let pts: Vec<Value> = (0..(cs + 1) * (rs + 1)).map(|i| json!([(i % (cs + 1)) as f64 / cs as f64, (i / (cs + 1)) as f64 / rs as f64])).collect();
                    m["points"] = json!(pts);
                }
                if let Some(slot) = m.get_mut("points").and_then(Value::as_array_mut).and_then(|a| a.get_mut(idx)) {
                    *slot = json!(q);
                }
            }
        }
        plans.push((*id, Some(m)));
    }
    let doc = c.doc_mut()?;
    for (id, m) in &plans {
        if let Some(it) = doc.item_mut(*id) {
            match m {
                Some(m) => {
                    it.extra.insert("mesh".into(), m.clone());
                    it.extra.remove("freeform");
                }
                None => { it.extra.remove("mesh"); }
            }
        }
    }
    c.label("Gradient Mesh");
    Ok(json!({"items": plans.len()}))
}

fn freeform_gradient(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let clear = args::opt_bool(p, "clear").unwrap_or(false);
    let mut pts = Vec::new();
    if !clear {
        let doc = c.doc()?;
        let list = p.get("points").and_then(Value::as_array).ok_or_else(|| EngineError::new("`points` is a list of [x 0..1, y 0..1, colour] across the object's bounds (colour a swatch name or #rrggbb), or clear: true"))?;
        for q in list {
            let q = q.as_array().filter(|q| q.len() >= 3).ok_or_else(|| EngineError::new("each point is [x, y, colour]"))?;
            let (Some(u), Some(v), Some(name)) = (q[0].as_f64(), q[1].as_f64(), q[2].as_str()) else {
                return Err(EngineError::new("each point is [x 0..1, y 0..1, colour]"));
            };
            let col = doc.paint_color(&wire::paint_in(name, None)).ok_or_else(|| EngineError::new(format!("no colour for {name}: use a swatch name or #rrggbb")))?;
            let rgb = col.to_rgb();
            pts.push(json!([u.clamp(0.0, 1.0), v.clamp(0.0, 1.0), rgb[0], rgb[1], rgb[2]]));
        }
        if pts.is_empty() {
            return Err(EngineError::new("give at least one point"));
        }
    }
    let spread = args::opt_f64(p, "spread");
    let doc = c.doc_mut()?;
    let mut n = 0;
    for id in &ids {
        let Some(it) = doc.item_mut(*id) else { continue };
        if clear {
            it.extra.remove("freeform");
            it.extra.remove("freeformSpread");
        } else {
            it.extra.insert("freeform".into(), Value::Array(pts.clone()));
            match spread {
                Some(s) => { it.extra.insert("freeformSpread".into(), json!(s)); }
                None => { it.extra.remove("freeformSpread"); }
            }
        }
        n += 1;
    }
    c.label("Freeform Gradient");
    Ok(json!({"items": n}))
}

fn repeat(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let release = args::opt_bool(p, "release").unwrap_or(false);
    let expand = args::opt_bool(p, "expand").unwrap_or(false);
    let doc = c.doc_mut()?;
    let single_group = ids.len() == 1 && doc.item(ids[0]).is_some_and(|i| matches!(i.kind, ItemKind::Group { .. }));
    if release || expand {
        let gid = if single_group { ids[0] } else { match doc.item(ids[0]).and_then(|i| i.group) { Some(g) => g, None => ids[0] } };
        let Some(spec) = doc.item(gid).and_then(|g| g.extra.get("repeat").cloned()) else {
            return Err(EngineError::new("the selection is not a Repeat"));
        };
        if expand {
            let g_world = doc.world_xf(gid);
            let xfs = formdesign_render::scene::repeat_xfs(&spec, doc.bounds(gid));
            let kids = doc.item(gid).map(|g| g.children()).unwrap_or_default();
            let mut made = Vec::new();
            for t in xfs.iter().skip(1) {
                for k in &kids {
                    let Some(nid) = ops::duplicate(doc, *k, Vec2::ZERO, None) else { continue };
                    if let Some(it) = doc.item_mut(nid) {
                        it.xf = g_world.inverse() * *t * g_world * it.xf;
                    }
                    made.push(nid);
                }
            }
            if let Some(g) = doc.item_mut(gid) {
                g.extra.remove("repeat");
            }
            c.label("Expand Repeat");
            return Ok(json!({"id": gid, "added": made.len()}));
        }
        if let Some(g) = doc.item_mut(gid) {
            g.extra.remove("repeat");
        }
        c.label("Release Repeat");
        return Ok(json!({"id": gid}));
    }
    let kind = args::opt_str(p, "kind").filter(|k| ["radial", "grid", "mirror"].contains(k)).ok_or_else(|| EngineError::new("kind is radial, grid or mirror (or release: true, expand: true)"))?;
    let mut spec = p.as_object().cloned().unwrap_or_default();
    for k in ["ids", "release", "expand"] {
        spec.remove(k);
    }
    spec.insert("kind".into(), json!(kind));
    let gid = if single_group {
        ids[0]
    } else if ids.len() == 1 {
        let dummy = ops::duplicate(doc, ids[0], Vec2::ZERO, None).ok_or_else(|| EngineError::new("cannot group the object"))?;
        let g = ops::group(doc, &[ids[0], dummy])?;
        doc.remove_item(dummy);
        g
    } else {
        ops::group(doc, &super::art::paint_order(doc, &ids))?
    };
    if let Some(g) = doc.item_mut(gid) {
        g.extra.insert("repeat".into(), Value::Object(spec));
    }
    c.set_selection(Selection::items(vec![gid]));
    c.label("Repeat");
    Ok(json!({"id": gid}))
}

fn hsl_adjust(rgb: [f64; 3], hue: f64, sat: f64, light: f64) -> [f64; 3] {
    let (r, g, b) = (rgb[0], rgb[1], rgb[2]);
    let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
    let l = (mx + mn) / 2.0;
    let d = mx - mn;
    let mut s = if d < 1e-9 { 0.0 } else { d / (1.0 - (2.0 * l - 1.0).abs()).max(1e-9) };
    let mut h = if d < 1e-9 {
        0.0
    } else if mx == r {
        60.0 * (((g - b) / d).rem_euclid(6.0))
    } else if mx == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    h = (h + hue).rem_euclid(360.0);
    s = (s * (1.0 + sat / 100.0)).clamp(0.0, 1.0);
    let l = if light >= 0.0 { l + (1.0 - l) * light / 100.0 } else { l * (1.0 + light / 100.0) }.clamp(0.0, 1.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = l - c / 2.0;
    let (r1, g1, b1) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [r1 + m, g1 + m, b1 + m]
}

fn recolor(c: &mut Ctx, p: &Value) -> Result<Value> {
    use formdesign_model::color::{Color, Paint};
    let ids = items(c, p)?;
    let (hue, sat, light) = (args::opt_f64(p, "hue").unwrap_or(0.0), args::opt_f64(p, "saturation").unwrap_or(0.0), args::opt_f64(p, "brightness").unwrap_or(0.0));
    let doc = c.doc()?;
    let mut map: Vec<([f64; 3], Paint)> = Vec::new();
    if let Some(list) = p.get("map").and_then(Value::as_array) {
        for pair in list {
            let pair = pair.as_array().filter(|a| a.len() == 2).ok_or_else(|| EngineError::new("each map entry is [from, to] (swatch names or #rrggbb)"))?;
            let (Some(from), Some(to)) = (pair[0].as_str(), pair[1].as_str()) else { return Err(EngineError::new("each map entry is [from, to]")) };
            let fc = doc.paint_color(&wire::paint_in(from, None)).ok_or_else(|| EngineError::new(format!("no colour for {from}")))?;
            let tp = wire::paint_in(to, None);
            if doc.paint_color(&tp).is_none() {
                return Err(EngineError::new(format!("no colour for {to}")));
            }
            map.push((fc.to_rgb(), tp));
        }
    }
    if map.is_empty() && hue == 0.0 && sat == 0.0 && light == 0.0 {
        return Err(EngineError::new("give map: [[from, to], ...] or a hue, saturation or brightness change"));
    }
    let tol = args::opt_f64(p, "tolerance").unwrap_or(2.0) / 100.0;
    let all = doc.walk(&ids);
    let close = |a: [f64; 3], b: [f64; 3]| (0..3).all(|i| (a[i] - b[i]).abs() <= tol);
    let swap = |doc: &formdesign_model::doc::Doc, paint: &Paint| -> Option<Paint> {
        let col = doc.paint_color(paint)?;
        let rgb = col.to_rgb();
        if let Some((_, to)) = map.iter().find(|(f, _)| close(*f, rgb)) {
            return Some(to.clone());
        }
        if hue != 0.0 || sat != 0.0 || light != 0.0 {
            return Some(Paint::Color { color: Color::Rgb(hsl_adjust(rgb, hue, sat, light)) });
        }
        None
    };
    let mut plan: Vec<(ItemId, Option<Paint>, Option<Paint>, Option<Value>, Option<Value>)> = Vec::new();
    for id in &all {
        let Some(it) = doc.item(*id) else { continue };
        let fill = swap(doc, &it.fill);
        let stroke = swap(doc, &it.stroke.paint);
        let app = it.extra.get("appearance").and_then(Value::as_array).map(|list| {
            Value::Array(list.iter().map(|e| {
                let mut e = e.clone();
                if let Some(pt) = e.get("paint").and_then(|v| serde_json::from_value::<Paint>(v.clone()).ok())
                    && let Some(np) = swap(doc, &pt)
                {
                    e["paint"] = serde_json::to_value(np).unwrap_or(Value::Null);
                }
                e
            }).collect())
        });
        let free = it.extra.get("freeform").and_then(Value::as_array).map(|pts| {
            Value::Array(pts.iter().map(|q| {
                let a: Vec<f64> = q.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
                if a.len() < 5 {
                    return q.clone();
                }
                let rgb = [a[2], a[3], a[4]];
                let out = match map.iter().find(|(f, _)| close(*f, rgb)) {
                    Some((_, to)) => doc.paint_color(to).map(|c| c.to_rgb()).unwrap_or(rgb),
                    None if hue != 0.0 || sat != 0.0 || light != 0.0 => hsl_adjust(rgb, hue, sat, light),
                    None => rgb,
                };
                json!([a[0], a[1], out[0], out[1], out[2]])
            }).collect())
        });
        if fill.is_some() || stroke.is_some() || app.is_some() || free.is_some() {
            plan.push((*id, fill, stroke, app, free));
        }
    }
    let doc = c.doc_mut()?;
    let mut n = 0;
    for (id, fill, stroke, app, free) in plan {
        let Some(it) = doc.item_mut(id) else { continue };
        let mut hit = false;
        if let Some(f) = fill { it.fill = f; hit = true; }
        if let Some(s) = stroke { it.stroke.paint = s; hit = true; }
        if let Some(a) = app { it.extra.insert("appearance".into(), a); }
        if let Some(f) = free { it.extra.insert("freeform".into(), f); hit = true; }
        if hit { n += 1; }
    }
    c.label("Recolor Artwork");
    Ok(json!({"objects": n}))
}

fn brush(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let kind = args::opt_str(p, "kind").filter(|k| ["calligraphic", "scatter", "art", "pattern", "none"].contains(k)).ok_or_else(|| EngineError::new("kind is calligraphic, scatter, art, pattern or none"))?;
    if kind == "scatter" || kind == "art" || kind == "pattern" {
        let name = args::str_(p, "symbol", "the symbol the brush draws (symbol.list shows them)")?;
        for n in [Some(name), args::opt_str(p, "start"), args::opt_str(p, "end"), args::opt_str(p, "corner")].into_iter().flatten() {
            if formdesign_render::scene::symbol_master(c.doc()?, n).is_none() {
                return Err(EngineError::new(format!("no symbol named {n}")));
            }
        }
    }
    if let Some(f) = args::opt_str(p, "fit")
        && !["stretch", "space"].contains(&f)
    {
        return Err(EngineError::new("fit is stretch or space"));
    }
    let mut spec = p.as_object().cloned().unwrap_or_default();
    spec.remove("ids");
    let doc = c.doc_mut()?;
    let mut n = 0;
    for id in &ids {
        let Some(it) = doc.item_mut(*id) else { continue };
        if kind == "none" {
            it.extra.remove("brush");
        } else {
            it.extra.insert("brush".into(), Value::Object(spec.clone()));
            if it.stroke.weight <= 0.0 || matches!(it.stroke.paint, formdesign_model::color::Paint::None) {
                it.stroke.weight = 1.0;
                it.stroke.paint = ops::default_stroke(1.0).paint;
            }
        }
        n += 1;
    }
    c.label("Brush");
    Ok(json!({"items": n}))
}

/// Moving-least-squares rigid deformation (Schaefer, McPhail and Warren 2006) of `v` by pins moved from p to q:
/// the best rotation and translation for the pins weighted by 1 / distance squared, in complex form.
fn puppet_point(pins: &[(kurbo::Point, kurbo::Point)], v: kurbo::Point) -> kurbo::Point {
    let mut ws = Vec::with_capacity(pins.len());
    for (p, _) in pins {
        let d2 = p.distance_squared(v);
        if d2 < 1e-9 {
            return pins.iter().find(|(a, _)| a.distance_squared(v) < 1e-9).map(|x| x.1).unwrap_or(v);
        }
        ws.push(1.0 / d2);
    }
    let wsum: f64 = ws.iter().sum();
    let ps = pins.iter().zip(&ws).fold(Vec2::ZERO, |m, ((p, _), w)| m + p.to_vec2() * *w) / wsum;
    let qs = pins.iter().zip(&ws).fold(Vec2::ZERO, |m, ((_, q), w)| m + q.to_vec2() * *w) / wsum;
    let (mut re, mut im) = (0.0, 0.0);
    for ((p, q), w) in pins.iter().zip(&ws) {
        let (ph, qh) = (p.to_vec2() - ps, q.to_vec2() - qs);
        re += w * (ph.x * qh.x + ph.y * qh.y);
        im += w * (ph.x * qh.y - ph.y * qh.x);
    }
    let mag = (re * re + im * im).sqrt();
    let (a, b) = if mag < 1e-12 { (1.0, 0.0) } else { (re / mag, im / mag) };
    let d = v.to_vec2() - ps;
    (Vec2::new(a * d.x - b * d.y, a * d.y + b * d.x) + qs).to_point()
}

fn envelope(c: &mut Ctx, p: &Value) -> Result<Value> {
    use formdesign_model::effects;
    let ids = items(c, p)?;
    let release = args::opt_bool(p, "release").unwrap_or(false);
    let expand = args::opt_bool(p, "expand").unwrap_or(false);
    let kind = args::opt_str(p, "kind");
    let moves: Vec<(usize, usize, f64, f64)> = p.get("moves").and_then(Value::as_array).map(|a| a.iter().filter_map(|m| {
        let m = m.as_array()?;
        Some((m.first()?.as_u64()? as usize, m.get(1)?.as_u64()? as usize, m.get(2)?.as_f64()?, m.get(3)?.as_f64()?))
    }).collect()).unwrap_or_default();
    let doc = c.doc()?;
    let leaves_of = |doc: &formdesign_model::doc::Doc, ids: &[ItemId]| -> Vec<ItemId> {
        doc.walk(ids).into_iter().filter(|i| doc.item(*i).is_some_and(|it| !matches!(it.kind, ItemKind::Group { .. } | ItemKind::States { .. }))).collect()
    };
    if release || expand || (kind.is_none() && !moves.is_empty()) {
        let leaves: Vec<ItemId> = leaves_of(doc, &ids).into_iter().filter(|i| doc.item(*i).is_some_and(|it| it.extra.contains_key("envelope"))).collect();
        if leaves.is_empty() {
            return Err(EngineError::new("the selection has no envelope"));
        }
        let doc = c.doc_mut()?;
        for id in &leaves {
            let world = doc.world_xf(*id);
            let Some(it) = doc.item_mut(*id) else { continue };
            if expand {
                if let Some((frame, cols, rows, grid)) = effects::envelope_of(&it.extra) {
                    let base = effects::apply(&it.extra, it.shape.to_path(&it.corner));
                    let closed = it.shape.closed();
                    it.shape = Shape::Path { path: world.inverse() * effects::envelope(&(world * base), frame, cols, rows, &grid), closed };
                    it.corner = Default::default();
                    it.extra.remove("effects");
                }
                it.extra.remove("envelope");
            } else if release {
                it.extra.remove("envelope");
            } else if let Some(env) = it.extra.get_mut("envelope") {
                let cols = env.get("cols").and_then(Value::as_u64).unwrap_or(0) as usize;
                if let Some(grid) = env.get_mut("grid").and_then(Value::as_array_mut) {
                    for (i, j, dx, dy) in &moves {
                        if let Some(q) = grid.get_mut(j * (cols + 1) + i).and_then(Value::as_array_mut) && q.len() == 2 {
                            q[0] = json!(q[0].as_f64().unwrap_or(0.0) + dx);
                            q[1] = json!(q[1].as_f64().unwrap_or(0.0) + dy);
                        }
                    }
                }
            }
        }
        c.label(if expand { "Expand Envelope" } else if release { "Release Envelope" } else { "Edit Envelope" });
        return Ok(json!({"items": leaves.len()}));
    }
    let kind = kind.ok_or_else(|| EngineError::new("kind is topObject or mesh (or release, expand, or moves on an existing envelope)"))?;
    let (targets, frame, cols, rows, grid, consumed) = match kind {
        "topObject" => {
            let order = super::art::paint_order(doc, &ids);
            if order.len() < 2 {
                return Err(EngineError::new("Make with Top Object needs the art and a closed shape on top of it"));
            }
            let top = order[order.len() - 1];
            let ti = doc.item(top).ok_or_else(|| EngineError::new("no such object"))?;
            if !ti.shape.closed() || matches!(ti.kind, ItemKind::Group { .. } | ItemKind::States { .. }) {
                return Err(EngineError::new("the topmost object must be a closed shape (it becomes the envelope)"));
            }
            let outline = doc.world_xf(top) * effects::apply(&ti.extra, ti.shape.to_path(&ti.corner));
            let rest: Vec<ItemId> = order[..order.len() - 1].to_vec();
            let frame = ops::union_bounds(doc, &rest).ok_or_else(|| EngineError::new("nothing to put in the envelope"))?;
            let n = args::opt_u64(p, "cols").unwrap_or(16).clamp(2, 64) as usize;
            let grid = effects::coons_grid(&outline, n, n).ok_or_else(|| EngineError::new("the top shape's outline could not be split into four sides"))?;
            (rest, frame, n, n, grid, Some(top))
        }
        "mesh" => {
            let frame = ops::union_bounds(doc, &ids).ok_or_else(|| EngineError::new("nothing to put in the envelope"))?;
            let cols = args::opt_u64(p, "cols").unwrap_or(4).clamp(1, 64) as usize;
            let rows = args::opt_u64(p, "rows").unwrap_or(4).clamp(1, 64) as usize;
            let mut grid = Vec::new();
            for j in 0..=rows {
                for i in 0..=cols {
                    grid.push(kurbo::Point::new(frame.x0 + frame.width() * i as f64 / cols as f64, frame.y0 + frame.height() * j as f64 / rows as f64));
                }
            }
            for (i, j, dx, dy) in &moves {
                if let Some(q) = grid.get_mut(j * (cols + 1) + i) {
                    *q += Vec2::new(*dx, *dy);
                }
            }
            (ids.clone(), frame, cols, rows, grid, None)
        }
        "puppet" => {
            let pins: Vec<(kurbo::Point, kurbo::Point)> = p.get("pins").and_then(Value::as_array).map(|a| a.iter().filter_map(|m| {
                let m = m.as_array()?;
                let (x, y) = (m.first()?.as_f64()?, m.get(1)?.as_f64()?);
                let (dx, dy) = (m.get(2).and_then(Value::as_f64).unwrap_or(0.0), m.get(3).and_then(Value::as_f64).unwrap_or(0.0));
                Some((kurbo::Point::new(x, y), kurbo::Point::new(x + dx, y + dy)))
            }).collect()).unwrap_or_default();
            if pins.is_empty() {
                return Err(EngineError::new("pins is a list of [x, y, dx, dy] in spread points: where each pin sits and how far it moves (0, 0 holds it)"));
            }
            let frame = ops::union_bounds(doc, &ids).ok_or_else(|| EngineError::new("nothing to warp"))?;
            let n = args::opt_u64(p, "cols").unwrap_or(16).clamp(2, 64) as usize;
            let mut grid = Vec::new();
            for j in 0..=n {
                for i in 0..=n {
                    let v = kurbo::Point::new(frame.x0 + frame.width() * i as f64 / n as f64, frame.y0 + frame.height() * j as f64 / n as f64);
                    grid.push(puppet_point(&pins, v));
                }
            }
            (ids.clone(), frame, n, n, grid, None)
        }
        _ => return Err(EngineError::new("kind is topObject, mesh or puppet")),
    };
    let leaves = leaves_of(doc, &targets);
    let env = json!({"frame": [frame.x0, frame.y0, frame.x1, frame.y1], "cols": cols, "rows": rows, "grid": grid.iter().map(|q| json!([q.x, q.y])).collect::<Vec<_>>()});
    let doc = c.doc_mut()?;
    for id in &leaves {
        if let Some(it) = doc.item_mut(*id) {
            it.extra.insert("envelope".into(), env.clone());
        }
    }
    if let Some(t) = consumed {
        doc.remove_item(t);
    }
    c.set_selection(Selection::items(targets.clone()));
    c.label("Envelope");
    Ok(json!({"items": leaves.len()}))
}

pub fn register(r: &mut Registry) {
    cmd("object.envelope", "Envelope Distort").menu(&["Object", "Envelope Distort", "Make..."]).needs(Needs::Items)
        .sig("{kind: topObject {cols?: grid detail (16)} | mesh {cols?, rows? (4)} | puppet {pins: [[x, y, dx, dy], ...] (spread points; 0, 0 holds a pin), cols?}, moves?: [[col, row, dx, dy], ...] (move mesh points; on an existing envelope edits it), release?: bool, expand?: bool, ids?}")
        .help("Bends the selected art to a shape: with topObject the topmost closed shape becomes the envelope and the art beneath fills it; with mesh the art is laid on a grid whose points can be moved; with puppet the art bends around pins, moved pins dragging it and still pins holding it; release removes the distortion and expand bakes it into the paths.").reg(r, envelope);
    cmd("object.brush", "Apply Brush").menu(&["Object", "Brush..."]).needs(Needs::Items)
        .sig("{kind: calligraphic {size?: pt (default the stroke weight), angle?: deg (45), roundness?: percent (30)} | scatter {symbol, spacing?: pt (default the symbol's size), scale?: percent, rotate?: bool (follow the path, default true)} | art {symbol, width?: percent, flip?: bool} | pattern {symbol (the side tile, repeated along the path), start?: symbol, end?: symbol, corner?: symbol (set at each sharp anchor; the side tiles restart after every corner either way), scale?: percent of the stroke weight (100), spacing?: percent of the tile's length between tiles (0), fit?: stretch|space, flip?: bool} | none, ids?}")
        .help("Draws the stroke with a brush: calligraphic strokes vary in width with the direction of the path like an angled pen nib; scatter brushes place copies of a symbol along the path; art brushes stretch a symbol's art along the whole path; none restores the plain stroke. The stroke's colour paints calligraphic strokes.").reg(r, brush);
    cmd("object.recolor", "Recolor Artwork").menu(&["Edit", "Edit Colors", "Recolor Artwork..."]).needs(Needs::Items)
        .sig("{map?: [[from, to], ...] (swatch names or #rrggbb), tolerance?: percent (default 2), hue?: degrees, saturation?: percent, brightness?: percent, ids?}")
        .help("Changes the colours of the selected art at once, inside groups too: map swaps each listed colour for another (a swatch stays linked), and hue, saturation and brightness shift every other colour; fills, strokes, extra fills and strokes and freeform gradient points all follow.").reg(r, recolor);
    cmd("object.repeat", "Repeat").menu(&["Object", "Repeat", "Make..."]).needs(Needs::Items)
        .sig("{kind: radial {count, radius} | grid {cols, rows, gapX, gapY, flip: none|rows|cols|both} | mirror {angle (axis, default 90 = vertical), gap}, release?: bool, expand?: bool, ids?}")
        .help("Repeats the selected art live as a radial ring, a grid or a mirrored pair; editing the original updates every copy; release removes the repeat, expand turns the copies into real objects.").reg(r, repeat);
    cmd("object.freeformGradient", "Freeform Gradient").menu(&["Object", "Freeform Gradient..."]).needs(Needs::Items).sig("{points: [[x 0..1, y 0..1, colour (swatch or #rrggbb)], ...] across the object's bounds, spread?: 0.5..8 (how far each colour reaches; default 2), clear?: bool, ids?}")
        .help("Fills the shape with colours placed at points that blend smoothly into each other; a lower spread mixes the colours more evenly, a higher one keeps each colour around its point; clear goes back to the ordinary fill.").reg(r, freeform_gradient);
    cmd("object.mesh", "Gradient Mesh").menu(&["Object", "Create Gradient Mesh..."]).needs(Needs::Items).sig("{cols?: 1..12, rows?: 1..12 (makes a new mesh; default 3 by 3 in the fill colour), highlight?: percent toward white at the inner points, point?: [col, row] (0-based on the grid of mesh points), color?: swatch or #rrggbb (the point's colour, or every point's on a new mesh), to?: [x 0..1, y 0..1] (moves the point, as a share of the bounds), clear?: bool, ids?}").reg(r, mesh);
    cmd("object.opacityMask", "Make Opacity Mask").menu(&["Object", "Opacity Mask", "Make Opacity Mask"]).needs(Needs::Items).sig("{clip?: bool (default true: outside the mask is hidden), release?: bool, ids?}")
        .help("Groups the selected objects and uses the topmost one's brightness as their opacity (white shows, black hides, greys fade); clip false keeps what lies outside the mask visible; release turns the mask back into a visible object in the group.").reg(r, opacity_mask);
    cmd("object.clippingMask", "Make Clipping Mask").menu(&["Object", "Clipping Mask", "Make"]).needs(Needs::Items).sig("{release?: bool, ids?}")
        .help("Groups the selected objects and clips them to the topmost object's shape, which stops drawing its own fill and stroke; release makes the mask an ordinary object again.").reg(r, clipping_mask);
    cmd("object.isolateBlending", "Isolate Blending").menu(&["Object", "Isolate Blending"]).needs(Needs::Items).sig("{on?: bool (default true), ids?}")
        .help("Keeps the blend modes inside a group from mixing with what lies behind it.").reg(r, isolate_blending);
    cmd("object.appearance", "Add Fill or Stroke").menu(&["Object", "Appearance", "Add Fill or Stroke..."]).needs(Needs::Items)
        .sig("{kind: fill|stroke, swatch, tint?, opacity?: percent, weight?, cap?, join?, dash?: [pt...], effects?: [{kind, ...}], above?: bool (over the main fill and stroke), evenOdd?, on?, index? (replace; index with on and no kind only shows or hides that entry), remove?: index, clear?: bool, ids?}")
        .help("Adds another fill or stroke to the object's appearance, each with its own colour, opacity, weight and effects, drawn under the main fill or over the main stroke; index replaces an entry, remove takes one out, clear empties the list.").reg(r, appearance);
    cmd("object.effect", "Add Effect").menu(&["Effect", "Distort & Transform", "Add Effect..."]).needs(Needs::Items)
        .sig("{kind: roundCorners {radius} | zigZag {size, ridges, smooth} | roughen {size, detail (per inch), smooth, seed} | puckerBloat {amount: -200..200} | twist {angle} | transform {scaleX, scaleY, moveX, moveY, rotate, copies} | warp {style: arc|arcLower|arcUpper|arch|bulge|shellLower|shellUpper|flag|wave|rise|fisheye|inflate|squeeze, bend: -100..100, horizontal?: bool}, on?: bool, index? (replace; index with on and no kind only shows or hides that effect), remove?: index, clear?: bool, ids?}")
        .help("Adds a live effect to the object's appearance stack, applied in order when it is drawn and kept editable: Round Corners, Zig Zag, Roughen, Pucker & Bloat, Twist, Transform (with copies) and the Warp styles; index replaces an entry, remove takes one out, clear empties the stack.").reg(r, effect);
    cmd("object.expandAppearance", "Expand Appearance").menu(&["Object", "Expand Appearance"]).needs(Needs::Items).sig("{ids?}")
        .help("Bakes the live effects into the object's path so it can be edited point by point, and turns extra fills and strokes into separate objects grouped with it.").reg(r, expand_appearance);
    cmd("object.widthProfile", "Width Profile").menu(&["Object", "Paths", "Width Profile..."]).needs(Needs::Items).sig("{profile?: uniform|lens|taperStart|taperEnd|bulge|pinch|wave, points?: [[at 0..1, width] or [at, left, right], ...] (multiples of the stroke weight), flip?: bool, clear?: bool, ids?}")
        .help("Gives the stroke a variable width along the path, from a named profile or width points (position along the path and the width on each side as multiples of the stroke weight); uniform or clear restores an even stroke.").reg(r, width_profile);
    cmd("object.blend", "Make Blend").menu(&["Object", "Blend", "Make..."]).needs(Needs::Items).sig("{steps?: (default 5), distance?: pt between steps (sets the step count from the distance), spine?: id of a third path the blend follows (the two ends move to its ends), noMorph?: bool, live?: bool (groups the ends, steps and spine and rebuilds the steps whenever an end, the spine or the settings change), ids?: two objects}")
        .help("Makes evenly spaced steps between two objects, interpolating fill and stroke colours, stroke weight and opacity; shapes with the same number of anchor points morph into each other, others move and scale.").reg(r, blend);
    cmd("object.drawMode", "Drawing Mode").key("Shift+D").sig("{mode?: normal|behind|inside (omit to cycle normal, behind, inside)}").query().reg(r, draw_mode);
    cmd("object.isolate", "Isolation Mode").sig("{exit?: bool, ids?: a group, or an object inside one}").query().reg(r, isolate);
    cmd("object.blend.expand", "Expand Blend").menu(&["Object", "Blend", "Expand"]).needs(Needs::Items).sig("{ids?: live blends}").reg(r, blend_expand);
    cmd("object.blend.release", "Release Blend").menu(&["Object", "Blend", "Release"]).needs(Needs::Items).sig("{ids?: live blends}").reg(r, blend_release);
    cmd("shape.spiral", "Spiral").menu(&["Object", "Shapes", "Spiral..."]).sig("{center: [x, y], radius?: pt, decay?: percent (default 80), segments?: quarter turns (default 10), clockwise?, weight?, spread?}")
        .help("Draws an open spiral from quarter-circle segments, each one the decay percentage of the one before.").reg(r, spiral);
    cmd("shape.arc", "Arc").menu(&["Object", "Shapes", "Arc..."]).sig("{a: [x, y], b: [x, y], slope?: -100..100 (concave to convex), closed?, weight?, spread?}")
        .help("Draws an arc from a to b, bowed by the slope; closed adds the two straight sides back to the corner.").reg(r, arc);
    cmd("shape.grid", "Rectangular Grid").menu(&["Object", "Shapes", "Rectangular Grid..."]).sig("{rect: [x0, y0, x1, y1], rows?: dividers (default 5), columns?: dividers (default 5), weight?, spread?}")
        .help("Draws a grid of evenly spaced horizontal and vertical lines with its outer frame, as one path.").reg(r, grid);
    cmd("shape.polarGrid", "Polar Grid").menu(&["Object", "Shapes", "Polar Grid..."]).sig("{rect: [x0, y0, x1, y1], rings?: concentric dividers (default 5), dividers?: radial (default 5), weight?, spread?}")
        .help("Draws concentric ellipses and radial lines filling the rectangle, as one path.").reg(r, polar_grid);
    cmd("transform.reflect", "Reflect").menu(&["Object", "Transform", "Reflect..."]).needs(Needs::Items).sig("{axis?: horizontal|vertical|angle, angle?: deg, copy?, ids?}")
        .help("Mirrors the selection across a horizontal, vertical or angled axis through its centre, optionally as a copy.").reg(r, reflect);
    cmd("transform.shear", "Shear").menu(&["Object", "Transform", "Shear..."]).needs(Needs::Items).sig("{angle: deg, axis?: horizontal|vertical, copy?, ids?}")
        .help("Slants the selection by the angle along the horizontal or vertical axis about its centre, optionally as a copy.").reg(r, shear);
    cmd("transform.each", "Transform Each").menu(&["Object", "Transform", "Transform Each..."]).needs(Needs::Items)
        .sig("{scaleX?: percent, scaleY?: percent, angle?: deg, dx?: pt, dy?: pt, reflectX?, reflectY?, copy?, ids?}")
        .help("Scales, rotates, reflects and moves every selected object about its own centre rather than the selection's.").reg(r, each);
}
