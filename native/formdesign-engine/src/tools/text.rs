use formdesign_model::geom::Point;
use formdesign_model::ids::{ItemId, StoryId};
use formdesign_model::item::{ItemKind, PathText, TextFrame};
use serde_json::json;

use super::geo;
use super::select::{here, start};
use super::{Ev, G, Key, Kind, Mods, View};
use crate::session::{Selection, Session, TextSel};
use crate::Result;

fn text_frame_at(ses: &Session, slot: crate::canvas::SpreadRef, p: Point, tol: f64) -> Option<ItemId> {
    let doc = &ses.active()?.doc;
    let hit = geo::hit_deep(doc, slot, p, tol)?;
    doc.item(hit)?.text().map(|_| hit)
}

fn pos_at(ses: &mut Session, id: ItemId, p: Point) -> Option<(StoryId, usize)> {
    let r = ses.renderer.clone();
    let lay = ses.active_mut()?.layout(&r);
    geo::text_hit(ses.active()?, &lay, id, p)
}

pub fn type_tool(ses: &mut Session, ev: &Ev, view: View) -> Result<()> {
    match ev.kind {
        Kind::Down => {
            let Some(h) = here(ses, ev.pos) else { return Ok(()) };
            let tol = view.tol(3.0);
            if let Some(id) = text_frame_at(ses, h.slot, h.p, tol) {
                let r = ses.renderer.clone();
                if let Some(st) = ses.active_mut() {
                    let lay = st.layout(&r);
                    if let Some(ct) = crate::celltext::hit(&st.doc, &lay, id, h.p) {
                        let mut sel = ct.selection();
                        if ev.mods.cmd
                            && let Some(prev) = st.selection.cells.filter(|x| x.story == ct.story && x.at == ct.at)
                            && let Some(cs) = sel.cells
                        {
                            let has = |r: &crate::session::CellSel| (r.r0.min(r.r1)..=r.r0.max(r.r1)).contains(&ct.row) && (r.c0.min(r.c1)..=r.c0.max(r.c1)).contains(&ct.col);
                            let mut more: Vec<_> = std::iter::once(prev).chain(st.selection.more_cells.iter().copied()).filter(|m| m.story == ct.story && m.at == ct.at).collect();
                            let before = more.len();
                            more.retain(|m| !has(m));
                            sel.cell_text = None;
                            if more.len() == before {
                                sel.more_cells = more;
                            } else if let Some(first) = more.first().copied() {
                                sel.cells = Some(first);
                                sel.more_cells = more[1..].to_vec();
                            } else {
                                sel.cells = Some(cs);
                            }
                        }
                        ses.select(sel);
                        return Ok(());
                    }
                }
                let Some((sid, pos)) = pos_at(ses, id, h.p) else { return Ok(()) };
                let cur = ses.active().and_then(|s| s.selection.text);
                let anchor = match cur {
                    Some(t) if ev.mods.shift && t.story == sid => t.anchor,
                    _ => pos,
                };
                ses.select(Selection::text(TextSel { story: sid, anchor, focus: pos }));
                start(ses, ev, &h, G::TextSelect { story: sid, anchor }, false);
                return Ok(());
            }
            let doc = ses.active().map(|s| s.doc.clone()).expect("checked");
            if let Some(id) = geo::hit_deep(&doc, h.slot, h.p, tol)
                && let Some(it) = doc.item(id)
                && it.shape.closed()
                && matches!(&it.kind, ItemKind::Plain | ItemKind::Graphic(formdesign_model::item::Graphic { asset: None, .. }))
            {
                ses.edit_now("Convert to Text Frame", |d, sel| {
                    let sid = d.new_story(formdesign_model::style::BASIC_PARA);
                    if let Some(x) = d.item_mut(id) {
                        x.kind = ItemKind::Text(TextFrame::new(sid));
                        x.style = Some(formdesign_model::style::BASIC_TEXT.into());
                    }
                    if let Some(s) = d.story_mut(sid) {
                        s.frames = vec![id];
                    }
                    *sel = Selection::text(TextSel::caret(sid, 0));
                });
                return Ok(());
            }
            let mut h = h;
            if view.snap {
                h.p = geo::snaps(&doc, h.slot, &[]).point(h.p, view.tol(6.0));
            }
            start(ses, ev, &h, G::Create { tool: "type".into() }, false);
        }
        Kind::Drag | Kind::Up => {
            if let Some(g) = ses.tools.gesture.clone()
                && let G::TextSelect { story, anchor } = g.kind
            {
                let p = g.slot_xf.inverse() * ev.pos;
                let frames = ses.active().and_then(|s| s.doc.story(story)).map(|s| s.frames.clone()).unwrap_or_default();
                let doc = ses.active().map(|s| s.doc.clone()).expect("checked");
                let target = frames.iter().copied().find(|f| doc.bounds(*f).is_some_and(|b| b.contains(p))).or_else(|| {
                    frames.iter().copied().filter(|f| geo::spread_of_item(&doc, *f) == Some(g.slot)).min_by(|a, b| {
                        let d = |f: &ItemId| doc.bounds(*f).map(|r| (r.center() - p).hypot()).unwrap_or(f64::MAX);
                        d(a).total_cmp(&d(b))
                    })
                });
                if let Some(f) = target
                    && let Some((_, pos)) = pos_at(ses, f, p)
                {
                    ses.select(Selection::text(TextSel { story, anchor, focus: pos }));
                }
                if ev.kind == Kind::Up {
                    ses.tools.gesture = None;
                }
                return Ok(());
            }
            if ev.kind == Kind::Drag {
                super::draw::drag(ses, ev, view);
            } else {
                super::draw::drag(ses, ev, view);
                super::draw::finish(ses, ev, view);
            }
        }
        Kind::Double => {
            let Some(h) = here(ses, ev.pos) else { return Ok(()) };
            if let Some(id) = text_frame_at(ses, h.slot, h.p, view.tol(3.0))
                && let Some((sid, pos)) = pos_at(ses, id, h.p)
                && let Some(st) = ses.active().and_then(|s| s.doc.story(sid))
            {
                let w = st.word_at(pos);
                ses.select(Selection::text(TextSel { story: sid, anchor: w.start, focus: w.end }));
            }
        }
        Kind::Move => {}
    }
    Ok(())
}

pub fn type_on_path(ses: &mut Session, ev: &Ev, view: View) -> Result<()> {
    if ev.kind != Kind::Down {
        return Ok(());
    }
    let Some(h) = here(ses, ev.pos) else { return Ok(()) };
    let doc = ses.active().map(|s| s.doc.clone()).expect("checked");
    let Some(id) = geo::hit_deep(&doc, h.slot, h.p, view.tol(5.0)) else { return Ok(()) };
    let Some(it) = doc.item(id) else { return Ok(()) };
    match &it.kind {
        ItemKind::Text(t) if t.on_path.is_some() => {
            let len = doc.story(t.story).map(|s| s.len()).unwrap_or(0);
            ses.select(Selection::text(TextSel::caret(t.story, len)));
        }
        ItemKind::Plain | ItemKind::Graphic(formdesign_model::item::Graphic { asset: None, .. }) => {
            ses.edit_now("Type on a Path", |d, sel| {
                let sid = d.new_story(formdesign_model::style::BASIC_PARA);
                if let Some(x) = d.item_mut(id) {
                    let mut tf = TextFrame::new(sid);
                    tf.on_path = Some(PathText { start: 0.0, flip: false, align: "baseline".into() });
                    x.kind = ItemKind::Text(tf);
                }
                if let Some(s) = d.story_mut(sid) {
                    s.frames = vec![id];
                }
                *sel = Selection::text(TextSel::caret(sid, 0));
            });
        }
        _ => {}
    }
    Ok(())
}

fn line_nav(ses: &mut Session, story: StoryId, pos: usize, dir: i32, edge: Option<bool>) -> Option<usize> {
    let r = ses.renderer.clone();
    let lay = ses.active_mut()?.layout(&r);
    let sl = lay.stories.get(&story)?;
    let flow = &sl.flow;
    let (para, byte) = flow.para_byte(pos)?;
    let lines = &flow.lines;
    let li = lines.iter().position(|l| l.para == para && byte >= l.line.start && byte < l.line.end).or_else(|| lines.iter().rposition(|l| l.para == para && byte >= l.line.start))?;
    let cur = &lines[li];
    if let Some(end) = edge {
        let b = if end { cur.line.end } else { cur.line.start };
        let p = flow.story_pos(cur.para, b);
        let next_starts_here = end && lines.get(li + 1).is_some_and(|n| n.para == cur.para);
        return Some(if next_starts_here { p.saturating_sub(1) } else { p });
    }
    let x = cur.line.carets.iter().min_by_key(|c| (c.0 as i64 - byte as i64).abs()).map(|c| c.1).unwrap_or(cur.line.left);
    let ti = li as i64 + dir as i64;
    if ti < 0 || ti as usize >= lines.len() {
        return Some(if dir < 0 { 0 } else { ses.active()?.doc.story(story)?.len() });
    }
    let t = &lines[ti as usize];
    let b = t.line.carets.iter().min_by(|a, c| (a.1 - x).abs().total_cmp(&(c.1 - x).abs())).map(|c| c.0).unwrap_or(t.line.start);
    Some(flow.story_pos(t.para, b))
}

fn word_step(text: &[char], pos: usize, forward: bool) -> usize {
    let mut k = pos;
    if forward {
        while k < text.len() && !text[k].is_alphanumeric() {
            k += 1;
        }
        while k < text.len() && text[k].is_alphanumeric() {
            k += 1;
        }
    } else {
        while k > 0 && !text[k - 1].is_alphanumeric() {
            k -= 1;
        }
        while k > 0 && text[k - 1].is_alphanumeric() {
            k -= 1;
        }
    }
    k
}

pub fn key(ses: &mut Session, key: Key, mods: Mods, _view: View) -> Result<bool> {
    if let Some(ct) = ses.active().and_then(|s| s.selection.cell_text) {
        return crate::celltext::key(ses, ct, key, mods);
    }
    let Some(t) = ses.active().and_then(|s| s.selection.text) else { return Ok(false) };
    let Some(story) = ses.active().and_then(|s| s.doc.story(t.story)).cloned() else { return Ok(false) };
    let len = story.len();
    let chars: Vec<char> = story.text().chars().collect();
    let word = mods.cmd || mods.alt;
    let moveto = |ses: &mut Session, p: usize| {
        let p = p.min(len);
        let anchor = if mods.shift { t.anchor } else { p };
        ses.select(Selection::text(TextSel { story: t.story, anchor, focus: p }));
    };
    match key {
        Key::Left => {
            let r = t.range();
            let p = if !mods.shift && !r.is_empty() { r.start } else if word { word_step(&chars, t.focus, false) } else { t.focus.saturating_sub(1) };
            moveto(ses, p);
        }
        Key::Right => {
            let r = t.range();
            let p = if !mods.shift && !r.is_empty() { r.end } else if word { word_step(&chars, t.focus, true) } else { t.focus + 1 };
            moveto(ses, p);
        }
        Key::Up | Key::Down => {
            let dir = if key == Key::Up { -1 } else { 1 };
            let p = if mods.cmd {
                let (pi, off) = story.locate(t.focus);
                if dir < 0 {
                    story.para_start(if off == 0 { pi.saturating_sub(1) } else { pi })
                } else if pi + 1 < story.paras.len() {
                    story.para_start(pi + 1)
                } else {
                    len
                }
            } else {
                line_nav(ses, t.story, t.focus, dir, None).unwrap_or(t.focus)
            };
            moveto(ses, p);
        }
        Key::Home | Key::End => {
            let end = key == Key::End;
            let p = if mods.cmd { if end { len } else { 0 } } else { line_nav(ses, t.story, t.focus, 0, Some(end)).unwrap_or(t.focus) };
            moveto(ses, p);
        }
        Key::PageUp => moveto(ses, 0),
        Key::PageDown => moveto(ses, len),
        Key::Backspace | Key::Delete => {
            ses.execute("text.delete", &json!({"forward": key == Key::Delete, "word": word}))?;
        }
        Key::Enter => {
            let s = if mods.shift { "\u{2028}" } else { "\n" };
            ses.execute("text.insert", &json!({"text": s, "raw": true}))?;
        }
        Key::Tab => {
            ses.execute("text.insert", &json!({"text": "\t", "raw": true}))?;
        }
        Key::Escape => {
            let frame = story.frames.first().copied();
            ses.tool = "selection".into();
            ses.select(Selection::items(frame.into_iter().collect()));
        }
    }
    Ok(true)
}

pub fn cursor(ses: &mut Session, at: Point, view: View) -> &'static str {
    let Some(h) = here(ses, at) else { return "text" };
    if text_frame_at(ses, h.slot, h.p, view.tol(3.0)).is_some() { "text" } else { "crosshair" }
}
