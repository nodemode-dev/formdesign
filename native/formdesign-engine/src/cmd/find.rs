use std::ops::Range;

use formdesign_model::attrs::Attrs;
use formdesign_model::doc::Doc;
use formdesign_model::ids::{ItemId, StoryId};
use formdesign_model::item::ItemKind;
use serde_json::{Map, Value, json};

use crate::registry::{Registry, cmd};
use crate::session::{Ctx, Selection, TextSel};
use crate::target::{byte_to_char, text_targets};
use crate::{EngineError, Result, args, wire};

struct Query {
    find: String,
    change: Option<String>,
    grep: bool,
    case: bool,
    whole: bool,
    find_format: Map<String, Value>,
    change_format: Map<String, Value>,
    locked: bool,
    hidden: bool,
}

fn query(p: &Value) -> Result<Query> {
    let find = args::opt_str(p, "find").unwrap_or("").to_string();
    let find_format = match p.get("findFormat").and_then(Value::as_object) {
        Some(m) => wire::char_in(m)?,
        None => Map::new(),
    };
    if find.is_empty() && find_format.is_empty() {
        return Err(EngineError::new("missing `find` (the text or GREP pattern to find)"));
    }
    let change_format = match p.get("changeFormat").and_then(Value::as_object) {
        Some(m) => wire::char_in(m)?,
        None => Map::new(),
    };
    let inc = p.get("include").and_then(Value::as_object);
    Ok(Query {
        find,
        change: p.get("change").and_then(Value::as_str).map(str::to_string),
        grep: args::opt_str(p, "mode") == Some("grep"),
        case: args::opt_bool(p, "caseSensitive").unwrap_or(false),
        whole: args::opt_bool(p, "wholeWord").unwrap_or(false),
        find_format,
        change_format,
        locked: inc.and_then(|o| o.get("locked")).and_then(Value::as_bool).unwrap_or(false),
        hidden: inc.and_then(|o| o.get("hidden")).and_then(Value::as_bool).unwrap_or(false),
    })
}

fn regex(q: &Query) -> Result<fancy_regex::Regex> {
    let pat = if q.grep { q.find.clone() } else { fancy_regex::escape(&q.find).to_string() };
    let pat = if q.whole { format!(r"\b(?:{pat})\b") } else { pat };
    let pat = if q.case { pat } else { format!("(?i){pat}") };
    fancy_regex::Regex::new(&pat).map_err(|e| EngineError::new(format!("the GREP pattern does not compile: {e}")))
}

fn story_ok(doc: &Doc, sid: StoryId, q: &Query) -> bool {
    let Some(st) = doc.story(sid) else { return false };
    if st.frames.is_empty() {
        return false;
    }
    st.frames.iter().any(|f| {
        doc.item(*f).is_some_and(|it| {
            let layer = doc.layer(it.layer);
            (q.locked || (!it.locked && !layer.is_some_and(|l| l.locked))) && (q.hidden || (!it.hidden && layer.is_none_or(|l| l.visible)))
        })
    })
}

fn format_ok(doc: &Doc, sid: StoryId, at: usize, f: &Map<String, Value>) -> bool {
    if f.is_empty() {
        return true;
    }
    let Some(st) = doc.story(sid) else { return false };
    let (pi, off) = st.locate(at);
    let para = &st.paras[pi];
    let (cs, over) = para.format_at(off + 1);
    let stack = doc.styles.char_stack(&para.style, &para.over, cs.as_deref(), &over);
    f.iter().all(|(k, v)| {
        let have = stack.get(k);
        match (have.as_f64(), v.as_f64()) {
            (Some(a), Some(b)) => (a - b).abs() < 1e-6,
            _ => {
                let pa = formdesign_model::color::Paint::from_value(&have);
                let pb = formdesign_model::color::Paint::from_value(v);
                if matches!(k.as_str(), "fill" | "stroke") { pa == pb } else { have == *v }
            }
        }
    })
}

type Hit = (Range<usize>, String);

fn matches(doc: &Doc, sid: StoryId, scope: Option<Range<usize>>, q: &Query, re: &fancy_regex::Regex) -> Vec<Hit> {
    let Some(st) = doc.story(sid) else { return Vec::new() };
    let text = st.text();
    let mut out = Vec::new();
    if q.find.is_empty() {
        let n = st.len();
        let r = scope.unwrap_or(0..n);
        let mut i = r.start;
        while i < r.end {
            if format_ok(doc, sid, i, &q.find_format) {
                let s = i;
                while i < r.end && format_ok(doc, sid, i, &q.find_format) {
                    i += 1;
                }
                out.push((s..i, q.change.clone().unwrap_or_else(|| text.chars().skip(s).take(i - s).collect())));
            }
            i += 1;
        }
        return out;
    }
    for m in re.captures_iter(text.as_str()).filter_map(|m| m.ok()) {
        let Some(whole) = m.get(0) else { continue };
        if whole.start() == whole.end() {
            continue;
        }
        let a = byte_to_char(&text, whole.start());
        let b = byte_to_char(&text, whole.end());
        if let Some(sc) = &scope
            && (a < sc.start || b > sc.end)
        {
            continue;
        }
        if !format_ok(doc, sid, a, &q.find_format) {
            continue;
        }
        let rep = match &q.change {
            Some(c) if q.grep => {
                let mut s = String::new();
                m.expand(c, &mut s);
                s
            }
            Some(c) => c.clone(),
            None => String::new(),
        };
        out.push((a..b, rep));
    }
    out
}

fn scope_stories(c: &Ctx, p: &Value, q: &Query) -> Result<Vec<(StoryId, Option<Range<usize>>)>> {
    let doc = c.doc()?;
    Ok(match args::opt_str(p, "scope").unwrap_or("document") {
        "story" => text_targets(c)?.into_iter().map(|(s, _)| (s, None)).collect(),
        "selection" => text_targets(c)?.into_iter().map(|(s, r)| (s, Some(r))).collect(),
        _ => doc.stories.keys().copied().filter(|s| story_ok(doc, *s, q)).map(|s| (s, None)).collect(),
    })
}

fn change(c: &mut Ctx, p: &Value) -> Result<Value> {
    let q = query(p)?;
    if q.find.is_empty() && q.change.is_none() && q.change_format.is_empty() {
        return Err(EngineError::new("a format-only find needs `change` (the new text) or `changeFormat`"));
    }
    let re = regex(&q)?;
    let stories = scope_stories(c, p, &q)?;
    let mut work: Vec<(StoryId, Vec<Hit>)> = Vec::new();
    {
        let doc = c.doc()?;
        for (sid, sc) in stories {
            let m = matches(doc, sid, sc, &q, &re);
            if !m.is_empty() {
                work.push((sid, m));
            }
        }
    }
    let total: usize = work.iter().map(|w| w.1.len()).sum();
    if total == 0 {
        return Ok(json!({"changed": 0}));
    }
    let format_only = q.change.as_deref().is_none_or(str::is_empty) && !q.change_format.is_empty();
    let doc = c.doc_mut()?;
    for (sid, list) in work {
        let Some(st) = doc.story_mut(sid) else { continue };
        for (r, rep) in list.into_iter().rev() {
            if format_only {
                st.set_char_attrs(r, &q.change_format)?;
                continue;
            }
            st.delete(r.clone());
            let before = st.len();
            crate::cmd::object::put_text(st, r.start, &rep);
            let n = st.len() - before;
            if n > 0 && !q.change_format.is_empty() {
                st.set_char_attrs(r.start..r.start + n, &q.change_format)?;
            }
        }
    }
    c.set_selection(Selection::default());
    c.label("Change All");
    Ok(json!({"changed": total}))
}

fn find_from(c: &mut Ctx, p: &Value, from_caret: bool) -> Result<Value> {
    let q = query(p)?;
    let re = regex(&q)?;
    let stories = scope_stories(c, p, &q)?;
    let start = c.selection().text.filter(|_| from_caret).map(|t| (t.story, t.range().end));
    let doc = c.doc()?;
    let mut all: Vec<(StoryId, Range<usize>)> = Vec::new();
    for (sid, sc) in stories {
        for (r, _) in matches(doc, sid, sc, &q, &re) {
            all.push((sid, r));
        }
    }
    let pick = match start {
        Some((s, at)) => {
            let after = all.iter().position(|(sid, r)| *sid == s && r.start >= at);
            let later_story = all.iter().position(|(sid, _)| *sid > s);
            after.or(later_story).or(if all.is_empty() { None } else { Some(0) })
        }
        None => (!all.is_empty()).then_some(0),
    };
    if let Ok(st) = c.state_mut() {
        st.last_find = Some(p.clone());
    }
    let Some(i) = pick else { return Ok(json!({"found": false, "count": 0})) };
    let (sid, r) = all[i].clone();
    let text = c.doc()?.story(sid).map(|s| s.text()).unwrap_or_default();
    c.set_selection(Selection::text(TextSel { story: sid, anchor: r.start, focus: r.end }));
    Ok(json!({"found": true, "count": all.len(), "story": sid, "start": crate::target::char_to_byte(&text, r.start), "end": crate::target::char_to_byte(&text, r.end)}))
}

fn find(c: &mut Ctx, p: &Value) -> Result<Value> {
    find_from(c, p, false)
}

fn find_next(c: &mut Ctx, p: &Value) -> Result<Value> {
    let q = if p.get("find").is_some() { p.clone() } else { c.state()?.last_find.clone().ok_or_else(|| EngineError::new("missing `find`: nothing has been searched for yet"))? };
    find_from(c, &q, true)
}

fn object_match(doc: &Doc, id: ItemId, p: &Value) -> bool {
    let Some(it) = doc.item(id) else { return false };
    if let Some(o) = args::opt_f64(p, "opacity")
        && (it.opacity - o).abs() > 1e-6
    {
        return false;
    }
    if let Some(f) = args::opt_str(p, "fill")
        && wire::paint_name(&it.fill) != wire::swatch_name(f)
    {
        return false;
    }
    if let Some(s) = args::opt_str(p, "stroke")
        && wire::paint_name(&it.stroke.paint) != wire::swatch_name(s)
    {
        return false;
    }
    if let Some(w) = args::opt_f64(p, "strokeWeight")
        && (it.stroke.weight - w).abs() > 1e-6
    {
        return false;
    }
    if let Some(l) = args::opt_u64(p, "layer")
        && it.layer.0 != l
    {
        return false;
    }
    if let Some(os) = args::opt_str(p, "objectStyle")
        && it.style.as_deref().unwrap_or("None") != formdesign_model::style::style_key(os)
    {
        return false;
    }
    if let Some(k) = args::opt_str(p, "kind") {
        let kind = match &it.kind {
            ItemKind::Text(_) => "text",
            ItemKind::Graphic(_) => "graphic",
            ItemKind::Group { .. } | ItemKind::States { .. } => "group",
            ItemKind::Plain => "shape",
        };
        if kind != k {
            return false;
        }
    }
    true
}

fn find_objects(c: &mut Ctx, p: &Value) -> Result<Value> {
    let doc = c.doc()?;
    let mut ids: Vec<ItemId> = doc.items.keys().copied().filter(|i| object_match(doc, *i, p)).collect();
    ids.sort();
    if args::opt_bool(p, "select").unwrap_or(true) {
        let tops: Vec<ItemId> = ids.iter().copied().filter(|i| doc.item(*i).is_some_and(|it| it.group.is_none())).collect();
        c.set_selection(Selection::items(tops));
    }
    Ok(json!({"ids": ids}))
}

fn change_objects(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ch = p.get("change").and_then(Value::as_object).cloned().ok_or_else(|| EngineError::new("missing `change` (the attributes to set, e.g. {fill: \"Accent 2\"})"))?;
    let doc = c.doc()?;
    let ids: Vec<ItemId> = doc.items.keys().copied().filter(|i| object_match(doc, *i, p)).collect();
    let style_attrs: Option<Attrs> = ch.get("objectStyle").and_then(Value::as_str).map(|n| {
        let mut out = Attrs::new();
        for a in doc.styles.chain(formdesign_model::style::StyleKind::Object, n) {
            out.extend(a.clone());
        }
        out
    });
    let doc = c.doc_mut()?;
    for id in &ids {
        if let Some(sa) = &style_attrs {
            crate::cmd::text::apply_object_style(doc, *id, sa);
        }
        let Some(it) = doc.item_mut(*id) else { continue };
        if let Some(f) = ch.get("fill").and_then(Value::as_str) {
            it.fill = wire::paint_in(f, ch.get("tint").and_then(Value::as_f64));
        }
        if let Some(s) = ch.get("stroke").and_then(Value::as_str) {
            it.stroke.paint = wire::paint_in(s, None);
        }
        if let Some(w) = ch.get("strokeWeight").and_then(Value::as_f64) {
            it.stroke.weight = w.max(0.0);
        }
        if let Some(o) = ch.get("opacity").and_then(Value::as_f64) {
            it.opacity = o.clamp(0.0, 1.0);
        }
        if let Some(n) = ch.get("objectStyle").and_then(Value::as_str) {
            it.style = Some(formdesign_model::style::style_key(n)).filter(|s| s != "None");
        }
    }
    c.label("Change All");
    Ok(json!({"changed": ids.len()}))
}

pub fn register(r: &mut Registry) {
    cmd("find.find", "Find...").menu(&["Edit", "Find/Change..."]).key("Mod+F").query().sig("{find, mode?: text|grep, caseSensitive?, wholeWord?, scope?: document|story|selection, findFormat?}").reg(r, find);
    cmd("find.next", "Find Next").menu(&["Edit", "Find Next"]).key("Mod+Alt+F").query().sig("{} (repeats the last search)").reg(r, find_next);
    cmd("find.change", "Change All").sig("{find, change, mode?: text|grep, caseSensitive?, wholeWord?, scope?, findFormat?, changeFormat?, include?: {locked, hidden}}").reg(r, change);
    cmd("find.objects", "Find Objects").query().sig("{opacity?, fill?, stroke?, strokeWeight?, layer?, objectStyle?, kind?, select?}").reg(r, find_objects);
    cmd("find.changeObjects", "Change Objects").sig("{fill?, stroke?, ... criteria, change: {fill?, stroke?, strokeWeight?, opacity?, objectStyle?}}").reg(r, change_objects);
}
