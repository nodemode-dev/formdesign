//! Mixed inks, mixed ink groups, the plates list, printer's marks options and colour separations written as files.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use formdesign_model::color::{MixPart, Swatch, SwatchValue, process_plate, swatch_key};
use formdesign_model::doc::Doc;
use formdesign_render::marks::Marks;
use formdesign_render::pdf::{MarkFont, PdfOpts};
use formdesign_render::scene::{PageScene, SceneOpts, page_scene};
use serde_json::{Value, json};

use crate::registry::{Registry, cmd};
use crate::session::Ctx;
use crate::{EngineError, Result, args, wire};

const PROCESS: [&str; 4] = ["Cyan", "Magenta", "Yellow", "Black"];

/// The sig fields every export that can draw printer's marks takes.
pub const MARKS_SIG: &str = "marks?: bool | {crop, bleed, registration, colorBars, pageInfo, type, weight, offset}, cropMarks?, bleedMarks?, registrationMarks?, colorBars?, pageInformation?, markType?: default|japanese, markWeight?: pt, markOffset?: pt";

/// Printer's marks asked for by `marks` (true for all five, or an object) and the individual flags; None when no
/// mark is on.
pub fn marks_from(p: &Value) -> Option<Marks> {
    let mut m = Marks { offset: 6.0, ..Marks::default() };
    match p.get("marks") {
        Some(Value::Bool(true)) => {
            m.crop = true;
            m.bleed = true;
            m.registration = true;
            m.color_bars = true;
            m.page_info = true;
        }
        Some(Value::Object(o)) => {
            let b = |k: &str| o.get(k).and_then(Value::as_bool).unwrap_or(false);
            m.crop = b("crop");
            m.bleed = b("bleed");
            m.registration = b("registration");
            m.color_bars = b("colorBars");
            m.page_info = b("pageInfo");
            m.japanese = o.get("type").and_then(Value::as_str).is_some_and(|t| t.starts_with("jap") || t.starts_with('j'));
            if let Some(w) = o.get("weight").and_then(Value::as_f64) {
                m.weight = w;
            }
            if let Some(w) = o.get("offset").and_then(Value::as_f64) {
                m.offset = w;
            }
        }
        _ => {}
    }
    for (key, slot) in [("cropMarks", &mut m.crop), ("bleedMarks", &mut m.bleed), ("registrationMarks", &mut m.registration), ("colorBars", &mut m.color_bars), ("pageInformation", &mut m.page_info)] {
        if let Some(v) = args::opt_bool(p, key) {
            *slot = v;
        }
    }
    if let Some(t) = args::opt_str(p, "markType") {
        m.japanese = t.starts_with('j');
    }
    if let Some(w) = args::opt_f64(p, "markWeight") {
        m.weight = w;
    }
    if let Some(o) = args::opt_f64(p, "markOffset") {
        m.offset = o;
    }
    m.weight = m.weight.clamp(0.05, 8.0);
    m.offset = m.offset.clamp(0.0, 72.0);
    m.any().then_some(m)
}

fn mix_parts(doc: &Doc, v: Option<&Value>, what: &str) -> Result<Vec<MixPart>> {
    let list = v.and_then(Value::as_array).ok_or_else(|| EngineError::new(format!("missing `{what}` ([{{ink, tint: 0..100}}])")))?;
    let mut parts = Vec::new();
    for c in list {
        let ink = c.get("ink").and_then(Value::as_str).ok_or_else(|| EngineError::new("each component needs an `ink`: Cyan, Magenta, Yellow, Black or a spot swatch name"))?;
        let key = ink_key(doc, ink)?;
        let tint = c.get("tint").and_then(Value::as_f64).unwrap_or(100.0);
        if !(0.0..=100.0).contains(&tint) {
            return Err(EngineError::new(format!("the tint of {ink} must be 0 to 100")));
        }
        if tint > 0.0 {
            parts.push(MixPart { ink: key, tint: tint / 100.0 });
        }
    }
    if parts.is_empty() {
        return Err(EngineError::new("a mixed ink needs at least one component with a tint above 0"));
    }
    Ok(parts)
}

fn ink_key(doc: &Doc, ink: &str) -> Result<String> {
    if let Some(i) = process_plate(ink) {
        return Ok(PROCESS[i].to_string());
    }
    match doc.swatch(ink) {
        Some(s) if s.spot && matches!(s.value, SwatchValue::Color { .. }) => Ok(s.name.clone()),
        Some(_) => Err(EngineError::new(format!("{ink} is not a spot ink: a mixed ink combines process inks and spot swatches"))),
        None => Err(EngineError::new(format!("no ink named {ink}: use Cyan, Magenta, Yellow, Black or a spot swatch name"))),
    }
}

fn mixed_json(doc: &Doc, name: &str) -> Value {
    let Some(SwatchValue::Mixed { parts }) = doc.swatch(name).map(|s| &s.value) else { return json!({"name": name}) };
    let e = doc.mix_eval(parts, 1.0);
    json!({
        "name": name,
        "components": parts.iter().map(|p| json!({"ink": wire::swatch_name(&p.ink), "tint": (p.tint * 1000.0).round() / 10.0})).collect::<Vec<_>>(),
        "hex": e.as_ref().map(|e| formdesign_model::color::Color::Rgb(e.rgb).hex()),
        "cmyk": e.as_ref().map(|e| e.cmyk.map(|v| (v * 1000.0).round() / 10.0)),
    })
}

fn mixed_ink(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = swatch_key(args::str_(p, "name", "the mixed ink's name")?);
    if name.is_empty() {
        return Err(EngineError::new("missing `name` (the mixed ink's name)"));
    }
    let parts = mix_parts(c.doc()?, p.get("components"), "components")?;
    let doc = c.doc_mut()?;
    match doc.swatches.iter_mut().find(|s| s.name == name) {
        Some(s) if matches!(s.value, SwatchValue::Mixed { .. }) => s.value = SwatchValue::Mixed { parts },
        Some(_) => return Err(EngineError::new(format!("there is already a swatch named {name} that is not a mixed ink"))),
        None => doc.swatches.push(Swatch { name: name.clone(), value: SwatchValue::Mixed { parts }, spot: false, group: None, locked: false }),
    }
    c.label("New Mixed Ink Swatch");
    Ok(mixed_json(c.doc()?, &name))
}

fn mixed_ink_group(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = swatch_key(args::str_(p, "name", "the group's name")?);
    if name.is_empty() {
        return Err(EngineError::new("missing `name` (the group's name)"));
    }
    struct Base {
        ink: String,
        initial: f64,
        repeat: u64,
        increment: f64,
    }
    let list = p.get("inks").and_then(Value::as_array).ok_or_else(|| EngineError::new("missing `inks` ([{ink, initial: 0..100, repeat: steps, increment: percent}])"))?;
    let mut bases = Vec::new();
    for b in list {
        let ink = b.get("ink").and_then(Value::as_str).ok_or_else(|| EngineError::new("each base ink needs an `ink`"))?;
        bases.push(Base {
            ink: ink_key(c.doc()?, ink)?,
            initial: b.get("initial").and_then(Value::as_f64).unwrap_or(0.0),
            repeat: b.get("repeat").and_then(Value::as_u64).unwrap_or(0),
            increment: b.get("increment").and_then(Value::as_f64).unwrap_or(0.0),
        });
    }
    if bases.is_empty() || bases.len() > 4 {
        return Err(EngineError::new("a mixed ink group takes one to four base inks"));
    }
    let total: u64 = bases.iter().map(|b| b.repeat + 1).product();
    if total > 1000 {
        return Err(EngineError::new(format!("that makes {total} swatches; a mixed ink group is limited to 1000")));
    }
    // One swatch for every combination of steps, the first ink varying slowest.
    let mut made: Vec<Swatch> = Vec::new();
    for n in 0..total {
        let mut rem = n;
        let mut steps = vec![0u64; bases.len()];
        for (i, b) in bases.iter().enumerate().rev() {
            steps[i] = rem % (b.repeat + 1);
            rem /= b.repeat + 1;
        }
        let parts: Vec<MixPart> = bases
            .iter()
            .zip(&steps)
            .map(|(b, s)| MixPart { ink: b.ink.clone(), tint: ((b.initial + b.increment * *s as f64).clamp(0.0, 100.0)) / 100.0 })
            .filter(|p| p.tint > 0.0)
            .collect();
        if parts.is_empty() {
            continue;
        }
        made.push(Swatch { name: format!("{name} {}", n + 1), value: SwatchValue::Mixed { parts }, spot: false, group: Some(name.clone()), locked: false });
    }
    let doc = c.doc_mut()?;
    // Regenerating a group replaces the swatches it made before.
    let old = doc.prefs.get("mixedInkGroups").and_then(|g| g.get(&name)).and_then(|g| g.get("swatches")).and_then(Value::as_array).cloned().unwrap_or_default();
    for o in old.iter().filter_map(Value::as_str) {
        doc.swatches.retain(|s| s.name != o);
    }
    for s in &made {
        if doc.swatches.iter().any(|x| x.name == s.name) {
            return Err(EngineError::new(format!("there is already a swatch named {}", s.name)));
        }
    }
    let names: Vec<String> = made.iter().map(|s| s.name.clone()).collect();
    doc.swatches.extend(made);
    let mut groups = doc.prefs.get("mixedInkGroups").and_then(Value::as_object).cloned().unwrap_or_default();
    groups.insert(
        name.clone(),
        json!({"inks": bases.iter().map(|b| json!({"ink": b.ink, "initial": b.initial, "repeat": b.repeat, "increment": b.increment})).collect::<Vec<_>>(), "swatches": names}),
    );
    doc.prefs.insert("mixedInkGroups".into(), Value::Object(groups));
    c.label("New Mixed Ink Group");
    let doc = c.doc()?;
    Ok(json!({"group": name, "count": names.len(), "swatches": names.iter().map(|n| mixed_json(doc, n)).collect::<Vec<_>>()}))
}

fn plates(c: &mut Ctx, _: &Value) -> Result<Value> {
    let doc = c.doc()?;
    let out: Vec<Value> = doc
        .plate_names()
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let hex = if i < 4 { ["#00aeef", "#ec008c", "#fff200", "#231f20"][i].to_string() } else { doc.swatch_color(n, 1.0, 0).map(|c| c.hex()).unwrap_or_default() };
            json!({"plate": i, "name": if i < 4 { n.clone() } else { wire::swatch_name(n) }, "kind": if i < 4 { "process" } else { "spot" }, "hex": hex})
        })
        .collect();
    Ok(json!({"plates": out}))
}

fn file_stem(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.' | '(' | ')') { c } else { '_' }).collect();
    s.trim().to_string()
}

/// The plates an export writes: the ones named in `inks`, or every plate that puts ink on a selected page.
fn chosen_plates(doc: &Doc, p: &Value, scenes: &[PageScene]) -> Result<Vec<usize>> {
    let names = doc.plate_names();
    if let Some(list) = p.get("inks").and_then(Value::as_array) {
        let mut out = Vec::new();
        for n in list.iter().filter_map(Value::as_str) {
            let i = process_plate(n).or_else(|| names.iter().position(|x| x.eq_ignore_ascii_case(&swatch_key(n)))).ok_or_else(|| EngineError::new(format!("no plate named {n}: the plates are {}", names.join(", "))))?;
            if !out.contains(&i) {
                out.push(i);
            }
        }
        return Ok(out);
    }
    let used: Vec<usize> = (0..names.len()).filter(|i| scenes.iter().any(|s| formdesign_render::sep::plate_used(doc, &s.ops, *i as u8))).collect();
    if used.is_empty() {
        return Err(EngineError::new("the selected pages put no ink on any plate"));
    }
    Ok(used)
}

/// The marks on a plate: what the request asks for, with registration marks added unless the request names marks of
/// its own; `marks: false` draws none.
fn plate_marks(p: &Value) -> Option<Marks> {
    if p.get("marks").and_then(Value::as_bool) == Some(false) {
        return None;
    }
    let asked = p.get("marks").is_some() || ["cropMarks", "bleedMarks", "registrationMarks", "colorBars", "pageInformation"].iter().any(|k| p.get(*k).is_some());
    match marks_from(p) {
        Some(m) if asked => Some(m),
        _ => Some(Marks { registration: true, offset: 6.0, ..Marks::default() }),
    }
}

/// One grey PDF per plate for the given pages, with the marks the request asks for and registration marks by default.
pub fn plate_pdfs(c: &mut Ctx, p: &Value, pages: &[usize], bleed: bool) -> Result<Vec<(String, Vec<u8>)>> {
    let lay = c.layout()?;
    let renderer = c.session.renderer.clone();
    let doc = c.doc()?;
    let opts = SceneOpts { printing: true, bleed, overprint: false };
    let scenes: Vec<PageScene> = pages.iter().filter_map(|pg| page_scene(doc, &lay, *pg, &opts)).collect();
    let marks = plate_marks(p);
    let font = renderer.fonts.any_face().map(MarkFont);
    let names = doc.plate_names();
    let mut out = Vec::new();
    for i in chosen_plates(doc, p, &scenes)? {
        let popts = PdfOpts { title: doc.title.clone(), marks: marks.clone(), mark_font: font.clone(), plate: Some(i as u8), ..Default::default() };
        let bytes = formdesign_render::pdf::export(&scenes, doc, &renderer.images, &popts).map_err(|e| EngineError::new(format!("separation PDF: {e}")))?;
        out.push((names[i].clone(), bytes));
    }
    Ok(out)
}

fn export_separations(c: &mut Ctx, p: &Value) -> Result<Value> {
    let pages = crate::cmd::file::page_list(c.doc()?, args::opt_str(p, "pages"))?;
    let bleed = args::opt_bool(p, "bleed").unwrap_or(false);
    let format = args::opt_str(p, "format").unwrap_or("pdf");
    if format != "pdf" && format != "png" {
        return Err(EngineError::new("`format` expected pdf or png"));
    }
    // A target with an extension names the files (`proof.pdf` gives `proof_Cyan.pdf`); without one it is a folder.
    let target: Option<(PathBuf, Option<String>)> = args::opt_str(p, "path").map(|s| {
        let path = Path::new(s);
        match path.extension().and_then(|e| e.to_str()) {
            Some(ext) if ext.eq_ignore_ascii_case("pdf") || ext.eq_ignore_ascii_case("png") => (path.parent().map(Path::to_path_buf).unwrap_or_default(), path.file_stem().map(|x| x.to_string_lossy().to_string())),
            _ => (path.to_path_buf(), None),
        }
    });
    let mut files: Vec<(String, String, Vec<u8>)> = Vec::new();
    let mut plate_list = Vec::new();
    if format == "pdf" {
        for (ink, bytes) in plate_pdfs(c, p, &pages, bleed)? {
            let file = match target.as_ref().and_then(|t| t.1.as_ref()) {
                Some(stem) => format!("{stem}_{}.pdf", file_stem(&ink)),
                None => format!("{}.pdf", file_stem(&ink)),
            };
            plate_list.push(ink.clone());
            files.push((ink, file, bytes));
        }
    } else {
        let lay = c.layout()?;
        let renderer: Arc<formdesign_render::Renderer> = c.session.renderer.clone();
        let doc = c.doc()?;
        let scale = args::opt_f64(p, "scale").unwrap_or(1.0).clamp(0.05, 8.0);
        let opts = SceneOpts { printing: true, bleed, overprint: false };
        let scenes: Vec<PageScene> = pages.iter().filter_map(|pg| page_scene(doc, &lay, *pg, &opts)).collect();
        let marks = plate_marks(p);
        let face = renderer.fonts.any_face();
        let names = doc.plate_names();
        for i in chosen_plates(doc, p, &scenes)? {
            plate_list.push(names[i].clone());
            for sc in &scenes {
                let framed = match &marks {
                    Some(m) => m.framed(doc, sc, &doc.title, Some(&names[i]), face.as_ref()),
                    None => PageScene { page: sc.page, width: sc.width, height: sc.height, bleed: sc.bleed, ops: sc.ops.clone() },
                };
                let r = formdesign_render::rasterize(&framed, doc, &renderer.outlines, &renderer.images, &formdesign_render::RasterOpts { scale, plate: Some(i as u8), ..Default::default() });
                let png = formdesign_render::encode_png(&r).ok_or_else(|| EngineError::new("could not encode a plate image"))?;
                let stem = target.as_ref().and_then(|t| t.1.clone());
                let page = doc.page_name(sc.page);
                let base = match (&stem, scenes.len()) {
                    (Some(s), 1) => format!("{s}_{}.png", file_stem(&names[i])),
                    (Some(s), _) => format!("{s}_{}_p{}.png", file_stem(&names[i]), file_stem(&page)),
                    (None, 1) => format!("{}.png", file_stem(&names[i])),
                    (None, _) => format!("{}_p{}.png", file_stem(&names[i]), file_stem(&page)),
                };
                files.push((names[i].clone(), base, png));
            }
        }
    }
    let mut out = Vec::new();
    for (ink, file, bytes) in files {
        match &target {
            Some((dir, _)) => {
                std::fs::create_dir_all(dir).map_err(|e| EngineError::new(format!("cannot create {}: {e}", dir.display())))?;
                let full = dir.join(&file);
                std::fs::write(&full, &bytes).map_err(|e| EngineError::new(format!("cannot write {}: {e}", full.display())))?;
                out.push(json!({"ink": ink, "file": file, "path": full.display().to_string(), "bytes": bytes.len()}));
            }
            None => out.push(json!({"ink": ink, "file": file, "base64": formdesign_interchange::idml::b64(&bytes), "bytes": bytes.len()})),
        }
    }
    Ok(json!({"plates": plate_list, "pages": pages.len(), "format": format, "files": out}))
}

pub fn register(r: &mut Registry) {
    cmd("swatch.mixedInk", "New Mixed Ink Swatch").menu(&["Window", "Color", "New Mixed Ink Swatch..."]).sig("{name, components: [{ink: Cyan|Magenta|Yellow|Black|spot swatch name, tint: 0..100}]}").reg(r, mixed_ink);
    cmd("swatch.mixedInkGroup", "New Mixed Ink Group").menu(&["Window", "Color", "New Mixed Ink Group..."]).sig("{name, inks: [{ink, initial: 0..100, repeat: steps, increment: percent}]}").reg(r, mixed_ink_group);
    cmd("ink.plates", "Separation Plates").query().sig("{}").reg(r, plates);
    cmd("file.exportSeparations", "Export Separations...")
        .menu(&["File", "Export", "Separations..."])
        .query()
        .sig("{pages?, bleed?, format?: pdf|png, inks?: [names], scale?: png pixels per point, marks?: bool | {crop, bleed, registration, colorBars, pageInfo, type, weight, offset}, cropMarks?, bleedMarks?, registrationMarks?, colorBars?, pageInformation?, markType?: default|japanese, markWeight?: pt, markOffset?: pt, path?: folder, or file name giving name_Ink.ext}")
        .reg(r, export_separations);
}
