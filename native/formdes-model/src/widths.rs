use kurbo::{BezPath, PathEl, Point, Vec2};

use crate::attrs::Attrs;

pub type Profile = Vec<(f64, f64, f64)>;

pub const PRESETS: [&str; 7] = ["uniform", "lens", "taperStart", "taperEnd", "bulge", "pinch", "wave"];

pub fn preset(name: &str) -> Option<Profile> {
    Some(match name {
        "uniform" => vec![(0.0, 1.0, 1.0), (1.0, 1.0, 1.0)],
        "lens" => vec![(0.0, 0.0, 0.0), (0.5, 1.0, 1.0), (1.0, 0.0, 0.0)],
        "taperStart" => vec![(0.0, 0.0, 0.0), (1.0, 1.0, 1.0)],
        "taperEnd" => vec![(0.0, 1.0, 1.0), (1.0, 0.0, 0.0)],
        "bulge" => vec![(0.0, 0.5, 0.5), (0.5, 1.5, 1.5), (1.0, 0.5, 0.5)],
        "pinch" => vec![(0.0, 1.0, 1.0), (0.5, 0.2, 0.2), (1.0, 1.0, 1.0)],
        "wave" => vec![(0.0, 0.4, 0.4), (0.25, 1.0, 1.0), (0.5, 0.4, 0.4), (0.75, 1.0, 1.0), (1.0, 0.4, 0.4)],
        _ => return None,
    })
}

pub fn profile_of(extra: &Attrs) -> Option<Profile> {
    let v = extra.get("widthProfile")?;
    if let Some(name) = v.as_str() {
        return preset(name);
    }
    let mut out: Profile = v.as_array()?.iter().filter_map(|q| {
        let q = q.as_array()?;
        let t = q.first()?.as_f64()?;
        let l = q.get(1)?.as_f64()?;
        let r = q.get(2).and_then(|x| x.as_f64()).unwrap_or(l);
        Some((t.clamp(0.0, 1.0), l.max(0.0), r.max(0.0)))
    }).collect();
    if out.is_empty() {
        return None;
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    Some(out)
}

fn at(prof: &Profile, t: f64) -> (f64, f64) {
    let first = prof[0];
    if t <= first.0 {
        return (first.1, first.2);
    }
    for w in prof.windows(2) {
        let (a, b) = (w[0], w[1]);
        if t <= b.0 {
            let span = (b.0 - a.0).max(1e-9);
            let u = (t - a.0) / span;
            let s = u * u * (3.0 - 2.0 * u);
            return (a.1 + (b.1 - a.1) * s, a.2 + (b.2 - a.2) * s);
        }
    }
    let last = prof[prof.len() - 1];
    (last.1, last.2)
}

fn subpaths(path: &BezPath, tol: f64) -> Vec<(Vec<Point>, bool)> {
    let mut out: Vec<(Vec<Point>, bool)> = Vec::new();
    kurbo::flatten(path.iter(), tol, |el| match el {
        PathEl::MoveTo(p) => out.push((vec![p], false)),
        PathEl::LineTo(p) => {
            if let Some(s) = out.last_mut() {
                if s.0.last().is_none_or(|q| q.distance(p) > 1e-3) {
                    s.0.push(p);
                }
            }
        }
        PathEl::ClosePath => {
            if let Some(s) = out.last_mut() {
                s.1 = true;
                while s.0.len() > 2 && s.0[0].distance(s.0[s.0.len() - 1]) < 1e-3 {
                    s.0.pop();
                }
            }
        }
        _ => {}
    });
    for s in out.iter_mut() {
        while s.0.len() > 2 && s.0[0].distance(s.0[s.0.len() - 1]) < 1e-3 {
            s.0.pop();
            s.1 = true;
        }
    }
    out.retain(|s| s.0.len() > 1);
    out
}

fn unit(v: Vec2) -> Vec2 {
    let h = v.hypot();
    if h < 1e-12 { Vec2::ZERO } else { v / h }
}

/// The filled outline of a stroke of `weight` whose width follows `prof` along each subpath's length.
/// Each profile entry is (position 0..1, left, right), the sides as multiples of the weight, left being the
/// side to the left of the path's direction. Closed subpaths give an outer and an inner ring (fill even-odd).
pub fn outline(path: &BezPath, prof: &Profile, weight: f64, closed_shape: bool) -> BezPath {
    let mut out = BezPath::new();
    for (pts, closed) in subpaths(path, (weight * 0.01).clamp(0.01, 0.25)) {
        let closed = closed || (closed_shape && pts.len() > 2);
        let n = pts.len();
        let mut acc = vec![0.0; n];
        for i in 1..n {
            acc[i] = acc[i - 1] + pts[i].distance(pts[i - 1]);
        }
        let total = if closed { acc[n - 1] + pts[n - 1].distance(pts[0]) } else { acc[n - 1] };
        if total < 1e-9 {
            continue;
        }
        let seg = |i: usize| -> Vec2 { unit(pts[(i + 1) % n] - pts[i]) };
        let mut left = Vec::with_capacity(n);
        let mut right = Vec::with_capacity(n);
        for i in 0..n {
            let (tin, tout) = match (i, closed) {
                (0, false) => (seg(0), seg(0)),
                (i, false) if i == n - 1 => (seg(n - 2), seg(n - 2)),
                (0, true) => (seg(n - 1), seg(0)),
                (i, _) => (seg(i - 1), seg(i)),
            };
            let n1 = Vec2::new(-tin.y, tin.x);
            let n2 = Vec2::new(-tout.y, tout.x);
            let mut nm = unit(n1 + n2);
            if nm == Vec2::ZERO {
                nm = n2;
            }
            let miter = 1.0 / nm.dot(n2).max(0.25);
            let (l, r) = at(prof, acc[i] / total);
            let h = weight * 0.5 * miter;
            left.push(pts[i] + nm * (l * h));
            right.push(pts[i] - nm * (r * h));
        }
        if closed {
            for ring in [&left, &right] {
                out.move_to(ring[0]);
                for p in &ring[1..] {
                    out.line_to(*p);
                }
                out.close_path();
            }
        } else {
            out.move_to(left[0]);
            for p in &left[1..] {
                out.line_to(*p);
            }
            for p in right.iter().rev() {
                out.line_to(*p);
            }
            out.close_path();
        }
    }
    out
}
