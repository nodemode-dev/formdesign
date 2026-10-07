use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use formdes_interchange::idml::{b64, unb64};
use formdes_model::color::{Color, Paint, Swatch, SwatchValue, swatch_key};
use formdes_model::doc::{Doc, Home};
use formdes_model::geom::{Affine, Vec2};
use formdes_model::ids::{AssetId, ItemId, StoryId};
use formdes_model::item::ItemKind;
use formdes_model::story::{Inline, Seg};
use formdes_model::style::StyleKind;
use serde_json::{Value, json};

use crate::registry::{Needs, Registry, cmd};
use crate::session::{Ctx, Selection};
use crate::{EngineError, Result, args, assets, ops, walk, wire};

fn ok() -> Result<Value> {
    Ok(json!({}))
}

fn swatch_create(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = swatch_key(args::str_(p, "name", "the swatch name")?);
    if name.is_empty() {
        return Err(EngineError::new("missing `name` (the swatch name)"));
    }
    let color = p.get("color").and_then(wire::color_in).ok_or_else(|| EngineError::new("missing `color` (\"#rrggbb\", {c, m, y, k} in percent, or a color.convert result)"))?;
    let spot = args::opt_bool(p, "spot").unwrap_or(false);
    let doc = c.doc_mut()?;
    if doc.swatch(&name).is_some() || name.eq_ignore_ascii_case("none") {
        return Err(EngineError::new(format!("there is already a swatch named {name}")));
    }
    doc.swatches.push(Swatch::color(&name, color, spot));
    Ok(json!({"name": name}))
}

fn swatch_edit(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = swatch_key(args::str_(p, "name", "the swatch to change")?);
    let color = p.get("color").and_then(wire::color_in);
    let spot = args::opt_bool(p, "spot");
    let rename = args::opt_str(p, "rename").map(swatch_key).filter(|r| *r != name);
    let doc = c.doc_mut()?;
    if let Some(r) = &rename
        && doc.swatch(r).is_some()
    {
        return Err(EngineError::new(format!("there is already a swatch named {r}")));
    }
    let sw = doc.swatches.iter_mut().find(|s| s.name == name).ok_or_else(|| EngineError::new(format!("no swatch named {name}")))?;
    if sw.locked && (rename.is_some() || name != "Paper") {
        return Err(EngineError::new(format!("[{name}] cannot be changed")));
    }
    if let Some(col) = color {
        sw.value = SwatchValue::Color { color: col };
    }
    if let Some(s) = spot {
        sw.spot = s;
    }
    if let Some(r) = rename {
        sw.name = r.clone();
        replace_swatch(doc, &name, &r);
    }
    c.label("Swatch Options");
    ok()
}

fn matches_swatch(v: &Value, name: &str) -> bool {
    match v {
        Value::String(s) => swatch_key(s) == name,
        Value::Object(o) => o.get("swatch").and_then(Value::as_str).is_some_and(|s| swatch_key(s) == name),
        _ => false,
    }
}

fn swap_value(v: &mut Value, to: &str) {
    match v {
        Value::Object(o) => {
            o.insert("swatch".into(), json!(to));
        }
        _ => *v = json!(to),
    }
}

pub fn replace_swatch(doc: &mut Doc, from: &str, to: &str) {
    let ids: Vec<ItemId> = doc.items.keys().copied().collect();
    let hit = |p: &Paint| matches!(p, Paint::Swatch { name, .. } if name == from);
    for id in ids {
        let Some(it) = doc.item(id) else { continue };
        if !hit(&it.fill) && !hit(&it.stroke.paint) {
            continue;
        }
        if let Some(it) = doc.item_mut(id) {
            for p in [&mut it.fill, &mut it.stroke.paint] {
                if let Paint::Swatch { name, .. } = p
                    && name == from
                {
                    *name = to.to_string();
                }
            }
        }
    }
    for sw in doc.swatches.iter_mut() {
        if let SwatchValue::Tint { base, .. } = &mut sw.value
            && base == from
        {
            *base = to.to_string();
        }
    }
    for kind in [StyleKind::Paragraph, StyleKind::Character] {
        for st in doc.styles.list_mut(kind).iter_mut() {
            for k in ["fill", "stroke"] {
                if let Some(v) = st.attrs.get_mut(k)
                    && matches_swatch(v, from)
                {
                    swap_value(v, to);
                }
            }
        }
    }
    let f = from.to_string();
    let t = to.to_string();
    walk::paras_where(
        doc,
        &move |p| ["fill", "stroke"].iter().any(|k| p.over.get(*k).is_some_and(|v| matches_swatch(v, &f)) || p.segs.iter().any(|s| s.over().get(*k).is_some_and(|v| matches_swatch(v, &f)))),
        &mut |p| {
            for k in ["fill", "stroke"] {
                if let Some(v) = p.over.get_mut(k)
                    && matches_swatch(v, from)
                {
                    swap_value(v, &t);
                }
                for s in p.segs.iter_mut() {
                    let (_, over) = s.format_mut();
                    if let Some(v) = over.get_mut(k)
                        && matches_swatch(v, from)
                    {
                        swap_value(v, &t);
                    }
                }
            }
        },
    );
}

fn swatch_delete(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = swatch_key(args::str_(p, "name", "the swatch to delete")?);
    let to = args::opt_str(p, "replaceWith").map(swatch_key).unwrap_or_else(|| "Black".into());
    let doc = c.doc_mut()?;
    let sw = doc.swatches.iter().find(|s| s.name == name).ok_or_else(|| EngineError::new(format!("no swatch named {name}")))?;
    if sw.locked {
        return Err(EngineError::new(format!("[{name}] cannot be deleted")));
    }
    doc.swatches.retain(|s| s.name != name);
    replace_swatch(doc, &name, &to);
    ok()
}

fn ase(doc: &Doc) -> Vec<u8> {
    let mut blocks: Vec<Vec<u8>> = Vec::new();
    for sw in &doc.swatches {
        let color = match &sw.value {
            SwatchValue::Color { color } => *color,
            SwatchValue::Tint { .. } => match doc.swatch_color(&sw.name, 1.0, 0) {
                Some(c) => c,
                None => continue,
            },
            SwatchValue::Gradient { .. } => continue,
        };
        let mut b = Vec::new();
        let name: Vec<u16> = sw.name.encode_utf16().chain(std::iter::once(0)).collect();
        b.extend((name.len() as u16).to_be_bytes());
        for u in &name {
            b.extend(u.to_be_bytes());
        }
        let (model, vals): (&[u8; 4], Vec<f32>) = match color {
            Color::Cmyk(v) => (b"CMYK", v.iter().map(|x| *x as f32).collect()),
            Color::Rgb(v) => (b"RGB ", v.iter().map(|x| *x as f32).collect()),
            Color::Lab([l, a, bb]) => (b"LAB ", vec![(l / 100.0) as f32, a as f32, bb as f32]),
            Color::Gray(g) => (b"Gray", vec![g as f32]),
        };
        b.extend(model);
        for v in vals {
            b.extend(v.to_be_bytes());
        }
        b.extend((if sw.spot { 1u16 } else { 2u16 }).to_be_bytes());
        blocks.push(b);
    }
    let mut out = Vec::new();
    out.extend(b"ASEF");
    out.extend(1u16.to_be_bytes());
    out.extend(0u16.to_be_bytes());
    out.extend((blocks.len() as u32).to_be_bytes());
    for b in blocks {
        out.extend(1u16.to_be_bytes());
        out.extend((b.len() as u32).to_be_bytes());
        out.extend(b);
    }
    out
}

fn swatch_save(c: &mut Ctx, p: &Value) -> Result<Value> {
    let bytes = ase(c.doc()?);
    if let Some(path) = args::opt_str(p, "path") {
        std::fs::write(path, &bytes).map_err(|e| EngineError::new(format!("cannot write {path}: {e}")))?;
        return Ok(json!({"path": path, "bytes": bytes.len()}));
    }
    Ok(json!({"base64": b64(&bytes)}))
}

fn swatch_list(c: &mut Ctx, _: &Value) -> Result<Value> {
    let doc = c.doc()?;
    let mut out = vec![json!({"name": "[None]", "kind": "none", "locked": true})];
    for s in &doc.swatches {
        let (kind, value) = match &s.value {
            SwatchValue::Color { color } => ("color", wire::color_out(*color)),
            SwatchValue::Tint { base, tint } => ("tint", json!({"base": wire::swatch_name(base), "tint": tint})),
            SwatchValue::Gradient { gradient } => ("gradient", serde_json::to_value(gradient).unwrap_or(Value::Null)),
        };
        let hex = doc.swatch_color(&s.name, 1.0, 0).map(|c| c.hex());
        out.push(json!({"name": wire::swatch_name(&s.name), "kind": kind, "value": value, "hex": hex, "spot": s.spot, "locked": s.locked}));
    }
    Ok(Value::Array(out))
}

fn color_convert(_c: &mut Ctx, p: &Value) -> Result<Value> {
    let color = p.get("color").and_then(wire::color_in).ok_or_else(|| EngineError::new("missing `color` (a colour value)"))?;
    let to = args::opt_str(p, "to").unwrap_or("cmyk");
    let out = match to {
        "cmyk" => match color {
            Color::Cmyk(_) => color,
            other => wire::rgb_to_cmyk(other.to_rgb()),
        },
        "rgb" | "hex" => Color::Rgb(color.to_rgb()),
        "gray" => {
            let [r, g, b] = color.to_rgb();
            Color::Gray(1.0 - (0.299 * r + 0.587 * g + 0.114 * b))
        }
        other => return Err(EngineError::new(format!("`to` expected cmyk, rgb or gray, not {other}"))),
    };
    let mut v = wire::color_out(out);
    if to == "hex" {
        v = json!(out.hex());
    }
    Ok(v)
}

fn icc_info(b: &[u8]) -> Option<(String, &'static str)> {
    if b.len() < 132 || &b[36..40] != b"acsp" {
        return None;
    }
    let kind = match &b[16..20] {
        b"CMYK" => "cmyk",
        b"RGB " => "rgb",
        b"GRAY" => "gray",
        b"Lab " => "lab",
        _ => "other",
    };
    let be32 = |i: usize| -> Option<usize> { Some(u32::from_be_bytes(b.get(i..i + 4)?.try_into().ok()?) as usize) };
    let count = be32(128)?;
    let mut name = None;
    for k in 0..count.min(200) {
        let at = 132 + k * 12;
        if b.get(at..at + 4)? == b"desc" {
            let off = be32(at + 4)?;
            let t = b.get(off..off + 4)?;
            if t == b"desc" {
                let n = be32(off + 8)?;
                name = b.get(off + 12..off + 12 + n).map(|s| String::from_utf8_lossy(s).trim_end_matches('\0').to_string());
            } else if t == b"mluc" {
                let rec_off = be32(off + 16 + 8)?;
                let len = be32(off + 16 + 4)?;
                let s = b.get(off + rec_off..off + rec_off + len)?;
                let u: Vec<u16> = s.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
                name = Some(String::from_utf16_lossy(&u).trim_end_matches('\0').to_string());
            }
        }
    }
    Some((name.filter(|n| !n.is_empty()).unwrap_or_else(|| "Profile".into()), kind))
}

fn load_profile(c: &mut Ctx, p: &Value) -> Result<Value> {
    let path = args::str_(p, "path", "an .icc or .icm file")?;
    let data = std::fs::read(path).map_err(|e| EngineError::new(format!("color profile: cannot read {path}: {e}")))?;
    let (name, kind) = icc_info(&data).ok_or_else(|| EngineError::new(format!("color profile: {} is not an ICC profile", Path::new(path).display())))?;
    let entry = json!({"name": name, "kind": kind, "builtin": false, "path": path});
    c.session.profiles.retain(|x| x.get("name").and_then(Value::as_str) != Some(name.as_str()));
    c.session.profiles.push(entry.clone());
    Ok(entry)
}

const SETTINGS: &[&str] = &["rgbProfile", "cmykProfile", "intent", "blackPointCompensation"];

fn color_settings(c: &mut Ctx, p: &Value) -> Result<Value> {
    let mut profiles = vec![json!({"name": "Working CMYK", "kind": "cmyk", "builtin": true}), json!({"name": "sRGB IEC61966-2.1", "kind": "rgb", "builtin": true})];
    profiles.extend(c.session.profiles.iter().cloned());
    let setting = SETTINGS.iter().any(|k| p.get(*k).is_some());
    if setting {
        if let Some(i) = args::opt_str(p, "intent")
            && !["perceptual", "relative", "saturation", "absolute"].contains(&i)
        {
            return Err(EngineError::new("`intent` expected perceptual, relative, saturation or absolute"));
        }
        let doc = c.doc_mut()?;
        let mut cs = doc.prefs.get("colorSettings").and_then(Value::as_object).cloned().unwrap_or_default();
        for k in SETTINGS {
            if let Some(v) = p.get(*k) {
                cs.insert((*k).into(), v.clone());
            }
        }
        doc.prefs.insert("colorSettings".into(), Value::Object(cs));
        c.label("Color Settings");
    }
    let current = c.state().ok().and_then(|s| s.doc.prefs.get("colorSettings").cloned()).unwrap_or(json!({"rgbProfile": "sRGB IEC61966-2.1", "cmykProfile": "Working CMYK", "intent": "relative", "blackPointCompensation": true}));
    Ok(json!({"profiles": profiles, "settings": current}))
}

fn effective_ppi(doc: &Doc, id: ItemId) -> Option<f64> {
    let it = doc.item(id)?;
    let g = it.graphic()?;
    let a = doc.assets.get(&g.asset?)?;
    let (px, _) = a.pixels?;
    let m = doc.world_xf(id) * g.content;
    let k = m.as_coeffs();
    let sx = k[0].hypot(k[1]);
    let (nw, _) = a.natural_size();
    let shown_in = nw * sx / 72.0;
    (shown_in > 0.0).then(|| (px as f64 / shown_in).round())
}

fn status(a: &formdes_model::item::Asset) -> &'static str {
    match &a.link {
        _ if a.embedded => "embedded",
        None => "embedded",
        Some(l) => {
            let p = Path::new(l);
            if !p.exists() {
                "missing"
            } else if a.modified != 0 && assets::mtime(p) != a.modified {
                "modified"
            } else {
                "ok"
            }
        }
    }
}

fn links_list(c: &mut Ctx, _: &Value) -> Result<Value> {
    let doc = c.doc()?;
    let mut uses: HashMap<AssetId, Vec<Value>> = HashMap::new();
    for it in doc.items.values() {
        if let Some(a) = it.graphic().and_then(|g| g.asset) {
            let page = doc.page_of(it.id).map(|p| doc.page_name(p));
            uses.entry(a).or_default().push(json!({"id": it.id, "page": page, "ppi": effective_ppi(doc, it.id)}));
        }
    }
    Ok(Value::Array(
        doc.assets
            .values()
            .map(|a| json!({"asset": a.id, "name": a.name, "path": a.link, "status": status(a), "pixels": a.pixels.map(|(w, h)| json!([w, h])), "mime": a.mime, "uses": uses.remove(&a.id).unwrap_or_default()}))
            .collect(),
    ))
}

fn asset_id(p: &Value) -> Result<AssetId> {
    args::opt_u64(p, "asset").map(AssetId).ok_or_else(|| EngineError::new("missing `asset` (a link's asset id from links.list)"))
}

fn links_goto(c: &mut Ctx, p: &Value) -> Result<Value> {
    let a = asset_id(p)?;
    let doc = c.doc()?;
    let id = doc.items.values().find(|it| it.graphic().and_then(|g| g.asset) == Some(a)).map(|it| it.id).ok_or_else(|| EngineError::new("no object uses that link"))?;
    let page = doc.page_of(id);
    c.set_selection(Selection::items(vec![id]));
    if let Some(pg) = page {
        c.request(json!({"kind": "view", "params": {"page": pg}}));
    }
    Ok(json!({"id": id, "page": page}))
}

fn refresh_asset(c: &mut Ctx, a: AssetId, path: &str) -> Result<()> {
    let page = c.doc()?.assets.get(&a).map(|x| x.page).unwrap_or(0);
    let mut fresh = assets::load(path, page)?;
    fresh.id = a;
    let doc = c.doc_mut()?;
    doc.assets.insert(a, fresh);
    c.session.renderer.images.forget(a);
    Ok(())
}

fn links_relink(c: &mut Ctx, p: &Value) -> Result<Value> {
    let a = asset_id(p)?;
    let path = args::str_(p, "path", "the new file")?.to_string();
    if !c.doc()?.assets.contains_key(&a) {
        return Err(EngineError::new(format!("no link {a}")));
    }
    refresh_asset(c, a, &path)?;
    ok()
}

fn links_update(c: &mut Ctx, p: &Value) -> Result<Value> {
    let list: Vec<(AssetId, String)> = match args::opt_u64(p, "asset") {
        Some(a) => {
            let x = c.doc()?.assets.get(&AssetId(a)).ok_or_else(|| EngineError::new(format!("no link {a}")))?;
            vec![(x.id, x.link.clone().ok_or_else(|| EngineError::new(format!("{} is embedded, there is no file to update from", x.name)))?)]
        }
        None => c.doc()?.assets.values().filter(|a| status(a) == "modified").filter_map(|a| Some((a.id, a.link.clone()?))).collect(),
    };
    for (a, path) in &list {
        refresh_asset(c, *a, path)?;
    }
    c.label("Update Link");
    Ok(json!({"updated": list.len()}))
}

fn links_embed(c: &mut Ctx, p: &Value) -> Result<Value> {
    let a = asset_id(p)?;
    let doc = c.doc_mut()?;
    let x = doc.assets.get_mut(&a).ok_or_else(|| EngineError::new(format!("no link {a}")))?;
    if x.data.is_empty() {
        let path = x.link.clone().ok_or_else(|| EngineError::new("the link has no data and no file"))?;
        x.data = Arc::new(std::fs::read(&path).map_err(|e| EngineError::new(format!("cannot read {path}: {e}")))?);
    }
    x.embedded = true;
    c.label("Embed Link");
    ok()
}

fn preflight(c: &mut Ctx, _: &Value) -> Result<Value> {
    let lay = c.layout()?;
    let families = c.session.renderer.fonts.families();
    let doc = c.doc()?;
    let mut issues = Vec::new();
    for st in doc.stories.values() {
        if lay.overset(st.id) {
            let last = st.frames.last().copied();
            let at = lay.stories.get(&st.id).and_then(|sl| sl.flow.overset_at.map(|(pa, b)| sl.flow.story_pos(pa, b))).unwrap_or(0);
            let n = st.len().saturating_sub(at).max(1);
            issues.push(json!({"severity": "error", "kind": "overset", "message": format!("Overset text in story {}: {n}", st.id), "item": last, "page": last.and_then(|f| doc.page_of(f))}));
        }
    }
    for it in doc.items.values() {
        if let Some(a) = it.graphic().and_then(|g| g.asset).and_then(|a| doc.assets.get(&a)) {
            match status(a) {
                "missing" => issues.push(json!({"severity": "error", "kind": "missingLink", "message": format!("Missing link: {}", a.name), "item": it.id, "page": doc.page_of(it.id)})),
                "modified" => issues.push(json!({"severity": "warning", "kind": "modifiedLink", "message": format!("Modified link: {}", a.name), "item": it.id, "page": doc.page_of(it.id)})),
                _ => {}
            }
            if !a.mime.starts_with("application/pdf")
                && let Some(ppi) = effective_ppi(doc, it.id)
                && ppi < 150.0
                && doc.settings.intent == formdes_model::doc::Intent::Print
            {
                issues.push(json!({"severity": "warning", "kind": "lowResolution", "message": format!("{} prints at {ppi} ppi (below 150)", a.name), "item": it.id, "page": doc.page_of(it.id)}));
            }
        }
    }
    let mut missing: Vec<String> = Vec::new();
    let mut check = |f: &str| {
        if !f.is_empty() && !families.iter().any(|x| x.eq_ignore_ascii_case(f)) && !missing.iter().any(|m| m == f) {
            missing.push(f.to_string());
        }
    };
    for k in [StyleKind::Paragraph, StyleKind::Character] {
        for s in doc.styles.list(k) {
            if let Some(f) = s.attrs.get("font").and_then(Value::as_str) {
                check(f);
            }
        }
    }
    walk::all_paras(doc, &mut |_, p| {
        for s in &p.segs {
            if let Some(f) = s.over().get("font").and_then(Value::as_str) {
                check(f);
            }
        }
    });
    if !families.is_empty() {
        for f in missing {
            issues.push(json!({"severity": "error", "kind": "missingFont", "message": format!("Missing font: {f}"), "item": null, "page": null}));
        }
    }
    Ok(json!({"issues": issues}))
}

fn snippet_export(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = match crate::target::items(c, p) {
        Ok(v) => v,
        Err(_) => return Ok(json!({})),
    };
    let src = c.doc()?;
    let mut keep: Vec<ItemId> = src.walk(&ids);
    let stories: Vec<StoryId> = keep.iter().filter_map(|i| src.item(*i).and_then(|it| it.story())).collect();
    for s in &stories {
        if let Some(st) = src.story(*s) {
            keep.extend(src.walk(&st.anchored_items()));
        }
    }
    let spread = src.spread_of(ids[0]).unwrap_or(0);
    let mut d = src.clone();
    d.items.retain(|k, _| keep.contains(k));
    d.stories.retain(|k, _| stories.contains(k));
    for st in d.stories.values_mut() {
        Arc::make_mut(st).frames.retain(|f| keep.contains(f));
    }
    let used: Vec<AssetId> = d.items.values().filter_map(|i| i.graphic().and_then(|g| g.asset)).collect();
    d.assets.retain(|k, _| used.contains(k));
    for s in d.spreads.iter_mut() {
        s.items.retain(|i| ids.contains(i));
    }
    for p in d.parents.iter_mut() {
        p.items.retain(|i| ids.contains(i));
    }
    d.prefs.insert("snippetRoots".into(), json!(ids));
    d.prefs.insert("snippetSpread".into(), json!(spread));
    let bytes = formdes_model::format::save(&d)?;
    Ok(json!({"base64": b64(&bytes), "ids": ids}))
}

pub fn import_items(dst: &mut Doc, src: &Doc, roots: &[ItemId], home: Home, offset: Vec2) -> Vec<ItemId> {
    let mut imap: HashMap<ItemId, ItemId> = HashMap::new();
    let mut smap: HashMap<StoryId, StoryId> = HashMap::new();
    let mut amap: HashMap<AssetId, AssetId> = HashMap::new();
    let mut all = src.walk(roots);
    let mut k = 0;
    while k < all.len() {
        if let Some(sid) = src.item(all[k]).and_then(|i| i.story())
            && let Some(st) = src.story(sid)
        {
            for a in src.walk(&st.anchored_items()) {
                if !all.contains(&a) {
                    all.push(a);
                }
            }
        }
        k += 1;
    }
    for id in &all {
        imap.insert(*id, ItemId(dst.alloc()));
        if let Some(it) = src.item(*id) {
            if let Some(sid) = it.story()
                && !smap.contains_key(&sid)
            {
                smap.insert(sid, StoryId(dst.alloc()));
            }
            if let Some(a) = it.graphic().and_then(|g| g.asset)
                && !amap.contains_key(&a)
                && let Some(asset) = src.assets.get(&a)
            {
                let mut x = asset.clone();
                x.id = AssetId(0);
                amap.insert(a, dst.add_asset(x));
            }
        }
    }
    for kind in [StyleKind::Paragraph, StyleKind::Character, StyleKind::Object] {
        for s in src.styles.list(kind) {
            if !dst.styles.exists(kind, &s.name) {
                dst.styles.list_mut(kind).push(s.clone());
            }
        }
    }
    for sw in &src.swatches {
        if dst.swatch(&sw.name).is_none() {
            dst.swatches.push(sw.clone());
        }
    }
    let layer = dst.layers.first().map(|l| l.id).unwrap_or_default();
    for id in &all {
        let Some(it) = src.item(*id) else { continue };
        let mut x = it.clone();
        x.id = imap[id];
        x.group = it.group.and_then(|g| imap.get(&g).copied());
        x.anchored_in = it.anchored_in.and_then(|s| smap.get(&s).copied());
        if dst.layer(x.layer).is_none() {
            x.layer = layer;
        }
        match &mut x.kind {
            ItemKind::Group { children } => *children = children.iter().filter_map(|c| imap.get(c).copied()).collect(),
            ItemKind::States { states, .. } => {
                for s in states.iter_mut() {
                    s.items = s.items.iter().filter_map(|c| imap.get(c).copied()).collect();
                }
            }
            ItemKind::Text(t) => t.story = smap[&t.story],
            ItemKind::Graphic(g) => g.asset = g.asset.and_then(|a| amap.get(&a).copied()),
            ItemKind::Plain => {}
        }
        if roots.contains(id) {
            x.xf = Affine::translate(offset) * x.xf;
        }
        dst.items.insert(x.id, Arc::new(x));
    }
    for (old, new) in &smap {
        let Some(st) = src.story(*old) else { continue };
        let mut s = st.clone();
        s.id = *new;
        s.frames = s.frames.iter().filter_map(|f| imap.get(f).copied()).collect();
        for p in s.paras.iter_mut() {
            for seg in p.segs.iter_mut() {
                if let Seg::Inline { inline: Inline::Anchored { item, .. }, .. } = seg
                    && let Some(n) = imap.get(item)
                {
                    *item = *n;
                }
            }
        }
        dst.stories.insert(*new, Arc::new(s));
    }
    let out: Vec<ItemId> = roots.iter().filter_map(|r| imap.get(r).copied()).collect();
    for r in &out {
        match home {
            Home::Spread(si) => {
                if let Some(s) = dst.spreads.get_mut(si) {
                    s.items.push(*r);
                }
            }
            Home::Parent(pi) => {
                if let Some(p) = dst.parents.get_mut(pi) {
                    p.items.push(*r);
                }
            }
            _ => {}
        }
    }
    out
}

fn snippet_place(c: &mut Ctx, p: &Value) -> Result<Value> {
    let bytes = unb64(args::str_(p, "base64", "a snippet from snippet.export")?);
    let src = formdes_model::format::load(&bytes).map_err(|e| EngineError::new(format!("snippet: {}", e.0)))?;
    let roots: Vec<ItemId> = src.prefs.get("snippetRoots").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).map(ItemId).collect()).unwrap_or_else(|| src.spreads.iter().flat_map(|s| s.items.clone()).collect());
    let roots: Vec<ItemId> = roots.into_iter().filter(|r| src.item(*r).is_some()).collect();
    if roots.is_empty() {
        return Err(EngineError::new("the snippet holds no objects"));
    }
    let orig_spread = src.prefs.get("snippetSpread").and_then(Value::as_u64).unwrap_or(0) as usize;
    let doc = c.doc()?;
    let si = args::opt_u64(p, "spread").map(|s| s as usize).unwrap_or(orig_spread).min(doc.spreads.len().saturating_sub(1));
    let b = ops::union_bounds(&src, &roots).unwrap_or_default();
    let offset = match (args::opt_f64(p, "x"), args::opt_f64(p, "y")) {
        (Some(x), Some(y)) => Vec2::new(x - b.x0, y - b.y0),
        _ => Vec2::ZERO,
    };
    let doc = c.doc_mut()?;
    let ids = import_items(doc, &src, &roots, Home::Spread(si), offset);
    c.set_selection(Selection::items(ids.clone()));
    c.label("Place");
    Ok(json!({"ids": ids}))
}

pub fn register(r: &mut Registry) {
    cmd("swatch.create", "New Swatch").sig("{name, color: \"#rrggbb\" | {c, m, y, k} (percent) | $N, spot?}").reg(r, swatch_create);
    cmd("swatch.edit", "Swatch Options").sig("{name, color?, spot?, rename?}").reg(r, swatch_edit);
    cmd("swatch.delete", "Delete Swatch").sig("{name, replaceWith?}").reg(r, swatch_delete);
    cmd("swatch.save", "Save Swatches").query().sig("{path?}").reg(r, swatch_save);
    cmd("swatch.list", "Swatches").query().sig("{}").reg(r, swatch_list);
    cmd("color.convert", "Convert Color").needs(Needs::Nothing).query().sig("{color, to: cmyk|rgb|hex|gray}").reg(r, color_convert);
    cmd("color.loadProfile", "Load Color Profile").needs(Needs::Nothing).query().sig("{path}").reg(r, load_profile);
    cmd("color.settings", "Color Settings...").menu(&["Edit", "Color Settings..."]).needs(Needs::Nothing).sig("{rgbProfile?, cmykProfile?, intent?, blackPointCompensation?}").reg(r, color_settings);
    cmd("links.list", "Links").query().sig("{}").reg(r, links_list);
    cmd("links.goTo", "Go to Link").query().sig("{asset}").reg(r, links_goto);
    cmd("links.relink", "Relink").sig("{asset, path}").reg(r, links_relink);
    cmd("links.update", "Update Link").sig("{asset?}").reg(r, links_update);
    cmd("links.embed", "Embed Link").sig("{asset}").reg(r, links_embed);
    cmd("preflight.run", "Preflight").query().sig("{}").reg(r, preflight);
    cmd("snippet.export", "Export Snippet").query().sig("{ids?}").reg(r, snippet_export);
    cmd("snippet.place", "Place Snippet").sig("{base64, x?, y?, spread?}").reg(r, snippet_place);
}
