use std::ops::Range;

use formdes_model::doc::Doc;
use formdes_model::ids::{ItemId, StoryId};
use formdes_model::story::Story;
use serde_json::Value;

use crate::session::Ctx;
use crate::{EngineError, Result};

fn id_of(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_f64().filter(|f| *f >= 0.0 && f.fract() == 0.0).map(|f| f as u64)).or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

pub fn ids_param(p: &Value, key: &str) -> Option<Vec<ItemId>> {
    match p.get(key)? {
        Value::Array(a) => Some(a.iter().filter_map(id_of).map(ItemId).collect()),
        Value::Null => None,
        v => id_of(v).map(|n| vec![ItemId(n)]),
    }
}

pub fn one_id(p: &Value, key: &str) -> Option<ItemId> {
    p.get(key).and_then(id_of).map(ItemId)
}

pub fn items(c: &Ctx, p: &Value) -> Result<Vec<ItemId>> {
    let doc = c.doc()?;
    let ids = match ids_param(p, "ids").or_else(|| ids_param(p, "id")) {
        Some(ids) => {
            if let Some(bad) = ids.iter().find(|i| doc.item(**i).is_none()) {
                return Err(EngineError::new(format!("no object with id {bad}")));
            }
            ids
        }
        None => c.selection().items,
    };
    let ids: Vec<ItemId> = ids.into_iter().filter(|i| doc.item(*i).is_some()).collect();
    if ids.is_empty() {
        return Err(EngineError::new("needs a selection: select one or more objects, or pass `ids`"));
    }
    Ok(ids)
}

pub fn items_or_empty(c: &Ctx, p: &Value) -> Result<Vec<ItemId>> {
    match items(c, p) {
        Ok(v) => Ok(v),
        Err(e) if e.0.starts_with("needs a selection") => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

pub fn text_frames_in(doc: &Doc, ids: &[ItemId]) -> Vec<StoryId> {
    let mut out = Vec::new();
    for id in doc.walk(ids) {
        if let Some(s) = doc.item(id).and_then(|i| i.story())
            && !out.contains(&s)
        {
            out.push(s);
        }
    }
    out
}

pub fn text_targets(c: &Ctx) -> Result<Vec<(StoryId, Range<usize>)>> {
    let sel = c.selection();
    let doc = c.doc()?;
    if let Some(t) = sel.text
        && doc.story(t.story).is_some()
    {
        return Ok(vec![(t.story, t.range())]);
    }
    let stories: Vec<_> = text_frames_in(doc, &sel.items).into_iter().filter_map(|s| doc.story(s).map(|st| (s, 0..st.len()))).collect();
    if stories.is_empty() {
        return Err(EngineError::new("needs a text selection: click into text with the Type tool, or select a text frame"));
    }
    Ok(stories)
}

pub fn char_to_byte(text: &str, c: usize) -> usize {
    text.char_indices().nth(c).map(|(i, _)| i).unwrap_or(text.len())
}

pub fn byte_to_char(text: &str, b: usize) -> usize {
    let mut b = b.min(text.len());
    while !text.is_char_boundary(b) {
        b -= 1;
    }
    text[..b].chars().count()
}

pub fn story_char(st: &Story, byte: usize) -> usize {
    byte_to_char(&st.text(), byte)
}

pub fn story_byte(st: &Story, ch: usize) -> usize {
    char_to_byte(&st.text(), ch)
}

pub fn story_id(p: &Value, key: &str) -> Option<StoryId> {
    p.get(key).and_then(id_of).map(StoryId)
}

pub fn need_story<'a>(doc: &'a Doc, p: &Value) -> Result<(StoryId, &'a Story)> {
    let sid = match story_id(p, "story") {
        Some(s) => s,
        None => match one_id(p, "frame") {
            Some(f) => doc.item(f).and_then(|i| i.story()).ok_or_else(|| EngineError::new(format!("object {f} is not a text frame")))?,
            None => return Err(EngineError::new("missing `story` (a story id) or `frame` (a text frame id)")),
        },
    };
    let st = doc.story(sid).ok_or_else(|| EngineError::new(format!("no story {sid}")))?;
    Ok((sid, st))
}
