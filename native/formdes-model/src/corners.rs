use kurbo::{BezPath, ParamCurve, ParamCurveArclen, ParamCurveDeriv, PathEl, PathSeg, Point, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CornerAt {
    pub radius: f64,
    #[serde(default = "rounded")]
    pub kind: String,
}

fn rounded() -> String {
    "rounded".into()
}

const ACC: f64 = 1e-3;

struct Sub {
    segs: Vec<PathSeg>,
    closed: bool,
}

fn subpaths(path: &BezPath) -> Vec<Sub> {
    let mut out = Vec::new();
    let mut segs: Vec<PathSeg> = Vec::new();
    let mut start = Point::ZERO;
    let mut last = Point::ZERO;
    let flush = |segs: &mut Vec<PathSeg>, out: &mut Vec<Sub>, closed: bool| {
        if !segs.is_empty() {
            out.push(Sub { segs: std::mem::take(segs), closed });
        }
    };
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => {
                flush(&mut segs, &mut out, false);
                start = p;
                last = p;
            }
            PathEl::LineTo(p) => {
                segs.push(PathSeg::Line(kurbo::Line::new(last, p)));
                last = p;
            }
            PathEl::QuadTo(c, p) => {
                segs.push(PathSeg::Quad(kurbo::QuadBez::new(last, c, p)));
                last = p;
            }
            PathEl::CurveTo(a, b, p) => {
                segs.push(PathSeg::Cubic(kurbo::CubicBez::new(last, a, b, p)));
                last = p;
            }
            PathEl::ClosePath => {
                if (last - start).hypot() > 1e-6 {
                    segs.push(PathSeg::Line(kurbo::Line::new(last, start)));
                }
                last = start;
                flush(&mut segs, &mut out, true);
            }
        }
    }
    flush(&mut segs, &mut out, false);
    out
}

pub fn anchor_count(path: &BezPath) -> usize {
    subpaths(path).iter().map(|s| if s.closed { s.segs.len() } else { s.segs.len() + 1 }).sum()
}

fn tangent_end(s: &PathSeg) -> Vec2 {
    let d = match s {
        PathSeg::Line(l) => l.p1 - l.p0,
        PathSeg::Quad(q) => q.deriv().eval(1.0).to_vec2(),
        PathSeg::Cubic(c) => c.deriv().eval(1.0).to_vec2(),
    };
    if d.hypot() > 1e-9 { d.normalize() } else { (s.end() - s.start()).normalize() }
}

fn tangent_start(s: &PathSeg) -> Vec2 {
    let d = match s {
        PathSeg::Line(l) => l.p1 - l.p0,
        PathSeg::Quad(q) => q.deriv().eval(0.0).to_vec2(),
        PathSeg::Cubic(c) => c.deriv().eval(0.0).to_vec2(),
    };
    if d.hypot() > 1e-9 { d.normalize() } else { (s.end() - s.start()).normalize() }
}

fn trimmed(s: &PathSeg, head: f64, tail: f64) -> Option<PathSeg> {
    let len = s.arclen(ACC);
    if len <= 1e-9 || head + tail >= len - 1e-9 {
        return None;
    }
    let t0 = if head > 0.0 { s.inv_arclen(head, ACC) } else { 0.0 };
    let t1 = if tail > 0.0 { s.inv_arclen(len - tail, ACC) } else { 1.0 };
    Some(s.subsegment(t0..t1))
}

fn at_len(s: &PathSeg, len: f64) -> (Point, Vec2) {
    let total = s.arclen(ACC);
    let t = if len <= 0.0 { 0.0 } else if len >= total { 1.0 } else { s.inv_arclen(len, ACC) };
    let d = match s {
        PathSeg::Line(l) => l.p1 - l.p0,
        PathSeg::Quad(q) => q.deriv().eval(t).to_vec2(),
        PathSeg::Cubic(c) => c.deriv().eval(t).to_vec2(),
    };
    let d = if d.hypot() > 1e-9 { d.normalize() } else { (s.end() - s.start()).normalize() };
    (s.eval(t), d)
}

struct Joint {
    d: f64,
    kind: String,
    turn: f64,
}

fn push_seg(p: &mut BezPath, s: &PathSeg) {
    match s {
        PathSeg::Line(l) => p.line_to(l.p1),
        PathSeg::Quad(q) => p.quad_to(q.p1, q.p2),
        PathSeg::Cubic(c) => p.curve_to(c.p1, c.p2, c.p3),
    }
}

fn corner(p: &mut BezPath, v: Point, a: Point, b: Point, tin: Vec2, tout: Vec2, j: &Joint) {
    match j.kind.as_str() {
        "bevel" | "chamfer" => p.line_to(b),
        "inset" => {
            p.line_to(a + (b - v));
            p.line_to(b);
        }
        "inverse" | "inverseRounded" | "invertedRound" => {
            let interior = std::f64::consts::PI - j.turn;
            let h = j.d * 4.0 / 3.0 * (interior / 4.0).tan();
            let ra = a - v;
            let rb = b - v;
            let mut ta = Vec2::new(-ra.y, ra.x).normalize();
            if ta.dot(b - a) < 0.0 {
                ta = -ta;
            }
            let mut tb = Vec2::new(-rb.y, rb.x).normalize();
            if tb.dot(a - b) < 0.0 {
                tb = -tb;
            }
            p.curve_to(a + ta * h, b + tb * h, b);
        }
        _ => {
            let h = j.d * 4.0 / 3.0 * (j.turn / 4.0).tan() / (j.turn / 2.0).tan().max(1e-9);
            p.curve_to(a + tin * h, b - tout * h, b);
        }
    }
}

pub fn round_vertices(path: &BezPath, at: &dyn Fn(usize) -> Option<(f64, String)>) -> BezPath {
    let mut out = BezPath::new();
    let mut index = 0usize;
    for sub in subpaths(path) {
        let n = sub.segs.len();
        let count = if sub.closed { n } else { n + 1 };
        let mut joints: Vec<Option<Joint>> = (0..count).map(|_| None).collect();
        for (k, joint) in joints.iter_mut().enumerate() {
            let Some((r, kind)) = at(index + k) else { continue };
            if r <= 0.0 || (!sub.closed && (k == 0 || k == n)) {
                continue;
            }
            let prev = &sub.segs[(k + n - 1) % n];
            let next = &sub.segs[k % n];
            let tin = tangent_end(prev);
            let tout = tangent_start(next);
            let turn = tin.dot(tout).clamp(-1.0, 1.0).acos();
            if turn < 1e-3 || turn > std::f64::consts::PI - 1e-3 {
                continue;
            }
            let interior = std::f64::consts::PI - turn;
            let d = match kind.as_str() {
                "bevel" | "chamfer" | "inset" | "inverse" | "inverseRounded" | "invertedRound" => r,
                _ => r / (interior / 2.0).tan().max(1e-9),
            };
            *joint = Some(Joint { d, kind, turn });
        }
        let lens: Vec<f64> = sub.segs.iter().map(|s| s.arclen(ACC)).collect();
        for k in 0..count {
            if joints[k].is_none() {
                continue;
            }
            let prev = (k + n - 1) % n;
            let next = k % n;
            let limit = (lens[prev] * 0.5).min(lens[next] * 0.5);
            if let Some(j) = joints[k].as_mut() {
                j.d = j.d.min(limit);
            }
        }
        let head = |k: usize| joints.get(k).and_then(|j| j.as_ref()).map_or(0.0, |j| j.d);
        let first = if sub.closed { joints[0].as_ref().map(|j| at_len(&sub.segs[0], j.d).0) } else { None };
        out.move_to(first.unwrap_or_else(|| sub.segs[0].start()));
        for (k, s) in sub.segs.iter().enumerate() {
            let end = if sub.closed { (k + 1) % n } else { k + 1 };
            if let Some(t) = trimmed(s, head(k), head(end)) {
                push_seg(&mut out, &t);
            } else {
                out.line_to(s.end() - tangent_end(s) * head(end));
            }
            if let Some(j) = joints.get(end).and_then(|j| j.as_ref()) {
                if !sub.closed && end == n {
                    continue;
                }
                let nexts = &sub.segs[end % n];
                let v = s.end();
                let (a, tin) = at_len(s, lens[k] - j.d);
                let (b, tout) = at_len(nexts, j.d);
                corner(&mut out, v, a, b, tin, tout, j);
            }
        }
        if sub.closed {
            out.close_path();
        }
        index += count;
    }
    out
}
