use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::attrs::Attrs;
use crate::ids::{ItemId, StoryId};
use crate::{Result, err};

pub const OBJ: char = '\u{FFFC}';
pub const LINE_BREAK: char = '\u{2028}';

pub type TextPos = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BreakKind {
    Column,
    Frame,
    Page,
    OddPage,
    EvenPage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Marker {
    PageNumber,
    NextPageNumber,
    PrevPageNumber,
    SectionName,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableCell {
    pub row: usize,
    pub col: usize,
    #[serde(default = "one")]
    pub rows: usize,
    #[serde(default = "one")]
    pub cols: usize,
    pub paras: Vec<Para>,
    #[serde(default)]
    pub attrs: Attrs,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
}

fn one() -> usize {
    1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Table {
    pub widths: Vec<f64>,
    pub heights: Vec<f64>,
    #[serde(default)]
    pub exact: Vec<bool>,
    pub cells: Vec<TableCell>,
    #[serde(default)]
    pub header_rows: usize,
    #[serde(default)]
    pub footer_rows: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    #[serde(default)]
    pub attrs: Attrs,
}

impl Table {
    pub fn new(rows: usize, cols: usize, width: f64) -> Table {
        let rows = rows.max(1);
        let cols = cols.max(1);
        let mut cells = Vec::new();
        for r in 0..rows {
            for c in 0..cols {
                cells.push(TableCell { row: r, col: c, rows: 1, cols: 1, paras: vec![Para::empty("Basic Paragraph")], attrs: Attrs::new(), style: None });
            }
        }
        Table { widths: vec![width / cols as f64; cols], heights: vec![18.0; rows], exact: vec![false; rows], cells, header_rows: 0, footer_rows: 0, style: None, attrs: Attrs::new() }
    }

    pub fn rows(&self) -> usize {
        self.heights.len()
    }

    pub fn cols(&self) -> usize {
        self.widths.len()
    }

    pub fn cell(&self, row: usize, col: usize) -> Option<&TableCell> {
        self.cells.iter().find(|c| row >= c.row && row < c.row + c.rows && col >= c.col && col < c.col + c.cols)
    }

    pub fn cell_mut(&mut self, row: usize, col: usize) -> Option<&mut TableCell> {
        self.cells.iter_mut().find(|c| row >= c.row && row < c.row + c.rows && col >= c.col && col < c.col + c.cols)
    }

    pub fn width(&self) -> f64 {
        self.widths.iter().sum()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Inline {
    Anchored { item: ItemId, #[serde(default)] attrs: Attrs },
    Footnote { paras: Vec<Para> },
    Table { table: Box<Table> },
    Variable { name: String },
    Marker { marker: Marker },
    Break { brk: BreakKind },
    IndentHere,
    RightTab,
    Note {
        id: u64,
        #[serde(default)]
        author: String,
        text: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "camelCase")]
pub enum Seg {
    Text {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        style: Option<String>,
        #[serde(default, skip_serializing_if = "Attrs::is_empty")]
        over: Attrs,
    },
    Inline {
        inline: Inline,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        style: Option<String>,
        #[serde(default, skip_serializing_if = "Attrs::is_empty")]
        over: Attrs,
    },
}

impl Seg {
    pub fn text(text: &str) -> Seg {
        Seg::Text { text: text.into(), style: None, over: Attrs::new() }
    }

    pub fn len(&self) -> usize {
        match self {
            Seg::Text { text, .. } => text.chars().count(),
            Seg::Inline { .. } => 1,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn style(&self) -> Option<&str> {
        match self {
            Seg::Text { style, .. } | Seg::Inline { style, .. } => style.as_deref(),
        }
    }

    pub fn over(&self) -> &Attrs {
        match self {
            Seg::Text { over, .. } | Seg::Inline { over, .. } => over,
        }
    }

    pub fn format_mut(&mut self) -> (&mut Option<String>, &mut Attrs) {
        match self {
            Seg::Text { style, over, .. } | Seg::Inline { style, over, .. } => (style, over),
        }
    }

    fn same_format(&self, other: &Seg) -> bool {
        self.style() == other.style() && self.over() == other.over()
    }

    fn with_text(&self, t: String) -> Seg {
        Seg::Text { text: t, style: self.style().map(str::to_string), over: self.over().clone() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Para {
    pub style: String,
    #[serde(default, skip_serializing_if = "Attrs::is_empty")]
    pub over: Attrs,
    pub segs: Vec<Seg>,
}

pub type Run = Seg;

impl Para {
    pub fn empty(style: &str) -> Para {
        Para { style: style.into(), over: Attrs::new(), segs: vec![Seg::text("")] }
    }

    pub fn from_text(style: &str, text: &str) -> Para {
        Para { style: style.into(), over: Attrs::new(), segs: vec![Seg::text(text)] }
    }

    pub fn len(&self) -> usize {
        self.segs.iter().map(Seg::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn text(&self) -> String {
        let mut s = String::new();
        for seg in &self.segs {
            match seg {
                Seg::Text { text, .. } => s.push_str(text),
                Seg::Inline { .. } => s.push(OBJ),
            }
        }
        s
    }

    pub fn plain(&self) -> String {
        let mut s = String::new();
        for seg in &self.segs {
            match seg {
                Seg::Text { text, .. } => s.push_str(text),
                Seg::Inline { inline: Inline::Variable { name }, .. } => s.push_str(&format!("<{name}>")),
                Seg::Inline { inline: Inline::Marker { .. }, .. } => s.push('#'),
                Seg::Inline { inline: Inline::RightTab, .. } => s.push('\t'),
                Seg::Inline { .. } => {}
            }
        }
        s
    }

    fn split_at(&mut self, at: usize) -> Vec<Seg> {
        let mut left = Vec::new();
        let mut right = Vec::new();
        let mut pos = 0;
        for seg in self.segs.drain(..) {
            let len = seg.len();
            if pos + len <= at {
                left.push(seg);
            } else if pos >= at {
                right.push(seg);
            } else {
                let Seg::Text { text, .. } = &seg else { unreachable!() };
                let cut = at - pos;
                let a: String = text.chars().take(cut).collect();
                let b: String = text.chars().skip(cut).collect();
                left.push(seg.with_text(a));
                right.push(seg.with_text(b));
            }
            pos += len;
        }
        self.segs = left;
        right
    }

    pub fn format_at(&self, at: usize) -> (Option<String>, Attrs) {
        let fmt = |s: &Seg| (s.style().map(str::to_string), s.over().clone());
        let target = at.saturating_sub(1);
        let mut pos = 0;
        for seg in &self.segs {
            let len = seg.len();
            if at == 0 || (len > 0 && target < pos + len) {
                return fmt(seg);
            }
            pos += len;
        }
        self.segs.last().map(fmt).unwrap_or((None, Attrs::new()))
    }

    pub fn normalize(&mut self) {
        let mut out: Vec<Seg> = Vec::with_capacity(self.segs.len());
        for seg in self.segs.drain(..) {
            let mergeable = matches!(&seg, Seg::Text { .. }) && matches!(out.last(), Some(p @ Seg::Text { .. }) if p.same_format(&seg));
            if mergeable
                && let (Some(Seg::Text { text: prev, .. }), Seg::Text { text, .. }) = (out.last_mut(), &seg)
            {
                prev.push_str(text);
                continue;
            }
            out.push(seg);
        }
        if out.len() > 1 {
            out.retain(|s| !matches!(s, Seg::Text { text, .. } if text.is_empty()));
        }
        if out.is_empty() {
            out.push(Seg::text(""));
        }
        self.segs = out;
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Story {
    pub id: StoryId,
    pub frames: Vec<ItemId>,
    pub paras: Vec<Para>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub xml_tag: Option<String>,
}

impl Story {
    pub fn new(id: StoryId, style: &str) -> Story {
        Story { id, frames: Vec::new(), paras: vec![Para::empty(style)], xml_tag: None }
    }

    pub fn len(&self) -> usize {
        let n: usize = self.paras.iter().map(Para::len).sum();
        n + self.paras.len().saturating_sub(1)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn text(&self) -> String {
        self.paras.iter().map(Para::text).collect::<Vec<_>>().join("\n")
    }

    pub fn plain(&self) -> String {
        self.paras.iter().map(Para::plain).collect::<Vec<_>>().join("\n")
    }

    pub fn locate(&self, pos: TextPos) -> (usize, usize) {
        let mut p = pos;
        for (i, para) in self.paras.iter().enumerate() {
            let len = para.len();
            if p <= len {
                return (i, p);
            }
            p -= len + 1;
        }
        let last = self.paras.len() - 1;
        (last, self.paras[last].len())
    }

    pub fn para_start(&self, index: usize) -> TextPos {
        self.paras.iter().take(index).map(|p| p.len() + 1).sum()
    }

    pub fn para_range(&self, index: usize) -> std::ops::Range<TextPos> {
        let s = self.para_start(index);
        s..s + self.paras.get(index).map_or(0, Para::len)
    }

    pub fn paras_in(&self, range: std::ops::Range<TextPos>) -> std::ops::RangeInclusive<usize> {
        let (a, _) = self.locate(range.start);
        let (b, _) = self.locate(range.end.max(range.start));
        a..=b
    }

    pub fn set_text(&mut self, text: &str) {
        let style = self.paras.first().map(|p| p.style.clone()).unwrap_or_else(|| "Basic Paragraph".into());
        self.paras = vec![Para::empty(&style)];
        self.insert_text(0, text);
    }

    pub fn insert_text(&mut self, pos: TextPos, text: &str) {
        if text.is_empty() {
            return;
        }
        let (pi, off) = self.locate(pos.min(self.len()));
        let (cstyle, cover) = self.paras[pi].format_at(off);
        let mut parts = text.split('\n');
        let first = parts.next().unwrap_or("");
        let rest: Vec<&str> = parts.collect();
        let tail = self.paras[pi].split_at(off);
        let seg = |t: &str| Seg::Text { text: t.to_string(), style: cstyle.clone(), over: cover.clone() };
        self.paras[pi].segs.push(seg(first));
        if rest.is_empty() {
            self.paras[pi].segs.extend(tail);
            self.paras[pi].normalize();
            return;
        }
        self.paras[pi].normalize();
        let style = self.paras[pi].style.clone();
        let over = self.paras[pi].over.clone();
        let mut new_paras = Vec::new();
        for (i, t) in rest.iter().enumerate() {
            let mut p = Para { style: style.clone(), over: over.clone(), segs: vec![seg(t)] };
            if i == rest.len() - 1 {
                p.segs.extend(tail.clone());
            }
            p.normalize();
            new_paras.push(p);
        }
        let at = pi + 1;
        self.paras.splice(at..at, new_paras);
    }

    pub fn insert_inline(&mut self, pos: TextPos, inline: Inline) {
        let (pi, off) = self.locate(pos.min(self.len()));
        let (style, over) = self.paras[pi].format_at(off);
        let tail = self.paras[pi].split_at(off);
        self.paras[pi].segs.push(Seg::Inline { inline, style, over });
        self.paras[pi].segs.extend(tail);
        self.paras[pi].normalize();
    }

    pub fn delete(&mut self, range: std::ops::Range<TextPos>) {
        let len = self.len();
        let (a, b) = (range.start.min(len), range.end.min(len));
        if a >= b {
            return;
        }
        let (pa, oa) = self.locate(a);
        let (pb, ob) = self.locate(b);
        let tail = self.paras[pb].split_at(ob);
        if pa != pb {
            self.paras.drain(pa + 1..=pb);
        }
        let _ = self.paras[pa].split_at(oa);
        self.paras[pa].segs.extend(tail);
        self.paras[pa].normalize();
    }

    pub fn slice(&self, range: std::ops::Range<TextPos>) -> Vec<Para> {
        let mut copy = self.clone();
        let len = copy.len();
        copy.delete(range.end.min(len)..len);
        copy.delete(0..range.start.min(range.end));
        copy.paras
    }

    pub fn insert_paras(&mut self, pos: TextPos, paras: Vec<Para>) {
        if paras.is_empty() {
            return;
        }
        let (pi, off) = self.locate(pos.min(self.len()));
        let tail = self.paras[pi].split_at(off);
        let mut paras = paras;
        let first = paras.remove(0);
        if self.paras[pi].is_empty() && off == 0 {
            self.paras[pi].style = first.style.clone();
            self.paras[pi].over = first.over.clone();
        }
        self.paras[pi].segs.extend(first.segs);
        if paras.is_empty() {
            self.paras[pi].segs.extend(tail);
            self.paras[pi].normalize();
            return;
        }
        self.paras[pi].normalize();
        let n = paras.len();
        if let Some(last) = paras.last_mut() {
            last.segs.extend(tail);
            last.normalize();
        }
        self.paras.splice(pi + 1..pi + 1, paras);
        let _ = n;
    }

    fn split_range(&mut self, range: std::ops::Range<TextPos>) {
        for edge in [range.end, range.start] {
            let (pi, off) = self.locate(edge);
            let tail = self.paras[pi].split_at(off);
            self.paras[pi].segs.extend(tail);
        }
    }

    pub fn for_segs_in(&mut self, range: std::ops::Range<TextPos>, mut f: impl FnMut(&mut Seg)) {
        if range.start >= range.end {
            return;
        }
        self.split_range(range.clone());
        let mut pos = 0;
        for para in self.paras.iter_mut() {
            for seg in para.segs.iter_mut() {
                let len = seg.len();
                if pos >= range.start && pos + len <= range.end && len > 0 {
                    f(seg);
                }
                pos += len;
            }
            pos += 1;
            para.normalize();
        }
    }

    pub fn set_char_attrs(&mut self, range: std::ops::Range<TextPos>, attrs: &serde_json::Map<String, Value>) -> Result<()> {
        let mut checked = Attrs::new();
        crate::attrs::set_checked(&mut checked, attrs, Some(crate::attrs::Level::Char))?;
        let clears: Vec<String> = attrs.iter().filter(|(_, v)| v.is_null()).map(|(k, _)| k.clone()).collect();
        self.for_segs_in(range, |seg| {
            let (_, over) = seg.format_mut();
            for (k, v) in &checked {
                over.insert(k.clone(), v.clone());
            }
            for k in &clears {
                over.remove(k);
            }
        });
        Ok(())
    }

    pub fn set_char_style(&mut self, range: std::ops::Range<TextPos>, style: Option<&str>, clear: bool) {
        let st = style.map(crate::style::style_key).filter(|s| s != crate::style::NO_CHAR);
        self.for_segs_in(range, |seg| {
            let (s, over) = seg.format_mut();
            *s = st.clone();
            if clear {
                over.clear();
            }
        });
    }

    pub fn set_para_style(&mut self, paras: std::ops::RangeInclusive<usize>, style: &str, clear: bool) {
        let st = crate::style::style_key(style);
        for i in paras {
            if let Some(p) = self.paras.get_mut(i) {
                p.style = st.clone();
                if clear {
                    p.over.clear();
                    for seg in p.segs.iter_mut() {
                        seg.format_mut().1.clear();
                    }
                    p.normalize();
                }
            }
        }
    }

    pub fn set_para_attrs(&mut self, paras: std::ops::RangeInclusive<usize>, attrs: &serde_json::Map<String, Value>) -> Result<()> {
        for i in paras {
            let Some(p) = self.paras.get_mut(i) else { continue };
            crate::attrs::set_checked(&mut p.over, attrs, Some(crate::attrs::Level::Para))?;
        }
        Ok(())
    }

    pub fn inlines(&self) -> Vec<(TextPos, &Inline)> {
        let mut out = Vec::new();
        let mut pos = 0;
        for para in &self.paras {
            for seg in &para.segs {
                if let Seg::Inline { inline, .. } = seg {
                    out.push((pos, inline));
                }
                pos += seg.len();
            }
            pos += 1;
        }
        out
    }

    pub fn inline_at_mut(&mut self, at: TextPos) -> Option<&mut Inline> {
        let mut pos = 0;
        for para in self.paras.iter_mut() {
            for seg in para.segs.iter_mut() {
                let len = seg.len();
                if pos == at
                    && let Seg::Inline { inline, .. } = seg
                {
                    return Some(inline);
                }
                pos += len;
            }
            pos += 1;
        }
        None
    }

    pub fn anchored_items(&self) -> Vec<ItemId> {
        self.inlines().into_iter().filter_map(|(_, i)| if let Inline::Anchored { item, .. } = i { Some(*item) } else { None }).collect()
    }

    pub fn find_all(&self, needle: &str, case: bool) -> Vec<std::ops::Range<TextPos>> {
        if needle.is_empty() {
            return Vec::new();
        }
        let hay: Vec<char> = self.text().chars().collect();
        let n: Vec<char> = needle.chars().collect();
        let eq = |a: char, b: char| if case { a == b } else { a.to_lowercase().eq(b.to_lowercase()) };
        let mut out = Vec::new();
        let mut i = 0;
        while i + n.len() <= hay.len() {
            if hay[i..i + n.len()].iter().zip(n.iter()).all(|(a, b)| eq(*a, *b)) {
                out.push(i..i + n.len());
                i += n.len();
            } else {
                i += 1;
            }
        }
        out
    }

    pub fn word_at(&self, pos: TextPos) -> std::ops::Range<TextPos> {
        let chars: Vec<char> = self.text().chars().collect();
        let is_word = |c: char| c.is_alphanumeric() || c == '\'' || c == '\u{2019}' || c == '-';
        let mut a = pos.min(chars.len());
        let mut b = a;
        while a > 0 && is_word(chars[a - 1]) {
            a -= 1;
        }
        while b < chars.len() && is_word(chars[b]) {
            b += 1;
        }
        a..b
    }

    pub fn check(&self) -> Result<()> {
        if self.paras.is_empty() {
            return err("a story needs at least one paragraph");
        }
        Ok(())
    }
}
