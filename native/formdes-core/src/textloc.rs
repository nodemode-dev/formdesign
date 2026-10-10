use std::ops::Range;

use formdes_engine::canvas::Canvas;
use formdes_engine::tools::geo;
use formdes_model::doc::Doc;
use formdes_model::geom::Affine;
use formdes_model::ids::{ItemId, StoryId};
use formdes_text::layout::DocLayout;
use formdes_model::story::{BreakKind, Inline, Marker, Para, Seg, Story};
use serde_json::{Value, json};

pub const FOOTNOTE_REF: char = '\u{E009}';
pub const TABLE_ANCHOR: char = '\u{E00A}';
pub const NOTE_MARK: char = '\u{E00F}';

pub fn structural(c: char) -> bool {
    ('\u{E009}'..='\u{E010}').contains(&c)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sub {
    Cell { at: usize, row: usize, col: usize },
    Note { n: usize },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Loc {
    pub story: StoryId,
    pub sub: Option<Sub>,
}

impl Loc {
    pub fn main(story: StoryId) -> Loc {
        Loc { story, sub: None }
    }

    pub fn json(&self) -> Value {
        let mut v = json!({"story": self.story.0});
        match self.sub {
            Some(Sub::Note { n }) => v["note"] = json!(n),
            Some(Sub::Cell { at, row, col }) => v["cell"] = json!({"table": at, "row": row, "col": col}),
            None => {}
        }
        v
    }

    pub fn parse(v: &Value) -> Option<Loc> {
        let story = StoryId(v.get("story")?.as_u64()?);
        let sub = if let Some(n) = v.get("note").and_then(Value::as_u64) {
            Some(Sub::Note { n: n as usize })
        } else if let Some(c) = v.get("cell").filter(|c| c.is_object()) {
            Some(Sub::Cell { at: c.get("table")?.as_u64()? as usize, row: c.get("row")?.as_u64()? as usize, col: c.get("col")?.as_u64()? as usize })
        } else {
            None
        };
        Some(Loc { story, sub })
    }
}

fn marker_char(i: &Inline) -> char {
    match i {
        Inline::Marker { marker: Marker::PageNumber } => '\u{E000}',
        Inline::Marker { marker: Marker::NextPageNumber } => '\u{E007}',
        Inline::Marker { marker: Marker::PrevPageNumber } => '\u{E008}',
        Inline::Marker { marker: Marker::SectionName } => '\u{E001}',
        Inline::Break { brk: BreakKind::Column } => '\u{E002}',
        Inline::Break { brk: BreakKind::Frame } => '\u{E003}',
        Inline::Break { .. } => '\u{E004}',
        Inline::IndentHere => '\u{E005}',
        Inline::RightTab => '\u{E006}',
        Inline::Footnote { .. } => FOOTNOTE_REF,
        Inline::Table { .. } => TABLE_ANCHOR,
        Inline::Anchored { .. } => '\u{E00E}',
        Inline::Note { .. } => NOTE_MARK,
        Inline::Variable { .. } | Inline::Index { .. } | Inline::Anchor { .. } | Inline::Xref { .. } => '\u{E010}',
    }
}

pub fn marker_text(paras: &[Para]) -> String {
    let mut s = String::new();
    for (i, p) in paras.iter().enumerate() {
        if i > 0 {
            s.push('\n');
        }
        for seg in &p.segs {
            match seg {
                Seg::Text { text, .. } => s.push_str(text),
                Seg::Inline { inline, .. } => s.push(marker_char(inline)),
            }
        }
    }
    s
}

fn as_story(id: StoryId, paras: Vec<Para>) -> Story {
    let mut st = Story::new(id, "Basic Paragraph");
    st.paras = if paras.is_empty() { vec![Para::empty("Basic Paragraph")] } else { paras };
    st
}

fn notes(st: &Story) -> Vec<(usize, &Vec<Para>)> {
    st.inlines().into_iter().filter_map(|(at, i)| if let Inline::Footnote { paras } = i { Some((at, paras)) } else { None }).collect()
}

pub fn anchor_of(doc: &Doc, loc: Loc) -> Option<usize> {
    let st = doc.story(loc.story)?;
    match loc.sub? {
        Sub::Cell { at, .. } => Some(at),
        Sub::Note { n } => notes(st).get(n).map(|x| x.0),
    }
}

pub fn view(doc: &Doc, loc: Loc) -> Option<Story> {
    let st = doc.story(loc.story)?;
    match loc.sub {
        None => Some((**doc.stories.get(&loc.story)?).clone()),
        Some(Sub::Note { n }) => notes(st).get(n).map(|(_, p)| as_story(loc.story, (*p).clone())),
        Some(Sub::Cell { at, row, col }) => st.inlines().into_iter().find_map(|(pos, i)| match i {
            Inline::Table { table } if pos == at => table.cell(row, col).map(|c| as_story(loc.story, c.paras.clone())),
            _ => None,
        }),
    }
}

pub fn put(doc: &mut Doc, loc: Loc, story: Story) {
    let Some(st) = doc.story_mut(loc.story) else { return };
    match loc.sub {
        None => st.paras = story.paras,
        Some(Sub::Note { n }) => {
            let Some(at) = notes(st).get(n).map(|x| x.0) else { return };
            if let Some(Inline::Footnote { paras }) = st.inline_at_mut(at) {
                *paras = story.paras;
            }
        }
        Some(Sub::Cell { at, row, col }) => {
            if let Some(Inline::Table { table }) = st.inline_at_mut(at)
                && let Some(c) = table.cell_mut(row, col)
            {
                c.paras = story.paras;
            }
        }
    }
}

pub fn locs(doc: &Doc, sid: StoryId) -> Vec<Loc> {
    let Some(st) = doc.story(sid) else { return Vec::new() };
    let mut out = vec![Loc::main(sid)];
    for (at, i) in st.inlines() {
        if let Inline::Table { table } = i {
            for r in 0..table.rows() {
                for c in 0..table.cols() {
                    if table.cell(r, c).is_some_and(|x| x.row == r && x.col == c && x.paras.iter().any(|p| !p.is_empty())) {
                        out.push(Loc { story: sid, sub: Some(Sub::Cell { at, row: r, col: c }) });
                    }
                }
            }
        }
    }
    for (n, _) in notes(st).iter().enumerate() {
        out.push(Loc { story: sid, sub: Some(Sub::Note { n }) });
    }
    out
}

pub struct Span {
    pub frame: ItemId,
    pub m: Affine,
    pub x0: f64,
    pub x1: f64,
    pub base: f64,
    pub asc: f64,
    pub desc: f64,
}

fn caret_x(carets: &[(usize, f64)], byte: usize, left: f64) -> f64 {
    match carets.binary_search_by_key(&byte, |c| c.0) {
        Ok(i) => carets[i].1,
        Err(0) => carets.first().map(|c| c.1).unwrap_or(left),
        Err(i) => carets[i - 1].1,
    }
}

pub fn spans(doc: &Doc, lay: &DocLayout, cv: &Canvas, sid: StoryId, r: Range<usize>) -> Vec<Span> {
    let (Some(st), Some(sl)) = (doc.story(sid), lay.stories.get(&sid)) else { return Vec::new() };
    let flow = &sl.flow;
    let mut out = Vec::new();
    let pi = st.paras_in(r.clone());
    for p in pi {
        let ps = st.para_start(p);
        let plen = st.paras[p].len();
        let (a, b) = (r.start.max(ps) - ps, r.end.min(ps + plen).max(r.start.max(ps)) - ps);
        let map = flow.para_maps.get(p);
        let tlen = flow.para_texts.get(p).map_or(0, String::len);
        let byte = |c: usize| map.and_then(|m| m.get(c).copied()).unwrap_or(tlen);
        let (ba, bb) = (byte(a), byte(b));
        for l in flow.lines.iter().filter(|l| l.para == p) {
            let (s0, e0) = (ba.max(l.line.start), bb.min(l.line.end));
            if s0 > e0 || (s0 == e0 && ba != bb) {
                continue;
            }
            let Some(&frame) = sl.frames.get(l.frame) else { continue };
            let Some(r) = geo::spread_of_item(doc, doc.top_of(frame)) else { continue };
            let Some(slot) = cv.slot(r) else { continue };
            let m = slot.xf * doc.world_xf(frame);
            out.push(Span { frame, m, x0: caret_x(&l.line.carets, s0, l.line.left), x1: caret_x(&l.line.carets, e0, l.line.left), base: l.baseline, asc: l.line.ascent, desc: l.line.descent });
        }
    }
    out
}

pub fn byte_to_char(t: &str, b: usize) -> usize {
    t.char_indices().take_while(|(i, _)| *i < b).count()
}

pub fn char_to_byte(t: &str, c: usize) -> usize {
    t.char_indices().nth(c).map(|x| x.0).unwrap_or(t.len())
}
