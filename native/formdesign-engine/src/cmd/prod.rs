use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use formdesign_model::attrs::{Attrs, KEYS, Kind, Level};
use formdesign_model::doc::{Doc, Home};
use formdesign_model::page::Section;
use formdesign_model::geom::{Rect, Vec2};
use formdesign_model::ids::ItemId;
use formdesign_model::item::{ItemKind, PathText, TextFrame, map_rect};
use formdesign_model::story::{Para, Seg};
use formdesign_model::style::StyleKind;
use formdesign_render::scene::{Brush, Op};
use kurbo::{BezPath, PathEl};
use serde_json::{Map, Value, json};

use crate::registry::{Needs, Registry, cmd};
use crate::session::{Ctx, Selection};
use crate::target::{items, text_targets};
use crate::{EngineError, Result, args, ops};

fn inks_of(doc: &Doc) -> Map<String, Value> {
    doc.prefs.get("inks").and_then(Value::as_object).cloned().unwrap_or_default()
}

fn ink_list(c: &mut Ctx, _: &Value) -> Result<Value> {
    let doc = c.doc()?;
    let inks = inks_of(doc);
    let all = inks.get("$allToProcess").and_then(Value::as_bool).unwrap_or(false);
    let out: Vec<Value> = doc
        .swatches
        .iter()
        .filter(|s| s.spot)
        .map(|s| {
            let o = inks.get(&s.name);
            let process = all || o.and_then(|o| o.get("toProcess")).and_then(Value::as_bool).unwrap_or(false);
            let alias = o.and_then(|o| o.get("alias")).and_then(Value::as_str).filter(|a| !a.is_empty());
            let hex = doc.swatch_color(&s.name, 1.0, 0).map(|c| c.hex());
            json!({"name": crate::wire::swatch_name(&s.name), "key": s.name, "process": process, "alias": alias, "hex": hex})
        })
        .collect();
    Ok(json!({"inks": out, "allToProcess": all}))
}

fn ink_key(doc: &Doc, name: &str) -> Result<String> {
    doc.swatch(name).filter(|s| s.spot).map(|s| s.name.clone()).ok_or_else(|| EngineError::new(format!("no spot ink named {name}")))
}

fn ink_options(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = args::str_(p, "ink", "a spot ink's name from ink.list")?.to_string();
    let doc = c.doc()?;
    let key = ink_key(doc, &name)?;
    let alias = match args::opt_str(p, "alias") {
        Some("") | Some("none") => Some(String::new()),
        Some(a) => {
            let k = ink_key(doc, a)?;
            if k == key {
                return Err(EngineError::new("an ink cannot be an alias of itself"));
            }
            Some(k)
        }
        None => None,
    };
    let process = args::opt_bool(p, "toProcess").or_else(|| args::opt_bool(p, "process"));
    let mut inks = inks_of(doc);
    let e = inks.entry(key.clone()).or_insert_with(|| json!({}));
    if let Some(v) = process {
        e["toProcess"] = json!(v);
    }
    if let Some(a) = alias {
        e["alias"] = json!(a);
    }
    c.doc_mut()?.prefs.insert("inks".into(), Value::Object(inks));
    c.label("Ink Manager");
    ink_list(c, &json!({}))
}

fn ink_all(c: &mut Ctx, p: &Value) -> Result<Value> {
    let on = args::opt_bool(p, "on").unwrap_or(true);
    let mut inks = inks_of(c.doc()?);
    inks.insert("$allToProcess".into(), json!(on));
    c.doc_mut()?.prefs.insert("inks".into(), Value::Object(inks));
    c.label("All Spots to Process");
    ink_list(c, &json!({}))
}

fn open_type(c: &mut Ctx, p: &Value) -> Result<Value> {
    let targets = text_targets(c)?;
    let doc = c.doc()?;
    let (sid, range) = targets[0].clone();
    let st = doc.story(sid).ok_or_else(|| EngineError::new("no such story"))?;
    let (pi, off) = st.locate(range.start);
    let para = &st.paras[pi];
    let (cs, over) = para.format_at(if range.is_empty() { off } else { off + 1 });
    let stack = doc.styles.char_stack(&para.style, &para.over, cs.as_deref(), &over);
    let mut set: BTreeSet<String> = match p.get("features") {
        Some(Value::Array(a)) => a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
        Some(Value::Object(o)) => {
            let mut s: BTreeSet<String> = stack.strings("features").into_iter().collect();
            for (k, v) in o {
                if v.as_bool().unwrap_or(false) {
                    s.insert(k.clone());
                } else {
                    s.remove(k);
                }
            }
            s
        }
        _ => stack.strings("features").into_iter().collect(),
    };
    let list = |k: &str| -> Vec<String> { p.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default() };
    for f in list("add") {
        set.insert(f);
    }
    for f in list("remove") {
        set.remove(&f);
    }
    if let Some(bad) = set.iter().find(|f| f.len() != 4 || !f.chars().all(|ch| ch.is_ascii_alphanumeric())) {
        return Err(EngineError::new(format!("{bad} is not an OpenType feature tag (four letters or digits, e.g. liga, onum, ss01)")));
    }
    let v: Vec<String> = set.into_iter().collect();
    let mut m = Map::new();
    m.insert("features".into(), json!(v));
    let doc = c.doc_mut()?;
    for (sid, range) in targets {
        if let Some(st) = doc.story_mut(sid) {
            st.set_char_attrs(range, &m)?;
        }
    }
    c.label("OpenType");
    Ok(json!({"features": v}))
}

fn change_case(c: &mut Ctx, p: &Value) -> Result<Value> {
    let mode = args::str_(p, "mode", "upper, lower, title or sentence")?.to_string();
    if !["upper", "lower", "title", "sentence"].contains(&mode.as_str()) {
        return Err(EngineError::new(format!("`mode` expected upper, lower, title or sentence, not {mode}")));
    }
    let targets = text_targets(c)?;
    let doc = c.doc_mut()?;
    for (sid, range) in targets {
        let Some(st) = doc.story_mut(sid) else { continue };
        let mut word_start = true;
        let mut sentence_start = range.start == 0;
        st.for_segs_in(range, |seg| {
            let Seg::Text { text, .. } = seg else { return };
            let mut out = String::with_capacity(text.len());
            for ch in text.chars() {
                let next = match mode.as_str() {
                    "upper" => ch.to_uppercase().collect::<String>(),
                    "lower" => ch.to_lowercase().collect(),
                    "title" => {
                        if word_start && ch.is_alphabetic() { ch.to_uppercase().collect() } else { ch.to_lowercase().collect() }
                    }
                    _ => {
                        if sentence_start && ch.is_alphabetic() { ch.to_uppercase().collect() } else { ch.to_lowercase().collect() }
                    }
                };
                out.push_str(&next);
                if ch.is_alphabetic() {
                    sentence_start = false;
                }
                if ".!?".contains(ch) || ch == '\n' {
                    sentence_start = true;
                }
                word_start = !(ch.is_alphanumeric() || ch == '\'' || ch == '\u{2019}');
            }
            if out.chars().count() == text.chars().count() {
                *text = out;
            }
        });
    }
    c.label("Change Case");
    Ok(json!({"mode": mode}))
}

fn clear_overrides(c: &mut Ctx, p: &Value) -> Result<Value> {
    let which = args::opt_str(p, "level").unwrap_or("all").to_string();
    let targets = text_targets(c)?;
    let doc = c.doc_mut()?;
    for (sid, range) in targets {
        let Some(st) = doc.story_mut(sid) else { continue };
        if which != "paragraph" {
            st.for_segs_in(range.clone(), |seg| {
                let (_, over) = seg.format_mut();
                over.retain(|k, _| ["change", "conditions", "xmlTag"].contains(&k.as_str()));
            });
        }
        if which != "character" {
            for i in st.paras_in(range) {
                st.paras[i].over.clear();
            }
        }
    }
    c.label("Clear Overrides");
    Ok(json!({}))
}

fn on_path(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let id = ids[0];
    let text = args::opt_str(p, "text").unwrap_or("").to_string();
    let doc = c.doc()?;
    let it = doc.item(id).ok_or_else(|| EngineError::new("no such object"))?;
    if matches!(it.kind, ItemKind::Group { .. } | ItemKind::States { .. } | ItemKind::Graphic(_)) {
        return Err(EngineError::new("type on a path needs a path, line or empty frame"));
    }
    if matches!(it.kind, ItemKind::Text(_)) && it.text().is_some_and(|t| t.on_path.is_none()) {
        return Err(EngineError::new("that frame already holds text: use a path, a line or an empty frame"));
    }
    let doc = c.doc_mut()?;
    let sid = match doc.item(id).and_then(|i| i.story()) {
        Some(s) => s,
        None => {
            let s = doc.new_story(formdesign_model::style::BASIC_PARA);
            let mut tf = TextFrame::new(s);
            tf.on_path = Some(PathText { start: args::opt_f64(p, "start").unwrap_or(0.0), flip: args::opt_bool(p, "flip").unwrap_or(false), align: args::opt_str(p, "align").unwrap_or("baseline").to_string() });
            if let Some(it) = doc.item_mut(id) {
                it.kind = ItemKind::Text(tf);
            }
            if let Some(st) = doc.story_mut(s) {
                st.frames = vec![id];
            }
            s
        }
    };
    if !text.is_empty()
        && let Some(st) = doc.story_mut(sid)
    {
        st.set_text(&text);
    }
    c.set_selection(Selection::items(vec![id]));
    c.label("Type on a Path");
    Ok(json!({"id": id, "story": sid}))
}

fn fonts_used(doc: &Doc) -> BTreeMap<String, usize> {
    let mut out: BTreeMap<String, usize> = BTreeMap::new();
    let mut add = |a: &Attrs| {
        if let Some(f) = a.get("font").and_then(Value::as_str) {
            *out.entry(f.to_string()).or_default() += 1;
        }
    };
    for s in doc.styles.paragraph.iter().chain(doc.styles.character.iter()) {
        add(&s.attrs);
    }
    crate::walk::all_paras(doc, &mut |_, p| {
        add(&p.over);
        for seg in &p.segs {
            add(seg.over());
        }
    });
    out
}

fn font_list(c: &mut Ctx, _: &Value) -> Result<Value> {
    let fams: BTreeSet<String> = c.session.renderer.fonts.families().into_iter().collect();
    let used = fonts_used(c.doc()?);
    let out: Vec<Value> = used.iter().map(|(f, n)| json!({"family": f, "uses": n, "available": fams.contains(f)})).collect();
    Ok(json!({"fonts": out, "families": fams}))
}

fn font_axes(c: &mut Ctx, p: &Value) -> Result<Value> {
    let mut family = args::opt_str(p, "font").map(str::to_string);
    let mut style = args::opt_str(p, "style").map(str::to_string);
    let mut set: BTreeMap<String, f32> = BTreeMap::new();
    if family.is_none()
        && let Ok(t) = text_targets(c)
        && let Some((sid, range)) = t.first().cloned()
        && let Ok(doc) = c.doc()
        && let Some(st) = doc.story(sid)
    {
        let (pi, off) = st.locate(range.start);
        let para = &st.paras[pi];
        let (cs, over) = para.format_at(if range.is_empty() { off } else { off + 1 });
        let stack = doc.styles.char_stack(&para.style, &para.over, cs.as_deref(), &over);
        family = Some(stack.text("font"));
        style = Some(stack.text("fontStyle"));
        if let Some(o) = stack.get("fontVariations").as_object() {
            for (k, v) in o {
                if let Some(f) = v.as_f64() {
                    set.insert(k.clone(), f as f32);
                }
            }
        }
    }
    if let Some(o) = p.get("variations").and_then(Value::as_object) {
        for (k, v) in o {
            if let Some(f) = v.as_f64() {
                set.insert(k.clone(), f as f32);
            }
        }
    }
    let family = family.ok_or_else(|| EngineError::new("missing `font` (a family name) and nothing is selected"))?;
    let style = style.unwrap_or_else(|| "Regular".into());
    let fonts = &c.session.renderer.fonts;
    let face = fonts.resolve(&family, &style).ok_or_else(|| EngineError::new(format!("no font named {family}")))?;
    let root = fonts.face(face.root).unwrap_or_else(|| face.clone());
    let settings: Vec<(String, f32)> = set.into_iter().collect();
    let cur = fonts.varied(&face, &settings);
    let axes = root.axes();
    let value_of = |tag: &str, default: f32| cur.variations.iter().find(|(t, _)| t == tag).map(|(_, v)| *v).unwrap_or(default);
    let out: Vec<Value> = axes
        .iter()
        .filter(|a| !a.hidden)
        .map(|a| json!({"tag": a.tag, "name": a.name, "min": a.min, "default": a.default, "max": a.max, "value": value_of(&a.tag, a.default)}))
        .collect();
    let instances: Vec<Value> = root.named_instances().iter().map(|i| json!({"name": i.name, "coords": i.coords.iter().map(|(t, v)| (t.clone(), json!(v))).collect::<Map<String, Value>>()})).collect();
    Ok(json!({"font": root.family, "style": cur.style, "variable": root.is_variable(), "axes": out, "instances": instances}))
}

fn font_replace(c: &mut Ctx, p: &Value) -> Result<Value> {
    let from = args::str_(p, "from", "the font family to replace")?.to_string();
    let to = args::str_(p, "to", "the font family to use instead")?.to_string();
    let style = args::opt_str(p, "style").map(str::to_string);
    let styles_too = args::opt_bool(p, "styles").unwrap_or(true);
    let fix = |a: &mut Attrs| -> bool {
        if a.get("font").and_then(Value::as_str) == Some(from.as_str()) {
            a.insert("font".into(), json!(to));
            if let Some(s) = &style {
                a.insert("fontStyle".into(), json!(s));
            }
            true
        } else {
            false
        }
    };
    let doc = c.doc_mut()?;
    let mut n = 0;
    if styles_too {
        for s in doc.styles.paragraph.iter_mut().chain(doc.styles.character.iter_mut()) {
            n += usize::from(fix(&mut s.attrs));
        }
    }
    let mut runs = 0;
    crate::walk::paras_where(doc, &|q: &Para| q.over.get("font").and_then(Value::as_str) == Some(from.as_str()) || q.segs.iter().any(|s| s.over().get("font").and_then(Value::as_str) == Some(from.as_str())), &mut |q: &mut Para| {
        runs += usize::from(fix(&mut q.over));
        for seg in q.segs.iter_mut() {
            let (_, over) = seg.format_mut();
            runs += usize::from(fix(over));
        }
    });
    c.label("Find/Replace Font");
    Ok(json!({"styles": n, "runs": runs}))
}

fn load_doc(p: &Value) -> Result<Doc> {
    let (name, data) = match (args::opt_str(p, "base64"), args::opt_str(p, "path")) {
        (Some(b), _) => (args::opt_str(p, "name").unwrap_or("styles.fdoc").to_string(), formdesign_interchange::idml::unb64(b)),
        (None, Some(path)) => (path.to_string(), std::fs::read(path).map_err(|e| EngineError::new(format!("cannot read {path}: {e}")))?),
        _ => return Err(EngineError::new("missing `path` (a .fdes, .fdoc or .idml document) or `base64` with `name`")),
    };
    if crate::cmd::file::is_idml(&data) {
        return formdesign_interchange::idml::read::read(&data).map_err(|e| EngineError::new(format!("{name}: {e}")));
    }
    if let Ok(mut z) = zip::ZipArchive::new(std::io::Cursor::new(&data))
        && let Ok(mut f) = z.by_name("document.fdoc")
    {
        let mut inner = Vec::new();
        std::io::Read::read_to_end(&mut f, &mut inner).map_err(|e| EngineError::new(format!("{name}: {e}")))?;
        return formdesign_model::format::load(&inner).map_err(|e| EngineError::new(format!("{name}: {}", e.0)));
    }
    formdesign_model::format::load(&data).map_err(|e| EngineError::new(format!("{name}: {}", e.0)))
}

fn style_load(c: &mut Ctx, p: &Value) -> Result<Value> {
    let src = load_doc(p)?;
    let kinds: Vec<String> = match p.get("kinds").and_then(Value::as_array) {
        Some(a) => a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
        None => vec!["paragraph".into(), "character".into(), "object".into(), "table".into(), "cell".into()],
    };
    let overwrite = args::opt_bool(p, "overwrite").unwrap_or(true);
    let swatches = args::opt_bool(p, "swatches").unwrap_or(true);
    let doc = c.doc_mut()?;
    let mut added = 0;
    let mut replaced = 0;
    for k in &kinds {
        let Some(kind) = StyleKind::parse(k) else { return Err(EngineError::new(format!("`kinds` holds {k}: expected paragraph, character, object, table or cell"))) };
        let from = match kind {
            StyleKind::Paragraph => &src.styles.paragraph,
            StyleKind::Character => &src.styles.character,
            StyleKind::Object => &src.styles.object,
            StyleKind::Table => &src.styles.table,
            StyleKind::Cell => &src.styles.cell,
        };
        let list = doc.styles.list_mut(kind);
        for s in from {
            match list.iter().position(|x| x.name == s.name) {
                Some(i) if overwrite => {
                    list[i] = s.clone();
                    replaced += 1;
                }
                Some(_) => {}
                None => {
                    list.push(s.clone());
                    added += 1;
                }
            }
        }
    }
    let mut sw = 0;
    if swatches {
        for s in &src.swatches {
            if doc.swatch(&s.name).is_none() {
                doc.swatches.push(s.clone());
                sw += 1;
            }
        }
    }
    c.label("Load Styles");
    Ok(json!({"added": added, "replaced": replaced, "swatches": sw}))
}

fn place_load(c: &mut Ctx, p: &Value) -> Result<Value> {
    let paths: Vec<String> = p.get("paths").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
    if paths.is_empty() {
        return Err(EngineError::new("missing `paths` (the files to load into the place cursor)"));
    }
    c.state()?;
    for path in &paths {
        if !std::path::Path::new(path).is_file() {
            return Err(EngineError::new(format!("cannot read {path}")));
        }
    }
    for path in &paths {
        let name = std::path::Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or("placed").to_string();
        c.session.place_gun.push(json!({"name": name, "path": path, "page": 0}));
    }
    c.session.tool = "place".into();
    c.session.sel_rev += 1;
    Ok(json!({"loaded": c.session.place_gun.len()}))
}

fn which(cmd: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(cmd)).find(|p| p.is_file())
}

fn file_print(c: &mut Ctx, p: &Value) -> Result<Value> {
    let lay = c.layout()?;
    let renderer = c.session.renderer.clone();
    let doc = c.doc()?;
    let pages = crate::cmd::file::page_list(doc, args::opt_str(p, "pages"))?;
    let bleed = args::opt_bool(p, "bleed").unwrap_or(false);
    let popts = formdesign_render::pdf::PdfOpts { title: doc.title.clone(), marks: crate::cmd::inks::marks_from(p), mark_font: renderer.fonts.any_face().map(formdesign_render::pdf::MarkFont), ..Default::default() };
    let separations = args::opt_bool(p, "separations").unwrap_or(false);
    // Separations spool one grey plate per ink (the same plates file.exportSeparations writes); otherwise one composite.
    let jobs: Vec<(String, Vec<u8>)> = if separations {
        crate::cmd::inks::plate_pdfs(c, p, &pages, bleed)?
    } else {
        vec![("composite".to_string(), renderer.pdf_with(doc, &lay, Some(pages.clone()), bleed, &popts).map_err(|e| EngineError::new(format!("print: {e}")))?)]
    };
    let copies = args::opt_u64(p, "copies").unwrap_or(1).clamp(1, 999);
    let printer = args::opt_str(p, "printer").map(str::to_string);
    let tmps: Vec<(String, std::path::PathBuf)> = jobs
        .iter()
        .enumerate()
        .map(|(i, (ink, bytes))| {
            let tmp = std::env::temp_dir().join(format!("formdesign-print-{}-{i}.pdf", std::process::id()));
            std::fs::write(&tmp, bytes).map(|_| (ink.clone(), tmp)).map_err(|e| EngineError::new(format!("print: {e}")))
        })
        .collect::<Result<_>>()?;
    let (prog, argv): (std::path::PathBuf, Vec<String>) = if let Some(lp) = which("lp") {
        let mut a = vec!["-n".into(), copies.to_string()];
        if let Some(pr) = &printer {
            a.push("-d".into());
            a.push(pr.clone());
        }
        (lp, a)
    } else if let Some(lpr) = which("lpr") {
        let mut a = vec![format!("-#{copies}")];
        if let Some(pr) = &printer {
            a.push("-P".into());
            a.push(pr.clone());
        }
        (lpr, a)
    } else {
        let files: Vec<Value> = tmps.iter().map(|(ink, t)| json!({"plate": ink, "pdf": t.display().to_string()})).collect();
        let first = tmps.first().map(|(_, t)| t.display().to_string());
        return Ok(json!({"printed": false, "pdf": first, "plates": files, "message": "no print system (lp or lpr) was found; the pages were written to PDF files to print from another app"}));
    };
    let mut messages = Vec::new();
    for (ink, tmp) in &tmps {
        let mut a = argv.clone();
        a.push(tmp.display().to_string());
        let out = std::process::Command::new(&prog).args(&a).output().map_err(|e| EngineError::new(format!("print: {e}")))?;
        if !out.status.success() {
            return Err(EngineError::new(format!("print {ink}: {}", String::from_utf8_lossy(&out.stderr).trim())));
        }
        messages.push(String::from_utf8_lossy(&out.stdout).trim().to_string());
    }
    let plates: Vec<&str> = tmps.iter().map(|(i, _)| i.as_str()).collect();
    Ok(json!({"printed": true, "pages": pages.len(), "copies": copies, "printer": printer, "separations": separations, "plates": if separations { json!(plates) } else { Value::Null }, "message": messages.join("; ")}))
}

fn printers(_: &mut Ctx, _: &Value) -> Result<Value> {
    let Some(lp) = which("lpstat") else { return Ok(json!({"printers": [], "default": null})) };
    let list = std::process::Command::new(&lp).arg("-a").output().map(|o| String::from_utf8_lossy(&o.stdout).lines().filter_map(|l| l.split_whitespace().next().map(str::to_string)).collect::<Vec<_>>()).unwrap_or_default();
    let def = std::process::Command::new(&lp).arg("-d").output().ok().and_then(|o| String::from_utf8_lossy(&o.stdout).split(':').nth(1).map(|s| s.trim().to_string())).filter(|s| !s.is_empty());
    Ok(json!({"printers": list, "default": def}))
}

fn svg_path(p: &BezPath) -> String {
    let mut s = String::new();
    for el in p.elements() {
        let _ = match el {
            PathEl::MoveTo(a) => write!(s, "M{:.3} {:.3}", a.x, a.y),
            PathEl::LineTo(a) => write!(s, "L{:.3} {:.3}", a.x, a.y),
            PathEl::QuadTo(a, b) => write!(s, "Q{:.3} {:.3} {:.3} {:.3}", a.x, a.y, b.x, b.y),
            PathEl::CurveTo(a, b, d) => write!(s, "C{:.3} {:.3} {:.3} {:.3} {:.3} {:.3}", a.x, a.y, b.x, b.y, d.x, d.y),
            PathEl::ClosePath => write!(s, "Z"),
        };
    }
    s
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn paint_attr(b: &Brush, defs: &mut String, n: &mut usize) -> (String, f64) {
    let stops = |stops: &Vec<(f64, formdesign_model::color::Color, f64)>| stops.iter().map(|(at, col, a)| format!("<stop offset=\"{at:.4}\" stop-color=\"{}\" stop-opacity=\"{a:.3}\"/>", col.hex())).collect::<String>();
    match b {
        Brush::Solid(col, a) => (col.hex(), *a),
        Brush::Linear { start, end, stops: s, alpha } => {
            *n += 1;
            let _ = write!(defs, "<linearGradient id=\"g{n}\" gradientUnits=\"userSpaceOnUse\" x1=\"{:.3}\" y1=\"{:.3}\" x2=\"{:.3}\" y2=\"{:.3}\">{}</linearGradient>", start.x, start.y, end.x, end.y, stops(s));
            (format!("url(#g{n})"), *alpha)
        }
        Brush::Radial { center, radius, stops: s, alpha } => {
            *n += 1;
            let _ = write!(defs, "<radialGradient id=\"g{n}\" gradientUnits=\"userSpaceOnUse\" cx=\"{:.3}\" cy=\"{:.3}\" r=\"{:.3}\">{}</radialGradient>", center.x, center.y, radius, stops(s));
            (format!("url(#g{n})"), *alpha)
        }
    }
}

fn export_svg(c: &mut Ctx, p: &Value) -> Result<Value> {
    let lay = c.layout()?;
    let renderer = c.session.renderer.clone();
    let doc = c.doc()?;
    let page = match args::opt_u64(p, "page") {
        Some(0) => return Err(EngineError::new("`page` is 1-based")),
        Some(n) => n as usize - 1,
        None => 0,
    };
    let scene = formdesign_render::page_scene(doc, &lay, page, &formdesign_render::SceneOpts { printing: true, bleed: false, overprint: false }).ok_or_else(|| EngineError::new(format!("no page {}", page + 1)))?;
    let mut body = String::new();
    let mut defs = String::new();
    let mut n = 0usize;
    let mut open: Vec<&str> = Vec::new();
    // SVG has no mesh gradient that browsers draw, so meshes go out as their quads.
    let ops = formdesign_render::scene::flat_meshes(&scene.ops);
    for op in &ops {
        match op {
            Op::Fill { path, brush, even_odd } => {
                let (f, a) = paint_attr(brush, &mut defs, &mut n);
                let _ = write!(body, "<path d=\"{}\" fill=\"{f}\"{}{}/>", svg_path(path), if a < 0.999 { format!(" fill-opacity=\"{a:.3}\"") } else { String::new() }, if *even_odd { " fill-rule=\"evenodd\"" } else { "" });
            }
            Op::Stroke { path, brush, style } => {
                let (f, a) = paint_attr(brush, &mut defs, &mut n);
                let dash = if style.dash.is_empty() { String::new() } else { format!(" stroke-dasharray=\"{}\"", style.dash.iter().map(|d| format!("{d:.2}")).collect::<Vec<_>>().join(" ")) };
                let _ = write!(body, "<path d=\"{}\" fill=\"none\" stroke=\"{f}\" stroke-width=\"{:.3}\" stroke-linecap=\"{}\" stroke-linejoin=\"{}\" stroke-miterlimit=\"{}\"{}{dash}/>", svg_path(path), style.width, style.cap, formdesign_render::scene::join_parts(&style.join).0, formdesign_render::scene::join_parts(&style.join).1, if a < 0.999 { format!(" stroke-opacity=\"{a:.3}\"") } else { String::new() });
            }
            Op::Glyphs(g) => {
                let mut path = BezPath::new();
                for i in 0..g.glyphs.len() {
                    let outline = renderer.outlines.get(&g.face, g.glyphs[i].0);
                    for el in (formdesign_render::raster::glyph_xf(g, i) * outline.as_ref().clone()).elements() {
                        path.push(*el);
                    }
                }
                if path.elements().is_empty() {
                    continue;
                }
                let d = svg_path(&path);
                if let Some(b) = &g.fill {
                    let (f, a) = paint_attr(b, &mut defs, &mut n);
                    let _ = write!(body, "<path d=\"{d}\" fill=\"{f}\"{}><title>{}</title></path>", if a < 0.999 { format!(" fill-opacity=\"{a:.3}\"") } else { String::new() }, esc(&g.text));
                }
                if let Some((b, w)) = &g.stroke {
                    let (f, _) = paint_attr(b, &mut defs, &mut n);
                    let _ = write!(body, "<path d=\"{d}\" fill=\"none\" stroke=\"{f}\" stroke-width=\"{w:.3}\"/>");
                }
            }
            Op::Image { asset, xf, w, h } => {
                let Some(a) = doc.assets.get(asset) else { continue };
                let (mime, data) = match a.mime.as_str() {
                    "image/jpeg" | "image/png" | "image/gif" | "image/webp" => (a.mime.clone(), a.data.as_ref().clone()),
                    _ => match renderer.images.get(a, 2.0).and_then(|d| formdesign_render::encode_png(&formdesign_render::Raster { width: d.width, height: d.height, rgba: d.rgba.clone() })) {
                        Some(png) => ("image/png".to_string(), png),
                        None => continue,
                    },
                };
                let k = xf.as_coeffs();
                let _ = write!(body, "<image transform=\"matrix({:.6} {:.6} {:.6} {:.6} {:.4} {:.4})\" width=\"{w:.3}\" height=\"{h:.3}\" preserveAspectRatio=\"none\" href=\"data:{mime};base64,{}\"/>", k[0], k[1], k[2], k[3], k[4], k[5], formdesign_interchange::idml::b64(&data));
            }
            Op::PushClip(path) => {
                n += 1;
                let _ = write!(defs, "<clipPath id=\"c{n}\"><path d=\"{}\"/></clipPath>", svg_path(path));
                let _ = write!(body, "<g clip-path=\"url(#c{n})\">");
                open.push("g");
            }
            Op::PushOpacity(o) => {
                let _ = write!(body, "<g opacity=\"{o:.3}\">");
                open.push("g");
            }
            Op::PushMask(_) => {
                body.push_str("<g>");
                open.push("g");
            }
            Op::PushBlend(m) => {
                let mode: String = m.chars().flat_map(|ch| if ch.is_uppercase() { vec!['-', ch.to_ascii_lowercase()] } else { vec![ch] }).collect();
                let _ = write!(body, "<g style=\"mix-blend-mode:{mode}\">");
                open.push("g");
            }
            Op::Pop => {
                if open.pop().is_some() {
                    body.push_str("</g>");
                }
            }
            Op::Tag(_) | Op::Mesh { .. } => {}
            Op::Link { rect, target } => {
                let href = match target {
                    formdesign_render::scene::LinkTarget::Url(u) => esc(u),
                    formdesign_render::scene::LinkTarget::Page(p) => format!("#page-{}", p + 1),
                };
                let _ = write!(body, "<a href=\"{href}\"><rect x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" fill=\"transparent\"/></a>", rect.x0, rect.y0, rect.width(), rect.height());
            }
        }
    }
    while open.pop().is_some() {
        body.push_str("</g>");
    }
    let svg = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w:.3}pt\" height=\"{h:.3}pt\" viewBox=\"0 0 {w:.3} {h:.3}\"><title>{}</title><defs>{defs}</defs><rect width=\"{w:.3}\" height=\"{h:.3}\" fill=\"#ffffff\"/>{body}</svg>\n",
        esc(&doc.page_name(page)),
        w = scene.width,
        h = scene.height
    );
    crate::cmd::file::deliver(p, svg.into_bytes(), json!({"page": page + 1}))
}

fn lists_of(doc: &Doc) -> Map<String, Value> {
    doc.prefs.get("lists").and_then(Value::as_object).cloned().unwrap_or_default()
}

fn list_define(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = args::str_(p, "name", "the list's name")?.trim().to_string();
    if name.is_empty() {
        return Err(EngineError::new("`name` is empty"));
    }
    let mut lists = lists_of(c.doc()?);
    let e = lists.entry(name.clone()).or_insert_with(|| json!({"acrossStories": true, "continueFromPrevious": false}));
    for k in ["acrossStories", "continueFromPrevious"] {
        if let Some(v) = args::opt_bool(p, k) {
            e[k] = json!(v);
        }
    }
    let out = e.clone();
    c.doc_mut()?.prefs.insert("lists".into(), Value::Object(lists));
    c.label("Define Lists");
    Ok(json!({"name": name, "options": out}))
}

fn list_list(c: &mut Ctx, _: &Value) -> Result<Value> {
    let doc = c.doc()?;
    let mut lists = lists_of(doc);
    crate::walk::all_paras(doc, &mut |_, q| {
        if let Some(n) = q.over.get("listName").and_then(Value::as_str) {
            lists.entry(n.to_string()).or_insert_with(|| json!({"acrossStories": true, "continueFromPrevious": false}));
        }
    });
    Ok(json!({"lists": lists.into_iter().map(|(k, v)| json!({"name": k, "options": v})).collect::<Vec<_>>()}))
}

fn list_delete(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = args::str_(p, "name", "the list's name")?.to_string();
    let mut lists = lists_of(c.doc()?);
    if lists.remove(&name).is_none() {
        return Err(EngineError::new(format!("no list named {name}")));
    }
    c.doc_mut()?.prefs.insert("lists".into(), Value::Object(lists));
    c.label("Delete List");
    Ok(json!({}))
}

#[derive(Clone, Copy, Default)]
struct Pins {
    top: bool,
    bottom: bool,
    left: bool,
    right: bool,
    width: bool,
    height: bool,
}

fn pins_of(doc: &Doc, id: ItemId) -> Pins {
    let l = doc.item(id).and_then(|i| i.extra.get("liquid"));
    let b = |k: &str, d: bool| l.and_then(|o| o.get(k)).and_then(Value::as_bool).unwrap_or(d);
    Pins { top: b("pinTop", false), bottom: b("pinBottom", false), left: b("pinLeft", false), right: b("pinRight", false), width: b("resizeWidth", true), height: b("resizeHeight", true) }
}

fn axis(a0: f64, a1: f64, len: f64, len2: f64, pin_a: bool, pin_b: bool, resize: bool) -> (f64, f64) {
    let s = if len.abs() < 1e-9 { 1.0 } else { len2 / len };
    let w = a1 - a0;
    let nw = if resize { w * s } else { w };
    match (pin_a, pin_b) {
        (true, true) if resize => (a0, len2 - (len - a1)),
        (true, _) => (a0, a0 + nw),
        (false, true) => {
            let e = len2 - (len - a1);
            (e - nw, e)
        }
        (false, false) => {
            let cx = (a0 + a1) / 2.0 * s;
            (cx - nw / 2.0, cx + nw / 2.0)
        }
    }
}

pub fn adapt_page(doc: &mut Doc, abs: usize, old: (f64, f64), rule: &str) {
    let Some(pr) = doc.page_rect(abs) else { return };
    let (w2, h2) = (pr.width(), pr.height());
    let (w, h) = old;
    if (w - w2).abs() < 1e-6 && (h - h2).abs() < 1e-6 || rule == "off" {
        return;
    }
    let Some((si, _)) = doc.page_slot(abs) else { return };
    let ids: Vec<ItemId> = doc.spreads[si].items.iter().copied().filter(|i| doc.page_of(*i) == Some(abs)).collect();
    for id in ids {
        let Some(b) = doc.bounds(id) else { continue };
        let r = Rect::new(b.x0 - pr.x0, b.y0 - pr.y0, b.x1 - pr.x0, b.y1 - pr.y0);
        match rule {
            "scale" => {
                let s = (w2 / w).min(h2 / h);
                let off = ((w2 - w * s) / 2.0, (h2 - h * s) / 2.0);
                let m = kurbo::Affine::translate((pr.x0 + off.0, pr.y0 + off.1)) * kurbo::Affine::scale(s) * kurbo::Affine::translate((-pr.x0, -pr.y0));
                ops::transform_items(doc, &[id], m);
            }
            "recenter" => ops::move_items(doc, &[id], Vec2::new((w2 - w) / 2.0, (h2 - h) / 2.0)),
            _ => {
                let pins = if rule == "object" { pins_of(doc, id) } else { Pins { width: true, height: true, ..Pins::default() } };
                let (x0, x1) = axis(r.x0, r.x1, w, w2, pins.left, pins.right, pins.width);
                let (y0, y1) = axis(r.y0, r.y1, h, h2, pins.top, pins.bottom, pins.height);
                let nb = Rect::new(x0 + pr.x0, y0 + pr.y0, x1 + pr.x0, y1 + pr.y0);
                crate::cmd::object::resize_items(doc, &[id], map_rect(b, nb), true);
            }
        }
    }
}

const RULES: &[&str] = &["off", "scale", "recenter", "guide", "object"];

fn page_rule(c: &mut Ctx, p: &Value) -> Result<Value> {
    let rule = args::str_(p, "rule", "off, scale, recenter, guide or object")?.to_string();
    if !RULES.contains(&rule.as_str()) {
        return Err(EngineError::new(format!("`rule` expected off, scale, recenter, guide or object, not {rule}")));
    }
    let doc = c.doc()?;
    let n = doc.page_count();
    let pages: Vec<usize> = match p.get("pages").and_then(Value::as_array) {
        Some(a) => a.iter().filter_map(Value::as_u64).map(|k| k as usize).collect(),
        None => (1..=n).collect(),
    };
    if let Some(bad) = pages.iter().find(|k| **k == 0 || **k > n) {
        return Err(EngineError::new(format!("no page {bad} (pages are 1 to {n})")));
    }
    let doc = c.doc_mut()?;
    for k in &pages {
        if let Some(pg) = doc.page_mut(k - 1) {
            pg.layout_rule = if rule == "off" { None } else { Some(rule.clone()) };
        }
    }
    c.label("Liquid Page Rule");
    Ok(json!({"pages": pages, "rule": rule}))
}

fn liquid_object(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let doc = c.doc_mut()?;
    for id in &ids {
        if let Some(it) = doc.item_mut(*id) {
            let mut o = it.extra.get("liquid").and_then(Value::as_object).cloned().unwrap_or_default();
            for k in ["pinTop", "pinBottom", "pinLeft", "pinRight", "resizeWidth", "resizeHeight"] {
                if let Some(v) = p.get(k).and_then(Value::as_bool) {
                    o.insert(k.into(), json!(v));
                }
            }
            it.extra.insert("liquid".into(), Value::Object(o));
        }
    }
    c.label("Liquid Layout");
    Ok(json!({"ids": ids}))
}

fn page_size_liquid(c: &mut Ctx, p: &Value) -> Result<Value> {
    let w = args::f64_(p, "width", "page width in points")?.max(1.0);
    let h = args::f64_(p, "height", "page height in points")?.max(1.0);
    let doc = c.doc()?;
    let n = doc.page_count();
    let pages: Vec<usize> = match p.get("pages").and_then(Value::as_array) {
        Some(a) => a.iter().filter_map(Value::as_u64).map(|k| k as usize).collect(),
        None => (1..=n).collect(),
    };
    if let Some(bad) = pages.iter().find(|k| **k == 0 || **k > n) {
        return Err(EngineError::new(format!("no page {bad} (pages are 1 to {n})")));
    }
    let olds: Vec<(usize, (f64, f64), String)> = pages.iter().filter_map(|k| doc.page(k - 1).map(|pg| (k - 1, (pg.width, pg.height), args::opt_str(p, "rule").map(str::to_string).or_else(|| pg.layout_rule.clone()).unwrap_or_else(|| "off".into())))).collect();
    let set: BTreeSet<usize> = olds.iter().map(|o| o.0).collect();
    let doc = c.doc_mut()?;
    doc.repaginate(|pgs, _, _| {
        for (i, pg) in pgs.iter_mut().enumerate() {
            if set.contains(&i) {
                pg.width = w;
                pg.height = h;
            }
        }
    });
    for (abs, old, rule) in &olds {
        adapt_page(doc, *abs, *old, rule);
    }
    c.label("Page Size");
    Ok(json!({"pages": pages}))
}

fn create_alternate(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = args::opt_str(p, "name").unwrap_or("Alternate").to_string();
    let w = args::f64_(p, "width", "the alternate layout's page width in points")?.max(1.0);
    let h = args::f64_(p, "height", "the alternate layout's page height in points")?.max(1.0);
    let rule_param = args::opt_str(p, "rule").map(str::to_string);
    if let Some(r) = &rule_param
        && !RULES.contains(&r.as_str())
    {
        return Err(EngineError::new(format!("`rule` expected off, scale, recenter, guide or object, not {r}")));
    }
    let base = c.doc()?.clone();
    let n0 = base.page_count();
    let base_pages: Vec<formdesign_model::page::Page> = base.spreads.iter().flat_map(|s| s.pages.clone()).collect();
    let originals: Vec<(ItemId, usize)> = base.spreads.iter().flat_map(|s| s.items.clone()).filter_map(|i| base.page_of(i).map(|pg| (i, pg))).collect();
    let doc = c.doc_mut()?;
    doc.repaginate(|pages, _, d| {
        for pg in &base_pages {
            let id = d.blank_page().id;
            let mut x = pg.clone();
            x.id = id;
            x.width = w;
            x.height = h;
            x.joined = false;
            pages.push(x);
        }
    });
    let mut m = crate::transplant::Mapper::new(&base);
    for (id, k) in &originals {
        let (Some(o_old), Some(o_new), Some((si, _))) = (base.page_rect(*k), doc.page_rect(n0 + k), doc.page_slot(n0 + k)) else { continue };
        m.item(doc, *id, Home::Spread(si), Vec2::new(o_new.x0 - o_old.x0, o_new.y0 - o_old.y0));
    }
    for (k, pg) in base_pages.iter().enumerate() {
        let rule = rule_param.clone().or_else(|| pg.layout_rule.clone()).unwrap_or_else(|| "scale".into());
        adapt_page(doc, n0 + k, (pg.width, pg.height), &rule);
    }
    doc.sections.push(Section { start: n0, start_at: Some(1), style: "arabic".into(), prefix: String::new(), marker: name.clone(), include_prefix: false });
    doc.sections.sort_by_key(|s| s.start);
    let mut alts = doc.prefs.get("alternates").and_then(Value::as_array).cloned().unwrap_or_default();
    alts.push(json!({"name": name, "firstPage": n0, "pages": n0, "width": w, "height": h}));
    doc.prefs.insert("alternates".into(), Value::Array(alts));
    c.set_selection(Selection::default());
    c.label("Create Alternate Layout");
    Ok(json!({"name": name, "firstPage": n0 + 1, "pages": n0}))
}

fn to_spread(c: &mut Ctx, p: &Value) -> Result<Value> {
    let k = args::u64_(p, "page", "the 1-based page to join to the spread before it")? as usize;
    let join = args::opt_bool(p, "join").unwrap_or(true);
    let n = c.doc()?.page_count();
    if k < 2 || k > n {
        return Err(EngineError::new(format!("`page` must be 2 to {n}: the first page has no spread before it")));
    }
    let doc = c.doc_mut()?;
    doc.repaginate(|pages, _, _| {
        if let Some(pg) = pages.get_mut(k - 1) {
            pg.joined = join;
        }
    });
    c.label(if join { "Add Page to Spread" } else { "Remove Page from Spread" });
    Ok(json!({"spreads": c.doc()?.spreads.len()}))
}

fn shuffle(c: &mut Ctx, p: &Value) -> Result<Value> {
    let allow = args::opt_bool(p, "allow").unwrap_or(false);
    let doc = c.doc()?;
    let spreads: Vec<usize> = match args::opt_u64(p, "spread") {
        Some(s) if (s as usize) < doc.spreads.len() => vec![s as usize],
        Some(s) => return Err(EngineError::new(format!("no spread {s}"))),
        None => (0..doc.spreads.len()).collect(),
    };
    let mut targets: BTreeSet<usize> = BTreeSet::new();
    for si in &spreads {
        let first = doc.first_page_of_spread(*si);
        for k in 1..doc.spreads[*si].pages.len() {
            targets.insert(first + k);
        }
    }
    let doc = c.doc_mut()?;
    doc.repaginate(|pages, _, _| {
        for (i, pg) in pages.iter_mut().enumerate() {
            if targets.contains(&i) {
                pg.joined = !allow;
            }
        }
    });
    c.label("Allow Spread to Shuffle");
    Ok(json!({"allow": allow, "spreads": spreads}))
}

fn primary_frame(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let id = ids[0];
    let doc = c.doc()?;
    let Some(pi) = doc.parent_of(id) else { return Err(EngineError::new("a primary text frame is a text frame on a parent page")) };
    if doc.item(id).and_then(|i| i.text()).is_none() {
        return Err(EngineError::new("a primary text frame must be a text frame"));
    }
    let others: Vec<ItemId> = doc.parents[pi].items.clone();
    let doc = c.doc_mut()?;
    for o in others {
        if let Some(it) = doc.item_mut(o) {
            it.extra.remove("primary");
        }
    }
    if let Some(it) = doc.item_mut(id) {
        it.extra.insert("primary".into(), json!(true));
    }
    doc.settings.primary_text_frame = true;
    c.label("Primary Text Frame");
    Ok(json!({"id": id}))
}

fn attrs_catalog(c: &mut Ctx, p: &Value) -> Result<Value> {
    let level = match args::opt_str(p, "level") {
        Some("character") | Some("char") => Some(Level::Char),
        Some("paragraph") | Some("para") => Some(Level::Para),
        _ => None,
    };
    let mut values: Option<(Map<String, Value>, Map<String, Value>)> = None;
    if let Ok(t) = text_targets(c)
        && let Some((sid, range)) = t.first().cloned()
        && let Ok(doc) = c.doc()
        && let Some(st) = doc.story(sid)
    {
        let (pi, off) = st.locate(range.start);
        let para = &st.paras[pi];
        let (cs, over) = para.format_at(if range.is_empty() { off } else { off + 1 });
        let cst = doc.styles.char_stack(&para.style, &para.over, cs.as_deref(), &over);
        let pst = doc.styles.para_stack(&para.style, &para.over);
        let mut cm = Map::new();
        let mut pm = Map::new();
        for k in KEYS {
            match k.level {
                Level::Char => cm.insert(k.name.into(), cst.get(k.name)),
                Level::Para => pm.insert(k.name.into(), pst.get(k.name)),
            };
        }
        values = Some((cm, pm));
    }
    if let Some(name) = args::opt_str(p, "style") {
        let kind = if level == Some(Level::Char) { StyleKind::Character } else { StyleKind::Paragraph };
        let doc = c.doc()?;
        let key = formdesign_model::style::style_key(name);
        if !doc.styles.exists(kind, &key) {
            return Err(EngineError::new(format!("no style named {name}")));
        }
        let empty = Attrs::new();
        let st = if kind == StyleKind::Paragraph { doc.styles.para_stack(&key, &empty) } else { doc.styles.char_stack(formdesign_model::style::BASIC_PARA, &empty, Some(&key), &empty) };
        let mut cm = Map::new();
        let mut pm = Map::new();
        for k in KEYS {
            match k.level {
                Level::Char => cm.insert(k.name.into(), st.get(k.name)),
                Level::Para => pm.insert(k.name.into(), st.get(k.name)),
            };
        }
        values = Some((cm, pm));
    }
    let keys: Vec<Value> = KEYS
        .iter()
        .filter(|k| level.is_none_or(|l| l == k.level))
        .map(|k| {
            let (kind, choices): (&str, Vec<&str>) = match k.kind {
                Kind::Num => ("number", vec![]),
                Kind::Int => ("integer", vec![]),
                Kind::Bool => ("boolean", vec![]),
                Kind::Text => ("text", vec![]),
                Kind::Choice(c) => ("choice", c.to_vec()),
                Kind::List => ("list", vec![]),
                Kind::Paint => ("paint", vec![]),
                Kind::Leading => ("leading", vec![]),
                Kind::Object => ("object", vec![]),
            };
            let value = values.as_ref().and_then(|(cm, pm)| if k.level == Level::Char { cm.get(k.name) } else { pm.get(k.name) }).cloned();
            json!({"name": k.name, "level": if k.level == Level::Char { "character" } else { "paragraph" }, "kind": kind, "choices": choices, "about": k.about, "value": value})
        })
        .collect();
    Ok(json!({"keys": keys}))
}

fn type_attrs(c: &mut Ctx, p: &Value) -> Result<Value> {
    let chars = p.get("char").and_then(Value::as_object).cloned().unwrap_or_default();
    let para = p.get("para").and_then(Value::as_object).cloned().unwrap_or_default();
    if chars.is_empty() && para.is_empty() {
        return Err(EngineError::new("missing `char` or `para` (attributes by their model names, from type.attrsCatalog)"));
    }
    let mut probe = Attrs::new();
    formdesign_model::attrs::set_checked(&mut probe, &chars.iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| (k.clone(), v.clone())).collect(), Some(Level::Char))?;
    formdesign_model::attrs::set_checked(&mut probe, &para.iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| (k.clone(), v.clone())).collect(), Some(Level::Para))?;
    if let Some(name) = args::opt_str(p, "style") {
        let kind = if args::opt_str(p, "kind") == Some("character") { StyleKind::Character } else { StyleKind::Paragraph };
        let key = formdesign_model::style::style_key(name);
        let doc = c.doc_mut()?;
        let st = doc.styles.list_mut(kind).iter_mut().find(|s| s.name == key).ok_or_else(|| EngineError::new(format!("no style named {name}")))?;
        let mut all = chars.clone();
        all.extend(para.clone());
        formdesign_model::attrs::set_checked(&mut st.attrs, &all, None)?;
        c.label(if kind == StyleKind::Character { "Character Style Options" } else { "Paragraph Style Options" });
        return Ok(json!({"style": key}));
    }
    let targets = text_targets(c)?;
    if !chars.is_empty() && targets.len() == 1 && targets[0].1.is_empty() && c.selection().text.is_some() {
        let st = c.state_mut()?;
        for (k, v) in &chars {
            st.pending.insert(k.clone(), v.clone());
        }
    }
    let doc = c.doc_mut()?;
    for (sid, range) in targets {
        let Some(st) = doc.story_mut(sid) else { continue };
        if !chars.is_empty() && !range.is_empty() {
            st.set_char_attrs(range.clone(), &chars)?;
        }
        if !para.is_empty() {
            let ps = st.paras_in(range);
            st.set_para_attrs(ps, &para)?;
        }
    }
    c.label(if para.is_empty() { "Character Formatting" } else { "Paragraph Formatting" });
    Ok(json!({}))
}

fn gradient_swatch(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = args::str_(p, "name", "the gradient swatch's name")?.trim().to_string();
    let kind = args::opt_str(p, "type").unwrap_or("linear").to_string();
    let stops = p.get("stops").and_then(Value::as_array).filter(|a| a.len() >= 2).ok_or_else(|| EngineError::new("missing `stops`: two or more [{color: swatch name or #rrggbb, at: 0..1}]"))?;
    let doc = c.doc()?;
    let mut out = Vec::new();
    for (i, s) in stops.iter().enumerate() {
        let col = s.get("color").and_then(Value::as_str).ok_or_else(|| EngineError::new(format!("stop {i} has no `color`")))?;
        let color = formdesign_model::color::Color::from_hex(col).or_else(|| doc.swatch_color(col, 1.0, 0)).ok_or_else(|| EngineError::new(format!("stop {i}: {col} is neither a swatch nor a #rrggbb colour")))?;
        let at = s.get("at").and_then(Value::as_f64).unwrap_or(i as f64 / (stops.len() - 1) as f64).clamp(0.0, 1.0);
        out.push(formdesign_model::color::Stop { at, color, opacity: s.get("opacity").and_then(Value::as_f64).unwrap_or(1.0).clamp(0.0, 1.0) });
    }
    let gradient = formdesign_model::color::Gradient { radial: kind == "radial", stops: out, angle: args::opt_f64(p, "angle").unwrap_or(0.0) };
    let doc = c.doc_mut()?;
    if doc.swatch(&name).is_some() {
        return Err(EngineError::new(format!("a swatch named {name} exists already")));
    }
    doc.swatches.push(formdesign_model::color::Swatch { name: name.clone(), value: formdesign_model::color::SwatchValue::Gradient { gradient }, spot: false, group: None, locked: false });
    c.label("New Gradient Swatch");
    Ok(json!({"name": name}))
}

fn tint_swatch(c: &mut Ctx, p: &Value) -> Result<Value> {
    let base = args::str_(p, "base", "the swatch to tint")?.to_string();
    let tint = args::f64_(p, "tint", "the tint, 0 to 100 percent")?;
    let tint = if tint > 1.0 { tint / 100.0 } else { tint }.clamp(0.0, 1.0);
    let doc = c.doc()?;
    let key = doc.swatch(&base).map(|s| s.name.clone()).ok_or_else(|| EngineError::new(format!("no swatch named {base}")))?;
    let name = args::opt_str(p, "name").map(str::to_string).unwrap_or_else(|| format!("{} {:.0}%", crate::wire::swatch_name(&key), tint * 100.0));
    let doc = c.doc_mut()?;
    if doc.swatch(&name).is_some() {
        return Err(EngineError::new(format!("a swatch named {name} exists already")));
    }
    doc.swatches.push(formdesign_model::color::Swatch { name: name.clone(), value: formdesign_model::color::SwatchValue::Tint { base: key, tint }, spot: false, group: None, locked: false });
    c.label("New Tint Swatch");
    Ok(json!({"name": name}))
}

fn swatch_group(c: &mut Ctx, p: &Value) -> Result<Value> {
    let group = args::opt_str(p, "group").map(str::to_string).filter(|g| !g.is_empty());
    let names: Vec<String> = p.get("swatches").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
    if names.is_empty() {
        return Err(EngineError::new("missing `swatches` (the swatch names to group)"));
    }
    let doc = c.doc_mut()?;
    let mut n = 0;
    for nm in &names {
        let key = doc.swatch(nm).map(|s| s.name.clone());
        if let Some(k) = key
            && let Some(s) = doc.swatches.iter_mut().find(|s| s.name == k)
        {
            s.group = group.clone();
            n += 1;
        }
    }
    c.label("Colour Group");
    Ok(json!({"grouped": n, "group": group}))
}

fn style_group(c: &mut Ctx, p: &Value) -> Result<Value> {
    let kind = StyleKind::parse(args::opt_str(p, "kind").unwrap_or("paragraph")).ok_or_else(|| EngineError::new("`kind` expected paragraph, character, object, table or cell"))?;
    let group = args::opt_str(p, "group").map(str::to_string).filter(|g| !g.is_empty());
    let names: Vec<String> = p.get("styles").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(formdesign_model::style::style_key)).collect()).unwrap_or_default();
    if names.is_empty() {
        return Err(EngineError::new("missing `styles` (the style names to group)"));
    }
    let doc = c.doc_mut()?;
    let mut n = 0;
    for s in doc.styles.list_mut(kind).iter_mut() {
        if names.contains(&s.name) {
            s.group = group.clone();
            n += 1;
        }
    }
    c.label("Style Group");
    Ok(json!({"grouped": n, "group": group}))
}

fn rename_table_refs(doc: &mut Doc, kind: StyleKind, from: &str, to: &str) {
    const REGIONS: [&str; 5] = ["headerCellStyle", "footerCellStyle", "leftColumnCellStyle", "rightColumnCellStyle", "bodyCellStyle"];
    let rename_regions = |attrs: &mut Attrs| {
        for k in REGIONS {
            if attrs.get(k).and_then(Value::as_str) == Some(from) {
                attrs.insert(k.into(), json!(to));
            }
        }
    };
    let has_table = |p: &Para| p.segs.iter().any(|s| matches!(s, Seg::Inline { inline: formdesign_model::story::Inline::Table { .. }, .. }));
    crate::walk::paras_where(doc, &has_table, &mut |p| {
        for s in p.segs.iter_mut() {
            let Seg::Inline { inline: formdesign_model::story::Inline::Table { table }, .. } = s else { continue };
            if kind == StyleKind::Table {
                if table.style.as_deref() == Some(from) {
                    table.style = Some(to.to_string());
                }
                continue;
            }
            for cell in table.cells.iter_mut() {
                if cell.style.as_deref() == Some(from) {
                    cell.style = Some(to.to_string());
                }
            }
            rename_regions(&mut table.attrs);
        }
    });
    if kind == StyleKind::Cell {
        for st in doc.styles.list_mut(StyleKind::Table).iter_mut() {
            rename_regions(&mut st.attrs);
        }
    }
}

fn table_style_edit(c: &mut Ctx, p: &Value) -> Result<Value> {
    let kind = if args::opt_str(p, "kind") == Some("cell") { StyleKind::Cell } else { StyleKind::Table };
    let name = formdesign_model::style::style_key(args::str_(p, "name", "the style's name")?);
    if let Some(o) = p.get("attrs").and_then(Value::as_object) {
        if kind == StyleKind::Cell {
            super::table::validate_cell_style(c.doc()?, o)?;
        } else {
            super::table::validate_table_style(c.doc()?, o)?;
        }
    }
    let doc = c.doc_mut()?;
    let st = doc.styles.list_mut(kind).iter_mut().find(|s| s.name == name).ok_or_else(|| EngineError::new(format!("no {} style named {name}", if kind == StyleKind::Cell { "cell" } else { "table" })))?;
    if let Some(o) = p.get("attrs").and_then(Value::as_object) {
        for (k, v) in o {
            if v.is_null() {
                st.attrs.remove(k);
            } else {
                st.attrs.insert(k.clone(), v.clone());
            }
        }
    }
    let mut out = json!({"name": st.name, "attrs": st.attrs});
    if let Some(to) = args::opt_str(p, "rename").filter(|r| !r.trim().is_empty()).map(formdesign_model::style::style_key).filter(|t| *t != name) {
        doc.styles.rename(kind, &name, &to)?;
        rename_table_refs(doc, kind, &name, &to);
        out["name"] = json!(to);
    }
    c.label(if kind == StyleKind::Cell { "Cell Style Options" } else { "Table Style Options" });
    Ok(out)
}

pub fn register(r: &mut Registry) {
    cmd("ink.list", "Inks").query().sig("{}").reg(r, ink_list);
    cmd("ink.options", "Ink Manager").menu(&["Window", "Output", "Ink Manager..."]).sig("{ink, toProcess?, alias?: another spot ink or none}").reg(r, ink_options);
    cmd("ink.allToProcess", "All Spots to Process").sig("{on?}").reg(r, ink_all);
    cmd("type.openType", "OpenType").menu(&["Type", "OpenType..."]).needs(Needs::Text).sig("{features?: [tags] | {tag: on}, add?: [tags], remove?: [tags]}").help("Sets OpenType features on the selected text: liga, dlig, onum, lnum, tnum, pnum, frac, sups, subs, ordn, swsh, salt, smcp, c2sc, zero, ss01 to ss20.").reg(r, open_type);
    cmd("type.changeCase", "Change Case").menu(&["Type", "Change Case..."]).needs(Needs::Text).sig("{mode: upper|lower|title|sentence}").reg(r, change_case);
    cmd("type.clearOverrides", "Clear Overrides").menu(&["Type", "Clear Overrides"]).needs(Needs::Text).sig("{level?: all|character|paragraph}").reg(r, clear_overrides);
    cmd("type.onPath", "Type on a Path").menu(&["Type", "Type on a Path", "Add Type to Path"]).needs(Needs::Items).sig("{text?, start?, flip?, align?: baseline|ascender|descender|center, ids?}").reg(r, on_path);
    cmd("type.attrsCatalog", "Text Attributes").query().sig("{level?: character|paragraph, style?}").help("Every character or paragraph attribute with its kind, choices and description, and its value at the text selection (or in `style`).").reg(r, attrs_catalog);
    cmd("type.attrs", "Text Attributes").sig("{char?: {model attrs}, para?: {model attrs}, style?, kind?: paragraph|character}").help("Sets character and paragraph attributes by their model names (see type.attrsCatalog) on the selected text, or on a style; null removes an attribute.").reg(r, type_attrs);
    cmd("type.fontAxes", "Font Variation Axes").query().sig("{font?: family, style?: style name, variations?: {tag: value}}").help("The variation axes (tag, name, min, default, max, current value) and named instances of a variable font: the selection's font, or `font` and `style`. Set the values with type.attrs char.fontVariations {wght: 650}.").reg(r, font_axes);
    cmd("font.list", "Fonts in Use").query().sig("{}").reg(r, font_list);
    cmd("font.replace", "Find/Replace Font").menu(&["Type", "Find/Replace Font..."]).sig("{from, to, style?, styles?: true}").reg(r, font_replace);
    cmd("style.load", "Load Styles").menu(&["Type", "Load Styles..."]).sig("{path | base64 + name, kinds?: [paragraph, character, object, table, cell], overwrite?: true, swatches?: true}").reg(r, style_load);
    cmd("style.group", "Style Group").menu(&["Type", "New Style Group..."]).sig("{kind?: paragraph|character|object|table|cell, styles: [names], group}").reg(r, style_group);
    cmd("style.table.edit", "Table Style Options").menu(&["Table", "Table Style Options..."]).sig("{name, attrs?: {border, altRows, altCols, altRowStrokes, altColStrokes, headerCellStyle, footerCellStyle, leftColumnCellStyle, rightColumnCellStyle, bodyCellStyle}, rename?}").help("Edits a table style: border {weight, color}; altRows and altCols fills {first, next, firstColor, firstTint, nextColor, nextTint, skipFirst, skipLast}; altRowStrokes and altColStrokes {first, next, firstWeight, firstColor, firstTint, firstType, nextWeight, nextColor, nextTint, nextType, skipFirst, skipLast}; and the cell style each region (header rows, footer rows, left column, right column, body) uses. null removes a key.").reg(r, table_style_edit);
    cmd("style.cell.edit", "Cell Style Options").menu(&["Table", "Cell Style Options..."]).sig("{kind: cell, name, attrs?: {fill, fillTint, vj, insets, strokes, paraStyle, rotate, slant, diagonal, width?: pt (column width), minHeight?: pt (row at least), height?: pt (row exactly)}, rename?}").help("Edits a cell style: fill and fillTint, vj (top|center|bottom|justify), insets, strokes {top|bottom|left|right: {weight, color, tint, type}}, paraStyle (a paragraph style applied to the cell's text), rotate (0|90|180|270), slant (10 to 80 degrees for angled header text), diagonal {down?: stroke, up?: stroke, front?}, and the dimensions IDML cell styles cannot hold: width (the column's width), minHeight (the row is at least this tall) and height (the row is exactly this tall). null removes a key.").reg(r, table_style_edit);
    cmd("place.load", "Load Place Cursor").sig("{paths: [files]}").reg(r, place_load);
    cmd("file.print", "Print").menu(&["File", "Print..."]).key("Mod+P").query().sig("{printer?, copies?, pages?, bleed?, separations?: bool, inks?: [plate names], marks?: bool | {crop, bleed, registration, colorBars, pageInfo, type, weight, offset}, cropMarks?, bleedMarks?, registrationMarks?, colorBars?, pageInformation?, markType?: default|japanese, markWeight?: pt, markOffset?: pt}").reg(r, file_print);
    cmd("file.printers", "Printers").query().sig("{}").reg(r, printers);
    cmd("file.exportSvg", "Export SVG").menu(&["File", "Export", "SVG..."]).query().sig("{page?: 1-based, path?}").reg(r, export_svg);
    cmd("list.define", "Define Lists").menu(&["Type", "Bulleted and Numbered Lists", "Define Lists..."]).sig("{name, acrossStories?, continueFromPrevious?}").reg(r, list_define);
    cmd("list.list", "Lists").query().sig("{}").reg(r, list_list);
    cmd("list.delete", "Delete List").sig("{name}").reg(r, list_delete);
    cmd("liquid.pageRule", "Liquid Page Rule").menu(&["Layout", "Liquid Layout..."]).sig("{rule: off|scale|recenter|guide|object, pages?: [1-based]}").reg(r, page_rule);
    cmd("liquid.object", "Liquid Layout Object").menu(&["Object", "Liquid Layout Options..."]).needs(Needs::Items).sig("{pinTop?, pinBottom?, pinLeft?, pinRight?, resizeWidth?, resizeHeight?, ids?}").reg(r, liquid_object);
    cmd("liquid.pageSize", "Resize Pages").sig("{width, height, pages?: [1-based], rule?}").help("Changes the size of pages and moves and resizes their objects by each page's liquid rule (or `rule`).").reg(r, page_size_liquid);
    cmd("layout.createAlternate", "Create Alternate Layout").menu(&["Layout", "Create Alternate Layout..."]).sig("{name?, width, height, rule?: scale|recenter|guide|object|off}").reg(r, create_alternate);
    cmd("layout.pages.toSpread", "Add Page to Spread").menu(&["Layout", "Pages", "Add Page to Spread"]).sig("{page: 1-based, join?: true}").reg(r, to_spread);
    cmd("layout.spreadShuffle", "Allow Spread to Shuffle").menu(&["Layout", "Pages", "Allow Selected Spread to Shuffle"]).sig("{allow, spread?}").reg(r, shuffle);
    cmd("object.primaryTextFrame", "Primary Text Frame").needs(Needs::Items).sig("{ids?}").reg(r, primary_frame);
    cmd("swatch.createGradient", "New Gradient Swatch").menu(&["Window", "Color", "New Gradient Swatch..."]).sig("{name, type?: linear|radial, stops: [{color, at, opacity?}]}").reg(r, gradient_swatch);
    cmd("swatch.createTint", "New Tint Swatch").menu(&["Window", "Color", "New Tint Swatch..."]).sig("{base, tint: percent, name?}").reg(r, tint_swatch);
    cmd("swatch.group", "New Colour Group").menu(&["Window", "Color", "New Color Group..."]).sig("{swatches: [names], group}").reg(r, swatch_group);
}
