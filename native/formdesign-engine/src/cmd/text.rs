use formdesign_model::attrs::{Attrs, set_checked};
use formdesign_model::ids::{ItemId, StoryId};
use formdesign_model::item::{ItemKind, PathText};
use formdesign_model::story::{Para, Seg};
use formdesign_model::style::{StyleKind, style_key};
use serde_json::{Map, Value, json};

use crate::cmd::object::put_text;
use crate::registry::{Needs, Registry, cmd};
use crate::session::{Ctx, Selection, TextSel};
use crate::target::{self, need_story, story_byte, story_char, text_targets};
use crate::{EngineError, Result, args, walk, wire};

fn ok() -> Result<Value> {
    Ok(json!({}))
}

fn obj<'a>(p: &'a Value, k: &str) -> Option<&'a Map<String, Value>> {
    p.get(k).and_then(Value::as_object)
}

fn smart_quote(prev: Option<char>, q: char) -> char {
    let open = prev.is_none_or(|c| c.is_whitespace() || "([{\u{2014}\u{2013}-/".contains(c));
    match (q, open) {
        ('"', true) => '\u{201C}',
        ('"', false) => '\u{201D}',
        (_, true) => '\u{2018}',
        _ => '\u{2019}',
    }
}

pub fn insert_at_selection(c: &mut Ctx, text: &str, raw: bool) -> Result<usize> {
    if c.state()?.selection.text.is_none()
        && let Some(ct) = c.state()?.selection.cell_text
    {
        let text = if !raw && (text == "\"" || text == "'") { smart_quote(None, text.chars().next().unwrap_or('"')).to_string() } else { text.to_string() };
        let (caret, n) = crate::celltext::insert(c.doc_mut()?, &ct, &text)?;
        c.label("Typing");
        if let Ok(s) = c.state_mut() {
            s.selection = ct.caret(caret).selection();
        }
        c.session.sel_rev += 1;
        return Ok(n);
    }
    let st = c.state()?;
    let sel = st.selection.text.ok_or_else(|| EngineError::new("nothing is taking text: click into a text frame with the Type tool first"))?;
    let tracking = st.track_changes;
    let pending = st.pending.clone();
    let story = st.doc.story(sel.story).ok_or_else(|| EngineError::new(format!("no story {}", sel.story)))?;
    let range = sel.range();
    let prev = if range.start > 0 { story.text().chars().nth(range.start - 1) } else { None };
    let text: String = if !raw && (text == "\"" || text == "'") { smart_quote(prev, text.chars().next().unwrap_or('"')).to_string() } else { text.to_string() };
    let doc = c.doc_mut()?;
    let st = doc.story_mut(sel.story).ok_or_else(|| EngineError::new("no such story"))?;
    let mut at = range.start;
    if !range.is_empty() {
        if tracking {
            let mut m = Map::new();
            m.insert("change".into(), json!("deleted"));
            st.set_char_attrs(range.clone(), &m)?;
            at = range.end;
        } else {
            st.delete(range.clone());
        }
    }
    let before = st.len();
    put_text(st, at, &text);
    let n = st.len() - before;
    if n > 0 {
        let mut attrs = pending.clone();
        let cstyle = attrs.remove("$style");
        if tracking {
            attrs.insert("change".into(), json!("inserted"));
        }
        if !attrs.is_empty() {
            st.set_char_attrs(at..at + n, &attrs)?;
        }
        if let Some(cs) = cstyle {
            st.set_char_style(at..at + n, cs.as_str(), false);
        }
    }
    let caret = at + n;
    c.label("Typing");
    if let Ok(s) = c.state_mut() {
        s.selection = Selection::text(TextSel::caret(sel.story, caret));
    }
    c.session.sel_rev += 1;
    Ok(n)
}

pub fn text_delete(c: &mut Ctx, p: &Value) -> Result<Value> {
    if c.state()?.selection.text.is_none()
        && let Some(ct) = c.state()?.selection.cell_text
    {
        let caret = crate::celltext::delete(c.doc_mut()?, &ct, args::opt_bool(p, "forward").unwrap_or(false), args::opt_bool(p, "word").unwrap_or(false))?;
        c.label("Typing");
        if let Ok(s) = c.state_mut() {
            s.selection = ct.caret(caret).selection();
        }
        c.session.sel_rev += 1;
        return Ok(json!({"deleted": 1}));
    }
    let st = c.state()?;
    let sel = st.selection.text.ok_or_else(|| EngineError::new("needs a text cursor: click into text with the Type tool"))?;
    let tracking = st.track_changes;
    let story = st.doc.story(sel.story).ok_or_else(|| EngineError::new("no such story"))?;
    let len = story.len();
    let forward = args::opt_bool(p, "forward").unwrap_or(false);
    let word = args::opt_bool(p, "word").unwrap_or(false);
    let mut range = sel.range();
    if range.is_empty() {
        let at = range.start;
        if word {
            let chars: Vec<char> = story.text().chars().collect();
            let mut k = at;
            if forward {
                while k < chars.len() && !chars[k].is_alphanumeric() {
                    k += 1;
                }
                while k < chars.len() && chars[k].is_alphanumeric() {
                    k += 1;
                }
                range = at..k;
            } else {
                while k > 0 && !chars[k - 1].is_alphanumeric() {
                    k -= 1;
                }
                while k > 0 && chars[k - 1].is_alphanumeric() {
                    k -= 1;
                }
                range = k..at;
            }
        } else if forward {
            range = at..(at + 1).min(len);
        } else {
            range = at.saturating_sub(1)..at;
        }
    }
    if range.is_empty() {
        return Ok(json!({"deleted": 0}));
    }
    let n = range.len();
    let doc = c.doc_mut()?;
    let s = doc.story_mut(sel.story).ok_or_else(|| EngineError::new("no such story"))?;
    let caret = if tracking {
        let mut m = Map::new();
        m.insert("change".into(), json!("deleted"));
        s.set_char_attrs(range.clone(), &m)?;
        if forward { range.end } else { range.start }
    } else {
        s.delete(range.clone());
        range.start
    };
    c.label("Typing");
    if let Ok(st) = c.state_mut() {
        st.selection = Selection::text(TextSel::caret(sel.story, caret));
    }
    c.session.sel_rev += 1;
    Ok(json!({"deleted": n}))
}

fn text_insert(c: &mut Ctx, p: &Value) -> Result<Value> {
    let text = p.get("text").and_then(Value::as_str).ok_or_else(|| EngineError::new("missing `text` (the characters to insert)"))?.to_string();
    let n = insert_at_selection(c, &text, args::opt_bool(p, "raw").unwrap_or(false))?;
    Ok(json!({"inserted": n}))
}

fn text_select(c: &mut Ctx, p: &Value) -> Result<Value> {
    let (sid, st) = need_story(c.doc()?, p)?;
    let a = story_char(st, args::u64_(p, "anchor", "a byte offset in the story")? as usize);
    let f = args::opt_u64(p, "focus").map(|b| story_char(st, b as usize)).unwrap_or(a);
    c.set_selection(Selection::text(TextSel { story: sid, anchor: a, focus: f }));
    Ok(json!({"story": sid}))
}

fn overset_byte(c: &mut Ctx, sid: StoryId) -> Result<Option<usize>> {
    let lay = c.layout()?;
    let doc = c.doc()?;
    let Some(st) = doc.story(sid) else { return Ok(None) };
    let Some(sl) = lay.stories.get(&sid) else { return Ok(None) };
    if !lay.overset(sid) {
        return Ok(None);
    }
    let ch = match sl.flow.overset_at {
        Some((para, byte)) => sl.flow.story_pos(para, byte),
        None => st.len(),
    };
    Ok(Some(story_byte(st, ch).max(1)))
}

fn story_get(c: &mut Ctx, p: &Value) -> Result<Value> {
    let (sid, st) = need_story(c.doc()?, p)?;
    let text = st.text();
    let out = json!({"id": sid, "text": text, "frames": st.frames, "paragraphs": st.paras.len(), "length": text.len()});
    let over = overset_byte(c, sid)?;
    let mut out = out;
    out["overset"] = json!(over);
    Ok(out)
}

fn story_set_text(c: &mut Ctx, p: &Value) -> Result<Value> {
    let (sid, _) = need_story(c.doc()?, p)?;
    let text = p.get("text").and_then(Value::as_str).ok_or_else(|| EngineError::new("missing `text` (the new story text, paragraphs separated by \\n)"))?.to_string();
    let doc = c.doc_mut()?;
    let st = doc.story_mut(sid).ok_or_else(|| EngineError::new("no such story"))?;
    let first = st.paras.first().cloned().unwrap_or_else(|| Para::empty(formdesign_model::style::BASIC_PARA));
    let (cstyle, cover) = first.format_at(0);
    let mut p0 = Para::empty(&first.style);
    p0.over = first.over.clone();
    p0.segs = vec![Seg::Text { text: String::new(), style: cstyle, over: cover }];
    st.paras = vec![p0];
    put_text(st, 0, &text);
    c.label("Replace Text");
    let mut sel = c.selection();
    if sel.text.is_some_and(|t| t.story == sid) {
        sel.text = Some(TextSel::caret(sid, 0));
        c.set_selection(sel);
    }
    ok()
}

fn story_replace(c: &mut Ctx, p: &Value) -> Result<Value> {
    let (sid, st) = need_story(c.doc()?, p)?;
    let a = story_char(st, args::u64_(p, "start", "a byte offset")? as usize);
    let b = story_char(st, args::opt_u64(p, "end").unwrap_or(args::u64_(p, "start", "a byte offset")?) as usize).max(a);
    let text = p.get("text").and_then(Value::as_str).ok_or_else(|| EngineError::new("missing `text` (the replacement)"))?.to_string();
    let doc = c.doc_mut()?;
    let st = doc.story_mut(sid).ok_or_else(|| EngineError::new("no such story"))?;
    st.delete(a..b);
    let before = st.len();
    put_text(st, a, &text);
    let n = st.len() - before;
    c.label("Typing");
    let mut sel = c.selection();
    if sel.text.is_some_and(|t| t.story == sid) {
        sel.text = Some(TextSel::caret(sid, a + n));
        c.set_selection(sel);
    }
    ok()
}

fn story_paras(c: &mut Ctx, p: &Value) -> Result<Value> {
    let (sid, st) = need_story(c.doc()?, p)?;
    let text = st.text();
    let mut out = Vec::new();
    let mut ch = 0;
    for para in &st.paras {
        let len = para.len();
        out.push(json!({"start": target::char_to_byte(&text, ch), "end": target::char_to_byte(&text, ch + len), "style": wire::style_name(StyleKind::Paragraph, &para.style), "overrides": para.over.len()}));
        ch += len + 1;
    }
    Ok(json!({"story": sid, "paras": out}))
}

fn char_label(m: &Map<String, Value>) -> &'static str {
    match m.keys().next().map(String::as_str) {
        Some("font" | "fontStyle") => "Font",
        Some("size") => "Size",
        Some("leading") => "Leading",
        Some("tracking" | "kern" | "kerning") => "Tracking",
        Some("fill" | "fillTint" | "stroke") => "Apply Color",
        _ => "Character Formatting",
    }
}

fn type_char(c: &mut Ctx, p: &Value) -> Result<Value> {
    let attrs = obj(p, "attrs").ok_or_else(|| EngineError::new("missing `attrs` (character attributes, e.g. {size: 12})"))?;
    let m = wire::char_in(attrs)?;
    let mut probe = Attrs::new();
    set_checked(&mut probe, &m, Some(formdesign_model::attrs::Level::Char))?;
    let targets = text_targets(c)?;
    c.label(char_label(&m));
    if targets.len() == 1 && targets[0].1.is_empty() && c.selection().text.is_some() {
        let st = c.state_mut()?;
        for (k, v) in m {
            st.pending.insert(k, v);
        }
        return ok();
    }
    let doc = c.doc_mut()?;
    for (sid, range) in targets {
        if let Some(st) = doc.story_mut(sid) {
            st.set_char_attrs(range, &m)?;
        }
    }
    ok()
}

fn type_para(c: &mut Ctx, p: &Value) -> Result<Value> {
    let attrs = obj(p, "attrs").ok_or_else(|| EngineError::new("missing `attrs` (paragraph attributes, e.g. {align: \"center\"})"))?;
    let m = wire::para_in(attrs)?;
    let targets = text_targets(c)?;
    c.label(if m.contains_key("tabs") && m.len() == 1 { "Tabs" } else { "Paragraph Formatting" });
    let doc = c.doc_mut()?;
    for (sid, range) in targets {
        if let Some(st) = doc.story_mut(sid) {
            let paras = st.paras_in(range);
            st.set_para_attrs(paras, &m)?;
        }
    }
    ok()
}

const VOLATILE: [&str; 4] = ["change", "xmlTag", "conditions", "features"];

fn selection_attrs(c: &mut Ctx, _: &Value) -> Result<Value> {
    let targets = text_targets(c)?;
    let (sid, range) = targets[0].clone();
    let pending = c.state()?.pending.clone();
    let doc = c.doc()?;
    let st = doc.story(sid).ok_or_else(|| EngineError::new("no such story"))?;
    let (pi, off) = st.locate(range.start);
    let para = &st.paras[pi];
    let (cstyle, mut over) = if range.is_empty() { para.format_at(off) } else { para.format_at(off + 1) };
    let cstyle = match pending.get("$style").and_then(Value::as_str) {
        Some(s) => Some(style_key(s)).filter(|s| s != formdesign_model::style::NO_CHAR),
        None => cstyle,
    };
    for (k, v) in &pending {
        if !k.starts_with('$') {
            over.insert(k.clone(), v.clone());
        }
    }
    let stack = doc.styles.char_stack(&para.style, &para.over, cstyle.as_deref(), &over);
    let pstack = doc.styles.para_stack(&para.style, &para.over);
    Ok(json!({
        "story": sid,
        "paragraphStyle": wire::style_name(StyleKind::Paragraph, &para.style),
        "characterStyle": wire::style_name(StyleKind::Character, cstyle.as_deref().unwrap_or(formdesign_model::style::NO_CHAR)),
        "paraOverrides": para.over.len(),
        "charOverrides": over.keys().filter(|k| !VOLATILE.contains(&k.as_str())).count(),
        "chars": wire::char_out(&stack),
        "para": wire::para_out(&pstack),
    }))
}

fn path_options(c: &mut Ctx, p: &Value) -> Result<Value> {
    let id = target::one_id(p, "id").or_else(|| c.selection().items.first().copied()).ok_or_else(|| EngineError::new("missing `id` (a type-on-a-path frame)"))?;
    let has_path = matches!(c.doc()?.item(id).and_then(|i| i.text()), Some(t) if t.on_path.is_some());
    if args::opt_bool(p, "delete") == Some(true) {
        if !has_path {
            return Err(EngineError::new(format!("object {id} has no type on a path")));
        }
        c.label("Delete Type from Path");
        let doc = c.doc_mut()?;
        let sid = doc.item(id).and_then(|i| i.story());
        if let Some(it) = doc.item_mut(id) {
            it.kind = ItemKind::Plain;
            it.style = None;
        }
        if let Some(s) = sid {
            let others = doc.story(s).map(|st| st.frames.iter().any(|f| *f != id)).unwrap_or(false);
            if others {
                if let Some(st) = doc.story_mut(s) {
                    st.frames.retain(|f| *f != id);
                }
            } else {
                doc.stories.remove(&s);
            }
        }
        return ok();
    }
    let align = args::opt_str(p, "align").map(str::to_string);
    let flip = args::opt_bool(p, "flip");
    let start = args::opt_f64(p, "start");
    let doc = c.doc_mut()?;
    let it = doc.item_mut(id).ok_or_else(|| EngineError::new(format!("no object with id {id}")))?;
    let shape_ok = !matches!(it.kind, ItemKind::Group { .. } | ItemKind::States { .. } | ItemKind::Graphic(_));
    if !shape_ok {
        return Err(EngineError::new("type on a path needs a path, line or frame outline"));
    }
    if !matches!(it.kind, ItemKind::Text(_)) {
        return Err(EngineError::new(format!("object {id} has no type on a path: use the Type on a Path tool first")));
    }
    let t = it.text_mut().ok_or_else(|| EngineError::new("not a text path"))?;
    let pt = t.on_path.get_or_insert_with(|| PathText { start: 0.0, flip: false, align: "baseline".into() });
    if let Some(a) = align {
        pt.align = a;
    }
    if let Some(f) = flip {
        pt.flip = f;
    }
    if let Some(s) = start {
        pt.start = s.max(0.0);
    }
    c.label("Type on a Path Options");
    ok()
}

fn kind_of(id: &str) -> StyleKind {
    match id.split('.').nth(1) {
        Some("character") => StyleKind::Character,
        Some("object") => StyleKind::Object,
        Some("table") => StyleKind::Table,
        Some("cell") => StyleKind::Cell,
        _ => StyleKind::Paragraph,
    }
}

fn style_attrs(kind: StyleKind, p: &Value) -> Result<Map<String, Value>> {
    let mut m = Map::new();
    if kind == StyleKind::Paragraph
        && let Some(pa) = obj(p, "para")
    {
        m.extend(wire::para_in(pa)?);
    }
    if let Some(ch) = obj(p, "chars") {
        m.extend(wire::char_in(ch)?);
    }
    Ok(m)
}

fn selection_overrides(c: &Ctx, kind: StyleKind) -> Result<(Option<String>, Map<String, Value>)> {
    let targets = text_targets(c)?;
    let (sid, range) = targets[0].clone();
    let doc = c.doc()?;
    let st = doc.story(sid).ok_or_else(|| EngineError::new("no such story"))?;
    let (pi, off) = st.locate(range.start);
    let para = &st.paras[pi];
    let (cstyle, over) = if range.is_empty() { para.format_at(off) } else { para.format_at(off + 1) };
    let mut m = Map::new();
    let keep = |k: &str| !VOLATILE.contains(&k);
    if kind == StyleKind::Paragraph {
        for (k, v) in &para.over {
            m.insert(k.clone(), v.clone());
        }
        if let Some(cs) = &cstyle {
            for a in formdesign_model::style::char_stack_names(cs).iter().flat_map(|n| doc.styles.chain(StyleKind::Character, n)) {
                for (k, v) in a {
                    m.insert(k.clone(), v.clone());
                }
            }
        }
    }
    for (k, v) in &over {
        if keep(k) {
            m.insert(k.clone(), v.clone());
        }
    }
    let based = if kind == StyleKind::Paragraph { Some(para.style.clone()) } else { cstyle };
    Ok((based, m))
}

fn style_create(c: &mut Ctx, p: &Value, id: &str) -> Result<Value> {
    let kind = kind_of(id);
    let name = args::str_(p, "name", "the new style's name")?.to_string();
    let mut based = p.get("basedOn").and_then(Value::as_str).map(style_key);
    let mut attrs = style_attrs(kind, p)?;
    if args::opt_bool(p, "fromSelection") == Some(true) && matches!(kind, StyleKind::Paragraph | StyleKind::Character) {
        let (b, m) = selection_overrides(c, kind)?;
        if based.is_none() {
            based = b.filter(|s| s != "No Paragraph Style");
        }
        for (k, v) in m {
            attrs.entry(k).or_insert(v);
        }
    }
    let next = p.get("nextStyle").and_then(Value::as_str).map(style_key);
    let doc = c.doc_mut()?;
    if let Some(b) = &based
        && !doc.styles.exists(kind, b)
    {
        return Err(EngineError::new(format!("no {} style named {b} to base it on", formdesign_model::style::kind_word(kind))));
    }
    let st = doc.styles.create(kind, &name, based.as_deref())?;
    set_checked(&mut st.attrs, &attrs, None)?;
    st.next = next;
    Ok(json!({"name": style_key(&name)}))
}

fn para_style_create(c: &mut Ctx, p: &Value) -> Result<Value> {
    style_create(c, p, "style.paragraph.create")
}

fn char_style_create(c: &mut Ctx, p: &Value) -> Result<Value> {
    style_create(c, p, "style.character.create")
}

fn rename_refs(doc: &mut formdesign_model::doc::Doc, kind: StyleKind, from: &str, to: &str) {
    let (f, t) = (from.to_string(), to.to_string());
    match kind {
        StyleKind::Paragraph => {
            let ff = f.clone();
            walk::paras_where(doc, &move |p| p.style == ff, &mut |p| p.style = t.clone());
        }
        StyleKind::Character => {
            let ff = f.clone();
            walk::paras_where(doc, &move |p| p.segs.iter().any(|s| s.style() == Some(ff.as_str())), &mut |p| {
                for s in p.segs.iter_mut() {
                    let (st, _) = s.format_mut();
                    if st.as_deref() == Some(f.as_str()) {
                        *st = Some(t.clone());
                    }
                }
            });
        }
        StyleKind::Object => {
            let ids: Vec<ItemId> = doc.items.values().filter(|i| i.style.as_deref() == Some(from)).map(|i| i.id).collect();
            for id in ids {
                if let Some(it) = doc.item_mut(id) {
                    it.style = Some(to.to_string());
                }
            }
        }
        _ => {}
    }
}

fn style_edit(c: &mut Ctx, p: &Value, id: &str) -> Result<Value> {
    let kind = kind_of(id);
    let name = style_key(args::str_(p, "name", "the style to change")?);
    let attrs = style_attrs(kind, p)?;
    let doc = c.doc_mut()?;
    if !doc.styles.exists(kind, &name) {
        return Err(EngineError::new(format!("no {} style named {name}", formdesign_model::style::kind_word(kind))));
    }
    {
        let st = doc.styles.find_mut(kind, &name).ok_or_else(|| EngineError::new("no such style"))?;
        set_checked(&mut st.attrs, &attrs, None)?;
    }
    if let Some(b) = p.get("basedOn") {
        let based = b.as_str().map(style_key).filter(|s| !s.is_empty() && s != "No Paragraph Style" && s != "None");
        if let Some(bn) = &based {
            if !doc.styles.exists(kind, bn) {
                return Err(EngineError::new(format!("no {} style named {bn} to base it on", formdesign_model::style::kind_word(kind))));
            }
            if doc.styles.chain(kind, bn).len() > 30 || *bn == name {
                return Err(EngineError::new("a style cannot be based on itself"));
            }
        }
        if let Some(st) = doc.styles.find_mut(kind, &name) {
            st.based_on = based;
        }
    }
    if let Some(n) = p.get("nextStyle")
        && let Some(st) = doc.styles.find_mut(kind, &name)
    {
        st.next = n.as_str().map(style_key);
    }
    if let Some(to) = args::opt_str(p, "rename").map(style_key).filter(|t| *t != name) {
        doc.styles.rename(kind, &name, &to)?;
        rename_refs(doc, kind, &name, &to);
    }
    c.label(match kind {
        StyleKind::Paragraph => "Paragraph Style Options",
        StyleKind::Character => "Character Style Options",
        _ => "Style Options",
    });
    ok()
}

fn para_style_edit(c: &mut Ctx, p: &Value) -> Result<Value> {
    style_edit(c, p, "style.paragraph.edit")
}

fn char_style_edit(c: &mut Ctx, p: &Value) -> Result<Value> {
    style_edit(c, p, "style.character.edit")
}

fn para_style_apply(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = style_key(args::str_(p, "name", "a paragraph style name")?);
    if !c.doc()?.styles.exists(StyleKind::Paragraph, &name) {
        return Err(EngineError::new(format!("no paragraph style named {name}")));
    }
    let clear = args::opt_bool(p, "clearOverrides").unwrap_or(false);
    let targets = text_targets(c)?;
    let doc = c.doc_mut()?;
    for (sid, range) in targets {
        if let Some(st) = doc.story_mut(sid) {
            let paras = st.paras_in(range);
            st.set_para_style(paras, &name, clear);
        }
    }
    ok()
}

fn char_style_apply(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = style_key(args::str_(p, "name", "a character style name, or [None]")?);
    if name != formdesign_model::style::NO_CHAR && !c.doc()?.styles.exists(StyleKind::Character, &name) {
        return Err(EngineError::new(format!("no character style named {name}")));
    }
    let targets = text_targets(c)?;
    if targets.len() == 1 && targets[0].1.is_empty() && c.selection().text.is_some() {
        c.state_mut()?.pending.insert("$style".into(), json!(name));
        return ok();
    }
    let doc = c.doc_mut()?;
    for (sid, range) in targets {
        if let Some(st) = doc.story_mut(sid) {
            st.set_char_style(range, Some(&name), false);
        }
    }
    ok()
}

fn char_style_stack(c: &mut Ctx, p: &Value, remove: bool) -> Result<Value> {
    let name = style_key(args::str_(p, "name", "a character style name")?);
    if !c.doc()?.styles.exists(StyleKind::Character, &name) {
        return Err(EngineError::new(format!("no character style named {name}")));
    }
    let targets = text_targets(c)?;
    if targets.iter().all(|(_, r)| r.is_empty()) {
        return Err(EngineError::new("select some text to add or remove a character style on"));
    }
    let doc = c.doc_mut()?;
    for (sid, range) in targets {
        if let Some(st) = doc.story_mut(sid) {
            st.stack_char_style(range, &name, remove);
        }
    }
    ok()
}

fn char_style_add(c: &mut Ctx, p: &Value) -> Result<Value> {
    char_style_stack(c, p, false)
}

fn char_style_remove(c: &mut Ctx, p: &Value) -> Result<Value> {
    char_style_stack(c, p, true)
}

fn flatten(doc: &formdesign_model::doc::Doc, kind: StyleKind, name: &str) -> Attrs {
    let mut out = Attrs::new();
    for a in doc.styles.chain(kind, name) {
        for (k, v) in a {
            out.insert(k.clone(), v.clone());
        }
    }
    out
}

fn style_delete(c: &mut Ctx, p: &Value, id: &str) -> Result<Value> {
    let kind = kind_of(id);
    let name = style_key(args::str_(p, "name", "the style to delete")?);
    let doc = c.doc_mut()?;
    let look = flatten(doc, kind, &name);
    let repl = doc.styles.delete(kind, &name, args::opt_str(p, "replaceWith"))?;
    let base = flatten(doc, kind, &repl);
    let keep: Attrs = look.into_iter().filter(|(k, v)| base.get(k) != Some(v)).collect();
    match kind {
        StyleKind::Paragraph => {
            let n = name.clone();
            walk::paras_where(doc, &move |p| p.style == n, &mut |p| {
                p.style = repl.clone();
                for (k, v) in &keep {
                    p.over.entry(k.clone()).or_insert_with(|| v.clone());
                }
            });
        }
        StyleKind::Character => {
            let n = name.clone();
            let target = if repl == formdesign_model::style::NO_CHAR { None } else { Some(repl.clone()) };
            walk::paras_where(doc, &move |p| p.segs.iter().any(|s| s.style() == Some(n.as_str())), &mut |p| {
                for s in p.segs.iter_mut() {
                    let (st, over) = s.format_mut();
                    if st.as_deref() == Some(name.as_str()) {
                        *st = target.clone();
                        for (k, v) in &keep {
                            over.entry(k.clone()).or_insert_with(|| v.clone());
                        }
                    }
                }
                p.normalize();
            });
        }
        StyleKind::Object => rename_refs(doc, kind, &name, &repl),
        _ => {}
    }
    ok()
}

fn para_style_delete(c: &mut Ctx, p: &Value) -> Result<Value> {
    style_delete(c, p, "style.paragraph.delete")
}

fn char_style_delete(c: &mut Ctx, p: &Value) -> Result<Value> {
    style_delete(c, p, "style.character.delete")
}

fn object_style_attrs(it: &formdesign_model::item::Item) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("fill".into(), serde_json::to_value(&it.fill).unwrap_or(Value::Null));
    m.insert("stroke".into(), serde_json::to_value(&it.stroke).unwrap_or(Value::Null));
    m.insert("corner".into(), serde_json::to_value(&it.corner).unwrap_or(Value::Null));
    m.insert("opacity".into(), json!(it.opacity));
    if !it.effects.is_empty() {
        m.insert("effects".into(), serde_json::to_value(&it.effects).unwrap_or(Value::Null));
    }
    if let Some(w) = &it.wrap {
        m.insert("wrap".into(), serde_json::to_value(w).unwrap_or(Value::Null));
    }
    if let Some(t) = it.text() {
        m.insert("textFrame".into(), json!({"columns": t.columns, "gutter": t.gutter, "insets": t.insets, "vjust": t.vjust, "autosize": t.autosize, "ignoreWrap": t.ignore_wrap, "spanFootnotes": t.span_notes}));
    }
    m
}

pub fn apply_object_style(doc: &mut formdesign_model::doc::Doc, id: ItemId, attrs: &Attrs) {
    let first_para = attrs.get("paragraphStyle").and_then(Value::as_str).map(style_key);
    let Some(it) = doc.item_mut(id) else { return };
    let get = |k: &str| attrs.get(k).cloned();
    if let Some(v) = get("fill").and_then(|v| serde_json::from_value(v).ok()) {
        it.fill = v;
    }
    if let Some(v) = get("stroke").and_then(|v| serde_json::from_value(v).ok()) {
        it.stroke = v;
    }
    if let Some(v) = get("corner").and_then(|v| serde_json::from_value(v).ok()) {
        it.corner = v;
    }
    if let Some(v) = get("opacity").and_then(|v| v.as_f64()) {
        it.opacity = v;
    }
    if let Some(v) = get("effects").and_then(|v| serde_json::from_value(v).ok()) {
        it.effects = v;
    }
    if let Some(v) = get("wrap") {
        it.wrap = serde_json::from_value(v).ok();
    }
    if let (Some(tf), Some(t)) = (get("textFrame"), it.text_mut()) {
        if let Some(n) = tf.get("columns").and_then(Value::as_u64) {
            t.columns = n.max(1) as u32;
        }
        if let Some(g) = tf.get("gutter").and_then(Value::as_f64) {
            t.gutter = g;
        }
        if let Some(e) = tf.get("insets").and_then(|v| serde_json::from_value(v.clone()).ok()) {
            t.insets = e;
        }
        if let Some(v) = tf.get("vjust").and_then(|v| serde_json::from_value(v.clone()).ok()) {
            t.vjust = v;
        }
        if let Some(v) = tf.get("autosize").and_then(|v| serde_json::from_value(v.clone()).ok()) {
            t.autosize = v;
        }
        if let Some(v) = tf.get("ignoreWrap").and_then(Value::as_bool) {
            t.ignore_wrap = v;
        }
        if let Some(v) = tf.get("spanFootnotes").and_then(Value::as_bool) {
            t.span_notes = v;
        }
    }
    let sid = it.story();
    if let (Some(ps), Some(sid)) = (first_para, sid)
        && let Some(st) = doc.story_mut(sid)
    {
        let n = st.paras.len();
        st.set_para_style(0..=n.saturating_sub(1), &ps, false);
    }
}

fn object_style_create(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = args::str_(p, "name", "the new object style's name")?.to_string();
    let mut attrs = Map::new();
    if args::opt_bool(p, "fromSelection").unwrap_or(true) {
        let ids = target::items(c, p)?;
        let doc = c.doc()?;
        let it = doc.item(ids[0]).ok_or_else(|| EngineError::new("no such object"))?;
        attrs = object_style_attrs(it);
        if let Some(sid) = it.story()
            && let Some(ps) = doc.story(sid).and_then(|s| s.paras.first()).map(|p| p.style.clone())
        {
            attrs.insert("paragraphStyle".into(), json!(ps));
        }
    }
    if let Some(extra) = obj(p, "attrs") {
        attrs.extend(extra.clone());
    }
    let doc = c.doc_mut()?;
    let based = p.get("basedOn").and_then(Value::as_str).map(style_key);
    let st = doc.styles.create(StyleKind::Object, &name, based.as_deref())?;
    st.attrs = attrs.into_iter().collect();
    Ok(json!({"name": style_key(&name)}))
}

fn object_style_apply(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = style_key(args::str_(p, "name", "an object style name")?);
    let ids = target::items(c, p)?;
    let doc = c.doc_mut()?;
    if !doc.styles.exists(StyleKind::Object, &name) {
        return Err(EngineError::new(format!("no object style named {name}")));
    }
    let attrs = flatten(doc, StyleKind::Object, &name);
    for id in ids {
        apply_object_style(doc, id, &attrs);
        if let Some(it) = doc.item_mut(id) {
            it.style = if name == "None" { None } else { Some(name.clone()) };
        }
    }
    ok()
}

fn object_style_delete(c: &mut Ctx, p: &Value) -> Result<Value> {
    style_delete(c, p, "style.object.delete")
}

fn object_style_edit(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = style_key(args::str_(p, "name", "the object style to change")?);
    let doc = c.doc_mut()?;
    let st = doc.styles.find_mut(StyleKind::Object, &name).ok_or_else(|| EngineError::new(format!("no object style named {name}")))?;
    if let Some(a) = obj(p, "attrs") {
        for (k, v) in a {
            if v.is_null() {
                st.attrs.remove(k);
            } else {
                st.attrs.insert(k.clone(), v.clone());
            }
        }
    }
    if let Some(to) = args::opt_str(p, "rename").map(style_key).filter(|t| *t != name) {
        doc.styles.rename(StyleKind::Object, &name, &to)?;
        rename_refs(doc, StyleKind::Object, &name, &to);
    }
    let users: Vec<ItemId> = doc.items.values().filter(|i| i.style.as_deref() == Some(name.as_str())).map(|i| i.id).collect();
    let attrs = flatten(doc, StyleKind::Object, &name);
    for id in users {
        apply_object_style(doc, id, &attrs);
    }
    c.label("Object Style Options");
    ok()
}

fn style_list(c: &mut Ctx, _: &Value) -> Result<Value> {
    let doc = c.doc()?;
    let names = |k: StyleKind| doc.styles.list(k).iter().map(|s| wire::style_name(k, &s.name)).collect::<Vec<_>>();
    Ok(json!({
        "paragraph": names(StyleKind::Paragraph),
        "character": names(StyleKind::Character),
        "object": names(StyleKind::Object),
        "table": names(StyleKind::Table),
        "cell": names(StyleKind::Cell),
    }))
}

fn style_get(c: &mut Ctx, p: &Value) -> Result<Value> {
    let kind = StyleKind::parse(args::opt_str(p, "kind").unwrap_or("paragraph")).ok_or_else(|| EngineError::new("`kind` expected paragraph, character, object, table or cell"))?;
    let name = style_key(args::str_(p, "name", "a style name")?);
    let doc = c.doc()?;
    let s = doc.styles.find(kind, &name).ok_or_else(|| EngineError::new(format!("no {} style named {name}", formdesign_model::style::kind_word(kind))))?;
    let own: Attrs = s.attrs.clone();
    let mut out = json!({"name": wire::style_name(kind, &s.name), "basedOn": s.based_on.as_deref().map(|b| wire::style_name(kind, b)), "nextStyle": s.next, "attrs": own});
    if matches!(kind, StyleKind::Paragraph | StyleKind::Character) {
        let empty = Attrs::new();
        let (ps, cs) = if kind == StyleKind::Paragraph { (s.name.as_str(), None) } else { (formdesign_model::style::BASIC_PARA, Some(s.name.as_str())) };
        let stack = doc.styles.char_stack(ps, &empty, cs, &empty);
        out["chars"] = Value::Object(wire::char_out(&stack));
        if kind == StyleKind::Paragraph {
            out["para"] = Value::Object(wire::para_out(&doc.styles.para_stack(ps, &empty)));
        }
    }
    Ok(out)
}

pub fn register(r: &mut Registry) {
    cmd("text.insert", "Typing").needs(Needs::Text).sig("{text, raw?}").reg(r, text_insert);
    cmd("text.delete", "Typing").needs(Needs::Text).sig("{forward?, word?}").reg(r, text_delete);
    cmd("text.select", "Select Text").query().sig("{story, anchor, focus?} (byte offsets)").reg(r, text_select);
    cmd("story.get", "Story").query().sig("{story | frame}").reg(r, story_get);
    cmd("story.setText", "Replace Text").sig("{story | frame, text}").reg(r, story_set_text);
    cmd("story.replaceRange", "Typing").sig("{story, start, end, text} (byte offsets)").reg(r, story_replace);
    cmd("story.paras", "Paragraphs").query().sig("{story}").reg(r, story_paras);
    cmd("type.char", "Character Formatting").needs(Needs::Text).sig("{attrs: {size?, fontFamily?, fontStyle?, leading?, tracking?, fill?, ...}}").reg(r, type_char);
    cmd("type.para", "Paragraph Formatting").needs(Needs::Text).sig("{attrs: {align?, leftIndent?, spaceBefore?, tabs?, ruleAbove?, ruleBelow?, shading?, border?: {on, weight, top, bottom, left, right, color, tint, type: solid|dashed, corner: none|rounded, cornerSize, offsetTop, offsetBottom, offsetLeft, offsetRight, width: column|text, merge}, nested?, nestedLines?: [{style, lines}], ...}}").reg(r, type_para);
    cmd("type.selectionAttrs", "Selection Attributes").needs(Needs::Text).query().sig("{}").reg(r, selection_attrs);
    cmd("type.pathOptions", "Type on a Path Options").menu(&["Type", "Type on a Path", "Options..."]).needs(Needs::Items).sig("{id, align?, flip?, start?} or {id, delete: true}").reg(r, path_options);
    cmd("style.paragraph.create", "New Paragraph Style").sig("{name, fromSelection? | para? (same keys as type.para attrs incl. border, nestedLines), chars?, basedOn?, nextStyle?}").reg(r, para_style_create);
    cmd("style.paragraph.edit", "Paragraph Style Options").sig("{name, rename?, basedOn?, nextStyle?, para?: same keys as type.para attrs incl. border, nestedLines, chars?}").reg(r, para_style_edit);
    cmd("style.paragraph.apply", "Apply Paragraph Style").needs(Needs::Text).sig("{name, clearOverrides?}").reg(r, para_style_apply);
    cmd("style.paragraph.delete", "Delete Paragraph Style").sig("{name, replaceWith?}").reg(r, para_style_delete);
    cmd("style.character.create", "New Character Style").sig("{name, chars?, basedOn?, fromSelection?}").reg(r, char_style_create);
    cmd("style.character.edit", "Character Style Options").sig("{name, rename?, basedOn?, chars?}").reg(r, char_style_edit);
    cmd("style.character.apply", "Apply Character Style").needs(Needs::Text).sig("{name}").reg(r, char_style_apply);
    cmd("style.character.add", "Add Character Style").menu(&["Type", "Character Styles", "Add Character Style..."]).needs(Needs::Text).sig("{name}")
        .help("Adds a character style on top of the ones already on the selected text, so several styles combine, each setting only what it defines and the later one winning where two set the same thing. Adding a style already there moves it to the top.").reg(r, char_style_add);
    cmd("style.character.remove", "Remove Character Style").menu(&["Type", "Character Styles", "Remove Character Style..."]).needs(Needs::Text).sig("{name}")
        .help("Takes one character style off the selected text and keeps the others.").reg(r, char_style_remove);
    cmd("style.character.delete", "Delete Character Style").sig("{name, replaceWith?}").reg(r, char_style_delete);
    cmd("style.object.create", "New Object Style").needs(Needs::Items).sig("{name, fromSelection?, attrs?, basedOn?}").reg(r, object_style_create);
    cmd("style.object.apply", "Apply Object Style").needs(Needs::Items).sig("{name, ids?}").reg(r, object_style_apply);
    cmd("style.object.edit", "Object Style Options").sig("{name, rename?, attrs?}").reg(r, object_style_edit);
    cmd("style.object.delete", "Delete Object Style").sig("{name, replaceWith?}").reg(r, object_style_delete);
    cmd("style.list", "Styles").query().sig("{}").reg(r, style_list);
    cmd("style.get", "Style").query().sig("{kind?: paragraph|character|object|table|cell, name}").reg(r, style_get);
}
