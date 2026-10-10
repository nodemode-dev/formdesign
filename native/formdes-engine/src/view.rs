use formdes_model::geom::{Affine, Rect};
use formdes_render::scene::Target;
use formdes_render::view::{Placed, ViewOpts, render_view};
use formdes_render::{RasterOpts, SceneOpts, encode_jpeg, encode_png, page_scene, rasterize};
use serde_json::{Value, json};

use crate::canvas::SpreadRef;
use crate::session::Session;
use crate::{EngineError, Result};

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn rect_json(r: Rect) -> Value {
    json!([r2(r.x0), r2(r.y0), r2(r.x1), r2(r.y1)])
}

pub fn layout_json(ses: &Session) -> Value {
    let Some(st) = ses.active() else { return json!({"slots": [], "pages": [], "pasteboard": [-500, -500, 500, 500]}) };
    let doc = &st.doc;
    let cv = st.canvas();
    let slots: Vec<Value> = cv
        .slots
        .iter()
        .map(|s| {
            let spread = serde_json::to_value(s.spread).unwrap_or(Value::Null);
            json!({"spread": spread, "bounds": rect_json(s.bounds), "rotation": s.rotation, "xf": s.xf.as_coeffs()})
        })
        .collect();
    let mut pages = Vec::new();
    if !st.editing_parents {
        for abs in 0..doc.page_count() {
            if let Some(r) = cv.page_rect(doc, abs) {
                let (si, pi) = doc.page_slot(abs).unwrap_or((0, 0));
                let parent = doc.page(abs).and_then(|p| p.parent).and_then(|pid| doc.parent(pid)).map(|p| p.prefix.clone());
                pages.push(json!({"abs": abs, "name": doc.page_name(abs), "spread": si, "index": pi, "rect": rect_json(r), "parent": parent}));
            }
        }
    }
    let b = &doc.settings.bleed;
    json!({
        "slots": slots, "pages": pages, "pasteboard": rect_json(cv.pasteboard), "editingParents": st.editing_parents,
        "bleed": [b.top, b.bottom, b.inside, b.outside],
    })
}

pub fn view_opts(p: &Value) -> ViewOpts {
    let preview = p.get("preview").and_then(Value::as_bool).unwrap_or(false);
    let bg = p.get("bg").and_then(Value::as_array).map(|a| {
        let c = |i: usize| a.get(i).and_then(Value::as_u64).unwrap_or(0).min(255) as u8;
        [c(0), c(1), c(2), 255]
    });
    ViewOpts {
        background: bg.unwrap_or([28, 27, 29, 255]),
        paper: true,
        shadow: !preview,
        printing_only: preview,
        greek_px: p.get("greek").and_then(Value::as_f64).unwrap_or(3.0),
        plate: p.get("plate").and_then(Value::as_u64).map(|v| v.min(3) as u8),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum ProofTarget {
    Cmyk,
    Identity,
    MacGamma,
    Protan,
    Deutan,
}

pub struct Proof {
    target: ProofTarget,
    paper: bool,
    squeeze: f32,
}

fn proof_of(p: &Value) -> Result<Option<Proof>> {
    let Some(pr) = p.get("proof").filter(|v| v.is_object()) else { return Ok(None) };
    let name = pr.get("target").and_then(Value::as_str).unwrap_or("workingCmyk");
    let target = match name {
        "workingCmyk" => ProofTarget::Cmyk,
        n if n.starts_with("cmyk:") => ProofTarget::Cmyk,
        "srgb" | "monitorRgb" => ProofTarget::Identity,
        "legacyMacRgb" => ProofTarget::MacGamma,
        "protanopia" => ProofTarget::Protan,
        "deuteranopia" => ProofTarget::Deutan,
        other => return Err(EngineError::new(format!("unknown proof target `{other}`: use workingCmyk, cmyk:<profile>, srgb, legacyMacRgb, monitorRgb, protanopia or deuteranopia"))),
    };
    let intent = pr.get("intent").and_then(Value::as_str).unwrap_or("relative");
    let squeeze = match intent {
        "perceptual" => 0.94,
        "relative" | "saturation" | "absolute" => 1.0,
        other => return Err(EngineError::new(format!("unknown rendering intent `{other}`: use perceptual, relative, saturation or absolute"))),
    };
    let paper = pr.get("simulatePaper").and_then(Value::as_bool).unwrap_or(false) || intent == "absolute";
    Ok(Some(Proof { target, paper, squeeze }))
}

fn lin(v: f32) -> f32 {
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

fn gam(v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.0031308 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
}

fn proof_px(rgb: [f32; 3], pr: &Proof) -> [f32; 3] {
    match pr.target {
        ProofTarget::Identity => rgb,
        ProofTarget::MacGamma => rgb.map(|v| v.powf(2.2 / 1.8)),
        ProofTarget::Protan | ProofTarget::Deutan => {
            let [r, g, b] = rgb.map(lin);
            let m = if pr.target == ProofTarget::Protan { [0.11238, 0.88762, 0.0, 0.11238, 0.88762, 0.0, 0.00401, -0.00401, 1.0] } else { [0.29275, 0.70725, 0.0, 0.29275, 0.70725, 0.0, -0.02234, 0.02234, 1.0] };
            [gam(m[0] * r + m[1] * g + m[2] * b), gam(m[3] * r + m[4] * g + m[5] * b), gam(m[6] * r + m[7] * g + m[8] * b)]
        }
        ProofTarget::Cmyk => {
            let grey = (rgb[0] + rgb[1] + rgb[2]) / 3.0;
            let sq = rgb.map(|v| grey + (v - grey) * pr.squeeze);
            let c = crate::wire::rgb_to_cmyk([sq[0] as f64, sq[1] as f64, sq[2] as f64]);
            let formdes_model::color::Color::Cmyk(mut k) = c else { return rgb };
            let total = k.iter().sum::<f64>();
            if total > 3.0 {
                let over = (total - 3.0) / (k[0] + k[1] + k[2]).max(1e-6);
                for v in k.iter_mut().take(3) {
                    *v *= 1.0 - over;
                }
            }
            let out = formdes_model::color::Color::Cmyk(k).to_rgb();
            let mut o = [out[0] as f32, out[1] as f32, out[2] as f32];
            if pr.paper {
                let paper = [0.965, 0.955, 0.925];
                for i in 0..3 {
                    o[i] = 0.09 + o[i] * (paper[i] - 0.09);
                }
            }
            o
        }
    }
}

fn proof_pixels(rgba: &mut [u8], pr: &Proof, bg: [u8; 4]) {
    for px in rgba.chunks_exact_mut(4) {
        if px[..3] == bg[..3] {
            continue;
        }
        let o = proof_px([px[0] as f32 / 255.0, px[1] as f32 / 255.0, px[2] as f32 / 255.0], pr);
        for i in 0..3 {
            px[i] = (o[i].clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
}

pub fn tiles(ses: &mut Session, p: &Value) -> Result<(Value, Vec<u8>)> {
    let list = p.get("tiles").and_then(Value::as_array).ok_or_else(|| EngineError::new("missing `tiles`"))?;
    let opts = view_opts(p);
    let proof = proof_of(p)?;
    let ink_limit = p.get("inkLimit").and_then(Value::as_f64).filter(|v| *v > 0.0).map(|v| (v / 100.0) as f32);
    let overprint = p.get("overprint").and_then(Value::as_bool).unwrap_or(false);
    let notes = p.get("notes").and_then(Value::as_bool).unwrap_or(false) && !opts.printing_only;
    let changes = p.get("changes").and_then(Value::as_bool).unwrap_or(false) && !opts.printing_only;
    let renderer = ses.renderer.clone();
    let st = ses.active_mut().ok_or_else(|| EngineError::new("no document open"))?;
    let cv = st.canvas();
    let mut placed = Vec::new();
    for s in &cv.slots {
        let (target, pages): (Target, Vec<Rect>) = match s.spread {
            SpreadRef::Doc(i) => {
                let sp = &st.doc.spreads[i];
                (Target::Spread(i), (0..sp.pages.len()).map(|k| Rect::new(sp.page_x(k), 0.0, sp.page_x(k) + sp.pages[k].width, sp.pages[k].height)).collect())
            }
            SpreadRef::Parent(i) => {
                let pa = &st.doc.parents[i];
                (Target::Parent(i), (0..pa.pages.len()).map(|k| Rect::new(pa.page_x(k), 0.0, pa.page_x(k) + pa.pages[k].width, pa.pages[k].height)).collect())
            }
        };
        let mut ops = st.ops(&renderer, target, opts.printing_only, overprint);
        if (notes || changes)
            && let Target::Spread(si) = target
        {
            let lay = st.layout(&renderer);
            let extra = crate::marks::spread_marks(&st.doc, &lay, si, notes, changes);
            if !extra.is_empty() {
                let mut v = (*ops).clone();
                v.extend(extra);
                ops = std::sync::Arc::new(v);
            }
        }
        placed.push(Placed { xf: s.xf, pages, ops });
    }
    let doc = st.doc.clone();
    let mut bin = Vec::new();
    let mut out = Vec::new();
    for t in list.iter().take(256) {
        let sc = t.get("s").and_then(Value::as_f64).filter(|v| v.is_finite() && *v > 0.0).ok_or_else(|| EngineError::new("tile needs a positive `s` (pixels per point)"))?;
        let ox = t.get("ox").and_then(Value::as_f64).unwrap_or(0.0);
        let oy = t.get("oy").and_then(Value::as_f64).unwrap_or(0.0);
        let w = t.get("w").and_then(Value::as_u64).unwrap_or(256).clamp(1, 4096) as u32;
        let h = t.get("h").and_then(Value::as_u64).unwrap_or(256).clamp(1, 4096) as u32;
        let view = Affine::scale(sc) * Affine::translate((-ox, -oy));
        let mut img = render_view(&doc, &placed, w, h, view, &opts, &renderer.outlines, &renderer.images);
        if let Some(limit) = ink_limit {
            let mut sum = vec![0f32; img.rgba.len() / 4];
            for plate in 0..4u8 {
                let po = ViewOpts { plate: Some(plate), ..opts.clone() };
                let pi = render_view(&doc, &placed, w, h, view, &po, &renderer.outlines, &renderer.images);
                for (s, px) in sum.iter_mut().zip(pi.rgba.chunks_exact(4)) {
                    *s += 1.0 - px[0] as f32 / 255.0;
                }
            }
            let bg = opts.background;
            for (s, px) in sum.iter().zip(img.rgba.chunks_exact_mut(4)) {
                if px[..3] != bg[..3] && *s > limit {
                    px.copy_from_slice(&[230, 40, 40, 255]);
                }
            }
        } else if let Some(pr) = &proof
            && opts.plate.is_none()
        {
            proof_pixels(&mut img.rgba, pr, opts.background);
        }
        let start = bin.len();
        bin.extend_from_slice(&img.rgba);
        out.push(json!({"key": t.get("key").cloned().unwrap_or(Value::Null), "w": img.width, "h": img.height, "offset": start, "len": img.rgba.len()}));
    }
    Ok((json!({"tiles": out, "format": "rgba"}), bin))
}

pub fn page_image(ses: &mut Session, p: &Value) -> Result<(Value, Vec<u8>)> {
    let renderer = ses.renderer.clone();
    let st = ses.active_mut().ok_or_else(|| EngineError::new("no document open"))?;
    let page = p.get("page").and_then(Value::as_u64).unwrap_or(0) as usize;
    let count = st.doc.page_count();
    let pg = st.doc.page(page).ok_or_else(|| EngineError::new(format!("no page {page} (the document has {count} pages, indices are 0-based)")))?;
    let bleed = p.get("bleed").and_then(Value::as_bool).unwrap_or(false);
    let scale = match p.get("maxSide").and_then(Value::as_f64) {
        Some(ms) if ms > 0.0 => (ms / pg.width.max(pg.height)).clamp(0.02, 16.0),
        _ => p.get("scale").and_then(Value::as_f64).unwrap_or(1.0).clamp(0.02, 16.0),
    };
    let lay = st.layout(&renderer);
    let scene = page_scene(&st.doc, &lay, page, &SceneOpts { printing: true, bleed, overprint: false }).ok_or_else(|| EngineError::new(format!("no page {page}")))?;
    let r = rasterize(&scene, &st.doc, &renderer.outlines, &renderer.images, &RasterOpts { scale, ..RasterOpts::default() });
    let jpeg = matches!(p.get("format").and_then(Value::as_str), Some("jpeg" | "jpg"));
    let bytes = if jpeg { encode_jpeg(&r, p.get("quality").and_then(Value::as_u64).unwrap_or(86).clamp(10, 100) as u8) } else { encode_png(&r) }.ok_or_else(|| EngineError::new("could not encode the page image"))?;
    Ok((json!({"width": r.width, "height": r.height, "mime": if jpeg { "image/jpeg" } else { "image/png" }}), bytes))
}
