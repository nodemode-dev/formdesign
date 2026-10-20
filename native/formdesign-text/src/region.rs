use kurbo::{BezPath, PathEl, Point};

pub type Span = (f64, f64);

pub fn polylines(path: &BezPath) -> Vec<Vec<Point>> {
    let mut out: Vec<Vec<Point>> = Vec::new();
    let mut cur: Vec<Point> = Vec::new();
    kurbo::flatten(path.iter(), 0.25, |el| match el {
        PathEl::MoveTo(p) => {
            if cur.len() > 1 {
                out.push(std::mem::take(&mut cur));
            }
            cur.clear();
            cur.push(p);
        }
        PathEl::LineTo(p) => cur.push(p),
        PathEl::ClosePath => {
            if let Some(f) = cur.first().copied() {
                cur.push(f);
            }
            if cur.len() > 1 {
                out.push(std::mem::take(&mut cur));
            }
        }
        _ => {}
    });
    if cur.len() > 1 {
        if let Some(f) = cur.first().copied() {
            cur.push(f);
        }
        out.push(cur);
    }
    out
}

pub fn spans_at(polys: &[Vec<Point>], y: f64) -> Vec<Span> {
    let mut xs: Vec<(f64, i32)> = Vec::new();
    for poly in polys {
        for w in poly.windows(2) {
            let (a, b) = (w[0], w[1]);
            if (a.y <= y && b.y > y) || (b.y <= y && a.y > y) {
                let t = (y - a.y) / (b.y - a.y);
                xs.push((a.x + t * (b.x - a.x), if b.y > a.y { 1 } else { -1 }));
            }
        }
    }
    xs.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out = Vec::new();
    let mut wind = 0;
    let mut start = 0.0;
    for (x, d) in xs {
        let before = wind;
        wind += d;
        if before == 0 && wind != 0 {
            start = x;
        } else if before != 0 && wind == 0 && x > start {
            out.push((start, x));
        }
    }
    out
}

fn samples(top: f64, bottom: f64) -> Vec<f64> {
    let h = (bottom - top).max(0.0);
    let n = ((h / 2.0).ceil() as usize).clamp(2, 24);
    (0..=n).map(|i| top + h * i as f64 / n as f64).collect()
}

pub fn merge(mut v: Vec<Span>) -> Vec<Span> {
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out: Vec<Span> = Vec::new();
    for s in v {
        match out.last_mut() {
            Some(l) if s.0 <= l.1 => l.1 = l.1.max(s.1),
            _ => out.push(s),
        }
    }
    out
}

pub fn intersect(a: &[Span], b: &[Span]) -> Vec<Span> {
    let mut out = Vec::new();
    for x in a {
        for y in b {
            let l = x.0.max(y.0);
            let r = x.1.min(y.1);
            if r > l {
                out.push((l, r));
            }
        }
    }
    merge(out)
}

pub fn subtract(a: &[Span], b: &[Span]) -> Vec<Span> {
    let mut cur: Vec<Span> = a.to_vec();
    for cut in b {
        let mut next = Vec::new();
        for s in cur {
            if cut.1 <= s.0 || cut.0 >= s.1 {
                next.push(s);
                continue;
            }
            if cut.0 > s.0 {
                next.push((s.0, cut.0));
            }
            if cut.1 < s.1 {
                next.push((cut.1, s.1));
            }
        }
        cur = next;
    }
    cur
}

pub fn band_cover(polys: &[Vec<Point>], top: f64, bottom: f64, pad: f64) -> Vec<Span> {
    let mut all = Vec::new();
    for y in samples(top - pad, bottom + pad) {
        all.extend(spans_at(polys, y).into_iter().map(|(a, b)| (a - pad, b + pad)));
    }
    merge(all)
}

pub fn band_inside(polys: &[Vec<Point>], top: f64, bottom: f64) -> Vec<Span> {
    let mut acc: Option<Vec<Span>> = None;
    for y in samples(top, bottom) {
        let s = merge(spans_at(polys, y));
        acc = Some(match acc {
            None => s,
            Some(a) => intersect(&a, &s),
        });
    }
    acc.unwrap_or_default()
}

pub struct ArcPath {
    pts: Vec<Point>,
    cum: Vec<f64>,
}

impl ArcPath {
    pub fn new(path: &BezPath) -> ArcPath {
        let mut pts: Vec<Point> = Vec::new();
        kurbo::flatten(path.iter(), 0.1, |el| match el {
            PathEl::MoveTo(p) if pts.is_empty() => pts.push(p),
            PathEl::LineTo(p) => pts.push(p),
            PathEl::ClosePath => {
                if let Some(f) = pts.first().copied() {
                    pts.push(f);
                }
            }
            _ => {}
        });
        let mut cum = vec![0.0];
        for w in pts.windows(2) {
            let l = cum.last().copied().unwrap_or(0.0) + (w[1] - w[0]).hypot();
            cum.push(l);
        }
        ArcPath { pts, cum }
    }

    pub fn reversed(&self) -> ArcPath {
        let pts: Vec<Point> = self.pts.iter().rev().copied().collect();
        let mut cum = vec![0.0];
        for w in pts.windows(2) {
            let l = cum.last().copied().unwrap_or(0.0) + (w[1] - w[0]).hypot();
            cum.push(l);
        }
        ArcPath { pts, cum }
    }

    pub fn length(&self) -> f64 {
        self.cum.last().copied().unwrap_or(0.0)
    }

    pub fn at(&self, s: f64) -> Option<(Point, f64)> {
        if self.pts.len() < 2 {
            return None;
        }
        let s = s.clamp(0.0, self.length());
        let i = self.cum.partition_point(|c| *c < s).clamp(1, self.pts.len() - 1);
        let (a, b) = (self.pts[i - 1], self.pts[i]);
        let seg = (self.cum[i] - self.cum[i - 1]).max(1e-9);
        let t = (s - self.cum[i - 1]) / seg;
        let p = a.lerp(b, t);
        Some((p, (b.y - a.y).atan2(b.x - a.x)))
    }
}
