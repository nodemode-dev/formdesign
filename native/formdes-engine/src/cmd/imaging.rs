use std::collections::BTreeMap;

use formdes_model::ids::{AssetId, ItemId};
use formdes_model::item::{Asset, Shape, TextWrap, WrapMode};
use formdes_render::clippath::{self, Detect};
use formdes_render::images::is_pdf;
use formdes_render::{pdflayers, psd};
use kurbo::{Affine, BezPath, Shape as _};
use serde_json::{Map, Value, json};

use crate::registry::{Needs, Registry, cmd};
use crate::session::Ctx;
use crate::target::items;
use crate::{EngineError, Result, args};

const TYPES: &str = "none, detectEdges, alphaChannel or photoshopPath";

fn graphics(c: &Ctx, ids: &[ItemId]) -> Result<Vec<(ItemId, Asset)>> {
    let doc = c.doc()?;
    let mut out: Vec<(ItemId, Asset)> = Vec::new();
    for id in ids {
        if let Some(a) = doc.item(*id).and_then(|i| i.graphic()).and_then(|g| g.asset).and_then(|a| doc.assets.get(&a)) {
            out.push((*id, a.clone()));
        }
    }
    if out.is_empty() {
        return Err(EngineError::new("select a placed graphic: this works on pictures, PDFs and PSD files in a graphic frame"));
    }
    Ok(out)
}

fn detect_of(p: &Value) -> Detect {
    let d = Detect::default();
    Detect {
        threshold: args::opt_f64(p, "threshold").unwrap_or(d.threshold),
        tolerance: args::opt_f64(p, "tolerance").unwrap_or(d.tolerance),
        inset: args::opt_f64(p, "inset").unwrap_or(d.inset),
        invert: args::opt_bool(p, "invert").unwrap_or(false),
        inside_edges: args::opt_bool(p, "includeInsideEdges").or_else(|| args::opt_bool(p, "insideEdges")).unwrap_or(false),
    }
}

fn alpha_names(asset: &Asset) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(p) = psd::Psd::parse(&asset.data) {
        let names = p.extra_channel_names();
        let extra = (p.channels as usize).saturating_sub(p.color_channels());
        for i in 0..extra {
            out.push(names.get(i).cloned().unwrap_or_else(|| format!("Alpha {}", i + 1)));
        }
    }
    out
}

fn build_path(c: &Ctx, asset: &Asset, ty: &str, p: &Value) -> Result<(BezPath, Value)> {
    let (nw, nh) = asset.natural_size();
    let opt = detect_of(p);
    match ty {
        "photoshopPath" => {
            let paths = psd::photoshop_paths(&asset.data);
            if paths.is_empty() {
                return Err(EngineError::new("this picture has no Photoshop paths (JPEG, TIFF and PSD files can carry them in their 8BIM resources)"));
            }
            let pick = match args::opt_str(p, "name") {
                Some(n) => paths.iter().find(|x| x.name.eq_ignore_ascii_case(n)).ok_or_else(|| EngineError::new(format!("no Photoshop path named {n}: the picture has {}", paths.iter().map(|x| x.name.clone()).collect::<Vec<_>>().join(", "))))?,
                None => paths.iter().find(|x| x.clipping).unwrap_or(&paths[0]),
            };
            let mut path = Affine::scale_non_uniform(nw, nh) * pick.path.clone();
            if opt.invert {
                path = clippath::invert_path(&path, nw, nh);
            }
            let points = path.elements().len();
            Ok((path, json!({"name": pick.name, "points": points})))
        }
        "alphaChannel" | "detectEdges" => {
            let d = c.session.renderer.images.get(asset, 1.0).ok_or_else(|| EngineError::new("cannot read the picture's pixels"))?;
            let alpha = ty == "alphaChannel";
            let mut plane: Option<Vec<u8>> = None;
            let mut channel_name = Value::Null;
            if alpha {
                let want = p.get("channel").filter(|v| !v.is_null());
                if let Some(w) = want {
                    let ps = psd::Psd::parse(&asset.data).ok_or_else(|| EngineError::new("`channel` picks one of a PSD file's alpha channels"))?;
                    let names = alpha_names(asset);
                    let idx = match w {
                        Value::Number(n) => n.as_u64().map(|n| n as usize),
                        Value::String(s) => names.iter().position(|n| n.eq_ignore_ascii_case(s)),
                        _ => None,
                    }
                    .filter(|i| *i < names.len())
                    .ok_or_else(|| EngineError::new(format!("no alpha channel {w}: the file has {}", if names.is_empty() { "none".to_string() } else { names.join(", ") })))?;
                    plane = Some(ps.extra_channel(idx).ok_or_else(|| EngineError::new("cannot read that alpha channel"))?);
                    channel_name = json!(names[idx]);
                } else if !clippath::has_alpha(&d) {
                    return Err(EngineError::new("this picture has no alpha channel or transparency"));
                }
            }
            let t = clippath::from_decoded(&d, &opt, alpha, plane.as_deref(), (nw, nh)).ok_or_else(|| EngineError::new("no edges found: lower the threshold or change the type"))?;
            Ok((t.path, json!({"points": t.points, "channel": channel_name})))
        }
        other => Err(EngineError::new(format!("`type` expected {TYPES}, not {other}"))),
    }
}

fn record(ty: &str, p: &Value, path: &BezPath, info: &Value) -> Value {
    let d = Detect::default();
    let o = detect_of(p);
    let bb = path.bounding_box();
    let mut m = Map::new();
    m.insert("type".into(), json!(ty));
    if ty == "detectEdges" {
        m.insert("threshold".into(), json!(o.threshold));
        m.insert("tolerance".into(), json!(o.tolerance));
        m.insert("includeInsideEdges".into(), json!(o.inside_edges));
    }
    if ty != "photoshopPath" {
        m.insert("inset".into(), json!(if ty == "detectEdges" { o.inset } else { d.inset }));
    }
    m.insert("invert".into(), json!(o.invert));
    if let Some(n) = info.get("name") {
        m.insert("name".into(), n.clone());
    }
    if let Some(n) = info.get("channel").filter(|v| !v.is_null()) {
        m.insert("channel".into(), n.clone());
    }
    m.insert("points".into(), json!(info.get("points").cloned().unwrap_or(json!(0))));
    m.insert("bounds".into(), json!([bb.x0, bb.y0, bb.x1, bb.y1]));
    m.insert("d".into(), json!(path.to_svg()));
    Value::Object(m)
}

fn clipping_path(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let ty = args::str_(p, "type", TYPES)?.to_string();
    let gs = graphics(c, &ids)?;
    if ty == "none" {
        let doc = c.doc_mut()?;
        for (id, _) in &gs {
            if let Some(it) = doc.item_mut(*id) {
                it.extra.remove("clipping");
            }
        }
        c.label("Clipping Path");
        return Ok(json!({"type": "none"}));
    }
    let mut made = Vec::new();
    for (id, asset) in &gs {
        let (path, info) = build_path(c, asset, &ty, p)?;
        made.push((*id, record(&ty, p, &path, &info)));
    }
    let doc = c.doc_mut()?;
    let mut out = Value::Null;
    for (id, rec) in made {
        if let Some(it) = doc.item_mut(id) {
            let mut shown = rec.clone();
            if let Some(o) = shown.as_object_mut() {
                o.remove("d");
            }
            out = shown;
            it.extra.insert("clipping".into(), rec);
        }
    }
    c.label("Clipping Path");
    Ok(out)
}

fn clipping_info(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let gs = graphics(c, &ids)?;
    let doc = c.doc()?;
    let (id, asset) = &gs[0];
    let cur = doc.item(*id).and_then(|i| i.extra.get("clipping")).cloned().map(|mut v| {
        if let Some(o) = v.as_object_mut() {
            o.remove("d");
        }
        v
    });
    let wrap = doc.item(*id).and_then(|i| i.extra.get("wrapContour")).cloned().map(|mut v| {
        if let Some(o) = v.as_object_mut() {
            o.remove("d");
        }
        v
    });
    let paths: Vec<Value> = psd::photoshop_paths(&asset.data).iter().map(|x| json!({"name": x.name, "clipping": x.clipping})).collect();
    let has_alpha = c.session.renderer.images.get(asset, 1.0).map(|d| clippath::has_alpha(&d)).unwrap_or(false);
    Ok(json!({"id": id, "clipping": cur, "wrapContour": wrap, "photoshopPaths": paths, "alphaChannels": alpha_names(asset), "hasTransparency": has_alpha}))
}

fn clip_to_frame(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let doc = c.doc()?;
    let mut paths = Vec::new();
    for id in &ids {
        if let Some(it) = doc.item(*id)
            && let Some(path) = it.image_path("clipping")
        {
            paths.push((*id, path));
        }
    }
    if paths.is_empty() {
        return Err(EngineError::new("no clipping path to convert: set one with object.clippingPath first"));
    }
    let n = paths.len();
    let doc = c.doc_mut()?;
    for (id, path) in paths {
        if let Some(it) = doc.item_mut(id) {
            it.shape = Shape::Path { path, closed: true };
            it.extra.remove("clipping");
        }
    }
    c.label("Convert Clipping Path to Frame");
    Ok(json!({"converted": n}))
}

fn wrap_contour(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let ty = args::str_(p, "type", "graphicFrame, sameAsClipping, detectEdges, alphaChannel or photoshopPath")?.to_string();
    let gs = graphics(c, &ids)?;
    let mut made = Vec::new();
    for (id, asset) in &gs {
        let rec = match ty.as_str() {
            "graphicFrame" | "sameAsClipping" => json!({"type": ty}),
            "detectEdges" | "alphaChannel" | "photoshopPath" => {
                let (path, info) = build_path(c, asset, &ty, p)?;
                record(&ty, p, &path, &info)
            }
            other => return Err(EngineError::new(format!("`type` expected graphicFrame, sameAsClipping, detectEdges, alphaChannel or photoshopPath, not {other}"))),
        };
        made.push((*id, rec));
    }
    let offset = args::opt_f64(p, "offset");
    let doc = c.doc_mut()?;
    for (id, rec) in made {
        if let Some(it) = doc.item_mut(id) {
            it.extra.insert("wrapContour".into(), rec);
            let w = it.wrap.get_or_insert_with(|| TextWrap { mode: WrapMode::Contour, offsets: Default::default(), invert: false, side: "both".into() });
            w.mode = WrapMode::Contour;
            if let Some(o) = offset {
                w.offsets = formdes_model::geom::Edges::all(o);
            }
        }
    }
    c.label("Text Wrap Contour");
    Ok(json!({"type": ty}))
}

struct Row {
    id: String,
    name: String,
    group: bool,
    depth: usize,
    file: bool,
}

fn rows_of(asset: &Asset) -> Option<(&'static str, Vec<Row>)> {
    if is_pdf(asset) {
        let ls = pdflayers::layers(&asset.data);
        if ls.is_empty() {
            return None;
        }
        return Some(("pdf", ls.into_iter().map(|l| Row { id: l.id.to_string(), name: l.name, group: false, depth: 0, file: l.visible }).collect()));
    }
    if psd::is_psd(&asset.data) {
        let p = psd::Psd::parse(&asset.data)?;
        if p.layers.is_empty() {
            return None;
        }
        let mut depth = 0usize;
        let mut out = Vec::new();
        for l in p.layers.iter().rev() {
            match l.kind {
                3 => depth = depth.saturating_sub(1),
                1 | 2 => {
                    out.push(Row { id: l.index.to_string(), name: l.name.clone(), group: true, depth, file: l.visible });
                    depth += 1;
                }
                _ => out.push(Row { id: l.index.to_string(), name: l.name.clone(), group: false, depth, file: l.visible }),
            }
        }
        return Some(("psd", out));
    }
    None
}

fn drop_view(c: &mut Ctx, id: ItemId) -> Result<()> {
    let view = c.doc()?.item(id).and_then(|i| i.extra.get("layers")).and_then(|l| l.get("view")).and_then(Value::as_u64);
    if let Some(v) = view {
        let doc = c.doc_mut()?;
        doc.assets.remove(&AssetId(v));
        c.session.renderer.images.forget(AssetId(v));
    }
    Ok(())
}

fn apply_layers(c: &mut Ctx, id: ItemId, change: Option<&Value>, show_all: bool, reset: bool, update: Option<&str>) -> Result<Value> {
    let doc = c.doc()?;
    let it = doc.item(id).ok_or_else(|| EngineError::new("no such object"))?;
    let src = it.graphic().and_then(|g| g.asset).ok_or_else(|| EngineError::new("this object holds no placed graphic"))?;
    let asset = doc.assets.get(&src).cloned().ok_or_else(|| EngineError::new("the graphic's file is missing"))?;
    let (kind, rows) = rows_of(&asset).ok_or_else(|| EngineError::new("this graphic has no layers (placed PDFs with optional content groups and PSD files with layers have them)"))?;
    let prev = it.extra.get("layers").cloned().unwrap_or(Value::Null);
    let old_view = prev.get("view").and_then(Value::as_u64);
    let mut over: BTreeMap<String, bool> = prev.get("overrides").and_then(Value::as_object).map(|o| o.iter().filter_map(|(k, v)| v.as_bool().map(|b| (k.clone(), b))).collect()).unwrap_or_default();
    over.retain(|k, _| rows.iter().any(|r| &r.id == k));
    let mode = update.map(str::to_string).or_else(|| prev.get("update").and_then(Value::as_str).map(str::to_string)).unwrap_or_else(|| "keep".into());
    if mode != "keep" && mode != "file" {
        return Err(EngineError::new("`update` expected keep or file"));
    }
    if reset {
        over.clear();
    }
    let find = |key: &str| rows.iter().find(|r| r.id == key).or_else(|| rows.iter().find(|r| r.name.eq_ignore_ascii_case(key)));
    let mut edits: Vec<(String, bool)> = Vec::new();
    match change {
        Some(Value::Object(m)) => {
            for (k, v) in m {
                let r = find(k).ok_or_else(|| EngineError::new(format!("no layer {k}: use an id or name from the list")))?;
                edits.push((r.id.clone(), v.as_bool().ok_or_else(|| EngineError::new("layer visibility is true or false"))?));
            }
        }
        Some(Value::Array(a)) => {
            for e in a {
                let k = e.get("id").map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())).or_else(|| e.get("name").and_then(Value::as_str).map(str::to_string)).ok_or_else(|| EngineError::new("each layer entry needs `id` or `name`"))?;
                let r = find(&k).ok_or_else(|| EngineError::new(format!("no layer {k}")))?;
                edits.push((r.id.clone(), e.get("visible").and_then(Value::as_bool).ok_or_else(|| EngineError::new("each layer entry needs `visible`"))?));
            }
        }
        _ => {}
    }
    if show_all {
        for r in &rows {
            edits.insert(0, (r.id.clone(), true));
        }
    }
    for (k, v) in edits {
        over.insert(k, v);
    }
    let file: BTreeMap<&str, bool> = rows.iter().map(|r| (r.id.as_str(), r.file)).collect();
    over.retain(|k, v| file.get(k.as_str()) != Some(v));
    let eff = |k: &str| over.get(k).copied().or_else(|| file.get(k).copied()).unwrap_or(true);
    let mut view_bytes: Option<(Vec<u8>, &'static str)> = None;
    if !over.is_empty() {
        if kind == "pdf" {
            view_bytes = pdflayers::apply(&asset.data, asset.page as usize, &|n| eff(&n.to_string())).map(|b| (b, "application/pdf"));
        } else {
            view_bytes = psd::composite_png(&asset.data, &|l| eff(&l.index.to_string())).map(|b| (b, "image/png"));
        }
    }
    let changed = change.is_some() || show_all || reset || update.is_some();
    let mut view_id = old_view;
    if changed {
        match &view_bytes {
            Some((bytes, mime)) => {
                let doc = c.doc_mut()?;
                let na = Asset {
                    id: AssetId(view_id.unwrap_or(0)),
                    name: format!("{} (layers)", asset.name),
                    mime: (*mime).to_string(),
                    link: None,
                    data: std::sync::Arc::new(bytes.clone()),
                    pixels: asset.pixels,
                    ppi: asset.ppi,
                    page: if kind == "pdf" { asset.page } else { 0 },
                    embedded: true,
                    modified: 0,
                };
                let nid = doc.add_asset(na);
                view_id = Some(nid.0);
                c.session.renderer.images.forget(nid);
            }
            None => {
                drop_view(c, id)?;
                view_id = None;
            }
        }
        let doc = c.doc_mut()?;
        if let Some(item) = doc.item_mut(id) {
            if over.is_empty() && mode == "keep" {
                item.extra.remove("layers");
            } else {
                item.extra.insert("layers".into(), json!({"source": src, "view": view_id, "overrides": over, "update": mode}));
            }
        }
        c.label("Object Layer Options");
    }
    let list: Vec<Value> = rows.iter().map(|r| json!({"id": r.id, "name": r.name, "group": r.group, "depth": r.depth, "file": r.file, "visible": eff(&r.id), "overridden": over.contains_key(&r.id)})).collect();
    Ok(json!({"kind": kind, "layers": list, "overrides": over.len(), "update": mode}))
}

fn layer_options(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let gs = graphics(c, &ids)?;
    let id = gs.iter().find(|(_, a)| rows_of(a).is_some()).map(|(i, _)| *i).ok_or_else(|| EngineError::new("this graphic has no layers (placed PDFs with optional content groups and PSD files with layers have them)"))?;
    apply_layers(c, id, p.get("layers"), args::opt_bool(p, "showAll").unwrap_or(false), args::opt_bool(p, "resetToFile").unwrap_or(false), args::opt_str(p, "update").or_else(|| args::opt_str(p, "whenUpdatingLink")))
}

pub fn relinked(c: &mut Ctx, asset: AssetId) -> Result<()> {
    let users: Vec<(ItemId, String)> = c
        .doc()?
        .items
        .values()
        .filter(|i| i.graphic().and_then(|g| g.asset) == Some(asset))
        .filter_map(|i| i.extra.get("layers").map(|l| (i.id, l.get("update").and_then(Value::as_str).unwrap_or("keep").to_string())))
        .collect();
    for (id, mode) in users {
        if mode == "file" {
            drop_view(c, id)?;
            if let Some(it) = c.doc_mut()?.item_mut(id) {
                it.extra.remove("layers");
            }
        } else if apply_layers(c, id, None, false, false, Some("keep")).is_err() {
            drop_view(c, id)?;
            if let Some(it) = c.doc_mut()?.item_mut(id) {
                it.extra.remove("layers");
            }
        }
    }
    Ok(())
}

/// Edits the pixels of each selected frame's placed raster picture and relinks the frame to the result: a new file
/// beside a linked original (`name-edit.ext`, numbered when taken) or a new embedded picture, never the original.
/// The kept pixels stay where they were on the page after a crop; a turned picture stays centred on the kept area.
fn image_edit(c: &mut Ctx, p: &Value) -> Result<Value> {
    use formdes_render::pixedit::{Edit, edit};
    let ids = items(c, p)?;
    let num = |k: &str| args::opt_f64(p, k).unwrap_or(0.0);
    let four = |k: &str, what: &str| -> Result<Option<Vec<f64>>> {
        match p.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v.as_array().and_then(|a| a.iter().map(Value::as_f64).collect::<Option<Vec<f64>>>()).map(Some).ok_or_else(|| EngineError::new(what.to_string())),
        }
    };
    let crop = four("crop", "crop is [x0, y0, x1, y1] as shares of the picture, 0 to 1")?
        .map(|v| if v.len() == 4 { Ok([v[0], v[1], v[2], v[3]]) } else { Err(EngineError::new("crop is [x0, y0, x1, y1] as shares of the picture, 0 to 1")) })
        .transpose()?;
    let levels = four("levels", "levels is [black 0..255, white 0..255, gamma]")?
        .map(|v| if v.len() == 3 && v[1] > v[0] { Ok((v[0], v[1], v[2])) } else { Err(EngineError::new("levels is [black 0..255, white 0..255 above black, gamma]")) })
        .transpose()?;
    let rotate = args::opt_u64(p, "rotate").unwrap_or(0) as u32;
    if rotate % 90 != 0 {
        return Err(EngineError::new("rotate is 0, 90, 180 or 270 (clockwise)"));
    }
    let e = Edit { crop, rotate: rotate % 360, flip_h: args::opt_bool(p, "flipH").unwrap_or(false), flip_v: args::opt_bool(p, "flipV").unwrap_or(false), exposure: num("exposure"), contrast: num("contrast"), levels, saturation: num("saturation"), temperature: num("temperature") };
    if e.crop.is_none() && e.rotate == 0 && !e.flip_h && !e.flip_v && !e.is_tonal() {
        return Err(EngineError::new("nothing to change: give crop, rotate, flipH, flipV, exposure, contrast, levels, saturation or temperature"));
    }
    let mut done = Vec::new();
    for id in ids {
        let doc = c.doc()?;
        let it = doc.item(id).ok_or_else(|| EngineError::new(format!("no object {id}")))?;
        let aid = it.graphic().and_then(|g| g.asset).ok_or_else(|| EngineError::new(format!("object {id} holds no picture: select a graphic frame with a placed image")))?;
        let a = doc.assets.get(&aid).cloned().ok_or_else(|| EngineError::new(format!("no link {}", aid.0)))?;
        let data: Vec<u8> = if a.data.is_empty() {
            let path = a.link.clone().ok_or_else(|| EngineError::new(format!("{} has no data and no file", a.name)))?;
            std::fs::read(&path).map_err(|err| EngineError::new(format!("cannot read {path}: {err}")))?
        } else {
            a.data.as_ref().clone()
        };
        if !a.mime.starts_with("image/") || is_pdf(&a) || psd::is_psd(&data) {
            return Err(EngineError::new(format!("{} is not a raster picture: JPEG, PNG, GIF, WebP, BMP and TIFF can be edited here; edit PDF, PSD and media files in their own apps (Edit > Edit Original)", a.name)));
        }
        let out = edit(&data, &e, a.ppi.map(|(x, _)| x)).map_err(EngineError::new)?;
        // An edited copy edited again stays `name-editN`, not `name-edit-edit`.
        let raw = std::path::Path::new(&a.name).file_stem().and_then(|s| s.to_str()).unwrap_or("picture");
        let stem = raw.rfind("-edit").filter(|i| raw[i + 5..].chars().all(|ch| ch.is_ascii_digit())).map_or(raw, |i| &raw[..i]).to_string();
        let name = format!("{stem}-edit.{}", out.ext);
        let link = match &a.link {
            Some(orig) => {
                let dir = std::path::Path::new(orig).parent().map(|d| d.to_path_buf()).unwrap_or_default();
                let free = (1..1000).map(|k| dir.join(if k == 1 { name.clone() } else { format!("{stem}-edit{k}.{}", out.ext) })).find(|p| !p.exists()).ok_or_else(|| EngineError::new("no free file name for the edited copy"))?;
                std::fs::write(&free, &out.bytes).map_err(|err| EngineError::new(format!("cannot write {}: {err}", free.display())))?;
                Some(free.to_string_lossy().to_string())
            }
            None => None,
        };
        let mut na = crate::assets::make(&name, out.bytes, link.clone(), 0)?;
        na.ppi = a.ppi;
        // Points per source pixel, so the kept area keeps its place and size on the page.
        let (w, h) = a.natural_size();
        let (kx, ky) = (w / out.source_px.0.max(1) as f64, h / out.source_px.1.max(1) as f64);
        let [cx, cy, cw, ch] = out.crop_px.map(|v| v as f64);
        let (nw, nh) = na.natural_size();
        let shift = Affine::translate((cx * kx, cy * ky)) * Affine::translate(((cw * kx - nw) / 2.0, (ch * ky - nh) / 2.0));
        let pixels = out.pixels;
        let doc = c.doc_mut()?;
        let new_id = doc.add_asset(na);
        if let Some(x) = doc.item_mut(id)
            && let formdes_model::item::ItemKind::Graphic(g) = &mut x.kind
        {
            g.content = g.content * shift;
            g.asset = Some(new_id);
        }
        // The original stays in the document only while another frame shows it.
        let still_used = doc.items.values().any(|x| x.graphic().and_then(|g| g.asset) == Some(aid));
        if !still_used {
            doc.assets.remove(&aid);
        }
        done.push(json!({"id": id, "asset": new_id.0, "path": link, "pixels": [pixels.0, pixels.1]}));
    }
    c.label("Edit Image");
    Ok(json!({"edited": done}))
}

pub fn register(r: &mut Registry) {
    cmd("image.edit", "Edit Image").menu(&["Object", "Image", "Edit Image..."]).needs(Needs::Items).sig("{crop?: [x0, y0, x1, y1] shares of the picture (0 to 1), rotate?: 0|90|180|270 (clockwise), flipH?, flipV?, exposure?: stops -3..3, contrast?: -100..100, levels?: [black 0..255, white 0..255, gamma], saturation?: -100..100, temperature?: -100..100, ids?} (writes an edited copy beside a linked original, or embeds it, and relinks the frame)").reg(r, image_edit);
    cmd("object.clippingPath", "Clipping Path").menu(&["Object", "Clipping Path", "Options..."]).needs(Needs::Items).sig("{type: none|detectEdges|alphaChannel|photoshopPath, threshold?: 0-255 (detect edges: how far from the background colour or alpha counts as picture), tolerance?: pixels of smoothing, inset?: pt (negative grows), invert?, includeInsideEdges?, name?: Photoshop path name, channel?: PSD alpha channel name or index, ids?}").help("Clips a placed picture to a path: detected from the picture's edges (a flat background or its transparency), its alpha channel (or one of a PSD file's), or a Photoshop path stored in the JPEG, TIFF or PSD. The clip shows on screen, in PDF and in text wrap set to follow the clipping path.").reg(r, clipping_path);
    cmd("object.clippingInfo", "Clipping Path Info").query().needs(Needs::Items).sig("{ids?}").help("The selected picture's clipping path settings, its Photoshop path names, PSD alpha channels and whether it has transparency.").reg(r, clipping_info);
    cmd("object.clipToFrame", "Convert Clipping Path to Frame").menu(&["Object", "Clipping Path", "Convert Clipping Path to Frame"]).needs(Needs::Items).sig("{ids?}").help("Turns the clipping path into the graphic frame's shape and drops the clip.").reg(r, clip_to_frame);
    cmd("object.wrapContour", "Text Wrap Contour").menu(&["Object", "Text Wrap Contour Options..."]).needs(Needs::Items).sig("{type: graphicFrame|sameAsClipping|detectEdges|alphaChannel|photoshopPath, threshold?, tolerance?, inset?, invert?, includeInsideEdges?, name?, channel?, offset?: pt, ids?}").help("Sets the contour a text wrap of mode contour follows around a picture and turns the wrap to contour.").reg(r, wrap_contour);
    cmd("object.layerOptions", "Object Layer Options").menu(&["Object", "Object Layer Options..."]).needs(Needs::Items).sig("{layers?: {layer id or name: true|false}, showAll?, resetToFile?, update?: keep|file (when updating the link: keep the overrides or use the file's)}").help("Lists the layers of a placed PDF (optional content groups) or PSD and shows or hides them for this placement. Without changes it only lists.").reg(r, layer_options);
}
