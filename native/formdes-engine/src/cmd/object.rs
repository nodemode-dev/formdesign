use std::sync::Arc;

use formdes_model::color::{Gradient, Paint, Stop};
use formdes_model::doc::{Doc, Home};
use formdes_model::geom::{Affine, Edges, Point, Rect, Vec2};
use formdes_model::ids::{ItemId, LayerId};
use formdes_model::item::{AutoSize, ColumnRule, Fitting, Graphic, Item, ItemKind, Shape, State, TextFrame, TextWrap, VJust, WrapMode};
use formdes_model::story::{Inline, Marker};
use serde_json::{Map, Value, json};

use crate::registry::{Needs, Registry, cmd};
use crate::session::{Ctx, Selection};
use crate::target::{items, one_id};
use crate::{EngineError, Result, args, ops, wire};

pub fn item_page(doc: &Doc, id: ItemId) -> Option<usize> {
    doc.page_of(id).or_else(|| {
        let si = doc.spread_of(id)?;
        let b = doc.bounds(id)?;
        Some(doc.first_page_of_spread(si) + doc.spreads[si].nearest_page(b.center().x))
    })
}

pub fn margin_rect(doc: &Doc, abs: usize) -> Option<Rect> {
    let r = doc.page_rect(abs)?;
    let p = doc.page(abs)?;
    let (l, rt) = p.margins.left_right(!doc.is_right_page(abs));
    Some(Rect::new(r.x0 + l, r.y0 + p.margins.top, r.x1 - rt, r.y1 - p.margins.bottom))
}

pub fn spread_rect(doc: &Doc, si: usize) -> Option<Rect> {
    let s = doc.spreads.get(si)?;
    Some(Rect::new(0.0, 0.0, s.width(), s.height()))
}

pub fn ref_point(b: Rect, r: u64) -> Point {
    let r = r.min(8);
    let (col, row) = ((r % 3) as f64, (r / 3) as f64);
    Point::new(b.x0 + b.width() * col / 2.0, b.y0 + b.height() * row / 2.0)
}

fn about(p: Point, m: Affine) -> Affine {
    Affine::translate(p.to_vec2()) * m * Affine::translate(-p.to_vec2())
}

pub fn decompose(xf: Affine) -> (f64, f64, f64, f64) {
    let [a, b, c, d, _, _] = xf.as_coeffs();
    let theta = b.atan2(a);
    let sx = a.hypot(b);
    let (cs, sn) = (theta.cos(), theta.sin());
    let m = c * cs + d * sn;
    let sy = -c * sn + d * cs;
    let shear = if sy.abs() > 1e-12 { (m / sy).atan() } else { 0.0 };
    let clean = |v: f64| if v.abs() < 1e-9 { 0.0 } else { (v * 1e6).round() / 1e6 };
    (clean(-theta.to_degrees()), clean(sx), clean(sy), clean(-shear.to_degrees()))
}

fn compose(rotation_deg: f64, sx: f64, sy: f64, shear_deg: f64) -> Affine {
    let r = Affine::rotate(-rotation_deg.to_radians());
    let m = -shear_deg.to_radians().tan() * sy;
    r * Affine::new([sx, 0.0, m, sy, 0.0, 0.0])
}

fn natural(doc: &Doc, g: &Graphic) -> Option<(f64, f64)> {
    let a = doc.assets.get(&g.asset?)?;
    Some(a.natural_size())
}

pub fn fit_content(doc: &mut Doc, id: ItemId, mode: &str, align: Option<u64>) -> Result<()> {
    let Some(it) = doc.item(id).cloned() else { return Err(EngineError::new(format!("no object with id {id}"))) };
    let ItemKind::Graphic(g) = &it.kind else { return Ok(()) };
    let Some((w, h)) = natural(doc, g) else { return Ok(()) };
    let r = it.shape.bbox();
    let refp = align.or_else(|| g.fitting.as_ref().and_then(|f| f.reference.parse().ok())).unwrap_or(4);
    let (col, row) = ((refp.min(8) % 3) as f64 / 2.0, (refp.min(8) / 3) as f64 / 2.0);
    let place = |s: f64| Affine::translate((r.x0 + (r.width() - w * s) * col, r.y0 + (r.height() - h * s) * row)) * Affine::scale(s);
    let content = match mode {
        "fillProportionally" | "fill" => place((r.width() / w).max(r.height() / h)),
        "fitProportionally" | "fit" => place((r.width() / w).min(r.height() / h)),
        "fitContentToFrame" | "stretch" => Affine::translate((r.x0, r.y0)) * Affine::scale_non_uniform(r.width() / w, r.height() / h),
        "centerContent" | "center" => {
            let c = g.content.as_coeffs();
            let s = c[0].hypot(c[1]).max(1e-6);
            let sy = c[2].hypot(c[3]).max(1e-6);
            Affine::translate((r.center().x - w * s / 2.0, r.center().y - h * sy / 2.0)) * Affine::scale_non_uniform(s, sy)
        }
        "fitFrameToContent" | "frameToContent" => {
            let b = g.content.transform_rect_bbox(Rect::new(0.0, 0.0, w, h));
            if let Some(m) = doc.item_mut(id) {
                m.shape = m.shape.with_rect(b);
            }
            return Ok(());
        }
        "none" => return Ok(()),
        other => return Err(EngineError::new(format!("`mode` expected fillProportionally, fitProportionally, fitContentToFrame, centerContent or fitFrameToContent, not {other}"))),
    };
    if let Some(ItemKind::Graphic(g)) = doc.item_mut(id).map(|i| &mut i.kind) {
        g.content = content;
    }
    Ok(())
}

pub fn resize_items(doc: &mut Doc, ids: &[ItemId], m: Affine, content: bool) {
    for id in ids {
        let Some(it) = doc.item(*id).cloned() else { continue };
        let world = doc.world_xf(*id);
        let [_, b, c, _, _, _] = world.as_coeffs();
        let grouped = matches!(it.kind, ItemKind::Group { .. } | ItemKind::States { .. });
        if grouped || b.abs() > 1e-9 || c.abs() > 1e-9 {
            ops::transform_items(doc, &[*id], m);
            continue;
        }
        let local = it.shape.bbox();
        let wr = world.transform_rect_bbox(local);
        let nr = m.transform_rect_bbox(wr);
        let nl = world.inverse().transform_rect_bbox(nr);
        let lm = world.inverse() * m * world;
        let auto = match &it.kind {
            ItemKind::Graphic(g) => g.fitting.as_ref().filter(|f| f.auto).map(|f| f.kind.clone()),
            _ => None,
        };
        if let Some(x) = doc.item_mut(*id) {
            x.shape = x.shape.with_rect(nl);
            if let ItemKind::Graphic(g) = &mut x.kind
                && content
            {
                g.content = lm * g.content;
            }
        }
        if let Some(kind) = auto
            && !content
        {
            let _ = fit_content(doc, *id, &kind, None);
        }
        if let Some(x) = doc.item_mut(*id) {
            let w = x.shape.bbox().width();
            if let Some(t) = x.text_mut()
                && t.column_rule != ColumnRule::Number
            {
                t.columns = t.column_plan((w - t.insets.inside - t.insets.outside).max(1.0)).0;
            }
        }
    }
}

fn ok() -> Result<Value> {
    Ok(json!({}))
}

fn set_each(c: &mut Ctx, ids: &[ItemId], f: impl Fn(&mut Item)) -> Result<()> {
    let doc = c.doc_mut()?;
    for id in ids {
        if let Some(it) = doc.item_mut(*id) {
            f(it);
        }
    }
    Ok(())
}

fn sel_bounds(c: &Ctx, ids: &[ItemId]) -> Result<Rect> {
    ops::union_bounds(c.doc()?, ids).ok_or_else(|| EngineError::new("the selection has no bounds"))
}

fn align(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let edge = args::str_(p, "edge", "left, hcenter, right, top, vcenter or bottom")?.to_string();
    let to = args::opt_str(p, "to").unwrap_or("selection").to_string();
    let doc = c.doc()?;
    let first_page = item_page(doc, ids[0]).unwrap_or(0);
    let refb = match to.as_str() {
        "keyObject" | "key" => doc.bounds(ids[0]),
        "margins" => margin_rect(doc, first_page),
        "page" => doc.page_rect(first_page),
        "spread" => doc.spread_of(ids[0]).and_then(|s| spread_rect(doc, s)),
        _ => ops::union_bounds(doc, &ids),
    }
    .ok_or_else(|| EngineError::new("nothing to align to"))?;
    let moves: Vec<(ItemId, Vec2)> = ids
        .iter()
        .filter_map(|id| {
            let b = doc.bounds(*id)?;
            let d = match edge.as_str() {
                "left" => Vec2::new(refb.x0 - b.x0, 0.0),
                "right" => Vec2::new(refb.x1 - b.x1, 0.0),
                "hcenter" | "center" => Vec2::new(refb.center().x - b.center().x, 0.0),
                "top" => Vec2::new(0.0, refb.y0 - b.y0),
                "bottom" => Vec2::new(0.0, refb.y1 - b.y1),
                _ => Vec2::new(0.0, refb.center().y - b.center().y),
            };
            Some((*id, d))
        })
        .collect();
    let doc = c.doc_mut()?;
    for (id, d) in moves {
        ops::move_items(doc, &[id], d);
    }
    ok()
}

fn distribute(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    if ids.len() < 3 && args::opt_str(p, "by") != Some("spacing") || ids.len() < 2 {
        return ok();
    }
    let vertical = args::opt_str(p, "axis") == Some("vertical");
    let spacing = args::opt_str(p, "by") == Some("spacing");
    let doc = c.doc()?;
    let mut bs: Vec<(ItemId, Rect)> = ids.iter().filter_map(|i| Some((*i, doc.bounds(*i)?))).collect();
    let key = |r: &Rect| if vertical { r.center().y } else { r.center().x };
    bs.sort_by(|a, b| key(&a.1).total_cmp(&key(&b.1)));
    let n = bs.len();
    let lo = |r: &Rect| if vertical { r.y0 } else { r.x0 };
    let hi = |r: &Rect| if vertical { r.y1 } else { r.x1 };
    let len = |r: &Rect| hi(r) - lo(r);
    let mut moves = Vec::new();
    if spacing {
        let span = hi(&bs[n - 1].1) - lo(&bs[0].1);
        let total: f64 = bs.iter().map(|b| len(&b.1)).sum();
        let gap = (span - total) / (n - 1) as f64;
        let mut at = lo(&bs[0].1);
        for (id, r) in &bs {
            moves.push((*id, at - lo(r)));
            at += len(r) + gap;
        }
    } else {
        let (a, b) = (key(&bs[0].1), key(&bs[n - 1].1));
        for (k, (id, r)) in bs.iter().enumerate() {
            moves.push((*id, a + (b - a) * k as f64 / (n - 1) as f64 - key(r)));
        }
    }
    let doc = c.doc_mut()?;
    for (id, d) in moves {
        ops::move_items(doc, &[id], if vertical { Vec2::new(0.0, d) } else { Vec2::new(d, 0.0) });
    }
    ok()
}

fn arrange(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let to = args::opt_str(p, "to").or_else(|| args::opt_str(p, "order")).unwrap_or("front").to_string();
    let op = match to.as_str() {
        "front" | "bringToFront" => "front",
        "forward" | "bringForward" => "forward",
        "backward" | "sendBackward" => "backward",
        "back" | "sendToBack" => "back",
        other => return Err(EngineError::new(format!("`to` expected front, forward, backward or back, not {other}"))),
    };
    let label = match op {
        "front" => "Bring to Front",
        "forward" => "Bring Forward",
        "backward" => "Send Backward",
        _ => "Send to Back",
    };
    c.label(label);
    let mut ids = ids;
    if op == "forward" || op == "front" {
        ids.reverse();
    }
    ops::arrange(c.doc_mut()?, &ids, op);
    ok()
}

fn attributes(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let np = args::opt_bool(p, "nonprinting");
    let of = args::opt_bool(p, "overprintFill");
    let os = args::opt_bool(p, "overprintStroke");
    set_each(c, &ids, |it| {
        if let Some(v) = np {
            it.nonprinting = v;
        }
        if let Some(v) = of {
            it.extra.insert("overprintFill".into(), json!(v));
        }
        if let Some(v) = os {
            it.extra.insert("overprintStroke".into(), json!(v));
        }
    })?;
    ok()
}

fn color(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let col = p.get("color").and_then(wire::color_in).ok_or_else(|| EngineError::new("missing `color` (\"#rrggbb\" or a colour object)"))?;
    let stroke = args::opt_str(p, "target") == Some("stroke");
    set_each(c, &ids, |it| {
        if stroke {
            it.stroke.paint = Paint::Color { color: col };
        } else {
            it.fill = Paint::Color { color: col };
        }
    })?;
    ok()
}

fn corner_options(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let shape = args::opt_str(p, "shape").unwrap_or("rounded");
    let kind = match shape {
        "inverseRounded" | "inverse" => "inverse",
        "inset" => "inset",
        "bevel" => "bevel",
        "fancy" => "fancy",
        "none" => "none",
        _ => "rounded",
    }
    .to_string();
    let size = args::opt_f64(p, "size").unwrap_or(12.0).max(0.0);
    set_each(c, &ids, |it| {
        it.corner.kind = if kind == "none" { "rounded".into() } else { kind.clone() };
        it.corner.radius = if kind == "none" { 0.0 } else { size };
    })?;
    ok()
}

fn drop_shadow(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let on = args::opt_bool(p, "on").unwrap_or(true);
    let distance = args::opt_f64(p, "distance").unwrap_or(6.0);
    let angle = args::opt_f64(p, "angle").unwrap_or(135.0).to_radians();
    let size = args::opt_f64(p, "size").unwrap_or(12.0);
    let opacity = args::opt_f64(p, "opacity").unwrap_or(35.0) / 100.0;
    let color = p.get("color").and_then(wire::color_in).map(wire::color_out);
    let mut fx = json!({
        "on": on, "mode": effect_mode(p, "multiply")?, "distance": distance, "angle": angle.to_degrees(), "size": size.max(0.0), "spread": percent(p, "spread", 0.0),
        "opacity": opacity.clamp(0.0, 1.0), "x": -distance * angle.cos(), "y": distance * angle.sin()
    });
    if let Some(col) = color
        && let Some(o) = fx.as_object_mut()
    {
        let cv = formdes_model::color::Color::from_value(&col).or_else(|| wire::color_in(&col)).map(|c| serde_json::to_value(c).unwrap_or(Value::Null));
        o.insert("color".into(), cv.unwrap_or(Value::Null));
    }
    set_each(c, &ids, |it| {
        it.effects.insert("dropShadow".into(), fx.clone());
    })?;
    ok()
}

fn effect_color(fx: &mut Value, p: &Value) {
    if let Some(col) = p.get("color").and_then(wire::color_in).map(wire::color_out)
        && let Some(o) = fx.as_object_mut()
    {
        let cv = formdes_model::color::Color::from_value(&col).or_else(|| wire::color_in(&col)).map(|c| serde_json::to_value(c).unwrap_or(Value::Null));
        o.insert("color".into(), cv.unwrap_or(Value::Null));
    }
}

fn effect_mode(p: &Value, default: &str) -> Result<String> {
    let mode = args::opt_str(p, "mode").unwrap_or(default);
    const MODES: [&str; 12] = ["normal", "multiply", "screen", "overlay", "darken", "lighten", "colorDodge", "colorBurn", "hardLight", "softLight", "difference", "exclusion"];
    if MODES.contains(&mode) { Ok(mode.to_string()) } else { Err(EngineError::new(format!("mode must be one of {}", MODES.join(", ")))) }
}

fn percent(p: &Value, key: &str, default: f64) -> f64 {
    (args::opt_f64(p, key).unwrap_or(default) / 100.0).clamp(0.0, 1.0)
}

fn set_effect(c: &mut Ctx, p: &Value, key: &str, fx: Value) -> Result<Value> {
    let ids = items(c, p)?;
    set_each(c, &ids, |it| {
        it.effects.insert(key.into(), fx.clone());
    })?;
    ok()
}

fn inner_shadow(c: &mut Ctx, p: &Value) -> Result<Value> {
    let distance = args::opt_f64(p, "distance").unwrap_or(5.0).max(0.0);
    let angle = args::opt_f64(p, "angle").unwrap_or(135.0).to_radians();
    let mut fx = json!({
        "on": args::opt_bool(p, "on").unwrap_or(true), "mode": effect_mode(p, "multiply")?, "opacity": percent(p, "opacity", 75.0),
        "distance": distance, "angle": angle.to_degrees(), "size": args::opt_f64(p, "size").unwrap_or(5.0).max(0.0), "choke": percent(p, "choke", 0.0),
        "x": -distance * angle.cos(), "y": distance * angle.sin()
    });
    effect_color(&mut fx, p);
    set_effect(c, p, "innerShadow", fx)
}

fn outer_glow(c: &mut Ctx, p: &Value) -> Result<Value> {
    let technique = if args::opt_str(p, "technique") == Some("precise") { "precise" } else { "softer" };
    let mut fx = json!({
        "on": args::opt_bool(p, "on").unwrap_or(true), "mode": effect_mode(p, "normal")?, "opacity": percent(p, "opacity", 75.0),
        "size": args::opt_f64(p, "size").unwrap_or(7.0).max(0.0), "spread": percent(p, "spread", 0.0), "technique": technique
    });
    effect_color(&mut fx, p);
    set_effect(c, p, "outerGlow", fx)
}

fn inner_glow(c: &mut Ctx, p: &Value) -> Result<Value> {
    let mut fx = json!({
        "on": args::opt_bool(p, "on").unwrap_or(true), "mode": effect_mode(p, "normal")?, "opacity": percent(p, "opacity", 75.0),
        "size": args::opt_f64(p, "size").unwrap_or(7.0).max(0.0), "choke": percent(p, "choke", 0.0)
    });
    effect_color(&mut fx, p);
    set_effect(c, p, "innerGlow", fx)
}

fn basic_feather(c: &mut Ctx, p: &Value) -> Result<Value> {
    let corners = match args::opt_str(p, "corners") {
        Some("sharp") => "sharp",
        Some("diffusion") => "diffusion",
        _ => "rounded",
    };
    let fx = json!({"on": args::opt_bool(p, "on").unwrap_or(true), "width": args::opt_f64(p, "width").unwrap_or(9.0).max(0.0), "choke": percent(p, "choke", 0.0), "corners": corners});
    set_effect(c, p, "basicFeather", fx)
}

fn gradient_feather(c: &mut Ctx, p: &Value) -> Result<Value> {
    let radial = args::opt_str(p, "type") == Some("radial");
    let mut stops = Vec::new();
    for s in p.get("stops").and_then(Value::as_array).cloned().unwrap_or_default() {
        let opacity = s.get("opacity").and_then(Value::as_f64).unwrap_or(100.0);
        let location = s.get("location").and_then(Value::as_f64).unwrap_or(0.0);
        stops.push((location.clamp(0.0, 100.0) / 100.0, (opacity / 100.0).clamp(0.0, 1.0)));
    }
    if stops.is_empty() {
        stops = vec![(0.0, 1.0), (1.0, 0.0)];
    }
    if stops.len() < 2 {
        return Err(EngineError::new("a gradient feather needs at least two `stops` ([{opacity: 0..100, location: 0..100}])"));
    }
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));
    let stops: Vec<Value> = stops.into_iter().map(|(l, o)| json!({"location": l, "opacity": o})).collect();
    let fx = json!({"on": args::opt_bool(p, "on").unwrap_or(true), "type": if radial { "radial" } else { "linear" }, "angle": args::opt_f64(p, "angle").unwrap_or(0.0), "stops": stops});
    set_effect(c, p, "gradientFeather", fx)
}

fn directional_feather(c: &mut Ctx, p: &Value) -> Result<Value> {
    let side = |k: &str| args::opt_f64(p, k).unwrap_or(9.0).max(0.0);
    let fx = json!({
        "on": args::opt_bool(p, "on").unwrap_or(true), "top": side("top"), "bottom": side("bottom"), "left": side("left"), "right": side("right"),
        "choke": percent(p, "choke", 0.0), "angle": args::opt_f64(p, "angle").unwrap_or(0.0)
    });
    set_effect(c, p, "directionalFeather", fx)
}

fn fill(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let name = args::str_(p, "swatch", "a swatch name such as Accent or [None]")?;
    let tint = args::opt_f64(p, "tint");
    if !wire::LOCKED.iter().any(|l| l.eq_ignore_ascii_case(&formdes_model::color::swatch_key(name))) && c.doc()?.swatch(name).is_none() && !name.starts_with('#') {
        return Err(EngineError::new(format!("no swatch named {name}")));
    }
    let paint = wire::paint_in(name, tint);
    set_each(c, &ids, |it| it.fill = paint.clone())?;
    ok()
}

fn stroke(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let swatch = args::opt_str(p, "swatch").map(str::to_string);
    if let Some(n) = &swatch
        && !wire::LOCKED.iter().any(|l| l.eq_ignore_ascii_case(&formdes_model::color::swatch_key(n)))
        && c.doc()?.swatch(n).is_none()
        && !n.starts_with('#')
    {
        return Err(EngineError::new(format!("no swatch named {n}")));
    }
    let tint = args::opt_f64(p, "tint");
    let weight = args::opt_f64(p, "weight");
    let cap = args::opt_str(p, "cap").map(|c| match c {
        "projecting" | "square" => "square".to_string(),
        other => other.to_string(),
    });
    let join = args::opt_str(p, "join").map(str::to_string);
    let align = args::opt_str(p, "align").map(str::to_string);
    let miter = args::opt_f64(p, "miterLimit");
    let start = args::opt_str(p, "start").map(str::to_string);
    let end = args::opt_str(p, "end").map(str::to_string);
    let start_scale = args::opt_f64(p, "startScale");
    let end_scale = args::opt_f64(p, "endScale");
    let align_dashes = args::opt_bool(p, "alignDashes");
    let dash: Option<Vec<f64>> = p.get("type").map(|t| match t.get("kind").and_then(Value::as_str) {
        Some("dashed" | "dotted") => t.get("pattern").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_else(|| vec![4.0, 4.0]),
        _ => Vec::new(),
    });
    set_each(c, &ids, |it| {
        let st = &mut it.stroke;
        if let Some(n) = &swatch {
            st.paint = wire::paint_in(n, tint);
        } else if let (Some(t), Paint::Swatch { tint: old, .. }) = (tint, &mut st.paint) {
            *old = t.clamp(0.0, 1.0);
        }
        if let Some(w) = weight {
            st.weight = w.max(0.0);
            if w > 0.0 && st.paint.is_none() && swatch.is_none() {
                st.paint = Paint::swatch("Black");
            }
        }
        if let Some(cp) = &cap {
            st.cap = cp.clone();
        }
        if let Some(j) = &join {
            st.join = j.clone();
        }
        if let Some(a) = &align {
            st.align = match a.as_str() {
                "inside" => formdes_model::item::StrokeAlign::Inside,
                "outside" => formdes_model::item::StrokeAlign::Outside,
                _ => formdes_model::item::StrokeAlign::Center,
            };
        }
        if let Some(d) = &dash {
            st.dash = d.clone();
        }
        if let Some(m) = miter {
            st.miter = m.clamp(1.0, 500.0);
        }
        if let Some(s) = &start {
            st.start = if s == "none" { String::new() } else { s.clone() };
        }
        if let Some(e) = &end {
            st.end = if e == "none" { String::new() } else { e.clone() };
        }
        if let Some(v) = start_scale {
            st.start_scale = v.clamp(1.0, 1000.0);
        }
        if let Some(v) = end_scale {
            st.end_scale = v.clamp(1.0, 1000.0);
        }
        if let Some(b) = align_dashes {
            st.align_dashes = b;
        }
    })?;
    ok()
}

fn fit(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let mode = args::str_(p, "mode", "fillProportionally, fitProportionally, fitContentToFrame, centerContent or fitFrameToContent")?.to_string();
    c.label(match mode.as_str() {
        "fillProportionally" => "Fill Frame Proportionally",
        "fitProportionally" => "Fit Content Proportionally",
        "fitContentToFrame" => "Fit Content to Frame",
        "centerContent" => "Center Content",
        _ => "Fit Frame to Content",
    });
    let doc = c.doc_mut()?;
    for id in doc.walk(&ids) {
        fit_content(doc, id, &mode, None)?;
    }
    ok()
}

fn fitting_options(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let auto = args::opt_bool(p, "autoFit");
    let kind = args::opt_str(p, "fitting").map(str::to_string);
    let align = args::opt_u64(p, "align");
    let doc = c.doc_mut()?;
    for id in &ids {
        let Some(ItemKind::Graphic(g)) = doc.item_mut(*id).map(|i| &mut i.kind) else { continue };
        let f = g.fitting.get_or_insert_with(|| Fitting { kind: "none".into(), auto: false, reference: "4".into() });
        if let Some(a) = auto {
            f.auto = a;
        }
        if let Some(k) = &kind {
            f.kind = k.clone();
        }
        if let Some(r) = align {
            f.reference = r.min(8).to_string();
        }
        let (k, a) = (f.kind.clone(), f.auto);
        if a && k != "none" {
            fit_content(doc, *id, &k, None)?;
        }
    }
    ok()
}

fn gradient(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let radial = args::opt_str(p, "kind") == Some("radial");
    let angle = args::opt_f64(p, "angle").unwrap_or(0.0);
    let stops_in = p.get("stops").and_then(Value::as_array).ok_or_else(|| EngineError::new("missing `stops` ([{location 0..100, color, opacity?}])"))?;
    let mut stops = Vec::new();
    for s in stops_in {
        let color = s.get("color").and_then(wire::color_in).or_else(|| {
            let name = s.get("color")?.as_str()?;
            c.doc().ok()?.swatch_color(name, 1.0, 0)
        });
        let color = color.ok_or_else(|| EngineError::new("each gradient stop needs a `color` (\"#rrggbb\", a swatch name or a colour object)"))?;
        stops.push(Stop { at: s.get("location").and_then(Value::as_f64).unwrap_or(0.0) / 100.0, color, opacity: s.get("opacity").and_then(Value::as_f64).unwrap_or(100.0) / 100.0 });
    }
    if stops.len() < 2 {
        return Err(EngineError::new("a gradient needs at least two `stops`"));
    }
    stops.sort_by(|a, b| a.at.total_cmp(&b.at));
    let g = Gradient { radial, stops, angle };
    let stroke = args::opt_str(p, "target") == Some("stroke");
    set_each(c, &ids, |it| {
        if stroke {
            it.stroke.paint = Paint::Gradient { gradient: g.clone() };
        } else {
            it.fill = Paint::Gradient { gradient: g.clone() };
        }
    })?;
    ok()
}

fn group(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let gid = ops::group(c.doc_mut()?, &ids)?;
    c.set_selection(Selection::items(vec![gid]));
    Ok(json!({"id": gid}))
}

fn ungroup(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let mut out = Vec::new();
    let doc = c.doc_mut()?;
    for id in ids {
        if matches!(doc.item(id).map(|i| &i.kind), Some(ItemKind::Group { .. })) {
            out.extend(ops::ungroup(doc, id)?);
        } else {
            out.push(id);
        }
    }
    c.set_selection(Selection::items(out.clone()));
    Ok(json!({"ids": out}))
}

fn label(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let l = p.get("label").and_then(Value::as_str).ok_or_else(|| EngineError::new("missing `label` (the script label text)"))?.to_string();
    set_each(c, &ids, |it| it.label = l.clone())?;
    ok()
}

const BLENDS: &[&str] = &["normal", "multiply", "screen", "overlay", "softLight", "hardLight", "colorDodge", "colorBurn", "darken", "lighten", "difference", "exclusion", "hue", "saturation", "color", "luminosity"];

fn opacity(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let op = args::opt_f64(p, "opacity");
    let blend = match args::opt_str(p, "blend") {
        Some(b) => Some(BLENDS.iter().find(|x| x.eq_ignore_ascii_case(b)).ok_or_else(|| EngineError::new(format!("`blend` expected one of {}", BLENDS.join(", "))))?.to_string()),
        None => None,
    };
    if op.is_none() && blend.is_none() {
        return Err(EngineError::new("missing `opacity` (0 to 1)"));
    }
    set_each(c, &ids, |it| {
        if let Some(o) = op {
            it.opacity = o.clamp(0.0, 1.0);
        }
        if let Some(b) = &blend {
            it.blend = b.clone();
        }
    })?;
    ok()
}

fn rename(c: &mut Ctx, p: &Value) -> Result<Value> {
    let id = one_id(p, "id").or_else(|| c.selection().items.first().copied()).ok_or_else(|| EngineError::new("missing `id` (the object to rename)"))?;
    let name = args::str_(p, "name", "the new name")?.to_string();
    if c.doc()?.item(id).is_none() {
        return Err(EngineError::new(format!("no object with id {id}")));
    }
    set_each(c, &[id], |it| it.name = name.clone())?;
    ok()
}

fn set_flags(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let hidden = args::opt_bool(p, "hidden");
    let locked = args::opt_bool(p, "locked");
    c.label(match (hidden, locked) {
        (Some(true), _) => "Hide",
        (Some(false), _) => "Show",
        (_, Some(true)) => "Lock",
        _ => "Unlock",
    });
    set_each(c, &ids, |it| {
        if let Some(h) = hidden {
            it.hidden = h;
        }
        if let Some(l) = locked {
            it.locked = l;
        }
    })?;
    if hidden == Some(true) {
        let mut sel = c.selection();
        sel.items.retain(|i| !ids.contains(i));
        c.set_selection(sel);
    }
    ok()
}

fn set_layer(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let layer = match args::opt_u64(p, "layer") {
        Some(l) => LayerId(l),
        None => c.target_layer()?,
    };
    if c.doc()?.layer(layer).is_none() {
        return Err(EngineError::new(format!("no layer with id {layer}")));
    }
    let doc = c.doc_mut()?;
    for id in doc.walk(&ids) {
        if let Some(it) = doc.item_mut(id) {
            it.layer = layer;
        }
    }
    ok()
}

fn text_frame_options(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let columns = args::opt_u64(p, "columns").map(|n| n.clamp(1, 40) as u32);
    let gutter = args::opt_f64(p, "gutter");
    let inset = p.get("inset").and_then(|v| match v {
        Value::Number(n) => n.as_f64().map(Edges::all),
        Value::Array(a) if a.len() == 4 => {
            let n: Vec<f64> = a.iter().filter_map(Value::as_f64).collect();
            (n.len() == 4).then(|| Edges { top: n[0], inside: n[1], bottom: n[2], outside: n[3] })
        }
        _ => None,
    });
    let vj = args::opt_str(p, "verticalJustification").map(|v| match v {
        "center" => VJust::Center,
        "bottom" => VJust::Bottom,
        "justify" => VJust::Justify,
        _ => VJust::Top,
    });
    let autosize = args::opt_str(p, "autoSize").map(|v| match v {
        "heightOnly" | "height" => AutoSize::Height,
        "widthOnly" | "width" => AutoSize::Width,
        "heightAndWidth" | "heightAndWidthProportionally" | "both" => AutoSize::Both,
        _ => AutoSize::Off,
    });
    let ignore = args::opt_bool(p, "ignoreWrap");
    let balance = args::opt_bool(p, "balanceColumns");
    let vertical = match args::opt_str(p, "orientation") {
        None => args::opt_bool(p, "vertical"),
        Some("vertical") => Some(true),
        Some("horizontal") => Some(false),
        Some(_) => return Err(EngineError::new("`orientation` expected horizontal or vertical")),
    };
    let fixed = args::opt_f64(p, "fixedColumnWidth");
    let rule = match args::opt_str(p, "columnRule") {
        None => None,
        Some("fixedNumber" | "number") => Some(ColumnRule::Number),
        Some("fixedWidth" | "fixed") => Some(ColumnRule::Fixed),
        Some("flexibleWidth" | "flexible") => Some(ColumnRule::Flexible),
        Some(_) => return Err(EngineError::new("`columnRule` expected fixedNumber, fixedWidth or flexibleWidth")),
    };
    let col_width = args::opt_f64(p, "columnWidth").map(|w| w.max(1.0));
    let max_width = args::opt_f64(p, "maxColumnWidth").map(|w| w.max(1.0));
    let doc = c.doc_mut()?;
    for id in doc.walk(&ids) {
        let Some(it) = doc.item_mut(id) else { continue };
        let bb = it.shape.bbox();
        let Some(t) = it.text_mut() else { continue };
        let inner_w = (bb.width() - t.insets.inside - t.insets.outside).max(1.0);
        let cur_w = (inner_w - t.gutter * (t.columns.max(1) as f64 - 1.0)) / t.columns.max(1) as f64;
        if let Some(r) = rule {
            t.column_rule = r;
            if r == ColumnRule::Fixed && t.fixed_column_width.is_none() && col_width.is_none() {
                t.fixed_column_width = Some(cur_w.max(1.0));
            }
            if r == ColumnRule::Flexible && t.max_column_width <= 0.0 && max_width.is_none() {
                t.max_column_width = cur_w.max(1.0);
            }
        }
        if let Some(w) = col_width {
            t.fixed_column_width = Some(w);
        }
        if let Some(w) = max_width {
            t.max_column_width = w;
        }
        if let Some(n) = columns {
            t.columns = n;
        }
        if let Some(g) = gutter {
            t.gutter = g.max(0.0);
        }
        if let Some(e) = inset {
            t.insets = e;
        }
        if let Some(v) = vj {
            t.vjust = v;
        }
        if let Some(a) = autosize {
            t.autosize = a;
        }
        if let Some(i) = ignore {
            t.ignore_wrap = i;
        }
        if let Some(b) = balance {
            t.balance = b;
        }
        if let Some(v) = vertical {
            t.vertical = v;
        }
        if let Some(f) = fixed {
            t.fixed_column_width = (f > 0.0).then_some(f);
        }
        let new_w = if t.column_rule == ColumnRule::Fixed && (columns.is_some() || col_width.is_some() || gutter.is_some() || inset.is_some() || rule.is_some()) {
            let n = if columns.is_some() { t.columns } else { t.column_plan(inner_w).0 };
            t.columns = n;
            t.fitted_width(n)
        } else {
            None
        };
        let n_now = t.column_plan(new_w.map(|w| w - t.insets.inside - t.insets.outside).unwrap_or(inner_w)).0;
        t.columns = n_now;
        if let Some(w) = new_w {
            it.shape = it.shape.with_rect(Rect::new(bb.x0, bb.y0, bb.x0 + w, bb.y1));
        }
    }
    ok()
}

fn text_wrap(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let mode = match args::opt_str(p, "mode").unwrap_or("boundingBox") {
        "none" => None,
        "contour" => Some(WrapMode::Contour),
        "jumpObject" => Some(WrapMode::JumpObject),
        "jumpToNextColumn" | "nextColumn" => Some(WrapMode::NextColumn),
        _ => Some(WrapMode::BoundingBox),
    };
    let offset = args::opt_f64(p, "offset");
    let invert = args::opt_bool(p, "invert");
    set_each(c, &ids, |it| match mode {
        None => it.wrap = None,
        Some(m) => {
            let w = it.wrap.get_or_insert_with(|| TextWrap { mode: m, offsets: Edges::default(), invert: false, side: "both".into() });
            w.mode = m;
            if let Some(o) = offset {
                w.offsets = Edges::all(o);
            }
            if let Some(i) = invert {
                w.invert = i;
            }
        }
    })?;
    ok()
}

fn flip(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let b = sel_bounds(c, &ids)?;
    let vertical = args::opt_str(p, "axis") == Some("vertical");
    c.label(if vertical { "Flip Vertical" } else { "Flip Horizontal" });
    let m = about(b.center(), if vertical { Affine::scale_non_uniform(1.0, -1.0) } else { Affine::scale_non_uniform(-1.0, 1.0) });
    ops::transform_items(c.doc_mut()?, &ids, m);
    ok()
}

fn info(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let doc = c.doc()?;
    let (rotation, sx, sy, shear) = decompose(doc.world_xf(ids[0]));
    let b = ops::union_bounds(doc, &ids).unwrap_or(Rect::ZERO);
    Ok(json!({"rotation": rotation, "scaleX": sx, "scaleY": sy, "shear": shear, "bounds": formdes_model::geom::rect_json(b)}))
}

fn move_(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let d = Vec2::new(args::opt_f64(p, "dx").unwrap_or(0.0), args::opt_f64(p, "dy").unwrap_or(0.0));
    if p.get("dx").is_none() && p.get("dy").is_none() {
        return Err(EngineError::new("missing `dx` or `dy` (points to move by)"));
    }
    ops::move_items(c.doc_mut()?, &ids, d);
    ok()
}

fn resize(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let to = args::rect_(p, "to", "the wanted bounds")?;
    let from = match p.get("from").and_then(args::rect) {
        Some(r) => r,
        None => sel_bounds(c, &ids)?,
    };
    let content = args::opt_bool(p, "content").unwrap_or(false);
    let m = formdes_model::item::map_rect(from, to);
    resize_items(c.doc_mut()?, &ids, m, content);
    ok()
}

fn rotate(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let a = args::f64_(p, "angle", "degrees, positive turns counter-clockwise")?;
    let b = sel_bounds(c, &ids)?;
    let pivot = match args::opt_u64(p, "ref") {
        Some(r) => ref_point(b, r),
        None => b.center(),
    };
    ops::transform_items(c.doc_mut()?, &ids, about(pivot, Affine::rotate(-a.to_radians())));
    ok()
}

fn scale(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let sx = args::opt_f64(p, "sx").or_else(|| args::opt_f64(p, "scale")).ok_or_else(|| EngineError::new("missing `sx` (a factor, 1 = 100%)"))?;
    let sy = args::opt_f64(p, "sy").unwrap_or(sx);
    if sx.abs() < 1e-6 || sy.abs() < 1e-6 {
        return Err(EngineError::new("`sx` and `sy` must not be 0"));
    }
    let b = sel_bounds(c, &ids)?;
    let pivot = match args::opt_u64(p, "ref") {
        Some(r) => ref_point(b, r),
        None => b.center(),
    };
    ops::transform_items(c.doc_mut()?, &ids, about(pivot, Affine::scale_non_uniform(sx, sy)));
    ok()
}

fn transform_set(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let refp = args::opt_u64(p, "ref").unwrap_or(0);
    if let Some(rot) = args::opt_f64(p, "rotation").or_else(|| args::opt_f64(p, "shear").map(|_| f64::NAN)) {
        let doc = c.doc()?;
        let world = doc.world_xf(ids[0]);
        let (r0, sx, sy, sh0) = decompose(world);
        let rot = if rot.is_nan() { r0 } else { rot };
        let sh = args::opt_f64(p, "shear").unwrap_or(sh0);
        let b = sel_bounds(c, &ids)?;
        let pivot = ref_point(b, refp);
        let old = Affine::new({
            let k = world.as_coeffs();
            [k[0], k[1], k[2], k[3], 0.0, 0.0]
        });
        let new = compose(rot, sx, sy, sh);
        let d = about(pivot, new * old.inverse());
        ops::transform_items(c.doc_mut()?, &ids, d);
        c.label(if p.get("rotation").is_some() { "Rotate" } else { "Shear" });
    }
    if p.get("width").is_some() || p.get("height").is_some() {
        let b = sel_bounds(c, &ids)?;
        let w = args::opt_f64(p, "width").unwrap_or(b.width()).max(0.01);
        let h = args::opt_f64(p, "height").unwrap_or(b.height()).max(0.01);
        let pivot = ref_point(b, refp);
        let sx = if b.width() > 1e-9 { w / b.width() } else { 1.0 };
        let sy = if b.height() > 1e-9 { h / b.height() } else { 1.0 };
        resize_items(c.doc_mut()?, &ids, about(pivot, Affine::scale_non_uniform(sx, sy)), false);
        c.label("Resize");
    }
    if p.get("x").is_some() || p.get("y").is_some() {
        let doc = c.doc()?;
        let b = sel_bounds(c, &ids)?;
        let page = doc.spread_of(ids[0]).map(|si| {
            let s = &doc.spreads[si];
            let pi = s.nearest_page(b.center().x);
            Point::new(s.page_x(pi), 0.0)
        });
        let origin = page.unwrap_or(Point::ZERO);
        let cur = ref_point(b, refp);
        let x = args::opt_f64(p, "x").map(|x| origin.x + x).unwrap_or(cur.x);
        let y = args::opt_f64(p, "y").map(|y| origin.y + y).unwrap_or(cur.y);
        ops::move_items(c.doc_mut()?, &ids, Vec2::new(x - cur.x, y - cur.y));
        c.label("Move");
    }
    ok()
}

pub fn spread_home(doc: &Doc, v: Option<&Value>) -> Result<Home> {
    match v {
        None | Some(Value::Null) => Ok(Home::Spread(0)),
        Some(Value::Object(o)) => {
            let idx = o.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
            if o.get("kind").and_then(Value::as_str) == Some("parent") {
                if idx >= doc.parents.len() {
                    return Err(EngineError::new(format!("no parent spread {idx} (the document has {})", doc.parents.len())));
                }
                Ok(Home::Parent(idx))
            } else {
                if idx >= doc.spreads.len() {
                    return Err(EngineError::new(format!("no spread {idx} (the document has {}, indices are 0-based)", doc.spreads.len())));
                }
                Ok(Home::Spread(idx))
            }
        }
        Some(v) => {
            let idx = v.as_u64().ok_or_else(|| EngineError::new("`spread` expected a spread index or {kind: \"parent\", index}"))? as usize;
            if idx >= doc.spreads.len() {
                return Err(EngineError::new(format!("no spread {idx} (the document has {}, indices are 0-based)", doc.spreads.len())));
            }
            Ok(Home::Spread(idx))
        }
    }
}

pub fn put_text(st: &mut formdes_model::story::Story, pos: usize, text: &str) {
    let mut at = pos;
    let mut buf = String::new();
    let flush = |st: &mut formdes_model::story::Story, at: &mut usize, buf: &mut String| {
        if !buf.is_empty() {
            st.insert_text(*at, buf);
            *at += buf.chars().count();
            buf.clear();
        }
    };
    for ch in text.chars() {
        let marker = match ch {
            '\u{E000}' => Some(Marker::PageNumber),
            '\u{E007}' => Some(Marker::NextPageNumber),
            '\u{E008}' => Some(Marker::PrevPageNumber),
            _ => None,
        };
        match marker {
            Some(m) => {
                flush(st, &mut at, &mut buf);
                st.insert_inline(at, Inline::Marker { marker: m });
                at += 1;
            }
            None => buf.push(if ch == '\r' { '\n' } else { ch }),
        }
    }
    flush(st, &mut at, &mut buf);
}

fn frame_create(c: &mut Ctx, p: &Value) -> Result<Value> {
    let home = spread_home(c.doc()?, p.get("spread"))?;
    let rect = args::rect_(p, "rect", "the frame's bounds in spread points")?.abs();
    let content = args::opt_str(p, "content").unwrap_or("unassigned").to_string();
    let shape_kind = args::opt_str(p, "shape").unwrap_or("rectangle").to_string();
    let mut shape = ops::shape_for(&shape_kind, rect);
    if let (Shape::Polygon { sides, .. }, Some(n)) = (&mut shape, args::opt_u64(p, "sides")) {
        *sides = n.clamp(3, 100) as u32;
    }
    let text = args::opt_str(p, "text").map(str::to_string);
    let vertical = args::opt_str(p, "orientation") == Some("vertical") || args::opt_bool(p, "vertical") == Some(true);
    let layer = c.target_layer()?;
    let doc = c.doc_mut()?;
    let (kind, story, style) = match content.as_str() {
        "text" => {
            let sid = doc.new_story(formdes_model::style::BASIC_PARA);
            if let (Some(t), Some(st)) = (&text, doc.story_mut(sid)) {
                put_text(st, 0, t);
            }
            let mut tf = TextFrame::new(sid);
            tf.vertical = vertical;
            (ItemKind::Text(tf), Some(sid), Some(formdes_model::style::BASIC_TEXT))
        }
        "graphic" | "image" => (ItemKind::Graphic(Graphic { asset: None, content: Affine::IDENTITY, fitting: None }), None, Some(formdes_model::style::BASIC_GRAPHIC)),
        _ => (ItemKind::Plain, None, None),
    };
    let mut it = Item::new(ItemId(0), kind, shape, layer);
    if content == "unassigned" || content == "shape" {
        it.stroke = ops::default_stroke(1.0);
    }
    it.style = style.map(str::to_string);
    let id = doc.insert_item(it, home);
    if let Some(sid) = story
        && let Some(st) = doc.story_mut(sid)
    {
        st.frames.push(id);
    }
    c.set_selection(Selection::items(vec![id]));
    Ok(json!({"id": id}))
}

fn point_of(p: &Value, k: &str) -> Result<(f64, f64)> {
    let a = p.get(k).and_then(Value::as_array).filter(|a| a.len() >= 2).ok_or_else(|| EngineError::new(format!("missing `{k}` (a point [x, y] in spread points)")))?;
    let n = |i: usize| a[i].as_f64().ok_or_else(|| EngineError::new(format!("`{k}` expected two numbers")));
    Ok((n(0)?, n(1)?))
}

fn line_create(c: &mut Ctx, p: &Value) -> Result<Value> {
    let home = spread_home(c.doc()?, p.get("spread"))?;
    let (from, to) = (point_of(p, "a")?, point_of(p, "b")?);
    let layer = c.target_layer()?;
    let weight = args::opt_f64(p, "weight").unwrap_or(1.0).max(0.0);
    let doc = c.doc_mut()?;
    let mut it = Item::new(ItemId(0), ItemKind::Plain, Shape::Line { from, to }, layer);
    it.stroke = ops::default_stroke(weight);
    let id = doc.insert_item(it, home);
    c.set_selection(Selection::items(vec![id]));
    Ok(json!({"id": id}))
}

fn thread(c: &mut Ctx, p: &Value) -> Result<Value> {
    let from = one_id(p, "from").ok_or_else(|| EngineError::new("missing `from` (a text frame id)"))?;
    let to = one_id(p, "to").ok_or_else(|| EngineError::new("missing `to` (a text or empty frame id)"))?;
    if c.doc()?.item(to).is_none() {
        return Err(EngineError::new(format!("no object with id {to}")));
    }
    ops::thread(c.doc_mut()?, from, to)?;
    c.set_selection(Selection::items(vec![to]));
    c.label("Thread Text Frames");
    Ok(json!({"from": from, "to": to}))
}

fn unthread(c: &mut Ctx, p: &Value) -> Result<Value> {
    let id = one_id(p, "id").ok_or_else(|| EngineError::new("missing `id` (the frame to break the thread after)"))?;
    ops::unthread_after(c.doc_mut()?, id)?;
    c.label("Break Thread");
    Ok(json!({"id": id}))
}

fn clear(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let doc = c.doc_mut()?;
    for id in &ids {
        doc.remove_item(*id);
    }
    c.set_selection(Selection::default());
    ok()
}

fn duplicate(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let off = Vec2::new(args::opt_f64(p, "dx").unwrap_or(12.0), args::opt_f64(p, "dy").unwrap_or(12.0));
    let doc = c.doc_mut()?;
    let out: Vec<ItemId> = ids.iter().filter_map(|id| ops::duplicate(doc, *id, off, None)).collect();
    c.set_selection(Selection::items(out.clone()));
    Ok(json!({"ids": out, "id": out.first()}))
}

fn select_all(c: &mut Ctx, _: &Value) -> Result<Value> {
    let st = c.state()?;
    let doc = &st.doc;
    let ids: Vec<ItemId> = if st.editing_parents {
        doc.parents.iter().flat_map(|p| p.items.clone()).collect()
    } else {
        doc.spreads.iter().flat_map(|s| s.items.clone()).collect()
    };
    let ids: Vec<ItemId> = ids.into_iter().filter(|i| doc.item(*i).is_some_and(|it| !it.locked && !it.hidden && doc.layer(it.layer).is_none_or(|l| l.visible && !l.locked))).collect();
    c.set_selection(Selection::items(ids.clone()));
    Ok(json!({"items": ids}))
}

fn states_create(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    if ids.len() < 2 {
        return Err(EngineError::new("needs two or more selected objects to make states"));
    }
    let doc = c.doc_mut()?;
    let home = doc.home(ids[0]).ok_or_else(|| EngineError::new("no such object"))?;
    let layer = doc.item(ids[0]).map(|i| i.layer).unwrap_or(LayerId(0));
    let list = ops::home_list(doc, &home).cloned().unwrap_or_default();
    let pos = list.iter().position(|x| ids.contains(x)).unwrap_or(list.len());
    let sid = ItemId(doc.alloc());
    let states: Vec<State> = ids.iter().enumerate().map(|(k, i)| State { name: format!("State {}", k + 1), items: vec![*i] }).collect();
    for id in &ids {
        doc.detach_from_home(*id);
    }
    let mut it = Item::new(sid, ItemKind::States { states, active: 0 }, Shape::Rect { rect: Rect::ZERO }, layer);
    it.name = "Multi-state".into();
    doc.items.insert(sid, Arc::new(it));
    if let Some(l) = ops::home_list(doc, &home) {
        l.insert(pos.min(l.len()), sid);
    }
    for id in &ids {
        if let Some(x) = doc.item_mut(*id) {
            x.group = Some(sid);
        }
    }
    c.set_selection(Selection::items(vec![sid]));
    Ok(json!({"id": sid}))
}

fn states_of(c: &Ctx, p: &Value) -> Result<(ItemId, Vec<State>, usize)> {
    let id = one_id(p, "id").or_else(|| c.selection().items.first().copied()).ok_or_else(|| EngineError::new("missing `id` (a multi-state object)"))?;
    match c.doc()?.item(id).map(|i| &i.kind) {
        Some(ItemKind::States { states, active }) => Ok((id, states.clone(), *active)),
        _ => Err(EngineError::new(format!("object {id} is not a multi-state object"))),
    }
}

fn states_list(c: &mut Ctx, p: &Value) -> Result<Value> {
    let (_, states, active) = states_of(c, p)?;
    Ok(json!({"states": states.iter().map(|s| s.name.clone()).collect::<Vec<_>>(), "active": active}))
}

fn states_show(c: &mut Ctx, p: &Value) -> Result<Value> {
    let (id, states, _) = states_of(c, p)?;
    let i = args::u64_(p, "index", "the state index, from 0")? as usize;
    if i >= states.len() {
        return Err(EngineError::new(format!("no state {i} (the object has {})", states.len())));
    }
    if let Some(ItemKind::States { active, .. }) = c.doc_mut()?.item_mut(id).map(|x| &mut x.kind) {
        *active = i;
    }
    ok()
}

fn states_rename(c: &mut Ctx, p: &Value) -> Result<Value> {
    let (id, states, _) = states_of(c, p)?;
    let i = args::u64_(p, "index", "the state index, from 0")? as usize;
    let name = args::str_(p, "name", "the new state name")?.to_string();
    if i >= states.len() {
        return Err(EngineError::new(format!("no state {i}")));
    }
    if let Some(ItemKind::States { states, .. }) = c.doc_mut()?.item_mut(id).map(|x| &mut x.kind) {
        states[i].name = name;
    }
    ok()
}

fn states_release(c: &mut Ctx, p: &Value) -> Result<Value> {
    let (id, states, _) = states_of(c, p)?;
    let doc = c.doc_mut()?;
    let home = doc.home(id).ok_or_else(|| EngineError::new("no such object"))?;
    let xf = doc.item(id).map(|i| i.xf).unwrap_or(Affine::IDENTITY);
    let pos = ops::home_list(doc, &home).and_then(|l| l.iter().position(|x| *x == id)).unwrap_or(0);
    let kids: Vec<ItemId> = states.iter().flat_map(|s| s.items.clone()).collect();
    for k in &kids {
        if let Some(it) = doc.item_mut(*k) {
            it.xf = xf * it.xf;
            it.group = if let Home::Group(g) = home { Some(g) } else { None };
        }
    }
    if let Some(l) = ops::home_list(doc, &home) {
        l.retain(|x| *x != id);
        for (k, kid) in kids.iter().enumerate() {
            l.insert((pos + k).min(l.len()), *kid);
        }
    }
    doc.items.remove(&id);
    c.set_selection(Selection::items(kids.clone()));
    Ok(json!({"ids": kids}))
}

fn extra_obj(it: &Item, key: &str) -> Option<Map<String, Value>> {
    it.extra.get(key).and_then(Value::as_object).cloned()
}

fn media_get(c: &mut Ctx, p: &Value) -> Result<Value> {
    let id = one_id(p, "id").or_else(|| c.selection().items.first().copied()).ok_or_else(|| EngineError::new("missing `id` (a media frame)"))?;
    let doc = c.doc()?;
    let it = doc.item(id).ok_or_else(|| EngineError::new(format!("no object with id {id}")))?;
    let asset = it.graphic().and_then(|g| g.asset).and_then(|a| doc.assets.get(&a)).filter(|a| a.is_media());
    let Some(a) = asset else { return Ok(Value::Null) };
    let o = extra_obj(it, "media").unwrap_or_default();
    let b = |k: &str, d: bool| o.get(k).and_then(Value::as_bool).unwrap_or(d);
    Ok(json!({"kind": if a.mime.starts_with("audio/") { "audio" } else { "video" }, "name": a.name, "playOnPageLoad": b("playOnPageLoad", false), "loop": b("loop", false), "controls": b("controls", true)}))
}

fn media_options(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let mut set = Map::new();
    for k in ["playOnPageLoad", "loop", "controls"] {
        if let Some(v) = args::opt_bool(p, k) {
            set.insert(k.into(), json!(v));
        }
    }
    set_each(c, &ids, |it| {
        let mut o = extra_obj(it, "media").unwrap_or_default();
        o.extend(set.clone());
        it.extra.insert("media".into(), Value::Object(o));
    })?;
    ok()
}

fn button_list(c: &mut Ctx, _: &Value) -> Result<Value> {
    let doc = c.doc()?;
    let out: Vec<Value> = doc
        .items
        .values()
        .filter_map(|it| {
            let b = extra_obj(it, "button")?;
            Some(json!({"id": it.id, "name": b.get("name").cloned().unwrap_or_else(|| json!(it.display_name())), "action": b.get("action").cloned().unwrap_or(json!({"kind": "goToNextPage"}))}))
        })
        .collect();
    Ok(Value::Array(out))
}

fn button_set(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let action = args::str_(p, "action", "page, firstPage, lastPage, nextPage, previousPage or url")?;
    let act = match action {
        "page" => {
            let n = args::u64_(p, "page", "a 1-based page number")?;
            let count = c.doc()?.page_count() as u64;
            if n == 0 || n > count {
                return Err(EngineError::new(format!("no page {n} (pages are numbered 1 to {count})")));
            }
            json!({"kind": "goToPage", "page": n - 1})
        }
        "firstPage" => json!({"kind": "goToFirstPage"}),
        "lastPage" => json!({"kind": "goToLastPage"}),
        "nextPage" => json!({"kind": "goToNextPage"}),
        "previousPage" => json!({"kind": "goToPreviousPage"}),
        "url" => json!({"kind": "goToUrl", "url": args::str_(p, "url", "the web address")?}),
        other => return Err(EngineError::new(format!("`action` expected page, firstPage, lastPage, nextPage, previousPage or url, not {other}"))),
    };
    let count = c.doc()?.items.values().filter(|i| i.extra.contains_key("button")).count();
    let name = args::opt_str(p, "name").map(str::to_string);
    let doc = c.doc_mut()?;
    for (k, id) in ids.iter().enumerate() {
        if let Some(it) = doc.item_mut(*id) {
            let mut b = extra_obj(it, "button").unwrap_or_default();
            if let Some(n) = &name {
                b.insert("name".into(), json!(n));
            }
            b.entry("name").or_insert_with(|| json!(format!("Button {}", count + k + 1)));
            b.insert("action".into(), act.clone());
            it.extra.insert("button".into(), Value::Object(b));
        }
    }
    ok()
}

fn extra_clear(c: &mut Ctx, p: &Value, key: &'static str) -> Result<Value> {
    let ids = items(c, p)?;
    set_each(c, &ids, |it| {
        it.extra.remove(key);
    })?;
    ok()
}

fn button_clear(c: &mut Ctx, p: &Value) -> Result<Value> {
    extra_clear(c, p, "button")
}

fn form_clear(c: &mut Ctx, p: &Value) -> Result<Value> {
    extra_clear(c, p, "form")
}

fn form_list(c: &mut Ctx, _: &Value) -> Result<Value> {
    let doc = c.doc()?;
    let out: Vec<Value> = doc
        .items
        .values()
        .filter_map(|it| {
            let mut f = extra_obj(it, "form")?;
            f.insert("id".into(), json!(it.id));
            f.entry("options").or_insert(json!([]));
            f.entry("value").or_insert(json!(""));
            Some(Value::Object(f))
        })
        .collect();
    Ok(Value::Array(out))
}

const FORM_KINDS: &[&str] = &["textField", "checkBox", "comboBox", "listBox", "signature", "radioButton"];

fn form_set(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    if let Some(k) = args::opt_str(p, "kind")
        && !FORM_KINDS.contains(&k)
    {
        return Err(EngineError::new(format!("`kind` expected one of {}", FORM_KINDS.join(", "))));
    }
    let mut set = Map::new();
    for k in ["kind", "name", "value", "options", "multiline", "required", "fontSize"] {
        if let Some(v) = p.get(k).filter(|v| !v.is_null()) {
            set.insert(k.into(), v.clone());
        }
    }
    set_each(c, &ids, |it| {
        let mut f = extra_obj(it, "form").unwrap_or_default();
        f.extend(set.clone());
        f.entry("kind").or_insert(json!("textField"));
        f.entry("name").or_insert_with(|| json!(format!("field{}", it.id)));
        it.extra.insert("form".into(), Value::Object(f));
    })?;
    ok()
}

pub fn register(r: &mut Registry) {
    cmd("object.align", "Align").menu(&["Object", "Align"]).needs(Needs::Items).sig("{edge: left|hcenter|right|top|vcenter|bottom, to?: selection|keyObject|margins|page|spread, ids?}").reg(r, align);
    cmd("object.distribute", "Distribute").menu(&["Object", "Distribute"]).needs(Needs::Items).sig("{axis: horizontal|vertical, by: centers|spacing, ids?}").reg(r, distribute);
    cmd("object.arrange", "Arrange").menu(&["Object", "Arrange"]).needs(Needs::Items).sig("{to: front|forward|backward|back, ids?}").reg(r, arrange);
    cmd("object.attributes", "Attributes").needs(Needs::Items).sig("{nonprinting?, overprintFill?, overprintStroke?, ids?}").reg(r, attributes);
    cmd("object.color", "Apply Color").needs(Needs::Items).sig("{color: \"#rrggbb\", target?: fill|stroke, ids?}").reg(r, color);
    cmd("object.cornerOptions", "Corner Options").menu(&["Object", "Corner Options..."]).needs(Needs::Items).sig("{shape?: rounded|inverseRounded|inset|bevel|fancy|none, size: pt, ids?}").reg(r, corner_options);
    cmd("object.dropShadow", "Drop Shadow").menu(&["Object", "Effects", "Drop Shadow..."]).needs(Needs::Items).sig("{on?, mode?: normal|multiply|screen|overlay|darken|lighten|colorDodge|colorBurn|hardLight|softLight|difference|exclusion, opacity?: 0..100, distance?: pt, angle?: deg, size?: pt, spread?: 0..100, color?, ids?}").reg(r, drop_shadow);
    cmd("object.innerShadow", "Inner Shadow").menu(&["Object", "Effects", "Inner Shadow..."]).needs(Needs::Items).sig("{on?, mode?: normal|multiply|screen|overlay|darken|lighten|colorDodge|colorBurn|hardLight|softLight|difference|exclusion, opacity?: 0..100, distance?: pt, angle?: deg, size?: pt, choke?: 0..100, color?, ids?}").reg(r, inner_shadow);
    cmd("object.outerGlow", "Outer Glow").menu(&["Object", "Effects", "Outer Glow..."]).needs(Needs::Items).sig("{on?, mode?: normal|multiply|screen|overlay|darken|lighten|colorDodge|colorBurn|hardLight|softLight|difference|exclusion, opacity?: 0..100, size?: pt, spread?: 0..100, technique?: softer|precise, color?, ids?}").reg(r, outer_glow);
    cmd("object.innerGlow", "Inner Glow").menu(&["Object", "Effects", "Inner Glow..."]).needs(Needs::Items).sig("{on?, mode?: normal|multiply|screen|overlay|darken|lighten|colorDodge|colorBurn|hardLight|softLight|difference|exclusion, opacity?: 0..100, size?: pt, choke?: 0..100, color?, ids?}").reg(r, inner_glow);
    cmd("object.basicFeather", "Basic Feather").menu(&["Object", "Effects", "Basic Feather..."]).needs(Needs::Items).sig("{on?, width?: pt, choke?: 0..100, corners?: sharp|rounded|diffusion, ids?}").reg(r, basic_feather);
    cmd("object.gradientFeather", "Gradient Feather").menu(&["Object", "Effects", "Gradient Feather..."]).needs(Needs::Items).sig("{on?, type?: linear|radial, angle?: deg, stops?: [{opacity: 0..100, location: 0..100}], ids?}").reg(r, gradient_feather);
    cmd("object.directionalFeather","Directional Feather").menu(&["Object", "Effects", "Directional Feather..."]).needs(Needs::Items).sig("{on?, top?: pt, bottom?: pt, left?: pt, right?: pt, choke?: 0..100, angle?: deg, ids?}").reg(r, directional_feather);
    cmd("object.fill", "Fill").needs(Needs::Items).sig("{swatch, tint?: 0..1, ids?}").reg(r, fill);
    cmd("object.stroke", "Stroke").needs(Needs::Items).sig("{swatch?, tint?, weight?: pt, type?: {kind, pattern?}, cap?, join?, miterLimit?: 1..500, align?, start?: none|simple|simpleWide|triangle|triangleWide|barbed|curved|circle|circleSolid|square|squareSolid|bar, end?: same, startScale?: percent, endScale?: percent, alignDashes?: bool, ids?}").help("Sets the stroke: colour, weight, dashes (alignDashes fits the pattern so dashes meet corners and path ends), caps, joins and miter limit, inside or outside alignment, and arrowheads at the start and end of open paths with their size in percent.").reg(r, stroke);
    cmd("object.fit", "Fit").menu(&["Object", "Fitting"]).needs(Needs::Items).sig("{mode: fillProportionally|fitProportionally|fitContentToFrame|centerContent|fitFrameToContent, ids?}").reg(r, fit);
    cmd("object.fittingOptions", "Frame Fitting Options").menu(&["Object", "Fitting", "Frame Fitting Options..."]).needs(Needs::Items).sig("{autoFit?, fitting?, align?: 0..8, ids?}").reg(r, fitting_options);
    cmd("object.gradient", "Gradient").needs(Needs::Items).sig("{kind?: linear|radial, angle?: deg, stops: [{location: 0..100, color, opacity?: 0..100}], target?, ids?}").reg(r, gradient);
    cmd("object.group", "Group").menu(&["Object", "Group"]).key("Mod+G").needs(Needs::Items).sig("{ids?: [id]}").reg(r, group);
    cmd("object.ungroup", "Ungroup").menu(&["Object", "Ungroup"]).key("Mod+Shift+G").needs(Needs::Items).sig("{ids?}").reg(r, ungroup);
    cmd("object.label", "Script Label").needs(Needs::Items).sig("{label, ids?}").reg(r, label);
    cmd("object.opacity", "Transparency").menu(&["Object", "Effects", "Transparency..."]).needs(Needs::Items).sig("{opacity?: 0..1, blend?: normal|multiply|screen|overlay|softLight|hardLight|colorDodge|colorBurn|darken|lighten|difference|exclusion|hue|saturation|color|luminosity, ids?}").reg(r, opacity);
    cmd("object.rename", "Rename").needs(Needs::Items).sig("{id, name}").reg(r, rename);
    cmd("object.setFlags", "Object Flags").needs(Needs::Items).sig("{hidden? | locked?, ids?}").reg(r, set_flags);
    cmd("object.setLayer", "Move to Layer").needs(Needs::Items).sig("{layer: layer id, ids?}").reg(r, set_layer);
    cmd("object.textFrameOptions", "Text Frame Options").menu(&["Object", "Text Frame Options..."]).key("Mod+B").needs(Needs::Items).sig("{columns?, gutter?, inset?: pt|[t,l,b,r], verticalJustification?, autoSize?, ignoreWrap?, balanceColumns?, orientation?: horizontal|vertical (vertical lines run top to bottom, columns right to left), columnRule?: fixedNumber|fixedWidth|flexibleWidth, columnWidth?: pt (fixedWidth), maxColumnWidth?: pt (flexibleWidth), ids?}").reg(r, text_frame_options);
    cmd("object.textWrap", "Text Wrap").menu(&["Object", "Text Wrap..."]).needs(Needs::Items).sig("{mode: none|boundingBox|contour|jumpObject|jumpToNextColumn, offset?: pt, invert?, ids?}").reg(r, text_wrap);
    cmd("object.duplicate", "Duplicate").menu(&["Edit", "Duplicate"]).key("Mod+Alt+Shift+D").needs(Needs::Items).sig("{dx?, dy?, ids?}").reg(r, duplicate);
    cmd("transform.flip", "Flip").menu(&["Object", "Transform"]).needs(Needs::Items).sig("{axis: horizontal|vertical, ids?}").reg(r, flip);
    cmd("transform.info", "Transform Info").needs(Needs::Items).query().sig("{}").reg(r, info);
    cmd("transform.move", "Move").menu(&["Object", "Transform", "Move..."]).key("Mod+Shift+M").needs(Needs::Items).sig("{dx, dy, ids?}").reg(r, move_);
    cmd("transform.resize", "Resize").needs(Needs::Items).sig("{from?: [x0,y0,x1,y1], to: [x0,y0,x1,y1], content?, ids?}").reg(r, resize);
    cmd("transform.rotate", "Rotate").menu(&["Object", "Transform", "Rotate..."]).needs(Needs::Items).sig("{angle: deg, ref?: 0..8, ids?}").reg(r, rotate);
    cmd("transform.scale", "Scale").menu(&["Object", "Transform", "Scale..."]).needs(Needs::Items).sig("{sx, sy?, ref?: 0..8, ids?}").reg(r, scale);
    cmd("transform.set", "Transform").needs(Needs::Items).sig("{x?, y?, width?, height?, rotation?, shear?, ref: 0..8, ids?}").reg(r, transform_set);
    cmd("frame.create", "New Frame").sig("{spread?, rect: [x0,y0,x1,y1], content?: text|graphic|unassigned, text?, orientation?: horizontal|vertical, shape?: rectangle|ellipse|polygon, sides?}").reg(r, frame_create);
    cmd("line.create", "New Line").sig("{spread?, a: [x, y], b: [x, y], weight?}").reg(r, line_create);
    cmd("frame.thread", "Thread Text Frames").sig("{from, to}").reg(r, thread);
    cmd("frame.unthread", "Break Thread").sig("{id}").reg(r, unthread);
    cmd("edit.clear", "Clear").menu(&["Edit", "Clear"]).key("Delete").needs(Needs::Items).sig("{ids?}").reg(r, clear);
    cmd("edit.selectAll", "Select All").menu(&["Edit", "Select All"]).key("Mod+A").query().reg(r, select_all);
    cmd("states.create", "New State").needs(Needs::Items).sig("{ids?}").reg(r, states_create);
    cmd("states.list", "States").needs(Needs::Items).query().sig("{id}").reg(r, states_list);
    cmd("states.show", "Show State").needs(Needs::Items).sig("{id, index}").reg(r, states_show);
    cmd("states.rename", "Rename State").needs(Needs::Items).sig("{id, index, name}").reg(r, states_rename);
    cmd("states.release", "Release State to Objects").needs(Needs::Items).sig("{id}").reg(r, states_release);
    cmd("media.get", "Media").needs(Needs::Items).query().sig("{id}").reg(r, media_get);
    cmd("media.options", "Media Options").needs(Needs::Items).sig("{playOnPageLoad?, loop?, controls?, ids?}").reg(r, media_options);
    cmd("button.list", "Buttons").query().reg(r, button_list);
    cmd("button.set", "Button Options").needs(Needs::Items).sig("{action: page|firstPage|lastPage|nextPage|previousPage|url, page?: 1-based, url?, name?, ids?}").reg(r, button_set);
    cmd("button.clear", "Convert to Object").needs(Needs::Items).sig("{ids?}").reg(r, button_clear);
    cmd("form.list", "Form Fields").query().reg(r, form_list);
    cmd("form.set", "Form Field Options").needs(Needs::Items).sig("{kind?, name?, value?, options?, multiline?, required?, fontSize?, ids?}").reg(r, form_set);
    cmd("form.clear", "Convert to Object").needs(Needs::Items).sig("{ids?}").reg(r, form_clear);
}
