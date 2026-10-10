use std::sync::Arc;

use formdes_model::attrs::Attrs;
use formdes_model::doc::{Doc, Home};
use formdes_model::geom::{Affine, Point, Rect, Vec2};
use formdes_model::ids::{ItemId, StoryId};
use formdes_model::item::{Item, ItemKind, TextFrame};
use formdes_model::story::{Inline, Para};
use serde_json::{Map, Value, json};

use crate::registry::{Needs, Registry, cmd};
use crate::session::{Ctx, Selection, TextSel};
use crate::transplant::Mapper;
use crate::{EngineError, Result, args, ops};

#[derive(Clone)]
pub enum Clip {
    Items { doc: Arc<Doc>, ids: Vec<ItemId>, spread: Option<usize>, uid: u64, text: String },
    Text { doc: Arc<Doc>, paras: Vec<Para>, text: String },
}

impl Clip {
    pub fn text(&self) -> &str {
        match self {
            Clip::Items { text, .. } | Clip::Text { text, .. } => text,
        }
    }
}

fn plain(paras: &[Para]) -> String {
    paras.iter().map(Para::plain).collect::<Vec<_>>().join("\n")
}

fn top_level(doc: &Doc, ids: &[ItemId]) -> Vec<ItemId> {
    let mut out: Vec<ItemId> = Vec::new();
    for id in ids {
        let mut up = doc.item(*id).and_then(|i| i.group);
        let mut covered = false;
        while let Some(g) = up {
            if ids.contains(&g) {
                covered = true;
                break;
            }
            up = doc.item(g).and_then(|i| i.group);
        }
        if !covered && !out.contains(id) {
            out.push(*id);
        }
    }
    out
}

fn take(c: &mut Ctx, p: &Value) -> Result<Clip> {
    let st = c.state()?;
    let sel = st.selection.clone();
    if let Some(t) = sel.text
        && !t.range().is_empty()
        && p.get("ids").is_none()
    {
        let story = st.doc.story(t.story).ok_or_else(|| EngineError::new("no such story"))?;
        let paras = story.slice(t.range());
        return Ok(Clip::Text { doc: st.doc.clone(), text: plain(&paras), paras });
    }
    let ids = crate::target::items(c, p).map_err(|_| EngineError::new("nothing to copy: select objects, or select text with the Type tool"))?;
    let st = c.state()?;
    let doc = &st.doc;
    let ids = top_level(doc, &ids);
    let spread = ids.first().and_then(|i| doc.spread_of(*i));
    let mut text = Vec::new();
    for s in crate::target::text_frames_in(doc, &ids) {
        if let Some(st) = doc.story(s) {
            text.push(st.plain());
        }
    }
    Ok(Clip::Items { doc: st.doc.clone(), ids, spread, uid: st.uid, text: text.join("\n") })
}

fn summary(clip: &Clip) -> Value {
    match clip {
        Clip::Items { ids, text, .. } => json!({"kind": "items", "count": ids.len(), "text": text}),
        Clip::Text { text, paras, .. } => json!({"kind": "text", "paragraphs": paras.len(), "text": text}),
    }
}

fn copy(c: &mut Ctx, p: &Value) -> Result<Value> {
    let clip = take(c, p)?;
    let out = summary(&clip);
    c.session.clipboard = Some(clip);
    Ok(out)
}

fn cut(c: &mut Ctx, p: &Value) -> Result<Value> {
    let clip = take(c, p)?;
    let out = summary(&clip);
    match &clip {
        Clip::Text { .. } => {
            crate::cmd::text::text_delete(c, &json!({}))?;
        }
        Clip::Items { ids, .. } => {
            let ids = ids.clone();
            let doc = c.doc_mut()?;
            for id in &ids {
                doc.remove_item(*id);
            }
            c.set_selection(Selection::default());
        }
    }
    c.session.clipboard = Some(clip);
    c.label("Cut");
    Ok(out)
}

fn target_spread(c: &Ctx, p: &Value, fallback: Option<usize>) -> Result<Home> {
    if p.get("spread").is_some_and(|v| !v.is_null()) {
        return crate::cmd::object::spread_home(c.doc()?, p.get("spread"));
    }
    let doc = c.doc()?;
    let sel = c.selection();
    if let Some(si) = sel.items.first().and_then(|i| doc.spread_of(*i)) {
        return Ok(Home::Spread(si));
    }
    if let Some(t) = sel.text
        && let Some(si) = doc.story(t.story).and_then(|s| s.frames.first().copied()).and_then(|f| doc.spread_of(f))
    {
        return Ok(Home::Spread(si));
    }
    let si = fallback.filter(|s| *s < doc.spreads.len()).unwrap_or(0);
    if doc.spreads.is_empty() {
        return Err(EngineError::new("the document has no spreads to paste onto"));
    }
    Ok(Home::Spread(si))
}

fn point(p: &Value) -> Option<Point> {
    let a = p.get("at")?.as_array()?;
    Some(Point::new(a.first()?.as_f64()?, a.get(1)?.as_f64()?))
}

fn paste_items(c: &mut Ctx, p: &Value, clip: &Clip, in_place: bool) -> Result<Vec<ItemId>> {
    let Clip::Items { doc: src, ids, spread, uid, .. } = clip else { return Ok(Vec::new()) };
    let home = target_spread(c, p, *spread)?;
    let same = c.state()?.uid == *uid && matches!(home, Home::Spread(s) if Some(s) == *spread);
    let bounds = ops::union_bounds(src, ids);
    let offset = match (point(p), bounds) {
        (Some(at), Some(b)) if !in_place => at - b.center(),
        _ if same && !in_place => Vec2::new(12.0, 12.0),
        _ => Vec2::ZERO,
    };
    let layer = c.target_layer()?;
    let src = src.clone();
    let doc = c.doc_mut()?;
    let mut m = Mapper::new(&src);
    if src.layers.iter().all(|l| doc.layer(l.id).is_none() && !doc.layers.iter().any(|d| d.name == l.name)) {
        m.layer = Some(layer);
    }
    let mut out = Vec::new();
    for id in ids {
        let Some(nid) = m.item(doc, *id, home, offset) else { continue };
        if src.item(*id).is_some_and(|i| i.group.is_some())
            && let Some(it) = doc.item_mut(nid)
        {
            it.xf = Affine::translate(offset) * src.world_xf(*id);
        }
        out.push(nid);
    }
    Ok(out)
}

fn paste_into_text(c: &mut Ctx, clip: &Clip, t: TextSel, formatted: bool) -> Result<usize> {
    if !formatted {
        return crate::cmd::text::insert_at_selection(c, clip.text(), true);
    }
    let tracking = c.state()?.track_changes;
    let (src, paras) = match clip {
        Clip::Text { doc, paras, .. } => (doc.clone(), paras.clone()),
        Clip::Items { doc, ids, .. } => {
            let mut segs = Vec::new();
            for id in ids {
                segs.push(formdes_model::story::Seg::Inline { inline: Inline::Anchored { item: *id, attrs: Attrs::new() }, style: None, over: Attrs::new() });
            }
            let style = c.doc()?.story(t.story).and_then(|s| s.paras.get(s.locate(t.range().start).0)).map(|p| p.style.clone()).unwrap_or_default();
            (doc.clone(), vec![Para { style, over: Attrs::new(), segs }])
        }
    };
    let doc = c.doc_mut()?;
    let mut m = Mapper::new(&src);
    let mapped = m.paras(doc, &paras, t.story);
    let st = doc.story_mut(t.story).ok_or_else(|| EngineError::new("no such story"))?;
    let range = t.range();
    let mut at = range.start;
    if !range.is_empty() {
        if tracking {
            let mut mk = Map::new();
            mk.insert("change".into(), json!("deleted"));
            st.set_char_attrs(range.clone(), &mk)?;
            at = range.end;
        } else {
            st.delete(range.clone());
        }
    }
    let before = st.len();
    st.insert_paras(at, mapped);
    let n = st.len() - before;
    if tracking && n > 0 {
        let mut mk = Map::new();
        mk.insert("change".into(), json!("inserted"));
        st.set_char_attrs(at..at + n, &mk)?;
    }
    c.set_selection(Selection::text(TextSel::caret(t.story, at + n)));
    Ok(n)
}

fn new_text_frame(c: &mut Ctx, p: &Value, clip: &Clip, formatted: bool) -> Result<ItemId> {
    let home = target_spread(c, p, None)?;
    let Home::Spread(si) = home else { return Err(EngineError::new("paste text onto a spread")) };
    let layer = c.target_layer()?;
    let doc = c.doc()?;
    let first = doc.first_page_of_spread(si);
    let m = crate::cmd::object::margin_rect(doc, first).unwrap_or(Rect::new(36.0, 36.0, 288.0, 180.0));
    let h = (m.height() / 3.0).max(72.0);
    let r = match point(p) {
        Some(at) => Rect::new(at.x - m.width() / 2.0, at.y - h / 2.0, at.x + m.width() / 2.0, at.y + h / 2.0),
        None => Rect::new(m.x0, m.y0, m.x1, m.y0 + h),
    };
    let doc = c.doc_mut()?;
    let sid: StoryId = doc.new_story(formdes_model::style::BASIC_PARA);
    let mut it = Item::new(ItemId(0), ItemKind::Text(TextFrame::new(sid)), formdes_model::item::Shape::rect(r), layer);
    it.style = Some(formdes_model::style::BASIC_TEXT.to_string());
    let id = doc.insert_item(it, home);
    if let Some(st) = doc.story_mut(sid) {
        st.frames = vec![id];
    }
    c.set_selection(Selection::text(TextSel::caret(sid, 0)));
    paste_into_text(c, clip, TextSel::caret(sid, 0), formatted)?;
    c.set_selection(Selection::items(vec![id]));
    Ok(id)
}

fn external(c: &Ctx, p: &Value) -> Option<Clip> {
    let t = args::opt_str(p, "text").filter(|t| !t.is_empty())?;
    if c.session.clipboard.as_ref().is_some_and(|k| k.text() == t) {
        return None;
    }
    let paras: Vec<Para> = t.replace("\r\n", "\n").split(['\n', '\r']).map(|l| Para::from_text(formdes_model::style::BASIC_PARA, l)).collect();
    Some(Clip::Text { doc: Arc::new(c.doc().ok()?.clone()), paras, text: t.to_string() })
}

fn do_paste(c: &mut Ctx, p: &Value, formatted: bool, in_place: bool) -> Result<Value> {
    let clip = external(c, p).or_else(|| c.session.clipboard.clone()).ok_or_else(|| EngineError::new("the clipboard is empty: copy something first"))?;
    let external_text = matches!(&clip, Clip::Text { .. }) && args::opt_str(p, "text").is_some_and(|t| t == clip.text()) && c.session.clipboard.as_ref().is_none_or(|k| k.text() != clip.text());
    let formatted = formatted && !external_text;
    let sel = c.selection();
    if let Some(t) = sel.text {
        let n = paste_into_text(c, &clip, t, formatted || matches!(clip, Clip::Items { .. }))?;
        c.label(if formatted { "Paste" } else { "Paste without Formatting" });
        return Ok(json!({"inserted": n}));
    }
    match &clip {
        Clip::Text { .. } => {
            let id = new_text_frame(c, p, &clip, formatted)?;
            c.label("Paste");
            Ok(json!({"ids": [id], "id": id}))
        }
        Clip::Items { .. } => {
            let out = paste_items(c, p, &clip, in_place)?;
            c.set_selection(Selection::items(out.clone()));
            c.label(if in_place { "Paste in Place" } else { "Paste" });
            Ok(json!({"ids": out, "id": out.first()}))
        }
    }
}

fn paste(c: &mut Ctx, p: &Value) -> Result<Value> {
    do_paste(c, p, true, false)
}

fn paste_plain(c: &mut Ctx, p: &Value) -> Result<Value> {
    do_paste(c, p, false, false)
}

fn paste_in_place(c: &mut Ctx, p: &Value) -> Result<Value> {
    do_paste(c, p, true, true)
}

fn paste_into(c: &mut Ctx, p: &Value) -> Result<Value> {
    let target = crate::target::items(c, p).map_err(|_| EngineError::new("Paste Into needs one selected frame to paste into"))?;
    let [frame] = target.as_slice() else { return Err(EngineError::new("Paste Into needs exactly one selected frame")) };
    let frame = *frame;
    let Some(Clip::Items { doc: src, ids, .. }) = c.session.clipboard.clone() else { return Err(EngineError::new("Paste Into takes a copied picture: copy a graphic frame first")) };
    let pic = ids.iter().find_map(|i| src.item(*i).and_then(|it| it.graphic().and_then(|g| g.asset.map(|a| (*i, a, g.content)))));
    let Some((pid, _, content)) = pic else { return Err(EngineError::new("Paste Into takes a copied picture: the clipboard holds no placed graphic")) };
    let src_world = src.world_xf(pid);
    let doc = c.doc()?;
    if doc.item(frame).is_some_and(|i| matches!(i.kind, ItemKind::Text(_) | ItemKind::Group { .. } | ItemKind::States { .. })) {
        return Err(EngineError::new("Paste Into works on an empty or graphic frame, not a text frame or group"));
    }
    let frame_world = doc.world_xf(frame);
    let doc = c.doc_mut()?;
    let mut m = Mapper::new(&src);
    let aid = src.item(pid).and_then(|i| i.graphic()).and_then(|g| g.asset).ok_or_else(|| EngineError::new("no picture"))?;
    let tmp = m.item(doc, pid, Home::Spread(0), Vec2::ZERO);
    let new_asset = tmp.and_then(|t| doc.item(t).and_then(|i| i.graphic()).and_then(|g| g.asset)).unwrap_or(aid);
    if let Some(t) = tmp {
        doc.remove_item(t);
    }
    let local = frame_world.inverse() * src_world * content;
    if let Some(it) = doc.item_mut(frame) {
        it.kind = ItemKind::Graphic(formdes_model::item::Graphic { asset: Some(new_asset), content: local, fitting: None });
        if it.style.is_none() {
            it.style = Some(formdes_model::style::BASIC_GRAPHIC.to_string());
        }
    }
    c.set_selection(Selection::items(vec![frame]));
    Ok(json!({"id": frame}))
}

fn step_and_repeat(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = crate::target::items(c, p)?;
    let ids = top_level(c.doc()?, &ids);
    let dx = args::opt_f64(p, "dx").or_else(|| args::opt_f64(p, "horizontal")).unwrap_or(12.0);
    let dy = args::opt_f64(p, "dy").or_else(|| args::opt_f64(p, "vertical")).unwrap_or(12.0);
    let grid = args::opt_bool(p, "grid").unwrap_or(false) || p.get("rows").is_some() || p.get("columns").is_some();
    let mut cells: Vec<Vec2> = Vec::new();
    if grid {
        let rows = args::opt_u64(p, "rows").unwrap_or(1).clamp(1, 200);
        let cols = args::opt_u64(p, "columns").unwrap_or(1).clamp(1, 200);
        if rows * cols > 2000 {
            return Err(EngineError::new("Step and Repeat makes at most 2000 copies"));
        }
        for r in 0..rows {
            for k in 0..cols {
                if r == 0 && k == 0 {
                    continue;
                }
                cells.push(Vec2::new(dx * k as f64, dy * r as f64));
            }
        }
    } else {
        let n = args::opt_u64(p, "count").unwrap_or(1);
        if n > 2000 {
            return Err(EngineError::new("Step and Repeat makes at most 2000 copies"));
        }
        for k in 1..=n {
            cells.push(Vec2::new(dx * k as f64, dy * k as f64));
        }
    }
    let doc = c.doc_mut()?;
    let mut out = Vec::new();
    for off in cells {
        for id in &ids {
            if let Some(n) = ops::duplicate(doc, *id, off, None) {
                out.push(n);
            }
        }
    }
    c.set_selection(Selection::items(out.clone()));
    Ok(json!({"ids": out, "count": out.len()}))
}

fn clip_get(c: &mut Ctx, _: &Value) -> Result<Value> {
    Ok(c.session.clipboard.as_ref().map(summary).unwrap_or(Value::Null))
}

pub fn register(r: &mut Registry) {
    cmd("edit.copy", "Copy").menu(&["Edit", "Copy"]).key("Mod+C").needs(Needs::Selection).query().sig("{ids?}").help("Copies the selected objects, or the selected text, to formdes's clipboard; the result's `text` is the plain text for the system clipboard.").reg(r, copy);
    cmd("edit.cut", "Cut").menu(&["Edit", "Cut"]).key("Mod+X").needs(Needs::Selection).sig("{ids?}").reg(r, cut);
    cmd("edit.paste", "Paste").menu(&["Edit", "Paste"]).key("Mod+V").sig("{text?: system clipboard text, spread?, at?: [x, y]}").help("Pastes into the text cursor (objects become inline anchored objects), or onto the spread: objects keep their position, offset when pasted onto the spread they came from, centred on `at` when given; text becomes a new text frame. `text` is the system clipboard's text, used when it differs from what formdes copied last.").reg(r, paste);
    cmd("edit.pasteWithoutFormatting", "Paste without Formatting").menu(&["Edit", "Paste without Formatting"]).key("Mod+Shift+V").sig("{text?, spread?, at?}").reg(r, paste_plain);
    cmd("edit.pasteInPlace", "Paste in Place").menu(&["Edit", "Paste in Place"]).key("Mod+Alt+Shift+V").sig("{spread?}").reg(r, paste_in_place);
    cmd("edit.pasteInto", "Paste Into").menu(&["Edit", "Paste Into"]).key("Mod+Alt+V").needs(Needs::Items).sig("{ids?}").help("Puts the copied picture inside the selected frame, keeping where it sat on the page.").reg(r, paste_into);
    cmd("edit.stepAndRepeat", "Step and Repeat").menu(&["Edit", "Step and Repeat..."]).key("Mod+Alt+U").needs(Needs::Items).sig("{count?, dx?, dy?, grid?, rows?, columns?, ids?}").reg(r, step_and_repeat);
    cmd("edit.clipboard", "Clipboard").query().sig("{}").reg(r, clip_get);
}
