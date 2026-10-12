use std::collections::HashMap;

use kurbo::{BezPath, Point};

use crate::images::Decoded;

#[derive(Clone, Debug)]
pub struct Detect {
    pub threshold: f64,
    pub tolerance: f64,
    pub inset: f64,
    pub invert: bool,
    pub inside_edges: bool,
}

impl Default for Detect {
    fn default() -> Self {
        Detect { threshold: 25.0, tolerance: 2.0, inset: 0.0, invert: false, inside_edges: false }
    }
}

pub fn has_alpha(d: &Decoded) -> bool {
    d.rgba.chunks_exact(4).any(|p| p[3] < 250)
}

fn edt(feature: &[bool], w: usize, h: usize) -> Vec<f32> {
    let inf = 1e7f32;
    let mut g: Vec<f32> = feature.iter().map(|f| if *f { 0.0 } else { inf }).collect();
    let pass = |line: &mut [f32]| {
        let n = line.len();
        let mut v = vec![0usize; n];
        let mut z = vec![0f32; n + 1];
        let mut k = 0usize;
        z[0] = -inf;
        z[1] = inf;
        for q in 1..n {
            let cut = |p: usize| ((line[q] + (q * q) as f32) - (line[p] + (p * p) as f32)) / (2.0 * (q as f32 - p as f32));
            let mut s = cut(v[k]);
            while s <= z[k] && k > 0 {
                k -= 1;
                s = cut(v[k]);
            }
            k += 1;
            v[k] = q;
            z[k] = s;
            z[k + 1] = inf;
        }
        let src = line.to_vec();
        let mut k = 0;
        for (q, out) in line.iter_mut().enumerate() {
            while z[k + 1] < q as f32 {
                k += 1;
            }
            let p = v[k];
            *out = (q as f32 - p as f32).powi(2) + src[p];
        }
    };
    for x in 0..w {
        let mut col: Vec<f32> = (0..h).map(|y| g[y * w + x]).collect();
        pass(&mut col);
        for y in 0..h {
            g[y * w + x] = col[y];
        }
    }
    for y in 0..h {
        pass(&mut g[y * w..(y + 1) * w]);
    }
    g.into_iter().map(|v| v.sqrt()).collect()
}

fn field_of(d: &Decoded, opt: &Detect, alpha_only: bool, channel: Option<&[u8]>, work: usize) -> (Vec<f32>, usize, usize) {
    let (sw, sh) = (d.width as usize, d.height as usize);
    let k = sw.max(sh).div_ceil(work).max(1);
    let (fw, fh) = (sw.div_ceil(k), sh.div_ceil(k));
    let use_alpha = alpha_only || channel.is_some() || has_alpha(d);
    let bg = if use_alpha { [0f32; 3] } else { border_colour(d) };
    let thr = opt.threshold.clamp(0.0, 255.0) as f32;
    let mut f = vec![0f32; fw * fh];
    for fy in 0..fh {
        for fx in 0..fw {
            let mut sum = 0f32;
            let mut n = 0f32;
            for y in fy * k..((fy + 1) * k).min(sh) {
                for x in fx * k..((fx + 1) * k).min(sw) {
                    let i = y * sw + x;
                    let inside = if let Some(ch) = channel {
                        ch.get(i).copied().unwrap_or(0) as f32 >= if alpha_only { 128.0 } else { thr.max(1.0) }
                    } else if use_alpha {
                        d.rgba[i * 4 + 3] as f32 > if alpha_only { 127.0 } else { thr }
                    } else {
                        let p = &d.rgba[i * 4..i * 4 + 3];
                        (0..3).map(|c| (p[c] as f32 - bg[c]).abs()).fold(0.0, f32::max) > thr
                    };
                    sum += f32::from(u8::from(inside));
                    n += 1.0;
                }
            }
            f[fy * fw + fx] = if n > 0.0 { sum / n } else { 0.0 };
        }
    }
    (f, fw, fh)
}

fn border_colour(d: &Decoded) -> [f32; 3] {
    let (w, h) = (d.width as usize, d.height as usize);
    let mut bins: HashMap<(u8, u8, u8), (u32, [u32; 3])> = HashMap::new();
    let mut add = |x: usize, y: usize| {
        let p = &d.rgba[(y * w + x) * 4..(y * w + x) * 4 + 3];
        let e = bins.entry((p[0] >> 4, p[1] >> 4, p[2] >> 4)).or_insert((0, [0; 3]));
        e.0 += 1;
        for c in 0..3 {
            e.1[c] += p[c] as u32;
        }
    };
    for x in 0..w {
        add(x, 0);
        add(x, h - 1);
    }
    for y in 0..h {
        add(0, y);
        add(w - 1, y);
    }
    let best = bins.values().max_by_key(|e| e.0).copied().unwrap_or((1, [255; 3]));
    [best.1[0] as f32 / best.0 as f32, best.1[1] as f32 / best.0 as f32, best.1[2] as f32 / best.0 as f32]
}

fn fill_holes(f: &mut [f32], w: usize, h: usize) {
    let mut outside = vec![false; w * h];
    let mut stack: Vec<usize> = Vec::new();
    let push = |i: usize, f: &[f32], outside: &mut Vec<bool>, stack: &mut Vec<usize>| {
        if f[i] < 0.5 && !outside[i] {
            outside[i] = true;
            stack.push(i);
        }
    };
    for x in 0..w {
        push(x, f, &mut outside, &mut stack);
        push((h - 1) * w + x, f, &mut outside, &mut stack);
    }
    for y in 0..h {
        push(y * w, f, &mut outside, &mut stack);
        push(y * w + w - 1, f, &mut outside, &mut stack);
    }
    while let Some(i) = stack.pop() {
        let (x, y) = (i % w, i / w);
        if x > 0 {
            push(i - 1, f, &mut outside, &mut stack);
        }
        if x + 1 < w {
            push(i + 1, f, &mut outside, &mut stack);
        }
        if y > 0 {
            push(i - w, f, &mut outside, &mut stack);
        }
        if y + 1 < h {
            push(i + w, f, &mut outside, &mut stack);
        }
    }
    for i in 0..w * h {
        if !outside[i] {
            f[i] = 1.0;
        }
    }
}

fn inset_field(f: &mut [f32], w: usize, h: usize, r: f32) {
    let inside: Vec<bool> = f.iter().map(|v| *v >= 0.5).collect();
    let outside: Vec<bool> = inside.iter().map(|v| !*v).collect();
    let d_in = edt(&outside, w, h);
    let d_out = edt(&inside, w, h);
    for i in 0..w * h {
        let sd = if inside[i] { d_in[i] - 0.5 } else { -(d_out[i] - 0.5) };
        f[i] = (0.5 + (sd - r)).clamp(0.0, 1.0);
    }
}

type Key = (i32, i32, u8);

fn trace(f: &[f32], w: usize, h: usize) -> Vec<Vec<Point>> {
    let pw = w + 2;
    let ph = h + 2;
    let at = |x: i32, y: i32| -> f32 {
        if x < 1 || y < 1 || x > w as i32 || y > h as i32 { 0.0 } else { f[(y as usize - 1) * w + (x as usize - 1)] }
    };
    let mut pts: HashMap<Key, Point> = HashMap::new();
    let mut next: HashMap<Key, Key> = HashMap::new();
    for y in 0..ph as i32 - 1 {
        for x in 0..pw as i32 - 1 {
            let (a, b, c, d) = (at(x, y), at(x + 1, y), at(x + 1, y + 1), at(x, y + 1));
            let ins = [a >= 0.5, b >= 0.5, c >= 0.5, d >= 0.5];
            if ins.iter().all(|v| *v) || ins.iter().all(|v| !*v) {
                continue;
            }
            let lerp = |v0: f32, v1: f32| -> f64 {
                let t = (0.5 - v0) / (v1 - v0);
                t.clamp(0.0, 1.0) as f64
            };
            let edges: [(Key, Point, bool, bool); 4] = [
                ((x, y, 0), Point::new(x as f64 + lerp(a, b), y as f64), ins[0], ins[1]),
                ((x + 1, y, 1), Point::new((x + 1) as f64, y as f64 + lerp(b, c)), ins[1], ins[2]),
                ((x, y + 1, 0), Point::new(x as f64 + lerp(d, c), (y + 1) as f64), ins[3], ins[2]),
                ((x, y, 1), Point::new(x as f64, y as f64 + lerp(a, d)), ins[0], ins[3]),
            ];
            let order = [(0usize, ins[0], ins[1]), (1, ins[1], ins[2]), (2, ins[2], ins[3]), (3, ins[3], ins[0])];
            let mut cross: Vec<(usize, bool)> = Vec::new();
            for (e, p, q) in order {
                if p != q {
                    cross.push((e, p));
                }
            }
            for (e, _) in &cross {
                pts.insert(edges[*e].0, edges[*e].1);
            }
            let centre = (a + b + c + d) / 4.0 >= 0.5;
            if cross.len() == 2 {
                let (ex, en) = if cross[0].1 { (cross[0].0, cross[1].0) } else { (cross[1].0, cross[0].0) };
                next.insert(edges[ex].0, edges[en].0);
            } else if cross.len() == 4 {
                for i in 0..4 {
                    if cross[i].1 {
                        let en = if centre { cross[(i + 1) % 4].0 } else { cross[(i + 3) % 4].0 };
                        next.insert(edges[cross[i].0].0, edges[en].0);
                    }
                }
            }
        }
    }
    let mut seen: HashMap<Key, bool> = HashMap::new();
    let mut loops = Vec::new();
    let keys: Vec<Key> = next.keys().copied().collect();
    for k in keys {
        if seen.contains_key(&k) {
            continue;
        }
        let mut poly = Vec::new();
        let mut cur = k;
        loop {
            if seen.insert(cur, true).is_some() {
                break;
            }
            let p = pts[&cur];
            poly.push(Point::new(p.x - 0.5, p.y - 0.5));
            match next.get(&cur) {
                Some(n) => cur = *n,
                None => break,
            }
            if cur == k {
                break;
            }
        }
        if poly.len() >= 3 {
            loops.push(poly);
        }
    }
    loops
}

fn area(p: &[Point]) -> f64 {
    let mut s = 0.0;
    for i in 0..p.len() {
        let (a, b) = (p[i], p[(i + 1) % p.len()]);
        s += a.x * b.y - b.x * a.y;
    }
    s / 2.0
}

fn dp(p: &[Point], eps: f64, out: &mut Vec<Point>) {
    if p.len() < 3 {
        out.extend_from_slice(p);
        return;
    }
    let (a, b) = (p[0], p[p.len() - 1]);
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = dx.hypot(dy).max(1e-9);
    let mut worst = (0.0, 0);
    for (i, q) in p.iter().enumerate().take(p.len() - 1).skip(1) {
        let d = ((q.x - a.x) * dy - (q.y - a.y) * dx).abs() / len;
        if d > worst.0 {
            worst = (d, i);
        }
    }
    if worst.0 > eps {
        dp(&p[..=worst.1], eps, out);
        out.pop();
        dp(&p[worst.1..], eps, out);
    } else {
        out.push(a);
        out.push(b);
    }
}

fn simplify(p: &[Point], eps: f64) -> Vec<Point> {
    if p.len() < 6 {
        return p.to_vec();
    }
    let far = (1..p.len()).max_by(|a, b| p[*a].distance(p[0]).total_cmp(&p[*b].distance(p[0]))).unwrap_or(1);
    let mut first = Vec::new();
    dp(&p[..=far], eps, &mut first);
    let mut tail: Vec<Point> = p[far..].to_vec();
    tail.push(p[0]);
    let mut second = Vec::new();
    dp(&tail, eps, &mut second);
    first.pop();
    second.pop();
    first.extend(second);
    first
}

pub struct Traced {
    pub path: BezPath,
    pub points: usize,
}

pub fn from_decoded(d: &Decoded, opt: &Detect, alpha_only: bool, channel: Option<&[u8]>, (out_w, out_h): (f64, f64)) -> Option<Traced> {
    if d.width == 0 || d.height == 0 {
        return None;
    }
    let work = 1100;
    let (mut f, fw, fh) = field_of(d, opt, alpha_only, channel, work);
    if !opt.inside_edges {
        fill_holes(&mut f, fw, fh);
    }
    let cell = (d.width as f64 / fw as f64).max(1.0);
    if opt.inset.abs() > 1e-6 {
        inset_field(&mut f, fw, fh, (opt.inset / cell) as f32);
    }
    let mut loops = trace(&f, fw, fh);
    let eps = (opt.tolerance * 0.5).max(0.35);
    let min_area = (opt.tolerance * opt.tolerance * 2.0).max(4.0);
    loops.retain(|l| area(l).abs() >= min_area);
    if loops.is_empty() {
        return None;
    }
    let mut polys: Vec<Vec<Point>> = loops.iter().map(|l| simplify(l, eps)).filter(|l| l.len() >= 3).collect();
    let outer_sign = polys.iter().map(|p| area(p)).max_by(|a, b| a.abs().total_cmp(&b.abs())).unwrap_or(1.0).signum();
    let sx = out_w / fw as f64;
    let sy = out_h / fh as f64;
    let mut path = BezPath::new();
    let mut points = 0;
    for p in &mut polys {
        let mut poly = p.clone();
        if opt.invert {
            poly.reverse();
        }
        points += poly.len();
        for (i, q) in poly.iter().enumerate() {
            let pt = Point::new((q.x * sx).clamp(0.0, out_w), (q.y * sy).clamp(0.0, out_h));
            if i == 0 { path.move_to(pt) } else { path.line_to(pt) }
        }
        path.close_path();
    }
    if opt.invert {
        let mut r = vec![Point::new(0.0, 0.0), Point::new(out_w, 0.0), Point::new(out_w, out_h), Point::new(0.0, out_h)];
        if area(&r).signum() != outer_sign {
            r.reverse();
        }
        path.move_to(r[0]);
        for q in &r[1..] {
            path.line_to(*q);
        }
        path.close_path();
    }
    Some(Traced { path, points })
}

pub fn invert_path(path: &BezPath, w: f64, h: f64) -> BezPath {
    use kurbo::Shape;
    let mut subs: Vec<BezPath> = Vec::new();
    for el in path.elements() {
        match el {
            kurbo::PathEl::MoveTo(_) => subs.push(BezPath::from_vec(vec![*el])),
            other => {
                if let Some(s) = subs.last_mut() {
                    s.push(*other);
                }
            }
        }
    }
    let outer = subs.iter().map(|s| s.area()).max_by(|a, b| a.abs().total_cmp(&b.abs())).unwrap_or(1.0).signum();
    let mut out = BezPath::new();
    for s in &subs {
        let mut r = s.clone();
        r.close_path();
        for el in r.reverse_subpaths().elements() {
            out.push(*el);
        }
    }
    let mut c = vec![Point::new(0.0, 0.0), Point::new(w, 0.0), Point::new(w, h), Point::new(0.0, h)];
    if area(&c).signum() != outer {
        c.reverse();
    }
    out.move_to(c[0]);
    for q in &c[1..] {
        out.line_to(*q);
    }
    out.close_path();
    out
}
