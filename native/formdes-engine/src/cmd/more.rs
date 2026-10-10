use formdes_model::attrs::Attrs;
use formdes_model::doc::{Doc, Home};
use formdes_model::ids::{ItemId, StoryId};
use formdes_model::item::ItemKind;
use formdes_model::story::{Inline, Para, Seg, Table};
use serde_json::{Value, json};

use crate::registry::{Needs, Registry, cmd};
use crate::session::{Ctx, Selection, TextSel};
use crate::{EngineError, Result, args};

fn frame_width(doc: &Doc, sid: StoryId) -> f64 {
    doc.story(sid)
        .and_then(|st| st.frames.first())
        .and_then(|f| doc.item(*f))
        .map(|it| {
            let tf = it.text();
            let w = it.shape.bbox().width() - tf.map(|t| t.insets.inside + t.insets.outside).unwrap_or(0.0);
            let cols = tf.map(|t| t.columns.max(1) as f64).unwrap_or(1.0);
            let gutter = tf.map(|t| t.gutter).unwrap_or(0.0);
            ((w - gutter * (cols - 1.0)) / cols).max(36.0)
        })
        .unwrap_or(300.0)
}

fn text_to_table(c: &mut Ctx, p: &Value) -> Result<Value> {
    let t = c.selection().text.ok_or_else(|| EngineError::new("needs text selected with the Type tool: the paragraphs become rows"))?;
    let sep = match args::opt_str(p, "columns").unwrap_or("tab") {
        "comma" | "," => ',',
        "semicolon" | ";" => ';',
        _ => '\t',
    };
    let header = args::opt_u64(p, "headerRows").unwrap_or(0) as usize;
    let doc = c.doc()?;
    let st = doc.story(t.story).ok_or_else(|| EngineError::new("no such story"))?;
    let range = st.paras_in(t.range());
    let (i0, i1) = (*range.start(), *range.end());
    if st.paras[i0..=i1].iter().any(|p| p.segs.iter().any(|s| matches!(s, Seg::Inline { inline: Inline::Table { .. }, .. }))) {
        return Err(EngineError::new("the selection already holds a table"));
    }
    let rows: Vec<Vec<String>> = st.paras[i0..=i1].iter().map(|p| p.plain().split(sep).map(|x| x.trim().to_string()).collect()).collect();
    let cols = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let style = st.paras[i0].style.clone();
    let width = frame_width(doc, t.story);
    let mut table = Table::new(rows.len(), cols, width);
    table.header_rows = header.min(rows.len());
    for (r, row) in rows.iter().enumerate() {
        for (k, text) in row.iter().enumerate() {
            if let Some(cell) = table.cell_mut(r, k) {
                cell.paras = vec![Para::from_text(&style, text)];
            }
        }
    }
    let start = st.para_start(i0);
    let doc = c.doc_mut()?;
    let st = doc.story_mut(t.story).ok_or_else(|| EngineError::new("no such story"))?;
    st.paras.splice(i0..=i1, std::iter::once(Para { style, over: Attrs::new(), segs: vec![Seg::Inline { inline: Inline::Table { table: Box::new(table) }, style: None, over: Attrs::new() }] }));
    c.set_selection(Selection::text(TextSel::caret(t.story, start + 1)));
    c.label("Convert Text to Table");
    Ok(json!({"rows": rows.len(), "columns": cols, "table": start}))
}

fn table_at(c: &Ctx, p: &Value) -> Result<(StoryId, usize)> {
    let sel = c.selection();
    if let Some(cs) = sel.cells {
        return Ok((cs.story, cs.at));
    }
    let t = sel.text.ok_or_else(|| EngineError::new("needs a table: click into it or put the cursor beside it"))?;
    let st = c.doc()?.story(t.story).ok_or_else(|| EngineError::new("no such story"))?;
    let want = args::opt_u64(p, "table").map(|v| v as usize);
    st.inlines()
        .into_iter()
        .filter(|(_, i)| matches!(i, Inline::Table { .. }))
        .map(|(a, _)| a)
        .find(|a| want.map_or(*a + 1 == t.focus || *a == t.focus || st.locate(*a).0 == st.locate(t.focus).0, |w| w == *a))
        .map(|a| (t.story, a))
        .ok_or_else(|| EngineError::new("no table at the cursor"))
}

fn table_to_text(c: &mut Ctx, p: &Value) -> Result<Value> {
    let (sid, at) = table_at(c, p)?;
    let sep = match args::opt_str(p, "columns").unwrap_or("tab") {
        "comma" | "," => ",",
        "semicolon" | ";" => ";",
        _ => "\t",
    };
    let doc = c.doc()?;
    let st = doc.story(sid).ok_or_else(|| EngineError::new("no such story"))?;
    let (pi, _) = st.locate(at);
    let Some(Seg::Inline { inline: Inline::Table { table }, .. }) = st.paras[pi].segs.iter().find(|s| matches!(s, Seg::Inline { inline: Inline::Table { .. }, .. })) else { return Err(EngineError::new("no table there")) };
    let style = table.cells.first().and_then(|c| c.paras.first()).map(|p| p.style.clone()).unwrap_or_else(|| st.paras[pi].style.clone());
    let mut lines = Vec::new();
    for r in 0..table.rows() {
        let row: Vec<String> = (0..table.cols()).map(|k| table.cells.iter().find(|x| x.row == r && x.col == k).map(|x| x.paras.iter().map(Para::plain).collect::<Vec<_>>().join(" ")).unwrap_or_default()).collect();
        lines.push(Para::from_text(&style, &row.join(sep)));
    }
    let n = lines.len();
    let doc = c.doc_mut()?;
    let st = doc.story_mut(sid).ok_or_else(|| EngineError::new("no such story"))?;
    let only_table = st.paras[pi].segs.iter().all(|s| matches!(s, Seg::Inline { inline: Inline::Table { .. }, .. }) || s.is_empty());
    if only_table {
        st.paras.splice(pi..=pi, lines);
    } else {
        st.paras[pi].segs.retain(|s| !matches!(s, Seg::Inline { inline: Inline::Table { .. }, .. }));
        st.paras.splice(pi + 1..pi + 1, lines);
    }
    c.set_selection(Selection::text(TextSel::caret(sid, at)));
    c.label("Convert Table to Text");
    Ok(json!({"paragraphs": n}))
}

fn home_list(doc: &Doc, id: ItemId) -> Vec<ItemId> {
    match doc.home(id) {
        Some(Home::Spread(si)) => doc.spreads[si].items.clone(),
        Some(Home::Parent(pi)) => doc.parents[pi].items.clone(),
        Some(Home::Group(g)) => doc.item(g).map(|i| i.children()).unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn select_container(c: &mut Ctx, _: &Value) -> Result<Value> {
    let sel = c.selection();
    let doc = c.doc()?;
    if sel.content && !sel.items.is_empty() {
        let ids = sel.items.clone();
        c.set_selection(Selection::items(ids.clone()));
        return Ok(json!({"items": ids}));
    }
    let id = *sel.items.first().ok_or_else(|| EngineError::new("needs a selected object"))?;
    let g = doc.item(id).and_then(|i| i.group).ok_or_else(|| EngineError::new("the object is not inside a group"))?;
    c.set_selection(Selection::items(vec![g]));
    Ok(json!({"items": [g]}))
}

fn select_content(c: &mut Ctx, _: &Value) -> Result<Value> {
    let sel = c.selection();
    let doc = c.doc()?;
    let id = *sel.items.first().ok_or_else(|| EngineError::new("needs a selected object"))?;
    let it = doc.item(id).ok_or_else(|| EngineError::new("no such object"))?;
    match &it.kind {
        ItemKind::Group { children } if !children.is_empty() => {
            let f = children[0];
            c.set_selection(Selection::items(vec![f]));
            Ok(json!({"items": [f]}))
        }
        ItemKind::States { states, active } => {
            let f = states.get(*active).and_then(|s| s.items.first().copied()).ok_or_else(|| EngineError::new("the state is empty"))?;
            c.set_selection(Selection::items(vec![f]));
            Ok(json!({"items": [f]}))
        }
        ItemKind::Graphic(g) if g.asset.is_some() => {
            c.set_selection(Selection { items: vec![id], content: true, ..Selection::default() });
            Ok(json!({"items": [id], "content": true}))
        }
        ItemKind::Text(tf) => {
            let sid = tf.story;
            c.set_selection(Selection::text(TextSel::caret(sid, 0)));
            Ok(json!({"story": sid}))
        }
        _ => Err(EngineError::new("the object has no content to select")),
    }
}

fn select_step(c: &mut Ctx, p: &Value) -> Result<Value> {
    let back = args::opt_bool(p, "previous").unwrap_or(false);
    let sel = c.selection();
    let doc = c.doc()?;
    let id = *sel.items.first().ok_or_else(|| EngineError::new("needs a selected object"))?;
    let list = home_list(doc, id);
    let i = list.iter().position(|x| *x == id).ok_or_else(|| EngineError::new("the object has no siblings"))?;
    let n = list.len();
    let j = if back { (i + n - 1) % n } else { (i + 1) % n };
    let next = list[j];
    c.set_selection(Selection::items(vec![next]));
    Ok(json!({"items": [next]}))
}

fn endnotes(doc: &Doc) -> Vec<Value> {
    doc.prefs.get("endnotes").and_then(|v| v.get("notes")).and_then(Value::as_array).cloned().unwrap_or_default()
}

type RefKey = (usize, i64, i64, usize);
type NoteRef = (StoryId, usize, String);

fn endnote_refs(doc: &Doc) -> Vec<NoteRef> {
    let skip = doc.prefs.get("endnotes").and_then(|v| v.get("story")).and_then(Value::as_u64).map(StoryId);
    let mut keyed: Vec<(RefKey, NoteRef)> = Vec::new();
    for (sid, st) in &doc.stories {
        if Some(*sid) == skip {
            continue;
        }
        let f = st.frames.first().copied();
        let page = f.and_then(|f| doc.page_of(f)).unwrap_or(usize::MAX);
        let b = f.and_then(|f| doc.bounds(f)).unwrap_or(formdes_model::geom::Rect::ZERO);
        for (at, inl) in st.inlines() {
            if let Inline::Xref { anchor, format, .. } = inl
                && format == "endnote"
            {
                keyed.push(((page, b.y0 as i64, b.x0 as i64, at), (*sid, at, anchor.clone())));
            }
        }
    }
    keyed.sort_by_key(|k| k.0);
    keyed.into_iter().map(|(_, v)| v).collect()
}

fn renumber(c: &mut Ctx) -> Result<usize> {
    let doc = c.doc()?;
    let refs = endnote_refs(doc);
    let notes = endnotes(doc);
    let n = refs.len();
    let story = doc.prefs.get("endnotes").and_then(|v| v.get("story")).and_then(Value::as_u64).map(StoryId).filter(|s| doc.story(*s).is_some_and(|st| !st.frames.is_empty()));
    let title = doc.prefs.get("endnotes").and_then(|v| v.get("title")).and_then(Value::as_str).unwrap_or("Notes").to_string();
    let doc = c.doc_mut()?;
    let mut paras = Vec::new();
    if !title.is_empty() {
        paras.push(Para::from_text("Endnote Title", &title));
    }
    for (k, (sid, at, anchor)) in refs.iter().enumerate() {
        if let Some(Inline::Xref { text, .. }) = doc.story_mut(*sid).and_then(|s| s.inline_at_mut(*at)) {
            *text = (k + 1).to_string();
        }
        let body = notes.iter().find(|v| v.get("id").and_then(Value::as_str) == Some(anchor.as_str())).and_then(|v| v.get("text")).and_then(Value::as_str).unwrap_or("");
        paras.push(Para::from_text("Endnote", &format!("{}\t{}", k + 1, body)));
    }
    if let Some(sid) = story {
        for (name, attrs) in [("Endnote Title", json!({"size": 14.0, "spaceAfter": 6.0})), ("Endnote", json!({"leftIndent": 18.0, "firstIndent": -18.0, "size": 9.0}))] {
            if !doc.styles.paragraph.iter().any(|s| s.name == name) {
                let mut s = formdes_model::style::NamedStyle::new(name);
                s.based_on = Some(formdes_model::style::BASIC_PARA.to_string());
                if let Value::Object(o) = attrs {
                    s.attrs.extend(o);
                }
                doc.styles.paragraph.push(s);
            }
        }
        if let Some(st) = doc.story_mut(sid) {
            st.paras = if paras.is_empty() { vec![Para::empty(formdes_model::style::BASIC_PARA)] } else { paras };
        }
    }
    Ok(n)
}

fn endnote_insert(c: &mut Ctx, p: &Value) -> Result<Value> {
    let t = c.selection().text.ok_or_else(|| EngineError::new("needs a text cursor: click into text with the Type tool"))?;
    let text = args::opt_str(p, "text").unwrap_or("").to_string();
    let doc = c.doc()?;
    let mut notes = endnotes(doc);
    let id = format!("endnote:{}", notes.iter().filter_map(|v| v.get("id").and_then(Value::as_str)).filter_map(|s| s.strip_prefix("endnote:")?.parse::<u64>().ok()).max().unwrap_or(0) + 1);
    notes.push(json!({"id": id, "text": text}));
    let mut prefs = doc.prefs.get("endnotes").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
    prefs["notes"] = Value::Array(notes);
    let r = t.range();
    let doc = c.doc_mut()?;
    doc.prefs.insert("endnotes".into(), prefs);
    let st = doc.story_mut(t.story).ok_or_else(|| EngineError::new("no such story"))?;
    if !r.is_empty() {
        st.delete(r.clone());
    }
    st.insert_inline(r.start, Inline::Xref { anchor: id.clone(), format: "endnote".into(), text: "#".into() });
    let mut sup = serde_json::Map::new();
    sup.insert("position".into(), json!("superscript"));
    st.set_char_attrs(r.start..r.start + 1, &sup)?;
    c.set_selection(Selection::text(TextSel::caret(t.story, r.start + 1)));
    let n = renumber(c)?;
    c.label("Insert Endnote");
    Ok(json!({"id": id, "count": n}))
}

fn endnote_set(c: &mut Ctx, p: &Value) -> Result<Value> {
    let id = args::str_(p, "id", "an endnote id from endnote.list")?.to_string();
    let text = args::str_(p, "text", "the endnote's text")?.to_string();
    let doc = c.doc()?;
    let mut notes = endnotes(doc);
    let e = notes.iter_mut().find(|v| v.get("id").and_then(Value::as_str) == Some(id.as_str())).ok_or_else(|| EngineError::new(format!("no endnote {id}")))?;
    e["text"] = json!(text);
    let mut prefs = doc.prefs.get("endnotes").cloned().unwrap_or_else(|| json!({}));
    prefs["notes"] = Value::Array(notes);
    c.doc_mut()?.prefs.insert("endnotes".into(), prefs);
    renumber(c)?;
    c.label("Edit Endnote");
    Ok(json!({}))
}

fn endnote_list(c: &mut Ctx, _: &Value) -> Result<Value> {
    let doc = c.doc()?;
    let notes = endnotes(doc);
    let out: Vec<Value> = endnote_refs(doc)
        .into_iter()
        .enumerate()
        .map(|(k, (sid, at, id))| json!({"number": k + 1, "id": id, "story": sid, "at": at, "text": notes.iter().find(|v| v.get("id").and_then(Value::as_str) == Some(id.as_str())).and_then(|v| v.get("text")).cloned().unwrap_or(Value::Null)}))
        .collect();
    Ok(json!({"endnotes": out}))
}

fn endnote_frame(c: &mut Ctx, p: &Value) -> Result<Value> {
    let doc = c.doc()?;
    let sid = match crate::target::one_id(p, "frame") {
        Some(f) => doc.item(f).and_then(|i| i.story()).ok_or_else(|| EngineError::new(format!("object {f} is not a text frame")))?,
        None => {
            let n = doc.page_count();
            let page = match args::opt_u64(p, "page") {
                Some(0) => return Err(EngineError::new("`page` is 1-based")),
                Some(k) if k as usize > n => return Err(EngineError::new(format!("no page {k}"))),
                Some(k) => k as usize - 1,
                None => n - 1,
            };
            let (si, _) = doc.page_slot(page).ok_or_else(|| EngineError::new("no such page"))?;
            let r = crate::cmd::object::margin_rect(doc, page).ok_or_else(|| EngineError::new("no such page"))?;
            let layer = c.target_layer()?;
            let doc = c.doc_mut()?;
            let sid = doc.new_story(formdes_model::style::BASIC_PARA);
            let mut it = formdes_model::item::Item::new(ItemId(0), ItemKind::Text(formdes_model::item::TextFrame::new(sid)), formdes_model::item::Shape::rect(r), layer);
            it.style = Some(formdes_model::style::BASIC_TEXT.to_string());
            let id = doc.insert_item(it, Home::Spread(si));
            if let Some(st) = doc.story_mut(sid) {
                st.frames = vec![id];
            }
            sid
        }
    };
    let doc = c.doc_mut()?;
    let mut prefs = doc.prefs.get("endnotes").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
    prefs["story"] = json!(sid);
    if let Some(t) = args::opt_str(p, "title") {
        prefs["title"] = json!(t);
    }
    doc.prefs.insert("endnotes".into(), prefs);
    let n = renumber(c)?;
    c.label("Endnote Frame");
    Ok(json!({"story": sid, "endnotes": n}))
}

pub fn register(r: &mut Registry) {
    cmd("table.fromText", "Convert Text to Table").menu(&["Table", "Convert Text to Table..."]).needs(Needs::Text).sig("{columns?: tab|comma|semicolon, headerRows?}").reg(r, text_to_table);
    cmd("table.toText", "Convert Table to Text").menu(&["Table", "Convert Table to Text..."]).sig("{columns?: tab|comma|semicolon, table?}").reg(r, table_to_text);
    cmd("edit.selectContainer", "Select Container").menu(&["Object", "Select", "Container"]).needs(Needs::Items).query().sig("{}").reg(r, select_container);
    cmd("edit.selectContent", "Select Content").menu(&["Object", "Select", "Content"]).needs(Needs::Items).query().sig("{}").reg(r, select_content);
    cmd("edit.selectNext", "Select Next Object").menu(&["Object", "Select", "Next Object"]).needs(Needs::Items).query().sig("{previous?}").reg(r, select_step);
    cmd("endnote.insert", "Insert Endnote").menu(&["Type", "Insert Endnote"]).needs(Needs::Text).sig("{text?}").help("Puts a superscript endnote number at the cursor and keeps the note's text; the endnotes are numbered in page order and written into the endnote frame (endnote.frame).").reg(r, endnote_insert);
    cmd("endnote.set", "Edit Endnote").sig("{id, text}").reg(r, endnote_set);
    cmd("endnote.list", "Endnotes").query().sig("{}").reg(r, endnote_list);
    cmd("endnote.frame", "Endnote Frame").menu(&["Type", "Document Endnote Options..."]).sig("{frame?, page?: 1-based, title?}").reg(r, endnote_frame);
}
