use std::collections::HashMap;

use formdes_model::color::{Color, Paint};
use formdes_model::geom::{Affine, Vec2};
use formdes_model::ids::ItemId;
use formdes_model::item::{ItemKind, Shape};
use kurbo::{BezPath, Point};
use serde_json::{Value, json};

use crate::registry::{Needs, Registry, cmd};
use crate::session::{Ctx, Selection};
use crate::target::items;
use crate::{EngineError, Result, args, ops};

struct Pix {
    w: usize,
    h: usize,
    rgba: Vec<[f64; 4]>,
}

fn downsample(w: usize, h: usize, rgba: &[u8], max_side: usize) -> Pix {
    let s = (w.max(h) as f64 / max_side as f64).max(1.0);
    let (nw, nh) = (((w as f64) / s).round().max(1.0) as usize, ((h as f64) / s).round().max(1.0) as usize);
    let mut out = vec![[0.0; 4]; nw * nh];
    for y in 0..nh {
        for x in 0..nw {
            let (x0, x1) = ((x as f64 * s) as usize, (((x + 1) as f64 * s) as usize).min(w).max((x as f64 * s) as usize + 1));
            let (y0, y1) = ((y as f64 * s) as usize, (((y + 1) as f64 * s) as usize).min(h).max((y as f64 * s) as usize + 1));
            let mut acc = [0.0; 4];
            let mut n = 0.0;
            for yy in y0..y1.min(h) {
                for xx in x0..x1.min(w) {
                    let i = (yy * w + xx) * 4;
                    for c in 0..4 {
                        acc[c] += rgba[i + c] as f64 / 255.0;
                    }
                    n += 1.0;
                }
            }
            out[y * nw + x] = if n > 0.0 { [acc[0] / n, acc[1] / n, acc[2] / n, acc[3] / n] } else { [1.0, 1.0, 1.0, 0.0] };
        }
    }
    Pix { w: nw, h: nh, rgba: out }
}

fn kmeans(px: &Pix, k: usize) -> (Vec<[f64; 3]>, Vec<usize>) {
    let opaque: Vec<usize> = (0..px.rgba.len()).filter(|i| px.rgba[*i][3] > 0.5).collect();
    let mut label = vec![usize::MAX; px.rgba.len()];
    let d2 = |a: [f64; 3], b: [f64; 4]| (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2);
    let sample: Vec<usize> = opaque.iter().copied().step_by((opaque.len() / 4000).max(1)).collect();
    let first = sample.first().map(|i| px.rgba[*i]).unwrap_or([1.0; 4]);
    let mut centres: Vec<[f64; 3]> = vec![[first[0], first[1], first[2]]];
    while centres.len() < k {
        let far = sample.iter().copied().max_by(|a, b| {
            let da = centres.iter().map(|c| d2(*c, px.rgba[*a])).fold(f64::MAX, f64::min);
            let db = centres.iter().map(|c| d2(*c, px.rgba[*b])).fold(f64::MAX, f64::min);
            da.total_cmp(&db)
        });
        let Some(i) = far else { break };
        let q = px.rgba[i];
        centres.push([q[0], q[1], q[2]]);
    }
    let k = centres.len();
    for _ in 0..12 {
        for &i in &opaque {
            label[i] = (0..k).min_by(|a, b| d2(centres[*a], px.rgba[i]).total_cmp(&d2(centres[*b], px.rgba[i]))).unwrap_or(0);
        }
        let mut sum = vec![[0.0; 4]; k];
        for &i in &opaque {
            let p = px.rgba[i];
            let s = &mut sum[label[i]];
            s[0] += p[0];
            s[1] += p[1];
            s[2] += p[2];
            s[3] += 1.0;
        }
        for j in 0..k {
            if sum[j][3] > 0.0 {
                centres[j] = [sum[j][0] / sum[j][3], sum[j][1] / sum[j][3], sum[j][2] / sum[j][3]];
            }
        }
    }
    (centres, label)
}

/// The outlines of a mask as closed pixel-edge loops, each directed so the inside lies on its left; holes come out
/// reversed, so the loops fill correctly with either fill rule.
fn loops(w: usize, h: usize, inside: &dyn Fn(usize, usize) -> bool) -> Vec<Vec<(i32, i32)>> {
    let at = |x: i32, y: i32| x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h && inside(x as usize, y as usize);
    let mut next: HashMap<(i32, i32), Vec<(i32, i32)>> = HashMap::new();
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            if !at(x, y) {
                continue;
            }
            if !at(x, y - 1) { next.entry((x + 1, y)).or_default().push((x, y)); }
            if !at(x, y + 1) { next.entry((x, y + 1)).or_default().push((x + 1, y + 1)); }
            if !at(x - 1, y) { next.entry((x, y)).or_default().push((x, y + 1)); }
            if !at(x + 1, y) { next.entry((x + 1, y + 1)).or_default().push((x + 1, y)); }
        }
    }
    let mut out = Vec::new();
    while let Some(&start) = next.keys().next() {
        let mut lp = vec![start];
        let mut cur = start;
        loop {
            let Some(list) = next.get_mut(&cur) else { break };
            let Some(nx) = list.pop() else { break };
            if list.is_empty() {
                next.remove(&cur);
            }
            if nx == start {
                break;
            }
            lp.push(nx);
            cur = nx;
            if lp.len() > 2_000_000 {
                break;
            }
        }
        if lp.len() >= 4 {
            out.push(lp);
        }
    }
    out
}

fn rdp(pts: &[Point], tol: f64, out: &mut Vec<Point>) {
    if pts.len() < 3 {
        out.extend_from_slice(&pts[..pts.len().saturating_sub(1)]);
        return;
    }
    let (a, b) = (pts[0], pts[pts.len() - 1]);
    let ab = b - a;
    let len = ab.hypot().max(1e-9);
    let (mut idx, mut far) = (0, 0.0);
    for (i, p) in pts.iter().enumerate().take(pts.len() - 1).skip(1) {
        let d = ((*p - a).cross(ab) / len).abs();
        if d > far {
            far = d;
            idx = i;
        }
    }
    if far > tol {
        rdp(&pts[..=idx], tol, out);
        rdp(&pts[idx..], tol, out);
    } else {
        out.push(a);
    }
}

fn smooth_loop(lp: &[(i32, i32)], tol: f64, curves: bool) -> BezPath {
    let raw: Vec<Point> = lp.iter().map(|(x, y)| Point::new(*x as f64, *y as f64)).collect();
    let n = raw.len();
    let mut pts: Vec<Point> = (0..n).map(|i| raw[i].midpoint(raw[(i + 1) % n])).collect();
    for _ in 0..2 {
        pts = (0..n).map(|i| {
            let (a, b, c) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
            Point::new(0.25 * a.x + 0.5 * b.x + 0.25 * c.x, 0.25 * a.y + 0.5 * b.y + 0.25 * c.y)
        }).collect();
    }
    pts.push(pts[0]);
    let far = (0..pts.len()).max_by(|a, b| pts[*a].distance(pts[0]).total_cmp(&pts[*b].distance(pts[0]))).unwrap_or(0);
    let mut keep = Vec::new();
    rdp(&pts[..=far], tol, &mut keep);
    rdp(&pts[far..], tol, &mut keep);
    let mut bp = BezPath::new();
    if keep.len() < 3 {
        return bp;
    }
    bp.move_to(keep[0]);
    if curves {
        let n = keep.len();
        for i in 0..n {
            let (p0, p1, p2, p3) = (keep[(i + n - 1) % n], keep[i], keep[(i + 1) % n], keep[(i + 2) % n]);
            bp.curve_to(p1 + (p2 - p0) / 6.0, p2 - (p3 - p1) / 6.0, p2);
        }
    } else {
        for p in &keep[1..] {
            bp.line_to(*p);
        }
    }
    bp.close_path();
    bp
}

fn trace(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let id = ids[0];
    let mode = args::opt_str(p, "mode").unwrap_or("blackWhite");
    if mode != "blackWhite" && mode != "colors" {
        return Err(EngineError::new("mode is blackWhite or colors"));
    }
    let threshold = args::opt_f64(p, "threshold").unwrap_or(128.0).clamp(0.0, 255.0) / 255.0;
    let k = args::opt_u64(p, "colors").unwrap_or(6).clamp(2, 32) as usize;
    let detail = args::opt_u64(p, "detail").unwrap_or(240).clamp(16, 1200) as usize;
    let min_area = args::opt_f64(p, "noise").unwrap_or(4.0).max(0.0);
    let tol = args::opt_f64(p, "paths").unwrap_or(0.5).clamp(0.1, 10.0);
    let curves = args::opt_bool(p, "curves").unwrap_or(true);
    let ignore_white = args::opt_bool(p, "ignoreWhite").unwrap_or(true);
    let keep = args::opt_bool(p, "keep").unwrap_or(false);
    let doc = c.doc()?;
    let it = doc.item(id).ok_or_else(|| EngineError::new("no such object"))?;
    let g = it.graphic().ok_or_else(|| EngineError::new("select a placed image to trace"))?;
    let asset = g.asset.and_then(|a| doc.assets.get(&a)).ok_or_else(|| EngineError::new("the frame holds no image"))?;
    let dec = formdes_render::images::decode(&asset.data).ok_or_else(|| EngineError::new("the image could not be decoded for tracing (PDF and vector placed files are not traced)"))?;
    let (nat_w, nat_h) = asset.natural_size();
    let px = downsample(dec.width as usize, dec.height as usize, &dec.rgba, detail);
    let to_local = g.content * Affine::scale_non_uniform(nat_w / px.w as f64, nat_h / px.h as f64);
    let lum = |q: [f64; 4]| 0.2126 * q[0] + 0.7152 * q[1] + 0.0722 * q[2];
    let mut layers: Vec<([f64; 3], BezPath)> = Vec::new();
    let build = |inside: &dyn Fn(usize, usize) -> bool| -> BezPath {
        let mut bp = BezPath::new();
        for lp in loops(px.w, px.h, inside) {
            let area: f64 = lp.iter().zip(lp.iter().cycle().skip(1)).map(|(a, b)| (a.0 * b.1 - b.0 * a.1) as f64).sum::<f64>().abs() / 2.0;
            if area < min_area {
                continue;
            }
            for el in smooth_loop(&lp, tol, curves).elements() {
                bp.push(*el);
            }
        }
        bp
    };
    if mode == "blackWhite" {
        let path = build(&|x, y| {
            let q = px.rgba[y * px.w + x];
            q[3] > 0.5 && lum(q) < threshold
        });
        layers.push(([0.0, 0.0, 0.0], path));
    } else {
        let (centres, label) = kmeans(&px, k);
        let mut order: Vec<usize> = (0..k).collect();
        order.sort_by(|a, b| lum([centres[*b][0], centres[*b][1], centres[*b][2], 1.0]).total_cmp(&lum([centres[*a][0], centres[*a][1], centres[*a][2], 1.0])));
        for j in order {
            let col = centres[j];
            if ignore_white && col.iter().all(|v| *v > 0.94) {
                continue;
            }
            let path = build(&|x, y| label[y * px.w + x] == j);
            layers.push((col, path));
        }
    }
    layers.retain(|(_, bp)| !bp.elements().is_empty());
    if layers.is_empty() {
        return Err(EngineError::new("nothing to trace at these settings: lower the noise or change the threshold"));
    }
    let doc = c.doc_mut()?;
    let mut made: Vec<ItemId> = Vec::new();
    for (col, bp) in &layers {
        let Some(nid) = ops::duplicate(doc, id, Vec2::ZERO, None) else { continue };
        if let Some(d) = doc.item_mut(nid) {
            d.kind = ItemKind::Plain;
            d.shape = Shape::Path { path: to_local * bp.clone(), closed: true };
            d.corner = Default::default();
            d.fill = Paint::Color { color: Color::Rgb(*col) };
            d.stroke = ops::default_stroke(0.0);
            d.extra.insert("fillRule".into(), json!("nonzero"));
            d.extra.remove("effects");
        }
        made.push(nid);
    }
    let frame_box = doc.item(id).map(|i| i.shape.bbox()).unwrap_or_default();
    let image_box = to_local.transform_rect_bbox(kurbo::Rect::new(0.0, 0.0, px.w as f64, px.h as f64));
    let cropped = image_box.x0 < frame_box.x0 - 0.5 || image_box.y0 < frame_box.y0 - 0.5 || image_box.x1 > frame_box.x1 + 0.5 || image_box.y1 > frame_box.y1 + 0.5;
    let mut mask = None;
    if cropped && let Some(m) = ops::duplicate(doc, id, Vec2::ZERO, None) {
        if let Some(d) = doc.item_mut(m) {
            d.kind = ItemKind::Plain;
            d.fill = Paint::None;
            d.stroke = ops::default_stroke(0.0);
        }
        made.push(m);
        mask = Some(m);
    }
    let result = if made.len() > 1 { ops::group(doc, &made)? } else { made[0] };
    if let Some(m) = mask
        && let Some(g) = doc.item_mut(result)
    {
        g.extra.insert("clipMask".into(), json!(m.0));
    }
    if !keep {
        doc.remove_item(id);
    }
    c.set_selection(Selection::items(vec![result]));
    c.label("Image Trace");
    Ok(json!({"id": result.0, "shapes": made.len(), "grid": [px.w, px.h]}))
}

pub fn register(r: &mut Registry) {
    cmd("object.imageTrace", "Image Trace").menu(&["Object", "Image Trace", "Make and Expand..."]).needs(Needs::Items)
        .sig("{mode?: blackWhite|colors, threshold?: 0..255 (blackWhite, default 128), colors?: 2..32 (default 6), detail?: pixels on the long side (default 240), paths?: fit tolerance in pixels (default 0.5), noise?: smallest area in pixels (default 4), curves?: bool, ignoreWhite?: bool, keep?: bool (keep the image), ids?}")
        .help("Turns the selected placed image into vector shapes: black and white by a brightness threshold, or a palette of colours found in the image, each colour one compound shape stacked from light to dark; higher detail follows the image more closely, noise drops specks, paths sets how loosely the outlines are fitted.").reg(r, trace);
}
