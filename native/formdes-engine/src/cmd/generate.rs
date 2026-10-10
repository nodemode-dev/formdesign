use std::collections::{BTreeMap, BTreeSet};

use formdes_model::doc::{Doc, Home};
use formdes_model::geom::{Rect, Vec2};
use formdes_model::ids::{ItemId, StoryId};
use formdes_model::item::{Item, ItemKind, TextFrame};
use formdes_model::story::{Inline, Para, Seg};
use formdes_model::style::{BASIC_PARA, NamedStyle};
use formdes_text::layout::DocLayout;
use serde_json::{Value, json};

use crate::registry::{Needs, Registry, cmd};
use crate::session::{Ctx, Selection, TextSel};
use crate::{EngineError, Result, args};

pub fn page_of_pos(doc: &Doc, lay: &DocLayout, story: StoryId, pos: usize) -> Option<usize> {
    let sl = lay.stories.get(&story)?;
    let (para, byte) = sl.flow.para_byte(pos)?;
    let mut lines = sl.flow.lines.iter().filter(|l| l.para == para);
    let line = lines.clone().find(|l| byte < l.line.end.max(l.line.start + 1)).or_else(|| lines.next_back())?;
    let frame = *sl.frames.get(line.frame)?;
    doc.page_of(frame).or_else(|| crate::cmd::object::item_page(doc, frame))
}

fn reading_order(doc: &Doc) -> Vec<StoryId> {
    let mut keyed: Vec<((usize, i64, i64), StoryId)> = doc
        .stories
        .values()
        .filter_map(|s| {
            let f = *s.frames.first()?;
            let page = doc.page_of(f).or_else(|| crate::cmd::object::item_page(doc, f)).unwrap_or(usize::MAX);
            let b = doc.bounds(f).unwrap_or(Rect::ZERO);
            Some(((page, b.y0 as i64, b.x0 as i64), s.id))
        })
        .collect();
    keyed.sort();
    keyed.into_iter().map(|(_, s)| s).collect()
}

fn generated(doc: &Doc) -> Vec<StoryId> {
    ["toc", "index"].iter().filter_map(|k| doc.prefs.get(*k).and_then(|v| v.get("story")).and_then(Value::as_u64)).map(StoryId).collect()
}

fn ensure_style(doc: &mut Doc, name: &str, attrs: Value) {
    if doc.styles.paragraph.iter().any(|s| s.name == name) {
        return;
    }
    let mut s = NamedStyle::new(name);
    s.based_on = Some(BASIC_PARA.to_string());
    if let Value::Object(o) = attrs {
        for (k, v) in o {
            s.attrs.insert(k, v);
        }
    }
    doc.styles.paragraph.push(s);
}

fn column_width(doc: &Doc, story: StoryId) -> f64 {
    let Some(f) = doc.story(story).and_then(|s| s.frames.first().copied()) else { return 300.0 };
    let Some(it) = doc.item(f) else { return 300.0 };
    let b = it.shape.bbox();
    match it.text() {
        Some(tf) => {
            let n = tf.columns.max(1) as f64;
            ((b.width() - tf.insets.inside - tf.insets.outside - tf.gutter * (n - 1.0)) / n).max(24.0)
        }
        None => b.width().max(24.0),
    }
}

fn target_story(c: &mut Ctx, p: &Value, key: &str) -> Result<StoryId> {
    let doc = c.doc()?;
    if let Some(f) = crate::target::one_id(p, "frame") {
        return doc.item(f).and_then(|i| i.story()).ok_or_else(|| EngineError::new(format!("object {f} is not a text frame")));
    }
    if p.get("page").is_none()
        && let Some(s) = doc.prefs.get(key).and_then(|v| v.get("story")).and_then(Value::as_u64).map(StoryId)
        && doc.story(s).is_some_and(|st| !st.frames.is_empty())
    {
        return Ok(s);
    }
    let sel = c.selection();
    if p.get("page").is_none() {
        if let Some(t) = sel.text {
            return Ok(t.story);
        }
        if let [one] = sel.items.as_slice()
            && let Some(s) = doc.item(*one).and_then(|i| i.story())
        {
            return Ok(s);
        }
    }
    let n = doc.page_count();
    let page = match args::opt_u64(p, "page") {
        Some(0) => return Err(EngineError::new("`page` is 1-based")),
        Some(k) if k as usize > n => return Err(EngineError::new(format!("no page {k} (the document has {n})"))),
        Some(k) => k as usize - 1,
        None => n - 1,
    };
    let (si, _) = doc.page_slot(page).ok_or_else(|| EngineError::new("no such page"))?;
    let r = crate::cmd::object::margin_rect(doc, page).ok_or_else(|| EngineError::new("no such page"))?;
    let layer = c.target_layer()?;
    let doc = c.doc_mut()?;
    let sid = doc.new_story(BASIC_PARA);
    let mut it = Item::new(ItemId(0), ItemKind::Text(TextFrame::new(sid)), formdes_model::item::Shape::rect(r), layer);
    it.style = Some(formdes_model::style::BASIC_TEXT.to_string());
    let id = doc.insert_item(it, Home::Spread(si));
    if let Some(st) = doc.story_mut(sid) {
        st.frames = vec![id];
    }
    Ok(sid)
}

fn write_story(doc: &mut Doc, sid: StoryId, paras: Vec<Para>) {
    if let Some(st) = doc.story_mut(sid) {
        st.paras = if paras.is_empty() { vec![Para::empty(BASIC_PARA)] } else { paras };
    }
}

fn entries_of(p: &Value, doc: &Doc) -> Result<Vec<(String, usize)>> {
    let mut out = Vec::new();
    if let Some(Value::Array(a)) = p.get("entries") {
        for (i, e) in a.iter().enumerate() {
            let (style, level) = match e {
                Value::String(s) => (s.clone(), i + 1),
                v => (v.get("style").and_then(Value::as_str).unwrap_or("").to_string(), v.get("level").and_then(Value::as_u64).map(|n| n as usize).unwrap_or(i + 1)),
            };
            if !style.is_empty() {
                out.push((style, level.clamp(1, 9)));
            }
        }
    }
    if out.is_empty() {
        for s in &doc.styles.paragraph {
            let n = s.name.to_ascii_lowercase();
            let lvl = ["heading 1", "head 1", "h1", "title", "chapter title", "heading 2", "head 2", "h2", "heading 3", "head 3", "h3"].iter().position(|x| *x == n).map(|i| match i {
                0..=4 => 1,
                5..=7 => 2,
                _ => 3,
            });
            if let Some(l) = lvl {
                out.push((s.name.clone(), l));
            }
        }
    }
    if out.is_empty() {
        let names: Vec<&str> = doc.styles.paragraph.iter().map(|s| s.name.as_str()).collect();
        return Err(EngineError::new(format!("missing `entries`: the paragraph styles to collect, e.g. [{{style, level}}]; the document's styles are {}", names.join(", "))));
    }
    Ok(out)
}

fn toc_build(c: &mut Ctx, settings: &Value) -> Result<(StoryId, usize)> {
    let lay = c.layout()?;
    let doc = c.doc()?;
    let entries = entries_of(settings, doc)?;
    let skip = generated(doc);
    let title = settings.get("title").and_then(Value::as_str).unwrap_or("Contents").to_string();
    let numbers = settings.get("pageNumbers").and_then(Value::as_str).unwrap_or("after") != "none";
    let leader = settings.get("leader").and_then(Value::as_str).unwrap_or(".").to_string();
    let mut found: Vec<(usize, usize, usize, usize, String)> = Vec::new();
    for (order, sid) in reading_order(doc).into_iter().enumerate() {
        if skip.contains(&sid) {
            continue;
        }
        let Some(st) = doc.story(sid) else { continue };
        for (i, para) in st.paras.iter().enumerate() {
            let Some((_, level)) = entries.iter().find(|(s, _)| *s == para.style) else { continue };
            let text = para.plain().trim().to_string();
            if text.is_empty() {
                continue;
            }
            let page = page_of_pos(doc, &lay, sid, st.para_start(i)).unwrap_or(usize::MAX);
            found.push((page, order, i, *level, text));
        }
    }
    found.sort_by_key(|f| (f.0, f.1, f.2));
    let sid = StoryId(settings.get("story").and_then(Value::as_u64).unwrap_or(0));
    let width = column_width(doc, sid);
    let names: Vec<String> = found.iter().map(|f| if f.0 == usize::MAX { String::new() } else { doc.page_name(f.0) }).collect();
    let doc = c.doc_mut()?;
    ensure_style(doc, "TOC Title", json!({"size": 18.0, "spaceAfter": 12.0}));
    let mut paras = Vec::new();
    if !title.is_empty() {
        paras.push(Para::from_text("TOC Title", &title));
    }
    for (k, f) in found.iter().enumerate() {
        let style = format!("TOC Level {}", f.3);
        ensure_style(doc, &style, json!({"leftIndent": 12.0 * (f.3 as f64 - 1.0), "tabs": [{"position": width, "align": "right", "leader": leader}]}));
        let line = if numbers && !names[k].is_empty() { format!("{}\t{}", f.4, names[k]) } else { f.4.clone() };
        paras.push(Para::from_text(&style, &line));
    }
    write_story(doc, sid, paras);
    Ok((sid, found.len()))
}

fn toc_generate(c: &mut Ctx, p: &Value) -> Result<Value> {
    let sid = target_story(c, p, "toc")?;
    let mut settings = c.doc()?.prefs.get("toc").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
    for k in ["title", "entries", "pageNumbers", "leader"] {
        if let Some(v) = p.get(k) {
            settings[k] = v.clone();
        }
    }
    settings["story"] = json!(sid);
    let (sid, n) = toc_build(c, &settings)?;
    c.doc_mut()?.prefs.insert("toc".into(), settings);
    c.set_selection(Selection::default());
    c.label("Table of Contents");
    Ok(json!({"story": sid, "entries": n}))
}

fn toc_update(c: &mut Ctx, _: &Value) -> Result<Value> {
    let settings = c.doc()?.prefs.get("toc").cloned().ok_or_else(|| EngineError::new("the document has no table of contents yet: run Layout > Table of Contents first"))?;
    let sid = StoryId(settings.get("story").and_then(Value::as_u64).unwrap_or(0));
    if c.doc()?.story(sid).is_none_or(|s| s.frames.is_empty()) {
        return Err(EngineError::new("the table of contents' frame was deleted: generate it again"));
    }
    let (_, n) = toc_build(c, &settings)?;
    c.label("Update Table of Contents");
    Ok(json!({"story": sid, "entries": n}))
}

fn caret(c: &Ctx) -> Result<TextSel> {
    c.selection().text.ok_or_else(|| EngineError::new("needs a text cursor: click into text with the Type tool"))
}

fn selected_text(c: &Ctx, t: TextSel) -> Option<String> {
    let st = c.doc().ok()?.story(t.story)?;
    let s: String = st.text().chars().skip(t.range().start).take(t.range().len()).filter(|ch| *ch != '\u{FFFC}').collect();
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    (!s.is_empty()).then_some(s)
}

fn insert_marker(c: &mut Ctx, t: TextSel, inline: Inline) -> Result<usize> {
    let r = t.range();
    let doc = c.doc_mut()?;
    let st = doc.story_mut(t.story).ok_or_else(|| EngineError::new("no such story"))?;
    st.insert_inline(r.start, inline);
    c.set_selection(Selection::text(TextSel { story: t.story, anchor: t.anchor + 1, focus: t.focus + 1 }));
    Ok(r.start)
}

fn index_add(c: &mut Ctx, p: &Value) -> Result<Value> {
    let t = caret(c)?;
    let topic = args::opt_str(p, "topic").map(str::to_string).or_else(|| selected_text(c, t)).ok_or_else(|| EngineError::new("missing `topic` (or select the words to index)"))?;
    let sub = args::opt_str(p, "sub").unwrap_or("").to_string();
    let see = args::opt_str(p, "see").unwrap_or("").to_string();
    let at = insert_marker(c, t, Inline::Index { topic: topic.clone(), sub, see })?;
    c.label("New Page Reference");
    Ok(json!({"topic": topic, "at": at}))
}

struct Topic {
    pages: BTreeSet<usize>,
    see: String,
    subs: BTreeMap<String, BTreeSet<usize>>,
}

fn collect_index(doc: &Doc, lay: &DocLayout) -> BTreeMap<String, (String, Topic)> {
    let skip = generated(doc);
    let mut out: BTreeMap<String, (String, Topic)> = BTreeMap::new();
    for (sid, st) in &doc.stories {
        if skip.contains(sid) {
            continue;
        }
        for (at, inl) in st.inlines() {
            let Inline::Index { topic, sub, see } = inl else { continue };
            let page = page_of_pos(doc, lay, *sid, at);
            let e = out.entry(topic.to_lowercase()).or_insert_with(|| (topic.clone(), Topic { pages: BTreeSet::new(), see: String::new(), subs: BTreeMap::new() }));
            if !see.is_empty() {
                e.1.see = see.clone();
            }
            let set = if sub.is_empty() { &mut e.1.pages } else { e.1.subs.entry(sub.clone()).or_default() };
            if let Some(pg) = page {
                set.insert(pg);
            }
        }
    }
    out
}

fn page_list(doc: &Doc, pages: &BTreeSet<usize>) -> String {
    let v: Vec<usize> = pages.iter().copied().collect();
    let mut parts = Vec::new();
    let mut i = 0;
    while i < v.len() {
        let mut j = i;
        while j + 1 < v.len() && v[j + 1] == v[j] + 1 {
            j += 1;
        }
        parts.push(if j > i + 1 { format!("{}\u{2013}{}", doc.page_name(v[i]), doc.page_name(v[j])) } else if j == i + 1 { format!("{}, {}", doc.page_name(v[i]), doc.page_name(v[j])) } else { doc.page_name(v[i]) });
        i = j + 1;
    }
    parts.join(", ")
}

fn index_list(c: &mut Ctx, _: &Value) -> Result<Value> {
    let lay = c.layout()?;
    let doc = c.doc()?;
    let topics: Vec<Value> = collect_index(doc, &lay)
        .values()
        .map(|(name, t)| json!({"topic": name, "pages": page_list(doc, &t.pages), "see": t.see, "subtopics": t.subs.iter().map(|(s, p)| json!({"topic": s, "pages": page_list(doc, p)})).collect::<Vec<_>>()}))
        .collect();
    Ok(json!({"topics": topics}))
}

fn index_build(c: &mut Ctx, settings: &Value) -> Result<usize> {
    let lay = c.layout()?;
    let doc = c.doc()?;
    let topics = collect_index(doc, &lay);
    let title = settings.get("title").and_then(Value::as_str).unwrap_or("Index").to_string();
    let heads = settings.get("headings").and_then(Value::as_bool).unwrap_or(true);
    let mut lines: Vec<(&str, String)> = Vec::new();
    let mut letter = None;
    for (key, (name, t)) in &topics {
        let first = key.chars().next().map(|ch| if ch.is_alphabetic() { ch.to_ascii_uppercase() } else { '#' });
        if heads && first != letter {
            letter = first;
            lines.push(("Index Section Head", first.map(String::from).unwrap_or_default()));
        }
        let mut s = name.clone();
        if !t.pages.is_empty() {
            s.push_str(", ");
            s.push_str(&page_list(doc, &t.pages));
        }
        if !t.see.is_empty() {
            s.push_str(&format!(". See {}", t.see));
        }
        lines.push(("Index Level 1", s));
        for (sub, pages) in &t.subs {
            let mut s = sub.clone();
            if !pages.is_empty() {
                s.push_str(", ");
                s.push_str(&page_list(doc, pages));
            }
            lines.push(("Index Level 2", s));
        }
    }
    let sid = StoryId(settings.get("story").and_then(Value::as_u64).unwrap_or(0));
    let n = topics.len();
    let doc = c.doc_mut()?;
    ensure_style(doc, "Index Title", json!({"size": 18.0, "spaceAfter": 12.0}));
    ensure_style(doc, "Index Section Head", json!({"spaceBefore": 9.0}));
    ensure_style(doc, "Index Level 1", json!({"leftIndent": 9.0, "firstIndent": -9.0}));
    ensure_style(doc, "Index Level 2", json!({"leftIndent": 18.0, "firstIndent": -9.0}));
    let mut paras = Vec::new();
    if !title.is_empty() {
        paras.push(Para::from_text("Index Title", &title));
    }
    paras.extend(lines.into_iter().map(|(s, t)| Para::from_text(s, &t)));
    write_story(doc, sid, paras);
    Ok(n)
}

fn index_generate(c: &mut Ctx, p: &Value) -> Result<Value> {
    let sid = target_story(c, p, "index")?;
    let mut settings = c.doc()?.prefs.get("index").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
    for k in ["title", "headings"] {
        if let Some(v) = p.get(k) {
            settings[k] = v.clone();
        }
    }
    settings["story"] = json!(sid);
    c.doc_mut()?.prefs.insert("index".into(), settings.clone());
    let n = index_build(c, &settings)?;
    c.set_selection(Selection::default());
    c.label("Generate Index");
    Ok(json!({"story": sid, "topics": n}))
}

fn anchors(doc: &Doc) -> Vec<(String, StoryId, usize)> {
    let mut out = Vec::new();
    for (sid, st) in &doc.stories {
        for (at, inl) in st.inlines() {
            if let Inline::Anchor { name } = inl {
                out.push((name.clone(), *sid, at));
            }
        }
    }
    out
}

fn anchor_create(c: &mut Ctx, p: &Value) -> Result<Value> {
    let t = caret(c)?;
    let existing = anchors(c.doc()?);
    let name = args::opt_str(p, "name").map(str::to_string).or_else(|| selected_text(c, t)).unwrap_or_else(|| format!("Anchor {}", existing.len() + 1));
    if existing.iter().any(|a| a.0 == name) {
        return Err(EngineError::new(format!("a text anchor named {name} exists already")));
    }
    let at = insert_marker(c, t, Inline::Anchor { name: name.clone() })?;
    c.label("New Hyperlink Destination");
    Ok(json!({"name": name, "at": at}))
}

fn anchor_list(c: &mut Ctx, _: &Value) -> Result<Value> {
    let lay = c.layout()?;
    let doc = c.doc()?;
    let out: Vec<Value> = anchors(doc).into_iter().map(|(n, s, at)| json!({"name": n, "story": s, "at": at, "page": page_of_pos(doc, &lay, s, at).map(|p| doc.page_name(p))})).collect();
    Ok(json!({"anchors": out}))
}

fn xref_text(doc: &Doc, lay: &DocLayout, anchor: &str, format: &str) -> String {
    let Some((_, sid, at)) = anchors(doc).into_iter().find(|a| a.0 == anchor) else { return format!("[{anchor}]") };
    let page = page_of_pos(doc, lay, sid, at).map(|p| doc.page_name(p)).unwrap_or_else(|| "?".into());
    let para = doc.story(sid).map(|st| st.paras.get(st.locate(at).0).map(Para::plain).unwrap_or_default()).unwrap_or_default();
    let para = para.trim();
    match format {
        "pageNumber" => page,
        "paragraph" => format!("\u{201C}{para}\u{201D}"),
        "paragraphText" | "text" => para.to_string(),
        "paragraphAndPage" => format!("\u{201C}{para}\u{201D} on page {page}"),
        _ => format!("page {page}"),
    }
}

fn xref_insert(c: &mut Ctx, p: &Value) -> Result<Value> {
    let t = caret(c)?;
    let anchor = args::str_(p, "anchor", "a text anchor's name from anchor.list")?.to_string();
    let format = args::opt_str(p, "format").unwrap_or("page").to_string();
    if !anchors(c.doc()?).iter().any(|a| a.0 == anchor) {
        return Err(EngineError::new(format!("no text anchor named {anchor}: make one with anchor.create")));
    }
    let lay = c.layout()?;
    let text = xref_text(c.doc()?, &lay, &anchor, &format);
    let r = t.range();
    let doc = c.doc_mut()?;
    let st = doc.story_mut(t.story).ok_or_else(|| EngineError::new("no such story"))?;
    if !r.is_empty() {
        st.delete(r.clone());
    }
    st.insert_inline(r.start, Inline::Xref { anchor, format, text: text.clone() });
    c.set_selection(Selection::text(TextSel::caret(t.story, r.start + 1)));
    c.label("New Cross-Reference");
    Ok(json!({"text": text}))
}

fn xref_update(c: &mut Ctx, _: &Value) -> Result<Value> {
    let lay = c.layout()?;
    let doc = c.doc()?;
    let mut changes: Vec<(StoryId, usize, String)> = Vec::new();
    for (sid, st) in &doc.stories {
        for (at, inl) in st.inlines() {
            if let Inline::Xref { anchor, format, text } = inl {
                let now = xref_text(doc, &lay, anchor, format);
                if &now != text {
                    changes.push((*sid, at, now));
                }
            }
        }
    }
    let n = changes.len();
    if n > 0 {
        let doc = c.doc_mut()?;
        for (sid, at, now) in changes {
            if let Some(Inline::Xref { text, .. }) = doc.story_mut(sid).and_then(|s| s.inline_at_mut(at)) {
                *text = now;
            }
        }
    }
    c.label("Update Cross-References");
    Ok(json!({"updated": n}))
}

fn xref_list(c: &mut Ctx, _: &Value) -> Result<Value> {
    let doc = c.doc()?;
    let mut out = Vec::new();
    for (sid, st) in &doc.stories {
        for (at, inl) in st.inlines() {
            if let Inline::Xref { anchor, format, text } = inl {
                out.push(json!({"story": sid, "at": at, "anchor": anchor, "format": format, "text": text}));
            }
        }
    }
    Ok(json!({"crossReferences": out}))
}

fn parse_csv(text: &str, delim: Option<char>) -> Vec<Vec<String>> {
    let text = text.trim_start_matches('\u{FEFF}');
    let first = text.lines().next().unwrap_or("");
    let d = delim.unwrap_or(if first.matches('\t').count() > first.matches(',').count() { '\t' } else { ',' });
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match (ch, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                cell.push('"');
                chars.next();
            }
            ('"', true) => quoted = false,
            ('"', false) if cell.is_empty() => quoted = true,
            (c2, false) if c2 == d => row.push(std::mem::take(&mut cell)),
            ('\r', false) => {}
            ('\n', false) => {
                row.push(std::mem::take(&mut cell));
                if !(row.len() == 1 && row[0].is_empty()) {
                    rows.push(std::mem::take(&mut row));
                } else {
                    row.clear();
                }
            }
            (c2, _) => cell.push(c2),
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows
}

fn data_load(c: &mut Ctx, p: &Value) -> Result<Value> {
    let text = match (args::opt_str(p, "text"), args::opt_str(p, "path")) {
        (Some(t), _) => t.to_string(),
        (None, Some(path)) => std::fs::read_to_string(path).map_err(|e| EngineError::new(format!("could not read {path}: {e}")))?,
        _ => return Err(EngineError::new("missing `text` (CSV or tab-separated data with a header row) or `path`")),
    };
    let delim = args::opt_str(p, "delimiter").and_then(|s| match s {
        "tab" | "\t" => Some('\t'),
        "comma" | "," => Some(','),
        "semicolon" | ";" => Some(';'),
        _ => None,
    });
    let mut rows = parse_csv(&text, delim);
    if rows.is_empty() {
        return Err(EngineError::new("the data source has no header row"));
    }
    let fields: Vec<String> = rows.remove(0).into_iter().map(|f| f.trim().to_string()).collect();
    rows.truncate(10_000);
    let n = rows.len();
    let source = args::opt_str(p, "path").unwrap_or("").to_string();
    c.doc_mut()?.prefs.insert("dataMerge".into(), json!({"fields": fields, "records": rows, "source": source}));
    c.label("Select Data Source");
    Ok(json!({"fields": fields, "records": n}))
}

fn data_get(c: &Ctx) -> Result<(Vec<String>, Vec<Vec<String>>)> {
    let v = c.doc()?.prefs.get("dataMerge").ok_or_else(|| EngineError::new("no data source: load one with data.load"))?;
    let fields: Vec<String> = v.get("fields").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
    let records: Vec<Vec<String>> = v.get("records").and_then(Value::as_array).map(|a| a.iter().map(|r| r.as_array().map(|c| c.iter().map(|x| x.as_str().unwrap_or("").to_string()).collect()).unwrap_or_default()).collect()).unwrap_or_default();
    Ok((fields, records))
}

fn data_fields(c: &mut Ctx, _: &Value) -> Result<Value> {
    let (fields, records) = data_get(c)?;
    Ok(json!({"fields": fields, "records": records.len(), "first": records.first()}))
}

fn data_insert_field(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = args::str_(p, "name", "a data field name")?.to_string();
    let (fields, _) = data_get(c)?;
    if !fields.contains(&name) {
        return Err(EngineError::new(format!("no field {name}; the fields are {}", fields.join(", "))));
    }
    caret(c)?;
    let n = crate::cmd::text::insert_at_selection(c, &format!("<<{name}>>"), true)?;
    c.label("Insert Data Field");
    Ok(json!({"inserted": n}))
}

fn fill_record(doc: &mut Doc, ids: &[ItemId], fields: &[String], rec: &[String]) {
    let value = |name: &str| fields.iter().position(|f| f == name).map(|i| rec.get(i).cloned().unwrap_or_default());
    for id in doc.walk(ids) {
        let Some(it) = doc.item(id).cloned() else { continue };
        if let Some(sid) = it.story()
            && let Some(st) = doc.story_mut(sid)
        {
            for para in st.paras.iter_mut() {
                for seg in para.segs.iter_mut() {
                    let Seg::Text { text, .. } = seg else { continue };
                    if !text.contains("<<") {
                        continue;
                    }
                    let mut out = String::new();
                    let mut rest = text.as_str();
                    while let Some(a) = rest.find("<<") {
                        out.push_str(&rest[..a]);
                        let after = &rest[a + 2..];
                        match after.find(">>") {
                            Some(b) => {
                                let key = &after[..b];
                                match value(key.trim_start_matches('@')) {
                                    Some(v) => out.push_str(&v),
                                    None => out.push_str(&rest[a..a + 2 + b + 2]),
                                }
                                rest = &after[b + 2..];
                            }
                            None => {
                                out.push_str(&rest[a..]);
                                rest = "";
                            }
                        }
                    }
                    out.push_str(rest);
                    *text = out;
                }
            }
        }
        let key = [it.label.as_str(), it.name.as_str()].into_iter().find_map(|s| s.strip_prefix("<<").and_then(|s| s.strip_suffix(">>")).map(|s| s.trim_start_matches('@').to_string()));
        if let Some(k) = key
            && !matches!(it.kind, ItemKind::Text(_) | ItemKind::Group { .. } | ItemKind::States { .. })
            && let Some(path) = value(&k).filter(|v| !v.is_empty())
            && let Ok(asset) = crate::assets::load(&path, 0)
        {
            let aid = doc.add_asset(asset);
            if let Some(m) = doc.item_mut(id) {
                m.kind = ItemKind::Graphic(formdes_model::item::Graphic { asset: Some(aid), content: formdes_model::geom::Affine::IDENTITY, fitting: None });
            }
            let _ = crate::cmd::object::fit_content(doc, id, "fillProportionally", None);
        }
    }
}

fn data_merge(c: &mut Ctx, p: &Value) -> Result<Value> {
    let (fields, records) = data_get(c)?;
    if records.is_empty() {
        return Err(EngineError::new("the data source has no records"));
    }
    let (from, to) = match p.get("records") {
        Some(Value::Array(a)) if a.len() == 2 => (a[0].as_u64().unwrap_or(1).max(1) as usize, a[1].as_u64().unwrap_or(records.len() as u64) as usize),
        _ => (1, records.len()),
    };
    let to = to.min(records.len()).min(from + 499);
    if from > to {
        return Err(EngineError::new(format!("no records in {from}..{to} (the source has {})", records.len())));
    }
    let base: Doc = c.doc()?.clone();
    let title = format!("{} merged", c.state()?.title());
    let mut doc = base.clone();
    let n0 = base.page_count();
    let originals: Vec<ItemId> = base.spreads.iter().flat_map(|s| s.items.clone()).collect();
    fill_record(&mut doc, &originals, &fields, &records[from - 1]);
    for (k, rec) in records.iter().enumerate().take(to).skip(from) {
        let block = k - from + 1;
        let mut m = crate::transplant::Mapper::new(&base);
        let mut copies: Vec<ItemId> = Vec::new();
        for id in &originals {
            let Some(Home::Spread(si)) = base.home(*id) else { continue };
            if let Some(n) = m.item(&mut doc, *id, Home::Spread(si), Vec2::ZERO) {
                copies.push(n);
            }
        }
        fill_record(&mut doc, &copies, &fields, rec);
        let base_pages: Vec<formdes_model::page::Page> = base.spreads.iter().flat_map(|s| s.pages.clone()).collect();
        doc.repaginate(|pages, carried, d| {
            for pg in &base_pages {
                let id = d.blank_page().id;
                let mut x = pg.clone();
                x.id = id;
                pages.push(x);
            }
            for c2 in carried.iter_mut() {
                if copies.contains(&c2.0) {
                    c2.1 += block * n0;
                }
            }
        });
    }
    doc.prefs.remove("dataMerge");
    let pages = doc.page_count();
    doc.title = title;
    let i = c.session.open_doc(doc, None);
    c.session.sel_rev += 1;
    Ok(json!({"document": i, "records": to - from + 1, "pages": pages}))
}

pub fn register(r: &mut Registry) {
    cmd("toc.generate", "Table of Contents").menu(&["Layout", "Table of Contents..."]).sig("{entries?: [{style, level?}] or [style names], title?, pageNumbers?: after|none, leader?, frame?, page?: 1-based}").help("Collects the paragraphs in the given styles, in page order, into a text frame (the selected one, `frame`, or a new frame on `page`, by default the last page) with their page numbers after a right tab.").reg(r, toc_generate);
    cmd("toc.update", "Update Table of Contents").menu(&["Layout", "Update Table of Contents"]).sig("{}").reg(r, toc_update);
    cmd("index.addReference", "New Page Reference").menu(&["Window", "Type & Tables", "Index"]).needs(Needs::Text).key("Mod+U").sig("{topic?: defaults to the selected text, sub?, see?}").reg(r, index_add);
    cmd("index.list", "Index Topics").query().sig("{}").reg(r, index_list);
    cmd("index.generate", "Generate Index").sig("{title?, headings?, frame?, page?: 1-based}").reg(r, index_generate);
    cmd("anchor.create", "New Hyperlink Destination").menu(&["Type", "Hyperlinks & Cross-References", "New Hyperlink Destination..."]).needs(Needs::Text).sig("{name?: defaults to the selected text}").reg(r, anchor_create);
    cmd("anchor.list", "Hyperlink Destinations").query().sig("{}").reg(r, anchor_list);
    cmd("xref.insert", "Insert Cross-Reference").menu(&["Type", "Hyperlinks & Cross-References", "Insert Cross-Reference..."]).needs(Needs::Text).sig("{anchor, format?: page|pageNumber|paragraph|paragraphText|paragraphAndPage}").reg(r, xref_insert);
    cmd("xref.update", "Update Cross-References").menu(&["Type", "Hyperlinks & Cross-References", "Update Cross-Reference"]).sig("{}").reg(r, xref_update);
    cmd("xref.list", "Cross-References").query().sig("{}").reg(r, xref_list);
    cmd("data.load", "Select Data Source").menu(&["Window", "Utilities", "Data Merge"]).sig("{text? | path?, delimiter?: comma|tab|semicolon}").reg(r, data_load);
    cmd("data.fields", "Data Fields").query().sig("{}").reg(r, data_fields);
    cmd("data.insertField", "Insert Data Field").needs(Needs::Text).sig("{name}").reg(r, data_insert_field);
    cmd("data.merge", "Create Merged Document").sig("{records?: [first, last] 1-based}").help("Opens a new document with the pages repeated for each record, <<Field>> placeholders in text replaced, and frames labelled <<@Field>> filled with the picture at that path.").reg(r, data_merge);
}
