use formdes_model::geom::{Affine, Point, Rect, Vec2};
use formdes_model::ids::ItemId;
use formdes_model::item::{Item, ItemKind, Shape};
use kurbo::BezPath;
use serde_json::{Value, json};

use crate::registry::{Needs, Registry, cmd};
use crate::session::{Ctx, Selection};
use crate::target::items;
use crate::{EngineError, Result, args, ops};

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

pub fn register(r: &mut Registry) {
    cmd("shape.spiral", "Spiral").menu(&["Object", "Shapes", "Spiral..."]).sig("{center: [x, y], radius?: pt, decay?: percent (default 80), segments?: quarter turns (default 10), clockwise?, weight?, spread?}")
        .help("Draws an open spiral from quarter-circle segments, each one the decay percentage of the one before, as Illustrator's Spiral tool does.").reg(r, spiral);
    cmd("shape.arc", "Arc").menu(&["Object", "Shapes", "Arc..."]).sig("{a: [x, y], b: [x, y], slope?: -100..100 (concave to convex), closed?, weight?, spread?}")
        .help("Draws an arc from a to b, bowed by the slope; closed adds the two straight sides back to the corner, like Illustrator's Arc tool.").reg(r, arc);
    cmd("shape.grid", "Rectangular Grid").menu(&["Object", "Shapes", "Rectangular Grid..."]).sig("{rect: [x0, y0, x1, y1], rows?: dividers (default 5), columns?: dividers (default 5), weight?, spread?}")
        .help("Draws a grid of evenly spaced horizontal and vertical lines with its outer frame, as one path.").reg(r, grid);
    cmd("shape.polarGrid", "Polar Grid").menu(&["Object", "Shapes", "Polar Grid..."]).sig("{rect: [x0, y0, x1, y1], rings?: concentric dividers (default 5), dividers?: radial (default 5), weight?, spread?}")
        .help("Draws concentric ellipses and radial lines filling the rectangle, as one path, like Illustrator's Polar Grid tool.").reg(r, polar_grid);
    cmd("transform.reflect", "Reflect").menu(&["Object", "Transform", "Reflect..."]).needs(Needs::Items).sig("{axis?: horizontal|vertical|angle, angle?: deg, copy?, ids?}")
        .help("Mirrors the selection across a horizontal, vertical or angled axis through its centre, optionally as a copy.").reg(r, reflect);
    cmd("transform.shear", "Shear").menu(&["Object", "Transform", "Shear..."]).needs(Needs::Items).sig("{angle: deg, axis?: horizontal|vertical, copy?, ids?}")
        .help("Slants the selection by the angle along the horizontal or vertical axis about its centre, optionally as a copy.").reg(r, shear);
    cmd("transform.each", "Transform Each").menu(&["Object", "Transform", "Transform Each..."]).needs(Needs::Items)
        .sig("{scaleX?: percent, scaleY?: percent, angle?: deg, dx?: pt, dy?: pt, reflectX?, reflectY?, copy?, ids?}")
        .help("Scales, rotates, reflects and moves every selected object about its own centre rather than the selection's, as Illustrator's Transform Each does.").reg(r, each);
}
