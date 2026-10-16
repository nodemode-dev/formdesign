use kurbo::{Affine, BezPath, CubicBez, ParamCurve, ParamCurveDeriv, PathEl, PathSeg, Point, Shape, Vec2};
use serde_json::Value;

use crate::attrs::Attrs;

pub const KINDS: [&str; 7] = ["roundCorners", "zigZag", "roughen", "puckerBloat", "twist", "transform", "warp"];

pub const WARPS: [&str; 13] = ["arc", "arcLower", "arcUpper", "arch", "bulge", "shellLower", "shellUpper", "flag", "wave", "rise", "fisheye", "inflate", "squeeze"];

fn num(e: &Value, k: &str, d: f64) -> f64 {
    e.get(k).and_then(Value::as_f64).unwrap_or(d)
}

fn flag(e: &Value, k: &str) -> bool {
    e.get(k).and_then(Value::as_bool).unwrap_or(false)
}

pub fn stack(extra: &Attrs) -> Vec<Value> {
    extra.get("effects").and_then(Value::as_array).map(|a| a.iter().filter(|e| e.get("on").and_then(Value::as_bool) != Some(false)).cloned().collect()).unwrap_or_default()
}

pub fn apply(extra: &Attrs, path: BezPath) -> BezPath {
    let fx = stack(extra);
    if fx.is_empty() {
        return path;
    }
    fx.iter().fold(path, |p, e| one(e, &p))
}

pub fn one(e: &Value, path: &BezPath) -> BezPath {
    match e.get("kind").and_then(Value::as_str).unwrap_or("") {
        "roundCorners" => {
            let r = num(e, "radius", 10.0).max(0.0);
            crate::corners::round_vertices(path, &|_| Some((r, "round".to_string())))
        }
        "zigZag" => zig_zag(path, num(e, "size", 5.0), num(e, "ridges", 4.0).round().clamp(0.0, 100.0) as usize, flag(e, "smooth")),
        "roughen" => roughen(path, num(e, "size", 5.0), num(e, "detail", 10.0).clamp(0.0, 100.0), flag(e, "smooth"), num(e, "seed", 1.0) as u64),
        "puckerBloat" => pucker_bloat(path, num(e, "amount", 30.0).clamp(-200.0, 200.0) / 100.0),
        "twist" => twist(path, num(e, "angle", 45.0)),
        "transform" => transform(path, e),
        "warp" => warp(path, e.get("style").and_then(Value::as_str).unwrap_or("arc"), num(e, "bend", 50.0).clamp(-100.0, 100.0) / 100.0, e.get("horizontal").and_then(Value::as_bool).unwrap_or(true)),
        _ => path.clone(),
    }
}

fn dense(segs: &[PathSeg], closed: bool) -> Vec<Point> {
    let mut pts = Vec::new();
    for seg in segs {
        let len = kurbo::ParamCurveArclen::arclen(seg, 0.1);
        let n = match seg {
            PathSeg::Line(_) => ((len / 2.0).ceil() as usize).clamp(1, 400),
            _ => ((len / 2.0).ceil() as usize).clamp(2, 400),
        };
        for j in 0..n {
            pts.push(seg.eval(j as f64 / n as f64));
        }
    }
    if !closed {
        pts.push(segs[segs.len() - 1].end());
    }
    pts
}

fn warp(path: &BezPath, style: &str, b: f64, horizontal: bool) -> BezPath {
    let bb0 = path.bounding_box();
    if b.abs() < 1e-6 || bb0.width() <= 0.0 || bb0.height() <= 0.0 {
        return path.clone();
    }
    let flip = |p: Point| if horizontal { p } else { Point::new(p.y, p.x) };
    let bb = if horizontal { bb0 } else { kurbo::Rect::new(bb0.y0, bb0.x0, bb0.y1, bb0.x1) };
    let (cx, cy, w, h) = (bb.center().x, bb.center().y, bb.width(), bb.height());
    let map = |p: Point| -> Point {
        let (x, y) = (p.x, p.y);
        let u = (x - cx) / (w / 2.0);
        let v = (y - cy) / (h / 2.0);
        let hump = 1.0 - u * u;
        match style {
            "arc" => {
                let a = b * std::f64::consts::PI;
                let r0 = w / a;
                let phi = (x - cx) / r0;
                let rho = r0 - (y - cy);
                Point::new(cx + rho * phi.sin(), cy + r0 - rho * phi.cos())
            }
            "arch" => Point::new(x, y - b * h * 0.5 * hump),
            "arcLower" => Point::new(x, y - b * h * 0.5 * hump * (v + 1.0) / 2.0),
            "arcUpper" => Point::new(x, y - b * h * 0.5 * hump * (1.0 - v) / 2.0),
            "bulge" => Point::new(x, cy + (y - cy) * (1.0 + b * hump)),
            "shellLower" => Point::new(x, if v > 0.0 { cy + (y - cy) * (1.0 + b * hump) } else { y }),
            "shellUpper" => Point::new(x, if v < 0.0 { cy + (y - cy) * (1.0 + b * hump) } else { y }),
            "flag" => Point::new(x, y - b * h * 0.5 * (u * std::f64::consts::PI).sin()),
            "wave" => Point::new(x, y - b * h * 0.5 * (u * std::f64::consts::PI).sin() * (1.0 - 0.5 * v.abs())),
            "rise" => Point::new(x, y - b * h * 0.5 * u),
            "fisheye" => {
                let r2 = (u * u + v * v) / 2.0;
                let k = 1.0 + b * (1.0 - r2).max(0.0);
                Point::new(cx + (x - cx) * k, cy + (y - cy) * k)
            }
            "inflate" => Point::new(cx + (x - cx) * (1.0 + b * 0.5 * (1.0 - v * v)), cy + (y - cy) * (1.0 + b * 0.5 * hump)),
            "squeeze" => Point::new(cx + (x - cx) * (1.0 - b * 0.5 * (1.0 - v * v)), y),
            _ => p,
        }
    };
    let mut out = BezPath::new();
    for (segs, closed) in subpaths(path) {
        let pts: Vec<Point> = dense(&segs, closed).into_iter().map(|p| flip(map(flip(p)))).collect();
        emit(&mut out, &pts, closed, false);
    }
    out
}

fn subpaths(path: &BezPath) -> Vec<(Vec<PathSeg>, bool)> {
    let mut out: Vec<(Vec<PathSeg>, bool)> = Vec::new();
    let mut start = Point::ZERO;
    let mut last = Point::ZERO;
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                out.push((Vec::new(), false));
                start = p;
                last = p;
            }
            PathEl::LineTo(p) => {
                if let Some(s) = out.last_mut() { s.0.push(PathSeg::Line(kurbo::Line::new(last, p))); }
                last = p;
            }
            PathEl::QuadTo(a, p) => {
                if let Some(s) = out.last_mut() { s.0.push(PathSeg::Quad(kurbo::QuadBez::new(last, a, p))); }
                last = p;
            }
            PathEl::CurveTo(a, b, p) => {
                if let Some(s) = out.last_mut() { s.0.push(PathSeg::Cubic(CubicBez::new(last, a, b, p))); }
                last = p;
            }
            PathEl::ClosePath => {
                if let Some(s) = out.last_mut() {
                    if last.distance(start) > 1e-6 {
                        s.0.push(PathSeg::Line(kurbo::Line::new(last, start)));
                    }
                    s.1 = true;
                }
                last = start;
            }
        }
    }
    for s in out.iter_mut() {
        if !s.1 && s.0.len() > 1 && s.0[0].start().distance(s.0[s.0.len() - 1].end()) < 1e-6 {
            s.1 = true;
        }
    }
    out.retain(|s| !s.0.is_empty());
    out
}

fn normal(seg: &PathSeg, t: f64) -> Vec2 {
    let d = match seg {
        PathSeg::Line(l) => l.p1 - l.p0,
        PathSeg::Quad(q) => q.deriv().eval(t).to_vec2(),
        PathSeg::Cubic(c) => c.deriv().eval(t).to_vec2(),
    };
    let h = d.hypot();
    if h < 1e-12 { Vec2::ZERO } else { Vec2::new(-d.y / h, d.x / h) }
}

fn emit(out: &mut BezPath, pts: &[Point], closed: bool, smooth: bool) {
    if pts.len() < 2 {
        return;
    }
    out.move_to(pts[0]);
    let n = pts.len();
    if !smooth {
        for p in &pts[1..] {
            out.line_to(*p);
        }
    } else {
        let at = |i: isize| -> Point {
            if closed { pts[i.rem_euclid(n as isize) as usize] } else { pts[i.clamp(0, n as isize - 1) as usize] }
        };
        let segs = if closed { n } else { n - 1 };
        for i in 0..segs as isize {
            let (p0, p1, p2, p3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
            out.curve_to(p1 + (p2 - p0) / 6.0, p2 - (p3 - p1) / 6.0, p2);
        }
    }
    if closed {
        out.close_path();
    }
}

fn zig_zag(path: &BezPath, size: f64, ridges: usize, smooth: bool) -> BezPath {
    let mut out = BezPath::new();
    for (segs, closed) in subpaths(path) {
        let mut pts = Vec::new();
        let mut sign = 1.0;
        let parts = ridges + 1;
        for seg in &segs {
            for j in 0..parts {
                let t = j as f64 / parts as f64;
                pts.push(seg.eval(t) + normal(seg, t) * (size * sign));
                sign = -sign;
            }
        }
        if !closed {
            let last = segs[segs.len() - 1];
            pts.push(last.end() + normal(&last, 1.0) * (size * sign));
        } else if pts.len() % 2 == 1 {
            pts.pop();
        }
        emit(&mut out, &pts, closed, smooth);
    }
    out
}

fn hash(seed: u64, i: u64) -> f64 {
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ i.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 31;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 29;
    (x >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
}

fn roughen(path: &BezPath, size: f64, detail: f64, smooth: bool, seed: u64) -> BezPath {
    let mut out = BezPath::new();
    let mut k = 0u64;
    for (segs, closed) in subpaths(path) {
        let mut pts = Vec::new();
        for seg in &segs {
            let len = kurbo::ParamCurveArclen::arclen(seg, 0.1);
            let n = ((len / 72.0 * detail).round() as usize).max(1);
            for j in 0..n {
                let t = j as f64 / n as f64;
                pts.push(seg.eval(t) + Vec2::new(hash(seed, k), hash(seed ^ 0x5555, k)) * size);
                k += 1;
            }
        }
        if !closed {
            pts.push(segs[segs.len() - 1].end() + Vec2::new(hash(seed, k), hash(seed ^ 0x5555, k)) * size);
            k += 1;
        }
        emit(&mut out, &pts, closed, smooth);
    }
    out
}

fn as_cubic(seg: &PathSeg) -> CubicBez {
    match seg {
        PathSeg::Line(l) => CubicBez::new(l.p0, l.p0.lerp(l.p1, 1.0 / 3.0), l.p0.lerp(l.p1, 2.0 / 3.0), l.p1),
        PathSeg::Quad(q) => q.raise(),
        PathSeg::Cubic(c) => *c,
    }
}

fn pucker_bloat(path: &BezPath, a: f64) -> BezPath {
    let c = path.bounding_box().center();
    let anchor = |p: Point| p + (c - p) * (a * 0.5);
    let handle = |p: Point| p - (c - p) * (a * 0.5);
    let mut out = BezPath::new();
    for (segs, closed) in subpaths(path) {
        let first = as_cubic(&segs[0]);
        out.move_to(anchor(first.p0));
        for s in &segs {
            let cb = as_cubic(s);
            out.curve_to(handle(cb.p1), handle(cb.p2), anchor(cb.p3));
        }
        if closed {
            out.close_path();
        }
    }
    out
}

fn twist(path: &BezPath, angle: f64) -> BezPath {
    let bb = path.bounding_box();
    let c = bb.center();
    let r = (bb.width().hypot(bb.height()) / 2.0).max(1e-9);
    let mut out = BezPath::new();
    for (segs, closed) in subpaths(path) {
        let mut pts = Vec::new();
        for seg in &segs {
            let len = kurbo::ParamCurveArclen::arclen(seg, 0.1);
            let n = ((len / 3.0).ceil() as usize).clamp(1, 400);
            for j in 0..n {
                pts.push(seg.eval(j as f64 / n as f64));
            }
        }
        if !closed {
            pts.push(segs[segs.len() - 1].end());
        }
        let pts: Vec<Point> = pts.into_iter().map(|p| {
            let d = (p - c).hypot() / r;
            Affine::rotate_about((angle * (1.0 - d).max(0.0)).to_radians(), c) * p
        }).collect();
        emit(&mut out, &pts, closed, false);
    }
    out
}

fn transform(path: &BezPath, e: &Value) -> BezPath {
    let c = path.bounding_box().center();
    let step = Affine::translate(Vec2::new(num(e, "moveX", 0.0), num(e, "moveY", 0.0)))
        * Affine::rotate_about(num(e, "rotate", 0.0).to_radians(), c)
        * Affine::translate(c.to_vec2())
        * Affine::scale_non_uniform(num(e, "scaleX", 100.0) / 100.0, num(e, "scaleY", 100.0) / 100.0)
        * Affine::translate(-c.to_vec2());
    let copies = num(e, "copies", 0.0).round().clamp(0.0, 200.0) as usize;
    let mut out = BezPath::new();
    let mut xf = if copies == 0 { step } else { Affine::IDENTITY };
    for _ in 0..=copies {
        for el in (xf * path.clone()).elements() {
            out.push(*el);
        }
        xf = step * xf;
    }
    out
}

/// An envelope: `grid` holds (cols + 1) by (rows + 1) points row by row, the warped positions of an even grid
/// over `frame`; every point of the path inside the frame is moved by bilinear interpolation in its cell.
pub fn envelope(path: &BezPath, frame: kurbo::Rect, cols: usize, rows: usize, grid: &[Point]) -> BezPath {
    if cols == 0 || rows == 0 || grid.len() != (cols + 1) * (rows + 1) || frame.width() <= 0.0 || frame.height() <= 0.0 {
        return path.clone();
    }
    let g = |i: usize, j: usize| grid[j * (cols + 1) + i];
    let map = |p: Point| -> Point {
        let u = ((p.x - frame.x0) / frame.width()).clamp(0.0, 1.0) * cols as f64;
        let v = ((p.y - frame.y0) / frame.height()).clamp(0.0, 1.0) * rows as f64;
        let (i, j) = ((u.floor() as usize).min(cols - 1), (v.floor() as usize).min(rows - 1));
        let (s, t) = (u - i as f64, v - j as f64);
        let top = g(i, j).lerp(g(i + 1, j), s);
        let bot = g(i, j + 1).lerp(g(i + 1, j + 1), s);
        top.lerp(bot, t)
    };
    let mut out = BezPath::new();
    for (segs, closed) in subpaths(path) {
        let pts: Vec<Point> = dense(&segs, closed).into_iter().map(map).collect();
        emit(&mut out, &pts, closed, false);
    }
    out
}

/// A Coons patch over a closed outline: its four sides run between the outline points nearest the corners of
/// its bounds, and the patch is sampled into a (cols + 1) by (rows + 1) grid for `envelope`.
pub fn coons_grid(outline: &BezPath, cols: usize, rows: usize) -> Option<Vec<Point>> {
    let segs = subpaths(outline).into_iter().next()?.0;
    let mut pts = dense(&segs, true);
    if pts.len() < 4 {
        return None;
    }
    let bb = outline.bounding_box();
    let corners = [Point::new(bb.x0, bb.y0), Point::new(bb.x1, bb.y0), Point::new(bb.x1, bb.y1), Point::new(bb.x0, bb.y1)];
    let near = |pts: &[Point], c: Point| (0..pts.len()).min_by(|a, b| pts[*a].distance(c).total_cmp(&pts[*b].distance(c))).unwrap_or(0);
    let tl = near(&pts, corners[0]);
    pts.rotate_left(tl);
    let (tr, bl) = (near(&pts, corners[1]), near(&pts, corners[3]));
    if tr > bl {
        pts[1..].reverse();
    }
    let (tr, br, bl) = (near(&pts, corners[1]), near(&pts, corners[2]), near(&pts, corners[3]));
    if !(0 < tr && tr < br && br < bl) {
        return None;
    }
    pts.push(pts[0]);
    let side = |a: usize, b: usize| -> Vec<Point> { pts[a..=b].to_vec() };
    let top = side(0, tr);
    let right = side(tr, br);
    let mut bottom = side(br, bl);
    bottom.reverse();
    let mut left = side(bl, pts.len() - 1);
    left.reverse();
    let at = |line: &[Point], t: f64| -> Point {
        let lens: Vec<f64> = line.windows(2).map(|w| w[0].distance(w[1])).collect();
        let total: f64 = lens.iter().sum::<f64>().max(1e-9);
        let mut want = t.clamp(0.0, 1.0) * total;
        for (k, l) in lens.iter().enumerate() {
            if want <= *l || k == lens.len() - 1 {
                return line[k].lerp(line[k + 1], (want / l.max(1e-9)).clamp(0.0, 1.0));
            }
            want -= l;
        }
        line[line.len() - 1]
    };
    let (p00, p10, p11, p01) = (top[0], top[top.len() - 1], bottom[bottom.len() - 1], bottom[0]);
    let mut grid = Vec::with_capacity((cols + 1) * (rows + 1));
    for j in 0..=rows {
        let v = j as f64 / rows as f64;
        for i in 0..=cols {
            let u = i as f64 / cols as f64;
            let (t, b, l, r) = (at(&top, u), at(&bottom, u), at(&left, v), at(&right, v));
            let x = (1.0 - v) * t.x + v * b.x + (1.0 - u) * l.x + u * r.x - ((1.0 - u) * (1.0 - v) * p00.x + u * (1.0 - v) * p10.x + (1.0 - u) * v * p01.x + u * v * p11.x);
            let y = (1.0 - v) * t.y + v * b.y + (1.0 - u) * l.y + u * r.y - ((1.0 - u) * (1.0 - v) * p00.y + u * (1.0 - v) * p10.y + (1.0 - u) * v * p01.y + u * v * p11.y);
            grid.push(Point::new(x, y));
        }
    }
    Some(grid)
}

pub fn envelope_of(extra: &Attrs) -> Option<(kurbo::Rect, usize, usize, Vec<Point>)> {
    let e = extra.get("envelope")?;
    let f = e.get("frame")?.as_array()?;
    let frame = kurbo::Rect::new(f.first()?.as_f64()?, f.get(1)?.as_f64()?, f.get(2)?.as_f64()?, f.get(3)?.as_f64()?);
    let cols = e.get("cols")?.as_u64()? as usize;
    let rows = e.get("rows")?.as_u64()? as usize;
    let grid: Vec<Point> = e.get("grid")?.as_array()?.iter().filter_map(|q| Some(Point::new(q.get(0)?.as_f64()?, q.get(1)?.as_f64()?))).collect();
    Some((frame, cols, rows, grid))
}
