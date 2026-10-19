use formdes_model::doc::{Hyperlink, LinkDest, LinkSource, Variable};
use formdes_model::geom::Rect;
use formdes_model::ids::{ItemId, StoryId};
use formdes_model::item::{Item, ItemKind, TextFrame};
use formdes_model::story::{Inline, Para};
use serde_json::{Value, json};

use crate::registry::{Needs, Registry, cmd};
use crate::session::{Ctx, Selection, TextSel};
use crate::target::items;
use crate::{EngineError, Result, args};

fn dest_of(p: &Value) -> Result<Option<LinkDest>> {
    if let Some(u) = args::opt_str(p, "url") {
        let u = u.trim();
        let u = if u.contains("://") || u.starts_with("mailto:") || u.starts_with('#') { u.to_string() } else { format!("https://{u}") };
        return Ok(Some(LinkDest::Url(u)));
    }
    if let Some(e) = args::opt_str(p, "email") {
        return Ok(Some(LinkDest::Email(e.trim().trim_start_matches("mailto:").to_string())));
    }
    if let Some(n) = args::opt_u64(p, "page") {
        if n == 0 {
            return Err(EngineError::new("`page` is 1-based"));
        }
        return Ok(Some(LinkDest::Page(n as usize - 1)));
    }
    if let Some(a) = args::opt_str(p, "anchor") {
        return Ok(Some(LinkDest::Anchor(a.to_string())));
    }
    Ok(None)
}

fn dest_json(d: &LinkDest) -> Value {
    match d {
        LinkDest::Url(u) => json!({"kind": "url", "url": u}),
        LinkDest::Email(e) => json!({"kind": "email", "email": e}),
        LinkDest::Page(p) => json!({"kind": "page", "page": p + 1}),
        LinkDest::Anchor(a) => json!({"kind": "anchor", "anchor": a}),
    }
}

fn link_json(c: &Ctx, h: &Hyperlink) -> Value {
    let doc = c.doc().ok();
    let source = match &h.source {
        LinkSource::Text { story, start, end } => {
            let text: String = doc.and_then(|d| d.story(*story)).map(|s| s.text().chars().skip(*start).take(end.saturating_sub(*start)).collect()).unwrap_or_default();
            json!({"kind": "text", "story": story, "start": start, "end": end, "text": text})
        }
        LinkSource::Item { item } => json!({"kind": "object", "id": item}),
    };
    json!({"id": h.id, "name": h.name, "source": source, "destination": dest_json(&h.dest)})
}

fn hyperlink_create(c: &mut Ctx, p: &Value) -> Result<Value> {
    let dest = dest_of(p)?.ok_or_else(|| EngineError::new("missing a destination: `url`, `email`, `page` (1-based) or `anchor`"))?;
    let sel = c.selection();
    let source = match (sel.text, p.get("ids").or_else(|| p.get("id"))) {
        (Some(t), None) if !t.range().is_empty() => LinkSource::Text { story: t.story, start: t.range().start, end: t.range().end },
        _ => {
            let ids = items(c, p).map_err(|_| EngineError::new("needs a source: select text with the Type tool, or select an object"))?;
            LinkSource::Item { item: ids[0] }
        }
    };
    let name = match args::opt_str(p, "name") {
        Some(n) if !n.trim().is_empty() => n.trim().to_string(),
        _ => match &dest {
            LinkDest::Url(u) => u.clone(),
            LinkDest::Email(e) => e.clone(),
            LinkDest::Page(n) => format!("Page {}", n + 1),
            LinkDest::Anchor(a) => a.clone(),
        },
    };
    let doc = c.doc_mut()?;
    let id = doc.hyperlinks.iter().map(|h| h.id).max().unwrap_or(0) + 1;
    let h = Hyperlink { id, name, source, dest };
    doc.hyperlinks.push(h.clone());
    c.label("New Hyperlink");
    Ok(link_json(c, &h))
}

fn hyperlink_list(c: &mut Ctx, _: &Value) -> Result<Value> {
    let links: Vec<Value> = c.doc()?.hyperlinks.iter().map(|h| link_json(c, h)).collect();
    Ok(json!({"hyperlinks": links}))
}

fn link_index(c: &Ctx, p: &Value) -> Result<usize> {
    let id = args::u64_(p, "id", "a hyperlink id from hyperlink.list")?;
    c.doc()?.hyperlinks.iter().position(|h| h.id == id).ok_or_else(|| EngineError::new(format!("no hyperlink {id}")))
}

fn hyperlink_edit(c: &mut Ctx, p: &Value) -> Result<Value> {
    let i = link_index(c, p)?;
    let dest = dest_of(p)?;
    let name = args::opt_str(p, "name").map(str::to_string);
    let doc = c.doc_mut()?;
    let h = &mut doc.hyperlinks[i];
    if let Some(d) = dest {
        h.dest = d;
    }
    if let Some(n) = name {
        h.name = n;
    }
    let h = h.clone();
    c.label("Hyperlink Options");
    Ok(link_json(c, &h))
}

fn hyperlink_delete(c: &mut Ctx, p: &Value) -> Result<Value> {
    let i = link_index(c, p)?;
    c.doc_mut()?.hyperlinks.remove(i);
    c.label("Delete Hyperlink");
    Ok(json!({}))
}

fn hyperlink_go(c: &mut Ctx, p: &Value) -> Result<Value> {
    let i = link_index(c, p)?;
    let h = c.doc()?.hyperlinks[i].clone();
    match h.source {
        LinkSource::Text { story, start, end } => c.set_selection(Selection::text(TextSel { story, anchor: start, focus: end })),
        LinkSource::Item { item } => c.set_selection(Selection::items(vec![item])),
    }
    Ok(json!({"id": h.id}))
}

fn bookmarks(c: &Ctx) -> Vec<Value> {
    c.doc().ok().and_then(|d| d.prefs.get("bookmarks")).and_then(Value::as_array).cloned().unwrap_or_default()
}

fn current_page(c: &Ctx) -> Option<usize> {
    let doc = c.doc().ok()?;
    let sel = c.selection();
    if let Some(t) = sel.text {
        let f = doc.story(t.story)?.frames.first().copied()?;
        return doc.page_of(f);
    }
    sel.items.first().and_then(|i| crate::cmd::object::item_page(doc, *i))
}

fn bookmark_add(c: &mut Ctx, p: &Value) -> Result<Value> {
    let page = match args::opt_u64(p, "page") {
        Some(0) => return Err(EngineError::new("`page` is 1-based")),
        Some(n) => n as usize - 1,
        None => current_page(c).unwrap_or(0),
    };
    if page >= c.doc()?.page_count() {
        return Err(EngineError::new(format!("no page {} (the document has {})", page + 1, c.doc()?.page_count())));
    }
    let mut list = bookmarks(c);
    let selected: Option<String> = c.selection().text.and_then(|t| {
        let st = c.doc().ok()?.story(t.story)?;
        let s: String = st.text().chars().skip(t.range().start).take(t.range().len()).collect();
        let s = s.trim().to_string();
        (!s.is_empty()).then_some(s)
    });
    let name = args::opt_str(p, "name").map(str::to_string).or(selected).unwrap_or_else(|| format!("Bookmark {}", list.len() + 1));
    let level = args::opt_u64(p, "level").unwrap_or(1).clamp(1, 2);
    let y = args::opt_f64(p, "y").unwrap_or(0.0);
    list.push(json!({"name": name, "page": page, "level": level, "y": y}));
    list.sort_by_key(|b| b.get("page").and_then(Value::as_u64).unwrap_or(0));
    c.doc_mut()?.prefs.insert("bookmarks".into(), Value::Array(list.clone()));
    c.label("New Bookmark");
    Ok(json!({"bookmarks": list}))
}

fn bookmark_list(c: &mut Ctx, _: &Value) -> Result<Value> {
    let list: Vec<Value> = bookmarks(c).into_iter().enumerate().map(|(i, mut b)| {
        b["index"] = json!(i);
        if let Some(pg) = b.get("page").and_then(Value::as_u64) {
            b["pageName"] = json!(c.doc().map(|d| d.page_name(pg as usize)).unwrap_or_default());
        }
        b
    }).collect();
    Ok(json!({"bookmarks": list}))
}

fn bookmark_delete(c: &mut Ctx, p: &Value) -> Result<Value> {
    let i = args::u64_(p, "index", "a bookmark index from bookmark.list")? as usize;
    let mut list = bookmarks(c);
    if i >= list.len() {
        return Err(EngineError::new(format!("no bookmark {i}")));
    }
    list.remove(i);
    c.doc_mut()?.prefs.insert("bookmarks".into(), Value::Array(list));
    c.label("Delete Bookmark");
    Ok(json!({}))
}

fn today() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

const KINDS: &[&str] = &["custom", "runningHeader", "chapterNumber", "creationDate", "modificationDate", "outputDate", "fileName", "lastPageNumber", "liveCaption"];

fn var_json(c: &Ctx, v: &Variable) -> Value {
    let sample = c.doc().map(|d| d.variable_text(&v.name, Some(0))).unwrap_or_default();
    json!({"name": v.name, "kind": v.kind, "value": v.value, "text": sample})
}

fn variables_list(c: &mut Ctx, _: &Value) -> Result<Value> {
    let out: Vec<Value> = c.doc()?.variables.iter().map(|v| var_json(c, v)).collect();
    Ok(json!({"variables": out}))
}

fn variables_define(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = args::str_(p, "name", "the variable's name")?.trim().to_string();
    if name.is_empty() {
        return Err(EngineError::new("`name` is empty"));
    }
    let existing = c.doc()?.variables.iter().position(|v| v.name == name);
    let kind = match args::opt_str(p, "kind") {
        Some(k) if KINDS.contains(&k) => k.to_string(),
        Some(k) => return Err(EngineError::new(format!("`kind` expected {}, not {k}", KINDS.join(", ")))),
        None => existing.map(|i| c.doc().map(|d| d.variables[i].kind.clone()).unwrap_or_default()).unwrap_or_else(|| "custom".into()),
    };
    let mut value: formdes_model::attrs::Attrs = existing.and_then(|i| c.doc().ok().map(|d| d.variables[i].value.clone())).unwrap_or_default();
    for k in ["text", "number", "style", "date"] {
        if let Some(v) = p.get(k).filter(|v| !v.is_null()) {
            value.insert(k.into(), match v {
                Value::Number(n) => json!(n.to_string()),
                v => v.clone(),
            });
        }
    }
    if let Some(Value::Object(o)) = p.get("value") {
        for (k, v) in o {
            value.insert(k.clone(), v.clone());
        }
    }
    if kind.ends_with("Date") && !value.contains_key("date") {
        value.insert("date".into(), json!(today()));
    }
    let v = Variable { name: name.clone(), kind, value };
    let doc = c.doc_mut()?;
    match existing {
        Some(i) => doc.variables[i] = v.clone(),
        None => doc.variables.push(v.clone()),
    }
    c.label("Define Text Variables");
    Ok(var_json(c, &v))
}

fn variables_delete(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = args::str_(p, "name", "the variable's name")?.to_string();
    let doc = c.doc_mut()?;
    let n = doc.variables.len();
    doc.variables.retain(|v| v.name != name);
    if doc.variables.len() == n {
        return Err(EngineError::new(format!("no text variable named {name}")));
    }
    c.label("Delete Text Variable");
    Ok(json!({}))
}

fn caret(c: &Ctx) -> Result<TextSel> {
    c.selection().text.ok_or_else(|| EngineError::new("needs a text cursor: click into text with the Type tool"))
}

fn insert_inline(c: &mut Ctx, inline: Inline, label: &str) -> Result<usize> {
    let t = caret(c)?;
    let doc = c.doc_mut()?;
    let st = doc.story_mut(t.story).ok_or_else(|| EngineError::new("no such story"))?;
    let r = t.range();
    if !r.is_empty() {
        st.delete(r.clone());
    }
    st.insert_inline(r.start, inline);
    c.set_selection(Selection::text(TextSel::caret(t.story, r.start + 1)));
    c.label(label);
    Ok(r.start)
}

fn variables_insert(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = args::str_(p, "name", "a text variable's name")?.to_string();
    if !c.doc()?.variables.iter().any(|v| v.name == name) {
        return Err(EngineError::new(format!("no text variable named {name}: define it with variables.define")));
    }
    let at = insert_inline(c, Inline::Variable { name }, "Insert Variable")?;
    Ok(json!({"at": at}))
}

fn footnote_insert(c: &mut Ctx, p: &Value) -> Result<Value> {
    let text = args::opt_str(p, "text").unwrap_or("");
    let style = formdes_model::style::style_key(args::opt_str(p, "style").unwrap_or(formdes_model::style::BASIC_PARA));
    let paras: Vec<Para> = if text.is_empty() { vec![Para::empty(&style)] } else { text.split('\n').map(|l| Para::from_text(&style, l)).collect() };
    let at = insert_inline(c, Inline::Footnote { paras }, "Insert Footnote")?;
    Ok(json!({"at": at}))
}

fn footnote_options(c: &mut Ctx, p: &Value) -> Result<Value> {
    const STYLES: [&str; 8] = ["arabic", "roman", "ROMAN", "alpha", "ALPHA", "01", "001", "symbols"];
    if let Some(s) = args::opt_str(p, "numbering")
        && !STYLES.contains(&s)
    {
        return Err(EngineError::new(format!("numbering is one of {}", STYLES.join(", "))));
    }
    if let Some(s) = args::opt_str(p, "restart")
        && !["continuous", "page", "section"].contains(&s)
    {
        return Err(EngineError::new("restart is continuous, page or section"));
    }
    if let Some(s) = args::opt_str(p, "scope")
        && !["story", "document"].contains(&s)
    {
        return Err(EngineError::new("scope is story or document"));
    }
    if let Some(v) = p.get("columns")
        && !v.is_null()
        && v.as_u64().is_none_or(|n| n > 6)
    {
        return Err(EngineError::new("columns is a whole number from 0 (follow the text columns) to 6"));
    }
    let doc = c.doc_mut()?;
    let mut cur = doc.prefs.get("footnotes").and_then(Value::as_object).cloned().unwrap_or_default();
    if let Some(v) = p.get("scope") {
        match v.as_str().filter(|s| *s == "document") {
            Some(s) => { cur.insert("scope".into(), json!(s)); }
            None => { cur.remove("scope"); }
        }
    }
    if let Some(v) = p.get("restart") {
        match v.as_str().filter(|s| *s != "continuous") {
            Some(s) => { cur.insert("restart".into(), json!(s)); }
            None => { cur.remove("restart"); }
        }
    }
    for k in ["numbering", "prefix", "suffix", "separator"] {
        if let Some(v) = p.get(k) {
            if v.is_null() { cur.remove(k); } else if let Some(s) = v.as_str() { cur.insert(k.into(), json!(s)); }
        }
    }
    if let Some(v) = p.get("start") {
        match v.as_u64().filter(|n| *n >= 1) {
            Some(n) => { cur.insert("start".into(), json!(n)); }
            None => { cur.remove("start"); }
        }
    }
    if let Some(v) = p.get("runIn") {
        match v.as_bool() {
            Some(true) => { cur.insert("runIn".into(), json!(true)); }
            _ => { cur.remove("runIn"); }
        }
    }
    if let Some(v) = p.get("runInSeparator") {
        match v.as_str().filter(|s| !s.is_empty()) {
            Some(s) => { cur.insert("runInSeparator".into(), json!(s)); }
            None => { cur.remove("runInSeparator"); }
        }
    }
    if let Some(v) = p.get("columns") {
        match v.as_u64().filter(|n| *n >= 1) {
            Some(n) => { cur.insert("columns".into(), json!(n)); }
            None => { cur.remove("columns"); }
        }
    }
    doc.prefs.insert("footnotes".into(), Value::Object(cur.clone()));
    c.label("Document Footnote Options");
    Ok(Value::Object(cur))
}

fn footnote_list(c: &mut Ctx, p: &Value) -> Result<Value> {
    let doc = c.doc()?;
    let only: Option<StoryId> = crate::target::story_id(p, "story");
    let mut out = Vec::new();
    for (sid, st) in &doc.stories {
        if only.is_some_and(|o| o != *sid) {
            continue;
        }
        for (at, inl) in st.inlines() {
            if let Inline::Footnote { paras } = inl {
                out.push(json!({"story": sid, "at": at, "text": paras.iter().map(Para::plain).collect::<Vec<_>>().join("\n")}));
            }
        }
    }
    Ok(json!({"footnotes": out}))
}

fn footnote_set(c: &mut Ctx, p: &Value) -> Result<Value> {
    let sid = crate::target::story_id(p, "story").ok_or_else(|| EngineError::new("missing `story`"))?;
    let at = args::u64_(p, "at", "the footnote's position from footnote.list")? as usize;
    let text = args::str_(p, "text", "the footnote's text")?.to_string();
    let doc = c.doc_mut()?;
    let st = doc.story_mut(sid).ok_or_else(|| EngineError::new(format!("no story {sid}")))?;
    match st.inline_at_mut(at) {
        Some(Inline::Footnote { paras }) => {
            let style = paras.first().map(|q| q.style.clone()).unwrap_or_else(|| formdes_model::style::BASIC_PARA.to_string());
            *paras = text.split('\n').map(|l| Para::from_text(&style, l)).collect();
        }
        _ => return Err(EngineError::new(format!("no footnote at {at} in story {sid}"))),
    }
    c.label("Edit Footnote");
    Ok(json!({}))
}

fn alt_text(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let source = args::opt_str(p, "source").unwrap_or("custom").to_string();
    let text = args::opt_str(p, "text").map(str::to_string);
    if source == "custom" && text.is_none() {
        return Err(EngineError::new("missing `text` (the alternative text), or pass `source`: name, label or fileName"));
    }
    let doc = c.doc()?;
    let values: Vec<(ItemId, String)> = ids
        .iter()
        .filter_map(|id| {
            let it = doc.item(*id)?;
            let v = match source.as_str() {
                "name" => it.name.clone(),
                "label" => it.label.clone(),
                "fileName" => it.graphic().and_then(|g| g.asset).and_then(|a| doc.assets.get(&a)).map(|a| a.name.clone()).unwrap_or_default(),
                _ => text.clone().unwrap_or_default(),
            };
            Some((*id, v))
        })
        .collect();
    let doc = c.doc_mut()?;
    for (id, v) in &values {
        if let Some(it) = doc.item_mut(*id) {
            it.alt_text = v.clone();
        }
    }
    c.label("Object Export Options");
    Ok(json!({"items": values.iter().map(|(i, v)| json!({"id": i, "altText": v})).collect::<Vec<_>>()}))
}

fn caption(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let source = args::opt_str(p, "source").unwrap_or("altText").to_string();
    let position = args::opt_str(p, "position").unwrap_or("below").to_string();
    let offset = args::opt_f64(p, "offset").unwrap_or(6.0);
    let height = args::opt_f64(p, "height").unwrap_or(24.0).max(6.0);
    let style = formdes_model::style::style_key(args::opt_str(p, "style").unwrap_or(formdes_model::style::BASIC_PARA));
    let prefix = args::opt_str(p, "prefix").unwrap_or("").to_string();
    let doc = c.doc()?;
    let mut plan: Vec<(ItemId, Rect, String, formdes_model::doc::Home, formdes_model::ids::LayerId)> = Vec::new();
    for id in &ids {
        let Some(it) = doc.item(*id) else { continue };
        let Some(b) = doc.bounds(*id) else { continue };
        let Some(home) = doc.home(doc.top_of(*id)) else { continue };
        if !matches!(home, formdes_model::doc::Home::Spread(_) | formdes_model::doc::Home::Parent(_)) {
            continue;
        }
        let asset = it.graphic().and_then(|g| g.asset).and_then(|a| doc.assets.get(&a));
        let v = match source.as_str() {
            "name" => it.name.clone(),
            "label" => it.label.clone(),
            "fileName" => asset.map(|a| a.name.clone()).unwrap_or_default(),
            "text" => args::opt_str(p, "text").unwrap_or("").to_string(),
            _ => it.alt_text.clone(),
        };
        let v = if v.is_empty() { asset.map(|a| a.name.clone()).unwrap_or_else(|| it.display_name()) } else { v };
        let r = match position.as_str() {
            "above" => Rect::new(b.x0, b.y0 - offset - height, b.x1, b.y0 - offset),
            "left" => Rect::new(b.x0 - offset - 144.0, b.y0, b.x0 - offset, b.y1),
            "right" => Rect::new(b.x1 + offset, b.y0, b.x1 + offset + 144.0, b.y1),
            _ => Rect::new(b.x0, b.y1 + offset, b.x1, b.y1 + offset + height),
        };
        plan.push((*id, r, format!("{prefix}{v}"), home, it.layer));
    }
    if plan.is_empty() {
        return Err(EngineError::new("needs one or more objects on a spread to caption"));
    }
    let doc = c.doc_mut()?;
    let mut made = Vec::new();
    for (_, r, text, home, layer) in plan {
        let sid = doc.new_story(&style);
        let mut it = Item::new(ItemId(0), ItemKind::Text(TextFrame::new(sid)), formdes_model::item::Shape::rect(r), layer);
        it.style = Some(formdes_model::style::BASIC_TEXT.to_string());
        it.name = "Caption".into();
        let nid = doc.insert_item(it, home);
        if let Some(st) = doc.story_mut(sid) {
            st.paras = vec![Para::from_text(&style, &text)];
            st.frames = vec![nid];
        }
        made.push(nid);
    }
    c.set_selection(Selection::items(made.clone()));
    c.label("Generate Static Caption");
    Ok(json!({"ids": made}))
}

fn live_caption(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let field = args::opt_str(p, "field").unwrap_or("name").to_string();
    if !formdes_model::captionmeta::FIELDS.contains(&field.as_str()) {
        return Err(EngineError::new(format!("`field` expected {}, not {field}", formdes_model::captionmeta::FIELDS.join(", "))));
    }
    let position = args::opt_str(p, "position").unwrap_or("below").to_string();
    let offset = args::opt_f64(p, "offset").unwrap_or(6.0);
    let height = args::opt_f64(p, "height").unwrap_or(24.0).max(6.0);
    let style = formdes_model::style::style_key(args::opt_str(p, "style").unwrap_or(formdes_model::style::BASIC_PARA));
    let prefix = args::opt_str(p, "prefix").unwrap_or("").to_string();
    let doc = c.doc()?;
    let mut plan: Vec<(ItemId, Rect, formdes_model::doc::Home, formdes_model::ids::LayerId)> = Vec::new();
    for id in &ids {
        let Some(it) = doc.item(*id) else { continue };
        if it.graphic().and_then(|g| g.asset).is_none() {
            continue;
        }
        let Some(b) = doc.bounds(*id) else { continue };
        let Some(home) = doc.home(doc.top_of(*id)) else { continue };
        if !matches!(home, formdes_model::doc::Home::Spread(_) | formdes_model::doc::Home::Parent(_)) {
            continue;
        }
        let r = match position.as_str() {
            "above" => Rect::new(b.x0, b.y0 - offset - height, b.x1, b.y0 - offset),
            "left" => Rect::new(b.x0 - offset - 144.0, b.y0, b.x0 - offset, b.y1),
            "right" => Rect::new(b.x1 + offset, b.y0, b.x1 + offset + 144.0, b.y1),
            _ => Rect::new(b.x0, b.y1 + offset, b.x1, b.y1 + offset + height),
        };
        plan.push((*id, r, home, it.layer));
    }
    if plan.is_empty() {
        return Err(EngineError::new("needs one or more placed images on a spread to caption"));
    }
    let doc = c.doc_mut()?;
    let mut made = Vec::new();
    let mut names = Vec::new();
    for (img, r, home, layer) in plan {
        let mut n = 1;
        while doc.variables.iter().any(|v| v.name == format!("Live Caption {n}")) {
            n += 1;
        }
        let name = format!("Live Caption {n}");
        let mut value = formdes_model::attrs::Attrs::new();
        value.insert("field".into(), json!(field));
        value.insert("image".into(), json!(img.0));
        if !prefix.is_empty() {
            value.insert("prefix".into(), json!(prefix));
        }
        doc.variables.push(Variable { name: name.clone(), kind: "liveCaption".into(), value });
        let sid = doc.new_story(&style);
        let mut it = Item::new(ItemId(0), ItemKind::Text(TextFrame::new(sid)), formdes_model::item::Shape::rect(r), layer);
        it.style = Some(formdes_model::style::BASIC_TEXT.to_string());
        it.name = "Live Caption".into();
        let nid = doc.insert_item(it, home);
        if let Some(st) = doc.story_mut(sid) {
            st.frames = vec![nid];
            st.insert_inline(0, Inline::Variable { name: name.clone() });
        }
        made.push(nid);
        names.push(name);
    }
    c.set_selection(Selection::items(made.clone()));
    c.label("Generate Live Caption");
    Ok(json!({"ids": made, "variables": names}))
}

const LOREM: &[&str] = &[
    "Lorem ipsum dolor sit amet, consectetur adipiscing elit.",
    "Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.",
    "Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.",
    "Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur.",
    "Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.",
    "Curabitur pretium tincidunt lacus, nulla gravida orci a odio.",
    "Nullam varius, turpis et commodo pharetra, est eros bibendum elit, nec luctus magna felis sollicitudin mauris.",
    "Integer in mauris eu nibh euismod gravida.",
    "Duis ac tellus et risus vulputate vehicula.",
    "Donec lobortis risus a elit, etiam tempor.",
    "Ut ullamcorper, ligula eu tempor congue, eros est euismod turpis, id tincidunt sapien risus a quam.",
    "Maecenas fermentum consequat mi, donec fermentum.",
];

fn lorem(n: usize) -> String {
    let mut out = String::new();
    for k in 0..n {
        if k > 0 {
            out.push(if k % 5 == 0 { '\n' } else { ' ' });
        }
        out.push_str(LOREM[k % LOREM.len()]);
    }
    out
}

fn placeholder(c: &mut Ctx, p: &Value) -> Result<Value> {
    let sid = match c.selection().text {
        Some(t) if p.get("ids").is_none() => t.story,
        _ => {
            let ids = items(c, p)?;
            let doc = c.doc()?;
            crate::target::text_frames_in(doc, &ids).first().copied().ok_or_else(|| EngineError::new("needs a text frame selected, or a text cursor"))?
        }
    };
    let fonts = c.session.renderer.clone();
    let doc = c.doc()?;
    let st = doc.story(sid).ok_or_else(|| EngineError::new("no such story"))?;
    let style = st.paras.first().map(|q| q.style.clone()).unwrap_or_else(|| formdes_model::style::BASIC_PARA.to_string());
    let base = st.paras.clone();
    let keep = !st.plain().trim().is_empty();
    let fits = |n: usize| -> bool {
        let mut d = doc.clone();
        if let Some(s) = d.story_mut(sid) {
            let mut paras = if keep { base.clone() } else { Vec::new() };
            paras.extend(lorem(n).split('\n').map(|l| Para::from_text(&style, l)));
            s.paras = paras;
        }
        !formdes_text::layout::layout_doc(&d, &fonts.fonts).overset(sid)
    };
    let (mut lo, mut hi) = (0usize, 1usize);
    while hi < 4096 && fits(hi) {
        lo = hi;
        hi *= 2;
    }
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if fits(mid) { lo = mid } else { hi = mid }
    }
    let n = lo.max(1);
    let doc = c.doc_mut()?;
    if let Some(s) = doc.story_mut(sid) {
        let mut paras = if keep { base } else { Vec::new() };
        paras.extend(lorem(n).split('\n').map(|l| Para::from_text(&style, l)));
        s.paras = paras;
    }
    c.label("Fill with Placeholder Text");
    Ok(json!({"story": sid, "sentences": n}))
}

fn rotate_view(c: &mut Ctx, p: &Value) -> Result<Value> {
    let angle = (args::opt_f64(p, "angle").unwrap_or(90.0).round() as i64).rem_euclid(360);
    if angle % 90 != 0 {
        return Err(EngineError::new("`angle` expected 0, 90, 180 or 270"));
    }
    let sel = c.selection();
    let doc = c.doc()?;
    let si = match args::opt_u64(p, "spread") {
        Some(s) if (s as usize) < doc.spreads.len() => s as usize,
        Some(s) => return Err(EngineError::new(format!("no spread {s}"))),
        None => sel.items.first().and_then(|i| doc.spread_of(*i)).unwrap_or(0),
    };
    let r = crate::canvas::SpreadRef::Doc(si);
    let relative = args::opt_bool(p, "relative").unwrap_or(true);
    let st = c.state_mut()?;
    let cur = st.rotations.iter().find(|(s, _)| *s == r).map(|(_, a)| *a).unwrap_or(0);
    let next = if relative { (cur + angle as u32) % 360 } else { angle as u32 };
    st.rotations.retain(|(s, _)| *s != r);
    if next != 0 {
        st.rotations.push((r, next));
    }
    st.revision += 1;
    Ok(json!({"spread": si, "angle": next}))
}

pub fn register(r: &mut Registry) {
    cmd("hyperlink.create", "New Hyperlink").menu(&["Type", "Hyperlinks & Cross-References", "New Hyperlink..."]).needs(Needs::Selection).sig("{url? | email? | page?: 1-based | anchor?, name?, ids?}").help("Links the selected text, or the selected object, to a web address, an email address, a page or a named anchor; PDF and EPUB exports carry it.").reg(r, hyperlink_create);
    cmd("hyperlink.list", "Hyperlinks").query().sig("{}").reg(r, hyperlink_list);
    cmd("hyperlink.edit", "Hyperlink Options").sig("{id, name?, url? | email? | page? | anchor?}").reg(r, hyperlink_edit);
    cmd("hyperlink.delete", "Delete Hyperlink").sig("{id}").reg(r, hyperlink_delete);
    cmd("hyperlink.goToSource", "Go to Hyperlink Source").query().sig("{id}").reg(r, hyperlink_go);
    cmd("bookmark.add", "New Bookmark").menu(&["Window", "Interactive", "Bookmarks"]).sig("{name?, page?: 1-based, level?: 1|2, y?}").help("Adds a PDF bookmark for a page; the name defaults to the selected text.").reg(r, bookmark_add);
    cmd("bookmark.list", "Bookmarks").query().sig("{}").reg(r, bookmark_list);
    cmd("bookmark.delete", "Delete Bookmark").sig("{index}").reg(r, bookmark_delete);
    cmd("variables.list", "Text Variables").query().sig("{}").reg(r, variables_list);
    cmd("variables.define", "Define Text Variables").menu(&["Type", "Text Variables", "Define..."]).sig("{name, kind?: custom|runningHeader|chapterNumber|creationDate|modificationDate|outputDate|fileName|lastPageNumber|liveCaption, text?, number?, style?, date?, value?: {field, image, prefix}}").reg(r, variables_define);
    cmd("variables.delete", "Delete Text Variable").sig("{name}").reg(r, variables_delete);
    cmd("variables.insert", "Insert Variable").menu(&["Type", "Text Variables", "Insert Variable"]).needs(Needs::Text).sig("{name}").reg(r, variables_insert);
    cmd("footnote.options", "Document Footnote Options").menu(&["Type", "Document Footnote Options..."]).sig("{numbering?: arabic|roman|ROMAN|alpha|ALPHA|01|001|symbols, start?: number, restart?: continuous|page|section, scope?: story|document (document numbers the notes of every story in one sequence, in page order), columns?: number (0 follows the text columns; 1 to 6 sets footnotes in that many columns across each frame), runIn?: bool (the notes of each note area run on as one paragraph), runInSeparator?: text between run-in notes (default an em space), prefix?, suffix?, separator?: text after the number in the note (default a space)}")
        .help("Sets how footnotes are numbered across the document: the number style (including symbols * † ‡ §), the first number, whether numbering restarts on every page or section, and the prefix, suffix and separator around the number where the footnote itself begins.").reg(r, footnote_options);
    cmd("footnote.insert", "Insert Footnote").menu(&["Type", "Insert Footnote"]).needs(Needs::Text).sig("{text?, style?}").reg(r, footnote_insert);
    cmd("footnote.list", "Footnotes").query().sig("{story?}").reg(r, footnote_list);
    cmd("footnote.set", "Edit Footnote").sig("{story, at, text}").reg(r, footnote_set);
    cmd("object.altText", "Alt Text").menu(&["Object", "Object Export Options..."]).needs(Needs::Items).sig("{text? | source?: custom|name|label|fileName, ids?}").reg(r, alt_text);
    cmd("object.liveCaption", "Generate Live Caption").menu(&["Object", "Captions", "Generate Live Caption"]).needs(Needs::Items).sig("{field?: name|path|title|description|creator|copyright|dimensions|ppi, position?: below|above|left|right, offset?, height?, style?, prefix?, ids?}").reg(r, live_caption);
    cmd("object.caption", "Generate Static Caption").menu(&["Object", "Captions", "Generate Static Caption"]).needs(Needs::Items).sig("{source?: altText|name|label|fileName|text, text?, position?: below|above|left|right, offset?, height?, style?, prefix?, ids?}").reg(r, caption);
    cmd("type.fillWithPlaceholder", "Fill with Placeholder Text").menu(&["Type", "Fill with Placeholder Text"]).needs(Needs::Selection).sig("{ids?}").reg(r, placeholder);
    cmd("layout.rotateSpreadView", "Rotate Spread").menu(&["View", "Rotate Spread"]).query().sig("{angle?: 90|180|270 (default 90), relative?: true, spread?}").reg(r, rotate_view);
}
