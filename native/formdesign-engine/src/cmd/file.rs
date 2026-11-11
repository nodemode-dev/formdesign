use std::path::{Path, PathBuf};
use std::sync::Arc;

use formdesign_interchange::idml::{b64, unb64};
use formdesign_interchange::publish;
use formdesign_interchange::text as timport;
use formdesign_model::attrs::Attrs;
use formdesign_model::doc::{Doc, DocSettings, Intent};
use formdesign_model::geom::{Affine, Edges};
use formdesign_model::ids::{ItemId, StoryId};
use formdesign_model::item::{Graphic, ItemKind, TextFrame};
use formdesign_model::story::{Para, Seg};
use formdesign_model::style::{BASIC_PARA, StyleKind, style_key};
use formdesign_render::scene::{Brush, Op, PageScene, SceneOpts, page_scene};
use serde_json::{Value, json};

use crate::cmd::object::fit_content;
use crate::registry::{Needs, Registry, cmd};
use crate::session::{Ctx, Selection};
use crate::target::{one_id, text_targets};
use crate::{EngineError, Result, args, assets};

pub const NATIVE_EXT: &str = "fdoc";

pub fn native_bytes(doc: &Doc) -> Result<Vec<u8>> {
    Ok(formdesign_model::format::save(doc)?)
}

fn edges_of(v: Option<&Value>) -> Option<Edges> {
    match v? {
        Value::Number(n) => n.as_f64().map(Edges::all),
        Value::Object(o) => {
            let g = |k: &str| o.get(k).and_then(Value::as_f64).unwrap_or(0.0);
            Some(Edges { top: g("top"), bottom: g("bottom"), inside: g("inside").max(g("left")), outside: g("outside").max(g("right")) })
        }
        Value::Array(a) => {
            let n: Vec<f64> = a.iter().filter_map(Value::as_f64).collect();
            (n.len() == 4).then(|| Edges { top: n[0], bottom: n[1], inside: n[2], outside: n[3] })
        }
        _ => None,
    }
}

pub fn new_doc(p: &Value, untitled: usize) -> Result<Doc> {
    let mut s = DocSettings::default();
    if let Some(name) = args::opt_str(p, "preset") {
        let (w, h) = formdesign_model::geom::units::page_size(name).ok_or_else(|| EngineError::new(format!("no page preset named {name} (try Letter, A4, A5, Tabloid, Legal, Instagram, Story, Web, Slide)")))?;
        s.page_width = w;
        s.page_height = h;
    }
    if let Some(w) = args::opt_f64(p, "width") {
        s.page_width = w.max(1.0);
    }
    if let Some(h) = args::opt_f64(p, "height") {
        s.page_height = h.max(1.0);
    }
    if let Some(f) = args::opt_bool(p, "facingPages") {
        s.facing = f;
    }
    if let Some(m) = edges_of(p.get("margins")) {
        s.margins = m;
    }
    if let Some(n) = args::opt_u64(p, "columns") {
        s.columns = n.clamp(1, 216) as u32;
    }
    if let Some(g) = args::opt_f64(p, "gutter") {
        s.gutter = g.max(0.0);
    }
    if let Some(b) = edges_of(p.get("bleed")) {
        s.bleed = b;
    }
    if let Some(b) = edges_of(p.get("slug")) {
        s.slug = b;
    }
    if let Some(i) = args::opt_str(p, "intent") {
        s.intent = match i {
            "web" => Intent::Web,
            "mobile" | "digital" => Intent::Mobile,
            _ => Intent::Print,
        };
        if s.intent != Intent::Print && p.get("facingPages").is_none() {
            s.facing = false;
        }
    }
    let pages = args::opt_u64(p, "pages").unwrap_or(1).clamp(1, 9999) as usize;
    let title = args::opt_str(p, "title").map(str::to_string).unwrap_or_else(|| format!("Untitled-{untitled}"));
    if let Some(sp) = args::opt_u64(p, "startPage") {
        s.start_left = s.facing && sp.max(1) % 2 == 0;
    }
    let mut doc = Doc::new(&title, s, pages);
    if let Some(sp) = args::opt_u64(p, "startPage")
        && let Some(sec) = doc.sections.first_mut()
    {
        sec.start_at = Some(sp.max(1) as u32);
    }
    Ok(doc)
}

fn file_new(c: &mut Ctx, p: &Value) -> Result<Value> {
    let n = c.session.docs.len() + 1;
    let doc = new_doc(p, n)?;
    let i = c.session.open_doc(doc, None);
    c.session.sel_rev += 1;
    let uid = c.session.docs[i].uid;
    Ok(json!({"index": i, "uid": uid}))
}

pub fn is_idml(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK") && zip::ZipArchive::new(std::io::Cursor::new(bytes)).map(|z| z.file_names().any(|n| n == "designmap.xml")).unwrap_or(false)
}

pub fn open_bytes(c: &mut Ctx, name: &str, bytes: &[u8], path: Option<PathBuf>) -> Result<Value> {
    if is_idml(bytes) {
        return Err(EngineError::new(format!("{name} is an IDML package: open it with file.openIdml")));
    }
    let mut doc = formdesign_model::format::load(bytes).map_err(|e| EngineError::new(format!("{name}: {}", e.0)))?;
    let stem = Path::new(name).file_stem().and_then(|s| s.to_str()).unwrap_or(name).to_string();
    if !stem.is_empty() {
        doc.title = stem;
    }
    let i = c.session.open_doc(doc, path);
    c.session.sel_rev += 1;
    Ok(json!({"index": i, "uid": c.session.docs[i].uid}))
}

fn file_open_bytes(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = args::opt_str(p, "name").unwrap_or("Untitled").to_string();
    let data = unb64(args::str_(p, "base64", "the whole file, base64")?);
    open_bytes(c, &name, &data, None)
}

fn file_open(c: &mut Ctx, p: &Value) -> Result<Value> {
    let path = args::str_(p, "path", "the document to open")?.to_string();
    let data = std::fs::read(&path).map_err(|e| EngineError::new(format!("cannot read {path}: {e}")))?;
    let name = Path::new(&path).file_name().and_then(|s| s.to_str()).unwrap_or("Untitled").to_string();
    if is_idml(&data) {
        return idml_open(c, &name, &data);
    }
    open_bytes(c, &name, &data, Some(PathBuf::from(&path)))
}

fn idml_open(c: &mut Ctx, name: &str, data: &[u8]) -> Result<Value> {
    let mut doc = formdesign_interchange::idml::read::read(data).map_err(|e| EngineError::new(format!("{name}: {e}")))?;
    doc.title = Path::new(name).file_stem().and_then(|s| s.to_str()).unwrap_or(name).to_string();
    let i = c.session.open_doc(doc, None);
    c.session.sel_rev += 1;
    Ok(json!({"index": i, "uid": c.session.docs[i].uid}))
}

fn file_open_idml(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = args::opt_str(p, "name").unwrap_or("document.idml").to_string();
    let data = unb64(args::str_(p, "base64", "the .idml package, base64")?);
    idml_open(c, &name, &data)
}

fn write_file(path: &str, bytes: &[u8]) -> Result<()> {
    if let Some(dir) = Path::new(path).parent()
        && !dir.as_os_str().is_empty()
    {
        std::fs::create_dir_all(dir).map_err(|e| EngineError::new(format!("cannot create {}: {e}", dir.display())))?;
    }
    std::fs::write(path, bytes).map_err(|e| EngineError::new(format!("cannot write {path}: {e}")))
}

fn mark_saved(c: &mut Ctx, path: Option<PathBuf>) -> Result<()> {
    let st = c.state_mut()?;
    st.saved = st.doc.clone();
    if let Some(p) = path {
        st.path = Some(p);
    }
    Ok(())
}

fn file_save(c: &mut Ctx, _: &Value) -> Result<Value> {
    let path = c.state()?.path.clone().ok_or_else(|| EngineError::new("missing `path`: this document has not been saved yet, use file.saveAs"))?;
    let bytes = native_bytes(c.doc()?)?;
    write_file(&path.display().to_string(), &bytes)?;
    mark_saved(c, None)?;
    Ok(json!({"path": path.display().to_string(), "bytes": bytes.len()}))
}

fn file_save_as(c: &mut Ctx, p: &Value) -> Result<Value> {
    let path = args::str_(p, "path", "where to save")?.to_string();
    let idml = path.to_ascii_lowercase().ends_with(".idml");
    let bytes = if idml { formdesign_interchange::idml::write::write(c.doc()?).map_err(EngineError::new)? } else { native_bytes(c.doc()?)? };
    write_file(&path, &bytes)?;
    if !idml {
        mark_saved(c, Some(PathBuf::from(&path)))?;
    }
    Ok(json!({"path": path, "bytes": bytes.len()}))
}

fn file_close(c: &mut Ctx, _: &Value) -> Result<Value> {
    let i = c.session.active.ok_or_else(|| EngineError::new("no document open"))?;
    c.session.close_doc(i);
    c.session.sel_rev += 1;
    Ok(json!({}))
}

fn ensure_styles(doc: &mut Doc, imp: &timport::Imported) {
    for s in &imp.para_styles {
        if !doc.styles.exists(StyleKind::Paragraph, s) {
            let _ = doc.styles.create(StyleKind::Paragraph, s, None);
        }
    }
    for s in &imp.char_styles {
        if !doc.styles.exists(StyleKind::Character, s) {
            let _ = doc.styles.create(StyleKind::Character, s, None);
        }
    }
}

pub fn place_text(doc: &mut Doc, frame: ItemId, imp: timport::Imported) -> Result<StoryId> {
    let it = doc.item(frame).cloned().ok_or_else(|| EngineError::new(format!("no object with id {frame}")))?;
    let sid = match &it.kind {
        ItemKind::Text(t) => t.story,
        ItemKind::Group { .. } | ItemKind::States { .. } => return Err(EngineError::new("cannot place text into a group: pick a frame")),
        _ => {
            let sid = doc.new_story(BASIC_PARA);
            if let Some(x) = doc.item_mut(frame) {
                x.kind = ItemKind::Text(TextFrame::new(sid));
                x.style = Some(formdesign_model::style::BASIC_TEXT.into());
            }
            if let Some(st) = doc.story_mut(sid) {
                st.frames = vec![frame];
            }
            sid
        }
    };
    ensure_styles(doc, &imp);
    let width = {
        let tf = it.text();
        let w = it.shape.bbox().width() - tf.map(|t| t.insets.inside + t.insets.outside).unwrap_or(0.0);
        let cols = tf.map(|t| t.columns.max(1) as f64).unwrap_or(1.0);
        let gutter = tf.map(|t| t.gutter).unwrap_or(0.0);
        ((w - gutter * (cols - 1.0)) / cols).max(36.0)
    };
    let st = doc.story_mut(sid).ok_or_else(|| EngineError::new("no such story"))?;
    let default = st.paras.first().map(|p| p.style.clone()).unwrap_or_else(|| BASIC_PARA.into());
    let paras: Vec<Para> = imp
        .paras
        .into_iter()
        .map(|mut p| {
            p.style = if p.style.is_empty() { default.clone() } else { style_key(&p.style) };
            for s in p.segs.iter_mut() {
                if let Seg::Inline { inline: formdesign_model::story::Inline::Table { table }, .. } = s {
                    let total = table.width();
                    if total > 0.0 {
                        let k = width / total;
                        for w in table.widths.iter_mut() {
                            *w *= k;
                        }
                    }
                    for cell in table.cells.iter_mut() {
                        for q in cell.paras.iter_mut() {
                            if q.style.is_empty() {
                                q.style = default.clone();
                            }
                        }
                    }
                }
                let (cs, _) = s.format_mut();
                if let Some(x) = cs.as_mut() {
                    *x = style_key(x);
                }
            }
            p
        })
        .collect();
    st.paras = if paras.is_empty() { vec![Para::empty(&default)] } else { paras };
    Ok(sid)
}

pub fn place_image(doc: &mut Doc, frame: ItemId, asset: formdesign_model::item::Asset) -> Result<()> {
    let it = doc.item(frame).cloned().ok_or_else(|| EngineError::new(format!("no object with id {frame}")))?;
    if matches!(it.kind, ItemKind::Group { .. } | ItemKind::States { .. }) {
        return Err(EngineError::new("cannot place an image into a group: pick a frame"));
    }
    let aid = doc.add_asset(asset);
    if let Some(sid) = it.story() {
        let mut drop = false;
        if let Some(st) = doc.story_mut(sid) {
            st.frames.retain(|f| *f != frame);
            drop = st.frames.is_empty();
        }
        if drop {
            doc.stories.remove(&sid);
        }
    }
    let fitting = it.graphic().and_then(|g| g.fitting.clone());
    let r = it.shape.bbox();
    if let Some(x) = doc.item_mut(frame) {
        x.kind = ItemKind::Graphic(Graphic { asset: Some(aid), content: Affine::translate((r.x0, r.y0)), fitting: fitting.clone() });
        if x.style.as_deref() == Some(formdesign_model::style::BASIC_TEXT) {
            x.style = Some(formdesign_model::style::BASIC_GRAPHIC.into());
        }
    }
    if let Some(f) = fitting.filter(|f| f.kind != "none") {
        fit_content(doc, frame, &f.kind, None)?;
    }
    Ok(())
}

fn file_place(c: &mut Ctx, p: &Value) -> Result<Value> {
    let path = args::opt_str(p, "path").map(str::to_string);
    let frame = one_id(p, "frame");
    let page = args::opt_u64(p, "page").unwrap_or(0) as u32;
    let (name, data) = match (args::opt_str(p, "base64"), &path) {
        (Some(b), _) => (args::opt_str(p, "name").unwrap_or("placed.txt").to_string(), unb64(b)),
        (None, Some(path)) => {
            let data = std::fs::read(path).map_err(|e| EngineError::new(format!("cannot read {path}: {e}")))?;
            (Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or("placed").to_string(), data)
        }
        (None, None) => return Err(EngineError::new("missing `path` (the file to place) or `base64` with `name`")),
    };
    let Some(frame) = frame else {
        c.state()?;
        let entry = json!({"name": name, "path": path, "base64": if path.is_none() { Some(b64(&data)) } else { None }, "page": page});
        c.session.place_gun.push(entry);
        c.session.tool = "place".into();
        c.session.sel_rev += 1;
        return Ok(json!({"loaded": true, "cursor": if timport::is_text_name(&name) { "loadedText" } else { "loadedGraphic" }}));
    };
    if c.doc()?.item(frame).is_none() {
        return Err(EngineError::new(format!("no object with id {frame}")));
    }
    if timport::is_text_name(&name) {
        let imp = timport::import(&name, &data).map_err(EngineError::new)?;
        let sid = place_text(c.doc_mut()?, frame, imp)?;
        c.set_selection(Selection::items(vec![frame]));
        c.label("Place");
        return Ok(json!({"frame": frame, "story": sid}));
    }
    let asset = assets::make(&name, data, path.clone(), page)?;
    place_image(c.doc_mut()?, frame, asset)?;
    c.set_selection(Selection::items(vec![frame]));
    c.label("Place");
    Ok(json!({"frame": frame}))
}

pub fn page_list(doc: &Doc, spec: Option<&str>) -> Result<Vec<usize>> {
    let n = doc.page_count();
    let Some(spec) = spec.map(str::trim).filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("all")) else { return Ok((0..n).collect()) };
    let find = |t: &str| -> Result<usize> {
        let t = t.trim();
        if let Some(i) = doc.find_page_by_name(t) {
            return Ok(i);
        }
        if let Some(abs) = t.strip_prefix('+').and_then(|x| x.parse::<usize>().ok()) {
            return if abs >= 1 && abs <= n { Ok(abs - 1) } else { Err(EngineError::new(format!("no page +{abs}"))) };
        }
        match t.parse::<usize>() {
            Ok(k) if k >= 1 && k <= n => Ok(k - 1),
            _ => Err(EngineError::new(format!("no page named {t}"))),
        }
    };
    let mut out = Vec::new();
    for part in spec.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        match part.split_once('-') {
            Some((a, b)) if !a.trim().is_empty() => {
                let (a, b) = (find(a)?, if b.trim().is_empty() { n - 1 } else { find(b)? });
                let (lo, hi) = (a.min(b), a.max(b));
                out.extend(lo..=hi);
            }
            _ => out.push(find(part)?),
        }
    }
    out.dedup();
    if out.is_empty() {
        return Err(EngineError::new("the page range selects no pages"));
    }
    Ok(out)
}

pub fn deliver(p: &Value, bytes: Vec<u8>, mut extra: Value) -> Result<Value> {
    match args::opt_str(p, "path") {
        Some(path) => {
            write_file(path, &bytes)?;
            extra["path"] = json!(path);
            extra["bytes"] = json!(bytes.len());
        }
        None => {
            extra["base64"] = json!(b64(&bytes));
            extra["bytes"] = json!(bytes.len());
        }
    }
    Ok(extra)
}

fn cmyk_profile(c: &Ctx) -> Option<Vec<u8>> {
    let name = c.doc().ok()?.prefs.get("colorSettings")?.get("cmykProfile")?.as_str()?.to_string();
    let path = c.session.profiles.iter().find(|x| x.get("name").and_then(Value::as_str) == Some(name.as_str()))?.get("path")?.as_str()?.to_string();
    std::fs::read(path).ok()
}

fn export_pdf(c: &mut Ctx, p: &Value) -> Result<Value> {
    let lay = c.layout()?;
    let renderer = c.session.renderer.clone();
    let doc = c.doc()?;
    let pages = page_list(doc, args::opt_str(p, "pages"))?;
    let bleed = args::opt_bool(p, "bleed").unwrap_or(false);
    let mut warnings: Vec<String> = Vec::new();
    let marks = crate::cmd::inks::marks_from(p);
    let mut standard = args::opt_str(p, "standard").filter(|s| *s != "none").map(str::to_string);
    // PDF/X has no krilla validator: the core checks what it can (transparency, colour), writes CMYK and the trim and bleed
    // boxes, and the backend adds the output intent, GTS_PDFXVersion, Trapped and the XMP packet to the finished file.
    let pdfx = standard.as_deref().and_then(pdfx_level);
    if standard.is_some() && pdfx.is_none() && standard.as_deref().is_some_and(|s| s.to_ascii_lowercase().contains("pdfx") || s.to_ascii_lowercase().starts_with('x')) {
        return Err(EngineError::new(format!("the PDF/X standard {} is not supported: use PDF/X-1a:2001, PDF/X-3:2002 or PDF/X-4", standard.unwrap_or_default())));
    }
    if pdfx.is_some() {
        standard = None;
    }
    let tagged = args::opt_bool(p, "tagged").unwrap_or(false) || standard.as_deref().is_some_and(|s| s.contains("ua") || s.contains("UA") || s.ends_with('a'));
    let popts = formdesign_render::pdf::PdfOpts { title: args::opt_str(p, "title").map(str::to_string).unwrap_or_else(|| doc.title.clone()), tagged, standard, language: args::opt_str(p, "language").map(str::to_string).or_else(|| Some("en".into())), cmyk_profile: if pdfx.is_some() { None } else { cmyk_profile(c) }, marks, mark_font: renderer.fonts.any_face().map(formdesign_render::pdf::MarkFont), plate: None, cmyk_only: pdfx == Some("x1a"), mesh_cmyk: pdfx.is_some() };
    if let Some(level) = pdfx
        && level != "x4"
    {
        let opts = SceneOpts { printing: true, bleed, overprint: false };
        let mut bad: Vec<String> = Vec::new();
        for pg in &pages {
            let Some(scene) = page_scene(doc, &lay, *pg, &opts) else { continue };
            let mut what: Vec<&str> = Vec::new();
            for op in &scene.ops {
                match op {
                    Op::PushOpacity(o) if *o < 1.0 && !what.contains(&"opacity") => what.push("opacity"),
                    Op::PushBlend(m) if m != "normal" && !what.contains(&"a blend mode") => what.push("a blend mode"),
                    Op::PushMask(_) if !what.contains(&"a soft mask (shadow, feather or glow)") => what.push("a soft mask (shadow, feather or glow)"),
                    _ => {}
                }
            }
            if !what.is_empty() {
                bad.push(format!("page {} uses {}", doc.page_name(*pg), what.join(", ")));
            }
        }
        if !bad.is_empty() {
            return Err(EngineError::new(format!("PDF/X-{} allows no transparency, and {}. Remove it (Object > Effects > Clear Effects, set opacity to 100 percent and blending to Normal) or export PDF/X-4", if level == "x1a" { "1a:2001" } else { "3:2002" }, bad.join("; "))));
        }
    }
    if args::opt_bool(p, "spreads") == Some(true) {
        warnings.push("spreads are exported as single pages for now".into());
    }
    let lay_ref = &lay;
    let mut overset = 0;
    for s in doc.stories.keys() {
        if lay_ref.overset(*s) {
            overset += 1;
        }
    }
    if overset > 0 {
        warnings.push(format!("{overset} {} overset text", if overset == 1 { "story has" } else { "stories have" }));
    }
    let bytes = renderer.pdf_with(doc, &lay, Some(pages.clone()), bleed, &popts).map_err(|e| EngineError::new(format!("PDF export: {e}")))?;
    deliver(p, bytes, json!({"pages": pages.len(), "warnings": warnings, "tagged": popts.tagged, "standard": popts.standard, "pdfx": pdfx}))
}

/// The PDF/X level a `standard` names: x1a, x3 or x4 (None for anything else).
fn pdfx_level(s: &str) -> Option<&'static str> {
    let k: String = s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
    let k = k.strip_prefix("pdf").unwrap_or(&k);
    match k {
        k if k.starts_with("x1a") => Some("x1a"),
        k if k.starts_with("x3") => Some("x3"),
        k if k.starts_with("x4") => Some("x4"),
        _ => None,
    }
}

/// What the interactive PDF needs from the document: per exported page its transition, the form fields, buttons and
/// media with their rectangles (page points from the top left of the trim box), and each media file written to `mediaDir`.
fn interactive_info(c: &mut Ctx, p: &Value) -> Result<Value> {
    let doc = c.doc()?;
    let pages = page_list(doc, args::opt_str(p, "pages"))?;
    let dir = args::opt_str(p, "mediaDir").map(PathBuf::from);
    let mut files: std::collections::HashMap<formdesign_model::ids::AssetId, String> = std::collections::HashMap::new();
    let mut out = Vec::new();
    for abs in &pages {
        let (Some(rect), Some((si, _))) = (doc.page_rect(*abs), doc.page_slot(*abs)) else { continue };
        let mut items = Vec::new();
        for id in doc.walk(&doc.spreads[si].items) {
            let Some(it) = doc.item(id) else { continue };
            if it.hidden || it.nonprinting || doc.page_of(id) != Some(*abs) {
                continue;
            }
            let Some(b) = doc.bounds(id) else { continue };
            let r = json!([b.x0 - rect.x0, b.y0 - rect.y0, b.x1 - rect.x0, b.y1 - rect.y0]);
            let name = if it.name.is_empty() { it.display_name() } else { it.name.clone() };
            if let Some(f) = it.extra.get("form") {
                items.push(json!({"type": "form", "id": id, "rect": r, "form": f}));
            }
            if let Some(bt) = it.extra.get("button") {
                items.push(json!({"type": "button", "id": id, "rect": r, "button": bt, "name": name}));
            }
            let asset = it.graphic().and_then(|g| g.asset).and_then(|a| doc.assets.get(&a)).filter(|a| a.is_media());
            if let (Some(a), Some(dir)) = (asset, dir.as_ref()) {
                let file = match files.get(&a.id) {
                    Some(f) => f.clone(),
                    None => {
                        let ext = Path::new(&a.name).extension().and_then(|e| e.to_str()).unwrap_or(if a.mime.starts_with("audio/") { "mp3" } else { "mp4" });
                        let f = format!("media{}.{}", files.len(), ext);
                        write_file(&dir.join(&f).to_string_lossy(), &a.data)?;
                        files.insert(a.id, f.clone());
                        f
                    }
                };
                items.push(json!({"type": "media", "id": id, "rect": r, "name": name, "mime": a.mime, "assetName": a.name, "file": file, "options": it.extra.get("media").cloned().unwrap_or(Value::Null)}));
            }
        }
        out.push(json!({"page": abs, "width": rect.width(), "height": rect.height(), "transition": doc.spreads[si].transition, "items": items}));
    }
    Ok(json!({"pages": out}))
}

fn export_idml(c: &mut Ctx, p: &Value) -> Result<Value> {
    let bytes = formdesign_interchange::idml::write::write(c.doc()?).map_err(|e| EngineError::new(format!("IDML export: {e}")))?;
    deliver(p, bytes, json!({}))
}

fn pub_opts(c: &Ctx, p: &Value) -> Result<publish::Opts> {
    let st = c.state()?;
    Ok(publish::Opts {
        title: args::opt_str(p, "title").map(str::to_string).unwrap_or_else(|| st.title()),
        author: args::opt_str(p, "author").unwrap_or("").to_string(),
        language: args::opt_str(p, "language").unwrap_or("en").to_string(),
    })
}

fn page_png(c: &mut Ctx, page: usize, scale: f64) -> Result<(Vec<u8>, String)> {
    let lay = c.layout()?;
    let renderer = c.session.renderer.clone();
    let doc = c.doc()?;
    let scene = page_scene(doc, &lay, page, &SceneOpts { printing: true, bleed: false, overprint: false }).ok_or_else(|| EngineError::new(format!("no page {page}")))?;
    let r = formdesign_render::rasterize(&scene, doc, &renderer.outlines, &renderer.images, &formdesign_render::RasterOpts { scale, ..Default::default() });
    let png = formdesign_render::encode_png(&r).ok_or_else(|| EngineError::new("could not encode a page image"))?;
    let text = page_text(doc, &lay, page);
    Ok((png, text))
}

fn page_text(doc: &Doc, lay: &formdesign_text::layout::DocLayout, page: usize) -> String {
    let mut out = Vec::new();
    let Some((si, _)) = doc.page_slot(page) else { return String::new() };
    for id in doc.walk(&doc.spreads[si].items) {
        if doc.page_of(id) != Some(page) {
            continue;
        }
        let Some((sl, fi)) = lay.frame(id) else { continue };
        let Some(st) = doc.story(sl.story) else { continue };
        let paras: Vec<usize> = sl.flow.lines.iter().filter(|l| l.frame == fi).map(|l| l.para).collect();
        let (a, b) = (paras.iter().min().copied(), paras.iter().max().copied());
        if let (Some(a), Some(b)) = (a, b) {
            for p in st.paras.get(a..=b).unwrap_or(&[]) {
                out.push(p.plain());
            }
        }
    }
    out.join("\n")
}

fn export_epub(c: &mut Ctx, p: &Value) -> Result<Value> {
    let o = pub_opts(c, p)?;
    let cover = if args::opt_bool(p, "cover").unwrap_or(false) { Some(page_png(c, 0, 1.5)?.0) } else { None };
    let bytes = publish::epub(c.doc()?, &o, cover, None).map_err(EngineError::new)?;
    deliver(p, bytes, json!({"warnings": []}))
}

fn export_fixed_epub(c: &mut Ctx, p: &Value) -> Result<Value> {
    let o = pub_opts(c, p)?;
    let n = c.doc()?.page_count();
    let mut pages = Vec::new();
    for i in 0..n {
        let (png, text) = page_png(c, i, 2.0)?;
        let pg = c.doc()?.page(i).ok_or_else(|| EngineError::new("no page"))?;
        pages.push(publish::FixedPage { png, width: pg.width, height: pg.height, text });
    }
    let bytes = publish::epub(c.doc()?, &o, None, Some(pages)).map_err(EngineError::new)?;
    deliver(p, bytes, json!({"pages": n, "warnings": ["fixed-layout pages are drawn as images with the page text kept for search and reading aids"]}))
}

fn export_html(c: &mut Ctx, p: &Value) -> Result<Value> {
    let o = pub_opts(c, p)?;
    let html = publish::html(c.doc()?, &o);
    if args::opt_str(p, "path").is_some() {
        return deliver(p, html.into_bytes(), json!({}));
    }
    Ok(json!({"text": html, "mime": "text/html"}))
}

fn rtf_escape(s: &str) -> String {
    let mut o = String::new();
    for ch in s.chars() {
        match ch {
            '\\' | '{' | '}' => {
                o.push('\\');
                o.push(ch);
            }
            '\t' => o.push_str("\\tab "),
            '\u{2028}' => o.push_str("\\line "),
            c if (c as u32) < 128 => o.push(c),
            c => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    o.push_str(&format!("\\u{}?", *u as i16));
                }
            }
        }
    }
    o
}

fn tagged_escape(s: &str) -> String {
    let mut o = String::new();
    for ch in s.chars() {
        match ch {
            '<' | '>' | '\\' => {
                o.push('\\');
                o.push(ch);
            }
            c if (c as u32) < 128 || c.is_alphanumeric() => o.push(c),
            c => o.push_str(&format!("<0x{:04X}>", c as u32)),
        }
    }
    o
}

fn export_text(c: &mut Ctx, p: &Value) -> Result<Value> {
    let targets = text_targets(c).map_err(|_| EngineError::new("needs a text frame or text cursor: select the story to export"))?;
    let doc = c.doc()?;
    let st = doc.story(targets[0].0).ok_or_else(|| EngineError::new("no such story"))?;
    let format = args::opt_str(p, "format").unwrap_or("txt");
    let text = match format {
        "rtf" => {
            let mut o = String::from("{\\rtf1\\ansi\\deff0{\\fonttbl{\\f0 Times New Roman;}}\n");
            for para in &st.paras {
                o.push_str(&rtf_escape(&para.plain()));
                o.push_str("\\par\n");
            }
            o.push('}');
            o
        }
        "tagged" => {
            let mut o = String::from("<UNICODE-WIN>\r\n");
            for para in &st.paras {
                o.push_str(&format!("<ParaStyle:{}>", tagged_escape(&para.style)));
                for s in &para.segs {
                    if let Seg::Text { text, style, .. } = s {
                        match style {
                            Some(cs) => o.push_str(&format!("<CharStyle:{}>{}<CharStyle:>", tagged_escape(cs), tagged_escape(text))),
                            None => o.push_str(&tagged_escape(text)),
                        }
                    }
                }
                o.push_str("\r\n");
            }
            o
        }
        _ => st.plain(),
    };
    if args::opt_str(p, "path").is_some() {
        return deliver(p, text.into_bytes(), json!({}));
    }
    Ok(json!({"text": text, "mime": if format == "rtf" { "application/rtf" } else { "text/plain" }}))
}

fn export_xml(c: &mut Ctx, p: &Value) -> Result<Value> {
    let xml = crate::cmd::xml::export(c.doc()?);
    if args::opt_str(p, "path").is_some() {
        return deliver(p, xml.into_bytes(), json!({}));
    }
    Ok(json!({"text": xml}))
}

fn shift_brush(b: &Brush, d: Affine) -> Brush {
    match b {
        Brush::Solid(c, a) => Brush::Solid(*c, *a),
        Brush::Linear { start, end, stops, alpha } => Brush::Linear { start: d * *start, end: d * *end, stops: stops.clone(), alpha: *alpha },
        Brush::Radial { center, radius, stops, alpha } => Brush::Radial { center: d * *center, radius: *radius, stops: stops.clone(), alpha: *alpha },
    }
}

fn shift_op(op: &Op, d: Affine) -> Op {
    match op {
        Op::Fill { path, brush, even_odd } => Op::Fill { path: d * path.clone(), brush: shift_brush(brush, d), even_odd: *even_odd },
        Op::Stroke { path, brush, style } => Op::Stroke { path: d * path.clone(), brush: shift_brush(brush, d), style: style.clone() },
        Op::Glyphs(g) => {
            let mut g = g.clone();
            g.xf = d * g.xf;
            if let Some(f) = &g.fill {
                g.fill = Some(shift_brush(f, d));
            }
            Op::Glyphs(g)
        }
        Op::Image { asset, xf, w, h } => Op::Image { asset: *asset, xf: d * *xf, w: *w, h: *h },
        Op::PushClip(p) => Op::PushClip(d * p.clone()),
        Op::PushMask(v) => Op::PushMask(v.iter().map(|o| shift_op(o, d)).collect()),
        Op::Mesh { patches, clip, fallback } => Op::Mesh {
            patches: patches.iter().map(|p| formdesign_render::scene::MeshPatch { pts: p.pts.map(|q| d * q), colors: p.colors }).collect(),
            clip: d * clip.clone(),
            fallback: fallback.iter().map(|o| shift_op(o, d)).collect(),
        },
        Op::Link { rect, target } => Op::Link { rect: d.transform_rect_bbox(*rect), target: target.clone() },
        other => other.clone(),
    }
}

fn print_booklet(c: &mut Ctx, p: &Value) -> Result<Value> {
    let lay = c.layout()?;
    let renderer = c.session.renderer.clone();
    let doc = c.doc()?;
    let list = page_list(doc, args::opt_str(p, "pages"))?;
    let kind = args::opt_str(p, "type").unwrap_or("2upSaddleStitch");
    let gap = args::opt_f64(p, "spaceBetweenPages").unwrap_or(0.0).max(0.0);
    let creep = args::opt_f64(p, "creep").unwrap_or(0.0);
    let mut order: Vec<Option<usize>> = list.iter().map(|x| Some(*x)).collect();
    let block = if kind == "consecutive" { 2 } else { 4 };
    while !order.len().is_multiple_of(block) {
        order.push(None);
    }
    let n = order.len();
    let mut pairs: Vec<(Option<usize>, Option<usize>, usize)> = Vec::new();
    match kind {
        "consecutive" | "2upPerfectBound" => {
            for k in (0..n).step_by(2) {
                pairs.push((order[k], order[k + 1], 0));
            }
        }
        _ => {
            for k in 0..n / 2 {
                let (a, b) = (order[n - 1 - k], order[k]);
                let sheet = k / 2;
                pairs.push(if k % 2 == 0 { (a, b, sheet) } else { (order[k], order[n - 1 - k], sheet) });
            }
        }
    }
    let w = doc.settings.page_width;
    let h = doc.settings.page_height;
    let scene = |abs: Option<usize>| abs.and_then(|a| page_scene(doc, &lay, a, &SceneOpts { printing: true, bleed: false, overprint: false }));
    let mut sheets = Vec::new();
    let sheets_total = (n / 4).max(1) as f64;
    for (i, (l, r, sheet)) in pairs.iter().enumerate() {
        let push = creep * (1.0 - *sheet as f64 / sheets_total);
        let mut ops = Vec::new();
        if let Some(s) = scene(*l) {
            let d = Affine::translate((-push, 0.0));
            ops.extend(s.ops.iter().map(|o| shift_op(o, d)));
        }
        if let Some(s) = scene(*r) {
            let d = Affine::translate((w + gap + push, 0.0));
            ops.extend(s.ops.iter().map(|o| shift_op(o, d)));
        }
        sheets.push(PageScene { page: i, width: w * 2.0 + gap, height: h, bleed: Edges::default(), ops });
    }
    let bytes = formdesign_render::pdf::export(&sheets, doc, &renderer.images, &formdesign_render::pdf::PdfOpts { title: doc.title.clone(), ..Default::default() }).map_err(|e| EngineError::new(format!("booklet PDF: {e}")))?;
    deliver(p, bytes, json!({"pages": sheets.len(), "warnings": []}))
}

fn fonts_used(doc: &Doc) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut add = |a: &Attrs| {
        if let Some(f) = a.get("font").and_then(Value::as_str) {
            let s = (f.to_string(), a.get("fontStyle").and_then(Value::as_str).unwrap_or("Regular").to_string());
            if !out.contains(&s) {
                out.push(s);
            }
        }
    };
    for k in [StyleKind::Paragraph, StyleKind::Character] {
        for s in doc.styles.list(k) {
            add(&s.attrs);
        }
    }
    crate::walk::all_paras(doc, &mut |_, p| {
        add(&p.over);
        for s in &p.segs {
            add(s.over());
        }
    });
    out.sort();
    out
}

fn font_restricted(data: &[u8], index: u32) -> bool {
    let be32 = |o: usize| data.get(o..o + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let be16 = |o: usize| data.get(o..o + 2).map(|b| u16::from_be_bytes([b[0], b[1]]) as usize);
    let start = if data.get(0..4) == Some(b"ttcf") { be32(12 + 4 * index as usize).unwrap_or(0) } else { 0 };
    let n = be16(start + 4).unwrap_or(0);
    for i in 0..n {
        let rec = start + 12 + 16 * i;
        if data.get(rec..rec + 4) == Some(b"OS/2") {
            let off = be32(rec + 8).unwrap_or(0);
            let fs_type = be16(off + 8).unwrap_or(0);
            return fs_type & 0x000f == 0x0002;
        }
    }
    false
}

fn package(c: &mut Ctx, p: &Value) -> Result<Value> {
    let dir = PathBuf::from(args::str_(p, "dir", "the folder to package into")?);
    let links = dir.join("Links");
    std::fs::create_dir_all(&links).map_err(|e| EngineError::new(format!("cannot create {}: {e}", links.display())))?;
    let title = c.state()?.title();
    let mut doc = c.doc()?.clone();
    let mut files: Vec<String> = Vec::new();
    let mut missing = Vec::new();
    let ids: Vec<_> = doc.assets.keys().copied().collect();
    for id in ids {
        let Some(a) = doc.assets.get_mut(&id) else { continue };
        let name = a.name.clone();
        let data: Option<Vec<u8>> = match &a.link {
            Some(l) if Path::new(l).exists() => std::fs::read(l).ok(),
            _ if !a.data.is_empty() => Some(a.data.to_vec()),
            _ => None,
        };
        let mut name = name;
        let mut k = 1;
        while files.contains(&format!("Links/{name}")) {
            k += 1;
            let p = Path::new(&a.name);
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("link");
            name = match p.extension().and_then(|s| s.to_str()) {
                Some(ext) => format!("{stem} {k}.{ext}"),
                None => format!("{stem} {k}"),
            };
        }
        match data {
            Some(d) => {
                let target = links.join(&name);
                std::fs::write(&target, &d).map_err(|e| EngineError::new(format!("cannot write {}: {e}", target.display())))?;
                a.link = Some(target.display().to_string());
                files.push(format!("Links/{name}"));
            }
            None => missing.push(name),
        }
    }
    let native = format!("{title}.{NATIVE_EXT}");
    std::fs::write(dir.join(&native), native_bytes(&doc)?).map_err(|e| EngineError::new(format!("cannot write the document: {e}")))?;
    files.insert(0, native);
    if args::opt_bool(p, "idml").unwrap_or(false) {
        let b = formdesign_interchange::idml::write::write(&doc).map_err(EngineError::new)?;
        let n = format!("{title}.idml");
        std::fs::write(dir.join(&n), b).map_err(|e| EngineError::new(format!("cannot write IDML: {e}")))?;
        files.push(n);
    }
    if args::opt_bool(p, "pdf").unwrap_or(false) {
        let lay = c.layout()?;
        let b = c.session.renderer.pdf(&doc, &lay, None, false).map_err(EngineError::new)?;
        let n = format!("{title}.pdf");
        std::fs::write(dir.join(&n), b).map_err(|e| EngineError::new(format!("cannot write PDF: {e}")))?;
        files.push(n);
    }
    let used = fonts_used(&doc);
    let copy_fonts = args::opt_bool(p, "fonts").unwrap_or(true);
    let mut fonts: Vec<String> = Vec::new();
    let mut font_lines: Vec<String> = Vec::new();
    let mut written: Vec<String> = Vec::new();
    if copy_fonts {
        std::fs::create_dir_all(dir.join("Document fonts")).map_err(|e| EngineError::new(format!("cannot create Document fonts: {e}")))?;
    }
    for (family, style) in &used {
        let label = format!("{family} {style}");
        fonts.push(label.clone());
        let Some(face) = c.session.renderer.fonts.resolve(family, style) else {
            font_lines.push(format!("  {label}: missing on this computer"));
            continue;
        };
        if face.family != *family {
            font_lines.push(format!("  {label}: missing, drawn with {} {}", face.family, face.style));
            continue;
        }
        if !copy_fonts {
            font_lines.push(format!("  {label}"));
            continue;
        }
        if font_restricted(&face.data, face.index) {
            font_lines.push(format!("  {label}: not copied, its licence forbids embedding (OS/2 fsType)"));
            continue;
        }
        let ext = match face.data.get(0..4) {
            Some(b"OTTO") => "otf",
            Some(b"ttcf") => "ttc",
            _ => "ttf",
        };
        let base: String = if face.postscript.is_empty() { label.replace(' ', "-") } else { face.postscript.clone() };
        let name = format!("{base}.{ext}");
        if !written.contains(&name) {
            std::fs::write(dir.join("Document fonts").join(&name), face.data.as_slice()).map_err(|e| EngineError::new(format!("cannot copy {label}: {e}")))?;
            written.push(name.clone());
            files.push(format!("Document fonts/{name}"));
        }
        font_lines.push(format!("  {label}: copied as Document fonts/{name}"));
    }
    let mut report = format!("Package report: {title}\n\nPages: {}\nLinks: {}\nMissing links: {}\n\nFonts ({}; check each font's licence before sending it):\n", doc.page_count(), doc.assets.len(), if missing.is_empty() { "none".into() } else { missing.join(", ") }, if copy_fonts { "copied into Document fonts unless noted" } else { "not copied" });
    for f in &font_lines {
        report.push_str(f);
        report.push('\n');
    }
    report.push_str("\nSwatches:\n");
    for s in &doc.swatches {
        report.push_str(&format!("  {}{}\n", s.name, if s.spot { " (spot)" } else { "" }));
    }
    let rn = format!("{title} Report.txt");
    std::fs::write(dir.join(&rn), &report).map_err(|e| EngineError::new(format!("cannot write the report: {e}")))?;
    files.push(rn);
    if let Some(ins) = args::opt_str(p, "instructions").filter(|s| !s.trim().is_empty()) {
        std::fs::write(dir.join("Instructions.txt"), ins).map_err(|e| EngineError::new(format!("cannot write instructions: {e}")))?;
        files.push("Instructions.txt".into());
    }
    Ok(json!({"dir": dir.display().to_string(), "files": files, "report": report, "missing": missing, "fonts": fonts}))
}

fn recovery_save(c: &mut Ctx, p: &Value) -> Result<Value> {
    let dir = PathBuf::from(args::str_(p, "dir", "the recovery folder")?);
    std::fs::create_dir_all(&dir).map_err(|e| EngineError::new(format!("cannot create {}: {e}", dir.display())))?;
    let mut saved = 0;
    for st in &c.session.docs {
        let base = dir.join(format!("{}.{NATIVE_EXT}", st.uid));
        let meta = dir.join(format!("{}.json", st.uid));
        if st.is_dirty() {
            std::fs::write(&base, native_bytes(&st.doc)?).map_err(|e| EngineError::new(format!("cannot write {}: {e}", base.display())))?;
            let _ = std::fs::write(&meta, json!({"title": st.title(), "path": st.path.as_ref().map(|p| p.display().to_string())}).to_string());
            saved += 1;
        } else {
            let _ = std::fs::remove_file(&base);
            let _ = std::fs::remove_file(&meta);
        }
    }
    for (path, _) in recovery_entries(&dir) {
        let stale = path.file_stem().and_then(|s| s.to_str()).and_then(|s| s.parse::<u64>().ok()).is_some_and(|uid| !c.session.docs.iter().any(|st| st.uid == uid && st.is_dirty()));
        if stale {
            let _ = std::fs::remove_file(&path);
            let _ = std::fs::remove_file(path.with_extension("json"));
        }
    }
    Ok(json!({"saved": saved}))
}

fn recovery_entries(dir: &Path) -> Vec<(PathBuf, Value)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<(PathBuf, Value)> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some(NATIVE_EXT))
        .map(|p| {
            let meta = std::fs::read_to_string(p.with_extension("json")).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()).unwrap_or(json!({}));
            (p, meta)
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn recovery_list(_c: &mut Ctx, p: &Value) -> Result<Value> {
    let dir = PathBuf::from(args::str_(p, "dir", "the recovery folder")?);
    Ok(Value::Array(
        recovery_entries(&dir)
            .into_iter()
            .map(|(path, meta)| json!({"title": meta.get("title").cloned().unwrap_or(json!("Untitled")), "file": path.file_name().and_then(|n| n.to_str()).unwrap_or("")}))
            .collect(),
    ))
}

fn recovery_open(c: &mut Ctx, p: &Value) -> Result<Value> {
    let dir = PathBuf::from(args::str_(p, "dir", "the recovery folder")?);
    let mut opened = 0;
    for (path, meta) in recovery_entries(&dir) {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let Ok(mut doc) = formdesign_model::format::load(&bytes) else { continue };
        if let Some(t) = meta.get("title").and_then(Value::as_str) {
            doc.title = t.to_string();
        }
        let orig = meta.get("path").and_then(Value::as_str).map(PathBuf::from);
        let modified = |f: &Path| std::fs::metadata(f).and_then(|m| m.modified()).ok();
        if let (Some(on_disk), Some(snap)) = (orig.as_deref().and_then(modified), modified(&path))
            && on_disk > snap
        {
            let _ = std::fs::remove_file(&path);
            let _ = std::fs::remove_file(path.with_extension("json"));
            continue;
        }
        let i = c.session.open_doc(doc, orig);
        let st = &mut c.session.docs[i];
        st.saved = Arc::new((*st.doc).clone());
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("json"));
        opened += 1;
    }
    c.session.sel_rev += 1;
    Ok(json!({"opened": opened}))
}

pub fn register(r: &mut Registry) {
    cmd("file.new", "New Document...").menu(&["File", "New", "Document..."]).key("Mod+N").needs(Needs::Nothing).query().sig("{width?, height?, preset?, pages?, facingPages?, margins?, columns?, gutter?, bleed?, title?, intent?}").reg(r, file_new);
    cmd("file.open", "Open...").menu(&["File", "Open..."]).key("Mod+O").needs(Needs::Nothing).query().sig("{path}").reg(r, file_open);
    cmd("file.openBytes", "Open").needs(Needs::Nothing).query().sig("{name, base64}").reg(r, file_open_bytes);
    cmd("file.openIdml", "Open IDML").needs(Needs::Nothing).query().sig("{name, base64}").reg(r, file_open_idml);
    cmd("file.save", "Save").menu(&["File", "Save"]).key("Mod+S").query().sig("{}").reg(r, file_save);
    cmd("file.saveAs", "Save As...").menu(&["File", "Save As..."]).key("Mod+Shift+S").query().sig("{path}").reg(r, file_save_as);
    cmd("file.close", "Close").menu(&["File", "Close"]).key("Mod+W").query().sig("{}").reg(r, file_close);
    cmd("file.place", "Place...").menu(&["File", "Place..."]).key("Mod+D").sig("{path} | {path, frame, page?} | {base64, name, frame}").reg(r, file_place);
    cmd("file.exportPdf", "Export PDF...").menu(&["File", "Export", "PDF..."]).query().sig("{pages?, bleed?, marks?: bool | {crop, bleed, registration, colorBars, pageInfo, type, weight, offset}, cropMarks?, bleedMarks?, registrationMarks?, colorBars?, pageInformation?, markType?: default|japanese, markWeight?: pt, markOffset?: pt, standard?, tagged?, spreads?, path?}").reg(r, export_pdf);
    cmd("file.interactiveInfo", "Interactive PDF Contents").query().sig("{pages?, mediaDir?}").reg(r, interactive_info);
    cmd("file.exportIdml", "Export IDML...").menu(&["File", "Export", "IDML..."]).query().sig("{path?, embedImages?}").reg(r, export_idml);
    cmd("file.exportEpub", "Export EPUB (Reflowable)...").menu(&["File", "Export", "EPUB (Reflowable)..."]).query().sig("{title?, author?, language?, cover?, path?}").reg(r, export_epub);
    cmd("file.exportFixedEpub", "Export EPUB (Fixed Layout)...").menu(&["File", "Export", "EPUB (Fixed Layout)..."]).query().sig("{title?, author?, language?, path?}").reg(r, export_fixed_epub);
    cmd("file.exportHtml", "Export HTML...").menu(&["File", "Export", "HTML..."]).query().sig("{title?, language?, path?}").reg(r, export_html);
    cmd("file.exportText", "Export Text...").menu(&["File", "Export", "Text..."]).needs(Needs::Text).query().sig("{format: txt|rtf|tagged, path?}").reg(r, export_text);
    cmd("file.exportXml", "Export XML...").menu(&["File", "Export", "XML..."]).query().sig("{path?}").reg(r, export_xml);
    cmd("file.printBooklet", "Print Booklet...").menu(&["File", "Print Booklet..."]).query().sig("{path?, type?: 2upSaddleStitch|2upPerfectBound|consecutive, spaceBetweenPages?, creep?, pages?}").reg(r, print_booklet);
    cmd("file.package", "Package...").menu(&["File", "Package..."]).key("Mod+Alt+Shift+P").query().sig("{dir, idml?, pdf?, fonts?: true, instructions?}").reg(r, package);
    cmd("file.recovery.save", "Save Recovery Data").needs(Needs::Nothing).query().sig("{dir}").reg(r, recovery_save);
    cmd("file.recovery.list", "Recoverable Documents").needs(Needs::Nothing).query().sig("{dir}").reg(r, recovery_list);
    cmd("file.recovery.open", "Recover Unsaved Documents").menu(&["File", "Recover Unsaved Documents"]).needs(Needs::Nothing).query().sig("{dir}").reg(r, recovery_open);
}
