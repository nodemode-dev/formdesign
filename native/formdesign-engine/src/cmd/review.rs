use formdesign_model::doc::{Condition, Doc};
use formdesign_model::ids::StoryId;
use formdesign_model::story::{Inline, Seg, Story};
use serde_json::{Map, Value, json};

use crate::registry::{Needs, Registry, cmd};
use crate::session::{Ctx, Selection, TextSel};
use crate::target::{char_to_byte, need_story, story_char, text_targets};
use crate::{EngineError, Result, args, walk, wire};

fn ok() -> Result<Value> {
    Ok(json!({}))
}

fn change_runs(st: &Story) -> Vec<(usize, usize, String)> {
    let mut out: Vec<(usize, usize, String)> = Vec::new();
    let mut pos = 0;
    for para in &st.paras {
        for seg in &para.segs {
            let len = seg.len();
            if let Some(k) = seg.over().get("change").and_then(Value::as_str).filter(|k| *k == "inserted" || *k == "deleted") {
                match out.last_mut() {
                    Some(last) if last.2 == k && last.1 + 1 >= pos => last.1 = pos + len,
                    _ => out.push((pos, pos + len, k.to_string())),
                }
            }
            pos += len;
        }
        pos += 1;
    }
    out
}

fn changes_list(c: &mut Ctx, _: &Value) -> Result<Value> {
    let doc = c.doc()?;
    let mut out = Vec::new();
    for st in doc.stories.values() {
        let text = st.text();
        let chars: Vec<char> = text.chars().collect();
        for (a, b, k) in change_runs(st) {
            let snippet: String = chars[a.min(chars.len())..b.min(chars.len())].iter().collect();
            out.push(json!({"story": st.id, "start": char_to_byte(&text, a), "end": char_to_byte(&text, b), "kind": k, "text": snippet}));
        }
    }
    Ok(Value::Array(out))
}

fn resolve(st: &mut Story, a: usize, b: usize, kind: &str, accept: bool) -> Result<()> {
    let keep_text = (kind == "inserted") == accept;
    if keep_text {
        let mut m = Map::new();
        m.insert("change".into(), Value::Null);
        st.set_char_attrs(a..b, &m)?;
    } else {
        st.delete(a..b);
    }
    Ok(())
}

fn change_one(c: &mut Ctx, p: &Value, accept: bool) -> Result<Value> {
    let (sid, st) = need_story(c.doc()?, p)?;
    let start = story_char(st, args::u64_(p, "start", "the change's start offset from changes.list")? as usize);
    let run = change_runs(st).into_iter().find(|r| r.0 == start).ok_or_else(|| EngineError::new("there is no tracked change at that position"))?;
    let doc = c.doc_mut()?;
    let st = doc.story_mut(sid).ok_or_else(|| EngineError::new("no such story"))?;
    resolve(st, run.0, run.1, &run.2, accept)?;
    c.label(if accept { "Accept Change" } else { "Reject Change" });
    ok()
}

fn accept(c: &mut Ctx, p: &Value) -> Result<Value> {
    change_one(c, p, true)
}

fn reject(c: &mut Ctx, p: &Value) -> Result<Value> {
    change_one(c, p, false)
}

fn change_all(c: &mut Ctx, accept: bool) -> Result<Value> {
    let doc = c.doc_mut()?;
    let ids: Vec<StoryId> = doc.stories.keys().copied().collect();
    let mut n = 0;
    for sid in ids {
        let runs = doc.story(sid).map(change_runs).unwrap_or_default();
        if runs.is_empty() {
            continue;
        }
        let Some(st) = doc.story_mut(sid) else { continue };
        for (a, b, k) in runs.into_iter().rev() {
            resolve(st, a, b, &k, accept)?;
            n += 1;
        }
    }
    c.label(if accept { "Accept All Changes" } else { "Reject All Changes" });
    Ok(json!({"changed": n}))
}

fn accept_all(c: &mut Ctx, _: &Value) -> Result<Value> {
    change_all(c, true)
}

fn reject_all(c: &mut Ctx, _: &Value) -> Result<Value> {
    change_all(c, false)
}

fn track(c: &mut Ctx, p: &Value) -> Result<Value> {
    let st = c.state_mut()?;
    st.track_changes = args::opt_bool(p, "on").unwrap_or(!st.track_changes);
    let on = st.track_changes;
    c.session.sel_rev += 1;
    Ok(json!({"on": on}))
}

fn condition_list(c: &mut Ctx, _: &Value) -> Result<Value> {
    let doc = c.doc()?;
    Ok(Value::Array(doc.conditions.iter().map(|k| json!({"name": k.name, "color": wire::rgb255(&k.color), "visible": k.visible})).collect()))
}

fn condition_new(c: &mut Ctx, p: &Value) -> Result<Value> {
    let doc = c.doc_mut()?;
    let name = args::opt_str(p, "name").map(str::to_string).unwrap_or_else(|| format!("Condition {}", doc.conditions.len() + 1));
    if doc.conditions.iter().any(|k| k.name == name) {
        return Err(EngineError::new(format!("there is already a condition named {name}")));
    }
    let color = p.get("color").and_then(wire::hex_of).unwrap_or_else(|| formdesign_model::page::LAYER_COLORS[doc.conditions.len() % 8].to_string());
    doc.conditions.push(Condition { name: name.clone(), visible: args::opt_bool(p, "visible").unwrap_or(true), color });
    Ok(json!({"name": name}))
}

fn conds_of(over: &formdesign_model::attrs::Attrs) -> Vec<String> {
    over.get("conditions").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default()
}

fn set_conds(seg: &mut Seg, list: Vec<String>) {
    let (_, over) = seg.format_mut();
    if list.is_empty() {
        over.remove("conditions");
    } else {
        over.insert("conditions".into(), json!(list));
    }
}

fn condition_apply(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = p.get("name").and_then(Value::as_str).map(str::to_string);
    let only = args::opt_bool(p, "only").unwrap_or(false);
    if let Some(n) = &name
        && !c.doc()?.conditions.iter().any(|k| k.name == *n)
    {
        return Err(EngineError::new(format!("no condition named {n}")));
    }
    if name.is_none() && !only {
        return Err(EngineError::new("missing `name` (a condition), or pass `name: null, only: true` to make the text unconditional"));
    }
    let targets: Vec<_> = text_targets(c)?.into_iter().filter(|(_, r)| !r.is_empty()).collect();
    if targets.is_empty() {
        return Err(EngineError::new("needs selected text: conditions apply to a text range"));
    }
    let doc = c.doc_mut()?;
    for (sid, range) in targets {
        let Some(st) = doc.story_mut(sid) else { continue };
        st.for_segs_in(range, |seg| {
            let mut list = if only { Vec::new() } else { conds_of(seg.over()) };
            if let Some(n) = &name
                && !list.contains(n)
            {
                list.push(n.clone());
            }
            set_conds(seg, list);
        });
    }
    c.label("Apply Condition");
    ok()
}

fn rename_in_runs(doc: &mut Doc, from: &str, to: Option<&str>) {
    let f = from.to_string();
    walk::paras_where(doc, &move |p| p.segs.iter().any(|s| conds_of(s.over()).contains(&f)), &mut |p| {
        for s in p.segs.iter_mut() {
            let mut list = conds_of(s.over());
            if !list.iter().any(|x| x == from) {
                continue;
            }
            list.retain(|x| x != from);
            if let Some(t) = to
                && !list.iter().any(|x| x == t)
            {
                list.push(t.to_string());
            }
            set_conds(s, list);
        }
        p.normalize();
    });
}

fn condition_options(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = args::str_(p, "name", "the condition")?.to_string();
    let doc = c.doc_mut()?;
    let k = doc.conditions.iter_mut().find(|k| k.name == name).ok_or_else(|| EngineError::new(format!("no condition named {name}")))?;
    if let Some(v) = args::opt_bool(p, "visible") {
        k.visible = v;
    }
    if let Some(col) = p.get("color").and_then(wire::hex_of) {
        k.color = col;
    }
    let to = args::opt_str(p, "newName").or_else(|| args::opt_str(p, "to")).map(str::to_string).filter(|t| *t != name);
    if let Some(t) = to {
        if doc.conditions.iter().any(|k| k.name == t) {
            return Err(EngineError::new(format!("there is already a condition named {t}")));
        }
        if let Some(k) = doc.conditions.iter_mut().find(|k| k.name == name) {
            k.name = t.clone();
        }
        rename_in_runs(doc, &name, Some(&t));
    }
    c.label("Condition Options");
    ok()
}

fn condition_delete(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = args::str_(p, "name", "the condition to delete")?.to_string();
    let repl = args::opt_str(p, "replaceWith").map(str::to_string).filter(|r| *r != name);
    let doc = c.doc_mut()?;
    if !doc.conditions.iter().any(|k| k.name == name) {
        return Err(EngineError::new(format!("no condition named {name}")));
    }
    if let Some(r) = &repl
        && !doc.conditions.iter().any(|k| k.name == *r)
    {
        return Err(EngineError::new(format!("no condition named {r}")));
    }
    doc.conditions.retain(|k| k.name != name);
    rename_in_runs(doc, &name, repl.as_deref());
    ok()
}

fn notes(doc: &Doc) -> Vec<(StoryId, u64, usize, String, String)> {
    let mut out = Vec::new();
    for st in doc.stories.values() {
        for (pos, inl) in st.inlines() {
            if let Inline::Note { id, author, text } = inl {
                out.push((st.id, *id, pos, author.clone(), text.clone()));
            }
        }
    }
    out
}

fn note_list(c: &mut Ctx, _: &Value) -> Result<Value> {
    let doc = c.doc()?;
    Ok(Value::Array(
        notes(doc)
            .into_iter()
            .map(|(s, id, at, author, text)| {
                let t = doc.story(s).map(|st| st.text()).unwrap_or_default();
                json!({"story": s, "id": id, "at": char_to_byte(&t, at), "author": author, "text": text})
            })
            .collect(),
    ))
}

fn note_new(c: &mut Ctx, p: &Value) -> Result<Value> {
    let text = args::str_(p, "text", "the note's text")?.trim().to_string();
    if text.is_empty() {
        return Err(EngineError::new("missing `text` (the note's text)"));
    }
    let author = args::opt_str(p, "author").unwrap_or("").to_string();
    let sel = c.selection().text.ok_or_else(|| EngineError::new("needs a text insertion point: click into text with the Type tool"))?;
    let at = sel.range().end;
    let doc = c.doc_mut()?;
    let id = doc.alloc();
    let st = doc.story_mut(sel.story).ok_or_else(|| EngineError::new("no such story"))?;
    st.insert_inline(at, Inline::Note { id, author, text });
    c.set_selection(Selection::text(TextSel::caret(sel.story, at + 1)));
    Ok(json!({"story": sel.story, "id": id}))
}

fn find_note(c: &Ctx, p: &Value) -> Result<(StoryId, u64, usize)> {
    let sid = args::opt_u64(p, "story").map(StoryId).ok_or_else(|| EngineError::new("missing `story` (from note.list)"))?;
    let id = args::u64_(p, "id", "the note id from note.list")?;
    notes(c.doc()?).into_iter().find(|n| n.0 == sid && n.1 == id).map(|n| (n.0, n.1, n.2)).ok_or_else(|| EngineError::new(format!("no note {id} in story {sid}")))
}

fn note_edit(c: &mut Ctx, p: &Value) -> Result<Value> {
    let (sid, _, at) = find_note(c, p)?;
    let text = args::str_(p, "text", "the note's new text")?.to_string();
    let doc = c.doc_mut()?;
    if let Some(Inline::Note { text: t, .. }) = doc.story_mut(sid).and_then(|st| st.inline_at_mut(at)) {
        *t = text;
    }
    ok()
}

fn note_delete(c: &mut Ctx, p: &Value) -> Result<Value> {
    let (sid, _, at) = find_note(c, p)?;
    if let Some(st) = c.doc_mut()?.story_mut(sid) {
        st.delete(at..at + 1);
    }
    ok()
}

fn note_to_text(c: &mut Ctx, p: &Value) -> Result<Value> {
    let (sid, _, at) = find_note(c, p)?;
    let text = notes(c.doc()?).into_iter().find(|n| n.0 == sid && n.2 == at).map(|n| n.4).unwrap_or_default();
    if let Some(st) = c.doc_mut()?.story_mut(sid) {
        st.delete(at..at + 1);
        st.insert_text(at, &text);
    }
    c.label("Convert Note to Text");
    ok()
}

fn words(doc: &Doc, key: &str) -> Vec<String> {
    doc.prefs.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default()
}

fn add_word(c: &mut Ctx, p: &Value) -> Result<Value> {
    let w = args::str_(p, "word", "the word to add")?.trim().to_string();
    if w.is_empty() {
        return Err(EngineError::new("missing `word` (the word to add)"));
    }
    let doc = c.doc_mut()?;
    let mut list = words(doc, "userWords");
    if !list.contains(&w) {
        list.push(w);
    }
    doc.prefs.insert("userWords".into(), json!(list));
    ok()
}

fn set_words(c: &mut Ctx, p: &Value) -> Result<Value> {
    let list: Vec<String> = p.get("words").and_then(Value::as_array).ok_or_else(|| EngineError::new("missing `words` (the full word list)"))?.iter().filter_map(|v| v.as_str().map(|s| s.trim().to_string())).filter(|s| !s.is_empty()).collect();
    c.doc_mut()?.prefs.insert("userWords".into(), json!(list));
    c.label("Dictionary");
    ok()
}

fn list_words(c: &mut Ctx, _: &Value) -> Result<Value> {
    Ok(json!(words(c.doc()?, "userWords")))
}

fn spell_change(c: &mut Ctx, p: &Value) -> Result<Value> {
    let (sid, st) = need_story(c.doc()?, p)?;
    let a = story_char(st, args::u64_(p, "start", "the word's start byte offset")? as usize);
    let b = story_char(st, args::u64_(p, "end", "the word's end byte offset")? as usize).max(a);
    let to = args::str_(p, "to", "the replacement")?.to_string();
    let doc = c.doc_mut()?;
    let st = doc.story_mut(sid).ok_or_else(|| EngineError::new("no such story"))?;
    st.delete(a..b);
    st.insert_text(a, &to);
    let n = to.chars().count();
    c.set_selection(Selection::text(TextSel::caret(sid, a + n)));
    c.label("Change");
    ok()
}

fn spell_check(c: &mut Ctx, _: &Value) -> Result<Value> {
    c.request(json!({"kind": "dialog", "id": "spelling.check", "params": {}}));
    ok()
}

fn hyph_list(c: &mut Ctx, _: &Value) -> Result<Value> {
    Ok(json!(words(c.doc()?, "hyphenationExceptions")))
}

fn hyph_add(c: &mut Ctx, p: &Value) -> Result<Value> {
    let w = args::str_(p, "word", "the word with ~ at allowed breaks, e.g. ex~am~ple")?.trim().to_string();
    if w.replace('~', "").is_empty() {
        return Err(EngineError::new("missing `word` (the word with ~ at allowed breaks)"));
    }
    let doc = c.doc_mut()?;
    let plain = w.replace('~', "").to_lowercase();
    let mut list: Vec<String> = words(doc, "hyphenationExceptions").into_iter().filter(|x| x.replace('~', "").to_lowercase() != plain).collect();
    list.push(w);
    doc.prefs.insert("hyphenationExceptions".into(), json!(list));
    c.label("Add Hyphenation Exception");
    ok()
}

fn hyph_remove(c: &mut Ctx, p: &Value) -> Result<Value> {
    let w = args::str_(p, "word", "the exception exactly as listed")?.to_string();
    let doc = c.doc_mut()?;
    let mut list = words(doc, "hyphenationExceptions");
    let before = list.len();
    list.retain(|x| *x != w);
    if list.len() == before {
        return Err(EngineError::new(format!("no hyphenation exception {w}")));
    }
    doc.prefs.insert("hyphenationExceptions".into(), json!(list));
    ok()
}

pub fn register(r: &mut Registry) {
    cmd("changes.track", "Track Changes").menu(&["Type", "Track Changes", "Track Changes in Current Story"]).query().sig("{on?}").reg(r, track);
    cmd("changes.list", "Changes").query().sig("{}").reg(r, changes_list);
    cmd("changes.accept", "Accept Change").sig("{story, start}").reg(r, accept);
    cmd("changes.reject", "Reject Change").sig("{story, start}").reg(r, reject);
    cmd("changes.acceptAll", "Accept All Changes").menu(&["Type", "Track Changes", "Accept All Changes"]).sig("{}").reg(r, accept_all);
    cmd("changes.rejectAll", "Reject All Changes").menu(&["Type", "Track Changes", "Reject All Changes"]).sig("{}").reg(r, reject_all);
    cmd("condition.list", "Conditions").query().sig("{}").reg(r, condition_list);
    cmd("condition.new", "New Condition").sig("{name?, color?: [r,g,b], visible?}").reg(r, condition_new);
    cmd("condition.apply", "Apply Condition").needs(Needs::Text).sig("{name | null, only?}").reg(r, condition_apply);
    cmd("condition.options", "Condition Options").sig("{name, visible?, color?, newName?}").reg(r, condition_options);
    cmd("condition.delete", "Delete Condition").sig("{name, replaceWith?}").reg(r, condition_delete);
    cmd("note.list", "Notes").query().sig("{}").reg(r, note_list);
    cmd("note.new", "New Note").needs(Needs::Text).sig("{text, author?}").reg(r, note_new);
    cmd("note.edit", "Edit Note").sig("{story, id, text}").reg(r, note_edit);
    cmd("note.delete", "Delete Note").sig("{story, id}").reg(r, note_delete);
    cmd("note.convertToText", "Convert Note to Text").sig("{story, id}").reg(r, note_to_text);
    cmd("spelling.addWord", "Add Word").sig("{word}").reg(r, add_word);
    cmd("spelling.setWords", "Dictionary").sig("{words: [..]}").reg(r, set_words);
    cmd("spelling.words", "Dictionary Words").query().sig("{}").reg(r, list_words);
    cmd("spelling.change", "Change").sig("{story, start, end, to} (byte offsets)").reg(r, spell_change);
    cmd("spelling.check", "Check Spelling...").menu(&["Edit", "Spelling", "Check Spelling..."]).key("Mod+I").query().sig("{}").reg(r, spell_check);
    cmd("hyphenation.list", "Hyphenation Exceptions").query().sig("{}").reg(r, hyph_list);
    cmd("hyphenation.addException", "Add Hyphenation Exception").sig("{word: ex~am~ple}").reg(r, hyph_add);
    cmd("hyphenation.removeException", "Remove Hyphenation Exception").sig("{word}").reg(r, hyph_remove);
}
