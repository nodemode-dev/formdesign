use formdes_model::doc::Doc;
use formdes_model::geom::Point;
use formdes_model::ids::{ItemId, StoryId};
use formdes_model::story::{Inline, Para, Story};
use formdes_text::flow::CellOut;
use formdes_text::layout::DocLayout;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::session::{CellSel, Selection, Session};
use crate::tools::{Key, Mods};
use crate::{EngineError, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellText {
    pub story: StoryId,
    pub at: usize,
    pub row: usize,
    pub col: usize,
    pub anchor: usize,
    pub focus: usize,
}

impl CellText {
    pub fn range(&self) -> std::ops::Range<usize> {
        self.anchor.min(self.focus)..self.anchor.max(self.focus)
    }

    pub fn caret(self, p: usize) -> CellText {
        CellText { anchor: p, focus: p, ..self }
    }

    pub fn selection(self) -> Selection {
        Selection { cells: Some(CellSel { story: self.story, at: self.at, r0: self.row, c0: self.col, r1: self.row, c1: self.col }), cell_text: Some(self), ..Selection::default() }
    }
}

pub fn paras(doc: &Doc, ct: &CellText) -> Option<Vec<Para>> {
    let st = doc.story(ct.story)?;
    st.inlines().into_iter().find_map(|(p, i)| match i {
        Inline::Table { table } if p == ct.at => table.cell(ct.row, ct.col).map(|c| c.paras.clone()),
        _ => None,
    })
}

pub fn set_paras(doc: &mut Doc, ct: &CellText, paras: Vec<Para>) -> bool {
    let Some(st) = doc.story_mut(ct.story) else { return false };
    let Some(Inline::Table { table }) = st.inline_at_mut(ct.at) else { return false };
    let Some(cell) = table.cell_mut(ct.row, ct.col) else { return false };
    cell.paras = if paras.is_empty() { vec![Para::empty(formdes_model::style::BASIC_PARA)] } else { paras };
    true
}

fn scratch(story: StoryId, paras: Vec<Para>) -> Story {
    let mut s = Story::new(story, formdes_model::style::BASIC_PARA);
    if !paras.is_empty() {
        s.paras = paras;
    }
    s
}

fn byte_to_char(text: &str, b: usize) -> usize {
    text.char_indices().take_while(|(i, _)| *i < b).count()
}

fn char_to_byte(text: &str, c: usize) -> usize {
    text.char_indices().nth(c).map(|(i, _)| i).unwrap_or(text.len())
}

fn cell_out<'a>(lay: &'a DocLayout, ct: &CellText) -> Option<(ItemId, &'a CellOut)> {
    let sl = lay.stories.get(&ct.story)?;
    let (para, _) = sl.flow.para_byte(ct.at)?;
    for frag in sl.flow.tables.iter().filter(|t| t.para == para) {
        if let Some(c) = frag.cells.iter().find(|c| c.row == ct.row && c.col == ct.col) {
            let fid = *sl.frames.get(frag.frame)?;
            return Some((fid, c));
        }
    }
    None
}

pub fn hit(doc: &Doc, lay: &DocLayout, frame: ItemId, p: Point) -> Option<CellText> {
    let sid = doc.item(frame)?.story()?;
    let sl = lay.stories.get(&sid)?;
    let fi = sl.frames.iter().position(|f| *f == frame)?;
    let local = doc.world_xf(frame).inverse() * p;
    let st = doc.story(sid)?;
    for frag in sl.flow.tables.iter().filter(|t| t.frame == fi) {
        let Some(cell) = frag.cells.iter().find(|c| c.rect.contains(local)) else { continue };
        let at = st.inlines().into_iter().filter(|(_, i)| matches!(i, Inline::Table { .. })).map(|(a, _)| a).find(|a| sl.flow.para_byte(*a).is_some_and(|(pp, _)| pp == frag.para))?;
        let mut ct = CellText { story: sid, at, row: cell.row, col: cell.col, anchor: 0, focus: 0 };
        let ps = paras(doc, &ct)?;
        let pos = pos_in(cell, &ps, local);
        ct.anchor = pos;
        ct.focus = pos;
        return Some(ct);
    }
    None
}

fn pos_in(cell: &CellOut, ps: &[Para], local: Point) -> usize {
    let Some(line) = cell.lines.iter().min_by(|a, b| {
        let d = |l: &formdes_text::flow::PlacedLine| if local.y < l.baseline - l.line.ascent { l.baseline - l.line.ascent - local.y } else if local.y > l.baseline + l.line.descent { local.y - l.baseline - l.line.descent } else { 0.0 };
        d(a).total_cmp(&d(b))
    }) else {
        return ps.iter().map(|p| p.len() + 1).sum::<usize>().saturating_sub(1);
    };
    let byte = line.line.carets.iter().min_by(|a, b| (a.1 - local.x).abs().total_cmp(&(b.1 - local.x).abs())).map(|c| c.0).unwrap_or(line.line.start);
    let base: usize = ps.iter().take(line.para).map(|p| p.len() + 1).sum();
    let text = ps.get(line.para).map(Para::text).unwrap_or_default();
    base + byte_to_char(&text, byte)
}

pub fn caret_line(doc: &Doc, lay: &DocLayout, ct: &CellText, pos: usize) -> Option<(ItemId, f64, f64, f64)> {
    let (fid, cell) = cell_out(lay, ct)?;
    let ps = paras(doc, ct)?;
    let st = scratch(ct.story, ps.clone());
    let (pi, off) = st.locate(pos.min(st.len()));
    let text = ps.get(pi).map(Para::text).unwrap_or_default();
    let byte = char_to_byte(&text, off);
    let lines: Vec<&formdes_text::flow::PlacedLine> = cell.lines.iter().filter(|l| l.para == pi).collect();
    let line = lines.iter().find(|l| byte < l.line.end).or(lines.last()).copied();
    match line {
        Some(l) => {
            let x = l.line.carets.iter().min_by_key(|c| (c.0 as i64 - byte as i64).unsigned_abs()).map(|c| c.1).unwrap_or(l.line.left);
            Some((fid, x, l.baseline - l.line.ascent, l.baseline + l.line.descent))
        }
        None => Some((fid, cell.rect.x0 + 4.0, cell.rect.y0 + 3.0, cell.rect.y0 + 14.0)),
    }
}

pub fn quads(doc: &Doc, lay: &DocLayout, ct: &CellText) -> Option<(ItemId, Vec<(f64, f64, f64, f64)>)> {
    let (fid, cell) = cell_out(lay, ct)?;
    let ps = paras(doc, ct)?;
    let st = scratch(ct.story, ps.clone());
    let r = ct.range();
    let (p0, o0) = st.locate(r.start);
    let (p1, o1) = st.locate(r.end);
    let mut out = Vec::new();
    for l in &cell.lines {
        if l.para < p0 || l.para > p1 {
            continue;
        }
        let t = ps.get(l.para).map(Para::text).unwrap_or_default();
        let b0 = if l.para == p0 { char_to_byte(&t, o0).max(l.line.start) } else { l.line.start };
        let b1 = if l.para == p1 { char_to_byte(&t, o1).min(l.line.end) } else { l.line.end };
        if b1 <= b0 {
            continue;
        }
        let x = |b: usize| l.line.carets.iter().min_by_key(|c| (c.0 as i64 - b as i64).unsigned_abs()).map(|c| c.1).unwrap_or(l.line.left);
        out.push((x(b0), x(b1), l.baseline - l.line.ascent, l.baseline + l.line.descent));
    }
    Some((fid, out))
}

pub fn edit(doc: &mut Doc, ct: &CellText, f: impl FnOnce(&mut Story) -> usize) -> Result<usize> {
    let ps = paras(doc, ct).ok_or_else(|| EngineError::new("the table cell is gone"))?;
    let mut st = scratch(ct.story, ps);
    let caret = f(&mut st);
    if !set_paras(doc, ct, st.paras) {
        return Err(EngineError::new("the table cell is gone"));
    }
    Ok(caret)
}

fn cell_count(doc: &Doc, ct: &CellText) -> Option<(usize, usize)> {
    let st = doc.story(ct.story)?;
    st.inlines().into_iter().find_map(|(p, i)| match i {
        Inline::Table { table } if p == ct.at => Some((table.rows(), table.cols())),
        _ => None,
    })
}

fn len_of(doc: &Doc, ct: &CellText) -> usize {
    paras(doc, ct).map(|ps| scratch(ct.story, ps).len()).unwrap_or(0)
}

pub fn key(ses: &mut Session, ct: CellText, key: Key, mods: Mods) -> Result<bool> {
    let Some(doc) = ses.active().map(|s| s.doc.clone()) else { return Ok(false) };
    let len = len_of(&doc, &ct);
    let text: Vec<char> = paras(&doc, &ct).map(|ps| scratch(ct.story, ps).text().chars().collect()).unwrap_or_default();
    let moveto = |ses: &mut Session, p: usize| {
        let p = p.min(len);
        let anchor = if mods.shift { ct.anchor } else { p };
        ses.select(CellText { anchor, focus: p, ..ct }.selection());
    };
    let goto_cell = |ses: &mut Session, row: usize, col: usize, end: bool| {
        let n = CellText { row, col, anchor: 0, focus: 0, ..ct };
        let l = len_of(&doc, &n);
        let p = if end { l } else { 0 };
        let sel = if end { n.caret(p) } else { CellText { anchor: 0, focus: l, ..n } };
        ses.select(sel.selection());
    };
    let (rows, cols) = cell_count(&doc, &ct).unwrap_or((1, 1));
    match key {
        Key::Left => {
            let r = ct.range();
            moveto(ses, if !mods.shift && !r.is_empty() { r.start } else { ct.focus.saturating_sub(1) });
        }
        Key::Right => {
            let r = ct.range();
            moveto(ses, if !mods.shift && !r.is_empty() { r.end } else { ct.focus + 1 });
        }
        Key::Home | Key::PageUp => moveto(ses, 0),
        Key::End | Key::PageDown => moveto(ses, len),
        Key::Up | Key::Down => {
            let up = key == Key::Up;
            let line_start = text[..ct.focus.min(text.len())].iter().rposition(|c| *c == '\n').map(|i| i + 1).unwrap_or(0);
            let line_end = text[ct.focus.min(text.len())..].iter().position(|c| *c == '\n').map(|i| ct.focus + i).unwrap_or(text.len());
            if up && line_start > 0 {
                moveto(ses, line_start - 1);
            } else if !up && line_end < text.len() {
                moveto(ses, line_end + 1);
            } else if up && ct.row > 0 {
                goto_cell(ses, ct.row - 1, ct.col, true);
            } else if !up && ct.row + 1 < rows {
                goto_cell(ses, ct.row + 1, ct.col, true);
            }
        }
        Key::Tab => {
            let idx = ct.row * cols + ct.col;
            let next = if mods.shift { idx.checked_sub(1) } else { Some(idx + 1).filter(|i| *i < rows * cols) };
            match next {
                Some(i) => goto_cell(ses, i / cols, i % cols, false),
                None if !mods.shift => {
                    ses.execute("table.insertRowBelow", &json!({}))?;
                    goto_cell(ses, ct.row + 1, 0, false);
                }
                None => {}
            }
        }
        Key::Backspace | Key::Delete => {
            ses.execute("text.delete", &json!({"forward": key == Key::Delete, "word": mods.cmd || mods.alt}))?;
        }
        Key::Enter => {
            ses.execute("text.insert", &json!({"text": "\n", "raw": true}))?;
        }
        Key::Escape => {
            ses.select(Selection { cells: Some(CellSel { story: ct.story, at: ct.at, r0: ct.row, c0: ct.col, r1: ct.row, c1: ct.col }), ..Selection::default() });
        }
    }
    Ok(true)
}

pub fn insert(ses_doc: &mut Doc, ct: &CellText, text: &str) -> Result<(usize, usize)> {
    let r = ct.range();
    let before = len_of(ses_doc, ct);
    let at = edit(ses_doc, ct, |st| {
        if !r.is_empty() {
            st.delete(r.clone());
        }
        crate::cmd::object::put_text(st, r.start, text);
        r.start
    })?;
    let after = len_of(ses_doc, ct);
    let n = (after + r.len()).saturating_sub(before);
    Ok((at + n, n))
}

pub fn delete(doc: &mut Doc, ct: &CellText, forward: bool, word: bool) -> Result<usize> {
    let len = len_of(doc, ct);
    let text: Vec<char> = paras(doc, ct).map(|ps| scratch(ct.story, ps).text().chars().collect()).unwrap_or_default();
    let mut r = ct.range();
    if r.is_empty() {
        let at = r.start;
        r = if word {
            let mut k = at;
            if forward {
                while k < text.len() && !text[k].is_alphanumeric() {
                    k += 1;
                }
                while k < text.len() && text[k].is_alphanumeric() {
                    k += 1;
                }
                at..k
            } else {
                while k > 0 && !text[k - 1].is_alphanumeric() {
                    k -= 1;
                }
                while k > 0 && text[k - 1].is_alphanumeric() {
                    k -= 1;
                }
                k..at
            }
        } else if forward {
            at..(at + 1).min(len)
        } else {
            at.saturating_sub(1)..at
        };
    }
    if r.is_empty() {
        return Ok(r.start);
    }
    edit(doc, ct, |st| {
        st.delete(r.clone());
        r.start
    })
}
