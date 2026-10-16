use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use formdes_model::attrs::{ListKind, Start};
use formdes_model::geom::{BezPath, Point, Rect};
use formdes_model::ids::ItemId;
use formdes_model::item::{FirstBaseline, VJust, WrapMode};
use formdes_model::story::{BreakKind, Para, Table};
use formdes_model::style::Styles;

use crate::compose::{Composer, DropCap, Line, ParaInput, Slot, shift_line, slice_input};
use crate::fonts::FontBook;
use crate::para::{self, Built, Context, Special};
use crate::region;

#[derive(Clone, Debug)]
pub struct Exclusion {
    pub rect: Rect,
    pub mode: WrapMode,
    pub side: String,
    pub path: Option<BezPath>,
    pub pad: f64,
    pub invert: bool,
}

#[derive(Clone, Debug)]
pub struct FrameSpec {
    pub item: ItemId,
    pub inner: Rect,
    pub columns: u32,
    pub gutter: f64,
    pub fixed_column: Option<f64>,
    pub vjust: VJust,
    pub first_baseline: FirstBaseline,
    pub first_min: f64,
    pub balance: bool,
    pub exclusions: Vec<Exclusion>,
    pub right_page: bool,
    pub grow: bool,
    pub grid: Option<(f64, f64)>,
    pub page: usize,
    pub shape: Option<BezPath>,
    pub vertical: bool,
    pub span_notes: bool,
    pub note_reserve: f64,
    pub note_columns: u32,
    pub note_split: Vec<usize>,
}

#[derive(Clone)]
pub struct PlacedLine {
    pub para: usize,
    pub frame: usize,
    pub column: usize,
    pub baseline: f64,
    pub line: Line,
}

#[derive(Clone)]
pub struct PlacedDrop {
    pub para: usize,
    pub frame: usize,
    pub x: f64,
    pub baseline: f64,
    pub drop: DropCap,
}

#[derive(Clone, Debug)]
pub struct ParaBox {
    pub para: usize,
    pub frame: usize,
    pub column: usize,
    pub top: f64,
    pub bottom: f64,
    pub left: f64,
    pub right: f64,
    pub first: bool,
    pub last: bool,
}

#[derive(Clone)]
pub struct CellOut {
    pub row: usize,
    pub col: usize,
    pub rect: Rect,
    pub lines: Vec<PlacedLine>,
    pub rotate: i32,
}

#[derive(Clone)]
pub struct TableFrag {
    pub para: usize,
    pub frame: usize,
    pub column: usize,
    pub rows: Vec<(usize, Rect)>,
    pub cells: Vec<CellOut>,
}

#[derive(Clone)]
pub struct NoteOut {
    pub ordinal: usize,
    pub frame: usize,
    pub column: usize,
    pub lines: Vec<PlacedLine>,
    pub sub: usize,
    pub top: f64,
    pub height: f64,
}

fn split_height(heights: &[f64], n: usize) -> f64 {
    let mut sums = vec![0.0; n.max(1)];
    for (h, s) in heights.iter().zip(note_split(heights, n)) {
        sums[s] += h;
    }
    sums.into_iter().fold(0.0, f64::max)
}

pub fn note_split(heights: &[f64], n: usize) -> Vec<usize> {
    let k = heights.len();
    let n = n.max(1).min(k.max(1));
    let mut pre = vec![0.0; k + 1];
    for (i, h) in heights.iter().enumerate() {
        pre[i + 1] = pre[i] + h;
    }
    let mut best = vec![vec![f64::INFINITY; k + 1]; n + 1];
    let mut cut = vec![vec![0usize; k + 1]; n + 1];
    best[0][0] = 0.0;
    for j in 1..=n {
        for i in 0..=k {
            for s in 0..=i {
                let v = best[j - 1][s].max(pre[i] - pre[s]);
                if v < best[j][i] - 1e-9 {
                    best[j][i] = v;
                    cut[j][i] = s;
                }
            }
        }
    }
    let mut out = vec![0; k];
    let mut i = k;
    for j in (1..=n).rev() {
        let s = cut[j][i];
        for o in &mut out[s..i] {
            *o = j - 1;
        }
        i = s;
    }
    out
}

#[derive(Clone, Debug)]
pub struct AnchorOut {
    pub item: ItemId,
    pub frame: usize,
    pub x: f64,
    pub y: f64,
    pub inline: bool,
}

#[derive(Clone, Debug, Default)]
pub struct FrameOut {
    pub used: f64,
    pub first_para: Option<usize>,
    pub last_para: Option<usize>,
    pub grown_height: Option<f64>,
    pub note_room: f64,
}

#[derive(Clone, Default)]
pub struct Flow {
    pub lines: Vec<PlacedLine>,
    pub drops: Vec<PlacedDrop>,
    pub boxes: Vec<ParaBox>,
    pub tables: Vec<TableFrag>,
    pub notes: Vec<NoteOut>,
    pub anchors: Vec<AnchorOut>,
    pub frames: Vec<FrameOut>,
    pub overset: bool,
    pub overset_at: Option<(usize, usize)>,
    pub para_offsets: Vec<usize>,
    pub para_maps: Vec<Vec<usize>>,
    pub para_texts: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Cursor {
    frame: usize,
    column: usize,
    y: f64,
    baseline: Option<f64>,
    top: bool,
}

#[derive(Clone, Copy, Debug)]
struct SlotPos {
    frame: usize,
    column: usize,
    band: usize,
    baseline: f64,
    left: f64,
    width: f64,
}

pub struct Flower<'a> {
    pub fonts: &'a FontBook,
    pub styles: &'a Styles,
    pub frames: &'a [FrameSpec],
    pub ctx: &'a dyn Context,
    note_height: RefCell<HashMap<(usize, usize), f64>>,
    note_cols: RefCell<HashMap<usize, Vec<f64>>>,
    shapes: Vec<Option<Vec<Vec<Point>>>>,
    ex_shapes: Vec<Vec<Option<Vec<Vec<Point>>>>>,
    region_top: RefCell<HashMap<usize, f64>>,
    span_frame: Cell<Option<usize>>,
}

const NOTE_GAP: f64 = 8.0;
const GROW_LIMIT: f64 = 1.0e6;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Caret {
    pub frame: usize,
    pub x: f64,
    pub baseline: f64,
    pub ascent: f64,
    pub descent: f64,
}

impl Flow {
    pub fn story_pos(&self, para: usize, byte: usize) -> usize {
        let base = self.para_offsets.get(para).copied().unwrap_or(0);
        let map = self.para_maps.get(para).map(Vec::as_slice).unwrap_or(&[]);
        let ch = match map.binary_search(&byte) {
            Ok(i) => i,
            Err(i) => i,
        };
        base + ch
    }

    pub fn para_byte(&self, pos: usize) -> Option<(usize, usize)> {
        let para = self.para_offsets.partition_point(|o| *o <= pos).checked_sub(1)?;
        let ch = pos - self.para_offsets[para];
        let map = self.para_maps.get(para)?;
        let byte = map.get(ch).copied().unwrap_or_else(|| self.lines.iter().filter(|l| l.para == para).map(|l| l.line.end).max().unwrap_or(0));
        Some((para, byte))
    }

    pub fn caret(&self, pos: usize) -> Option<Caret> {
        let (para, byte) = self.para_byte(pos)?;
        let lines: Vec<&PlacedLine> = self.lines.iter().filter(|l| l.para == para).collect();
        let line = lines.iter().find(|l| byte >= l.line.start && byte < l.line.end).or_else(|| lines.iter().rev().find(|l| byte >= l.line.start)).or(lines.first())?;
        let carets = &line.line.carets;
        let x = match carets.binary_search_by_key(&byte, |c| c.0) {
            Ok(i) => carets[i].1,
            Err(0) => carets.first().map(|c| c.1).unwrap_or(line.line.left),
            Err(i) => carets[i - 1].1,
        };
        Some(Caret { frame: line.frame, x, baseline: line.baseline, ascent: line.line.ascent, descent: line.line.descent })
    }

    pub fn hit(&self, frame: usize, x: f64, y: f64) -> Option<usize> {
        let line = self
            .lines
            .iter()
            .filter(|l| l.frame == frame)
            .min_by(|a, b| {
                let da = band_distance(a, x, y);
                let db = band_distance(b, x, y);
                da.total_cmp(&db)
            })?;
        let carets = &line.line.carets;
        let byte = carets.iter().min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs())).map(|c| c.0).unwrap_or(line.line.start);
        Some(self.story_pos(line.para, byte))
    }
}

fn band_distance(l: &PlacedLine, x: f64, y: f64) -> f64 {
    let top = l.baseline - l.line.ascent;
    let bottom = l.baseline + l.line.descent;
    let dy = if y < top { top - y } else if y > bottom { y - bottom } else { 0.0 };
    let left = l.line.left;
    let right = l.line.left + l.line.avail;
    let dx = if x < left { left - x } else if x > right { x - right } else { 0.0 };
    dy * 1000.0 + dx
}

impl<'a> Flower<'a> {
    pub fn new(fonts: &'a FontBook, styles: &'a Styles, frames: &'a [FrameSpec], ctx: &'a dyn Context) -> Self {
        let shapes = frames.iter().map(|f| f.shape.as_ref().map(region::polylines)).collect();
        let ex_shapes = frames.iter().map(|f| f.exclusions.iter().map(|e| e.path.as_ref().map(region::polylines)).collect()).collect();
        Flower {
            fonts,
            styles,
            frames,
            ctx,
            note_height: RefCell::new(HashMap::new()),
            note_cols: RefCell::new(HashMap::new()),
            shapes,
            ex_shapes,
            region_top: RefCell::new(HashMap::new()),
            span_frame: Cell::new(None),
        }
    }

    fn column_x(&self, f: usize, c: usize) -> (f64, f64) {
        let fr = &self.frames[f];
        if self.span_frame.get() == Some(f) {
            return (fr.inner.x0, fr.inner.width().max(1.0));
        }
        let n = fr.columns.max(1) as f64;
        let w = fr.fixed_column.unwrap_or((fr.inner.width() - fr.gutter * (n - 1.0)) / n).max(1.0);
        let x = fr.inner.x0 + c as f64 * (w + fr.gutter);
        (x, w)
    }

    fn note_key(&self, f: usize, c: usize) -> (usize, usize) {
        if self.frames[f].span_notes { (f, 0) } else { (f, c) }
    }

    fn bottom(&self, f: usize, c: usize) -> f64 {
        let fr = &self.frames[f];
        let base = if fr.grow { fr.inner.y0 + GROW_LIMIT } else { fr.inner.y1 };
        let mut notes = self.note_height.borrow().get(&self.note_key(f, c)).copied().unwrap_or(0.0);
        if fr.span_notes {
            notes = notes.max(fr.note_reserve);
        }
        base - if notes > 0.0 { notes + NOTE_GAP } else { 0.0 }
    }

    fn next_column(&self, cur: Cursor) -> Option<Cursor> {
        let fr = &self.frames[cur.frame];
        if self.span_frame.get() != Some(cur.frame) && cur.column + 1 < fr.columns.max(1) as usize {
            return Some(Cursor { frame: cur.frame, column: cur.column + 1, y: self.region_top(cur.frame), baseline: None, top: true });
        }
        self.next_frame(cur)
    }

    fn region_top(&self, f: usize) -> f64 {
        self.region_top.borrow().get(&f).copied().unwrap_or(self.frames[f].inner.y0)
    }

    fn next_frame(&self, cur: Cursor) -> Option<Cursor> {
        let f = cur.frame + 1;
        (f < self.frames.len()).then(|| Cursor { frame: f, column: 0, y: self.frames[f].inner.y0, baseline: None, top: true })
    }

    fn intervals(&self, f: usize, c: usize, top: f64, bottom: f64) -> Option<Vec<(f64, f64)>> {
        let (x0, w) = self.column_x(f, c);
        let mut ivs = vec![(x0, x0 + w)];
        if self.span_frame.get() != Some(f)
            && let Some(polys) = &self.shapes[f]
        {
            ivs = region::intersect(&ivs, &region::band_inside(polys, top, bottom));
        }
        for (ei, ex) in self.frames[f].exclusions.iter().enumerate() {
            let pad = ex.pad;
            if ex.rect.y1 + pad <= top || ex.rect.y0 - pad >= bottom || (!ex.invert && (ex.rect.x1 + pad <= x0 || ex.rect.x0 - pad >= x0 + w)) {
                if ex.invert {
                    return Some(Vec::new());
                }
                continue;
            }
            match ex.mode {
                WrapMode::None => {}
                WrapMode::JumpObject => return Some(Vec::new()),
                WrapMode::NextColumn => return None,
                _ => {
                    let cover = match &self.ex_shapes[f][ei] {
                        Some(polys) if ex.mode == WrapMode::Contour => region::band_cover(polys, top, bottom, pad),
                        _ => vec![(ex.rect.x0 - pad, ex.rect.x1 + pad)],
                    };
                    if ex.invert {
                        ivs = region::intersect(&ivs, &cover);
                        continue;
                    }
                    let cut: Vec<(f64, f64)> = match ex.side.as_str() {
                        "left" => cover.iter().map(|s| (s.0, f64::MAX)).collect(),
                        "right" => cover.iter().map(|s| (f64::MIN, s.1)).collect(),
                        "largest" => {
                            let lo = cover.first().map(|s| s.0).unwrap_or(x0);
                            let hi = cover.last().map(|s| s.1).unwrap_or(x0 + w);
                            if lo - x0 >= x0 + w - hi { vec![(lo, f64::MAX)] } else { vec![(f64::MIN, hi)] }
                        }
                        _ => cover,
                    };
                    ivs = region::subtract(&ivs, &cut);
                }
            }
        }
        Some(ivs.into_iter().filter(|(a, b)| b - a >= 12.0).collect())
    }

    fn first_baseline(&self, f: usize, ascent: f64, cap: f64, xh: f64, leading: f64, top: f64) -> f64 {
        let fr = &self.frames[f];
        let off = match fr.first_baseline {
            FirstBaseline::Ascent => ascent,
            FirstBaseline::CapHeight => cap,
            FirstBaseline::XHeight => xh,
            FirstBaseline::Leading => leading,
            FirstBaseline::Fixed => fr.first_min,
        };
        top + off.max(fr.first_min)
    }

    fn snap(&self, f: usize, b: f64) -> f64 {
        match self.frames[f].grid {
            Some((start, step)) if step > 0.0 => start + ((b - start - 0.01) / step).ceil() * step,
            _ => b,
        }
    }

    fn plan(&self, from: Cursor, est: (f64, f64, f64), count: usize, grid: bool) -> Vec<SlotPos> {
        let (lead, asc, desc) = est;
        let mut out = Vec::new();
        let mut cur = Some(from);
        let mut band = 0usize;
        while out.len() < count {
            let Some(c) = cur else { break };
            let mut baseline = match c.baseline {
                Some(b) => b + lead,
                None => self.first_baseline(c.frame, asc, asc * 0.7, asc * 0.5, lead, c.y),
            };
            if grid {
                baseline = self.snap(c.frame, baseline);
            }
            if baseline + desc > self.bottom(c.frame, c.column) + 0.01 && !(c.top && c.baseline.is_none() && out.is_empty() && self.frames[c.frame].inner.height() < lead) {
                cur = self.next_column(c);
                continue;
            }
            match self.intervals(c.frame, c.column, baseline - asc, baseline + desc) {
                None => {
                    cur = self.next_column(c);
                    continue;
                }
                Some(ivs) => {
                    for (a, b) in &ivs {
                        out.push(SlotPos { frame: c.frame, column: c.column, band, baseline, left: *a, width: b - a });
                    }
                    band += 1;
                    cur = Some(Cursor { baseline: Some(baseline), top: false, ..c });
                }
            }
        }
        out
    }

    pub fn flow(&self, paras: &[Para]) -> Flow {
        let mut out = Flow { frames: vec![FrameOut::default(); self.frames.len()], ..Flow::default() };
        if self.frames.is_empty() {
            out.overset = !paras.is_empty();
            out.overset_at = Some((0, 0));
            return out;
        }
        let mut cur = Cursor { frame: 0, column: 0, y: self.frames[0].inner.y0, baseline: None, top: true };
        let mut counters: HashMap<String, i64> = HashMap::new();
        let mut notes = 0usize;
        let mut offset = 0usize;
        let mut pending_break: Option<BreakKind> = None;
        for (pi, para) in paras.iter().enumerate() {
            out.para_offsets.push(offset);
            offset += para.len() + 1;
            let props = self.styles.para_props(&para.style, &para.over);
            let number = if props.list == ListKind::Number {
                let base = if props.list_name.is_empty() { para.style.clone() } else { props.list_name.clone() };
                let level = props.list_level.max(1);
                let restart = para.over.contains_key("startAt");
                let n = {
                    let n = counters.entry(format!("{base}\u{1}{level}")).or_insert(props.start_at - 1);
                    if restart {
                        *n = props.start_at - 1;
                    }
                    *n += 1;
                    *n
                };
                for deeper in level + 1..=9 {
                    counters.remove(&format!("{base}\u{1}{deeper}"));
                }
                let parents = (1..level).map(|l| counters.get(&format!("{base}\u{1}{l}")).copied().unwrap_or(1)).collect();
                Some(para::ListNo { n, parents })
            } else {
                None
            };
            if let Some(b) = pending_break.take() {
                match self.apply_break(cur, b) {
                    Some(c) => cur = c,
                    None => {
                        out.overset = true;
                        out.overset_at = Some((pi, 0));
                        break;
                    }
                }
            }
            let start = match props.start {
                Start::Anywhere => Some(cur),
                Start::NextColumn => if cur.top { Some(cur) } else { self.next_column(cur) },
                Start::NextFrame => if cur.top && cur.column == 0 { Some(cur) } else { self.next_frame(cur) },
                Start::NextPage | Start::NextOddPage | Start::NextEvenPage => self.page_start(cur, props.start),
            };
            let Some(start) = start else {
                out.overset = true;
                out.overset_at = Some((pi, 0));
                break;
            };
            cur = start;
            let mut built = para::build(self.styles, para, number.as_ref(), notes, self.ctx, cur.frame);
            if !props.nested_lines.is_empty() && built.table().is_none() {
                built = self.line_styled(para, number.as_ref(), notes, cur, built);
            }
            out.para_maps.push(built.chars.clone());
            out.para_texts.push(built.input.text.clone());
            notes += built.specials.iter().filter(|(_, s)| matches!(s, Special::Footnote { .. })).count();
            pending_break = built.brk;
            if out.frames[cur.frame].first_para.is_none() {
                out.frames[cur.frame].first_para = Some(pi);
            }
            let spans = built.input.props.span_columns > 1 && self.frames[cur.frame].columns > 1 && built.table().is_none();
            let res = if let Some(t) = built.table() {
                self.place_table(pi, t, cur, &mut out)
            } else if spans {
                let f = cur.frame;
                let top = self.region_top(f);
                let had = out.lines.iter().any(|l| l.frame == f && l.baseline - l.line.ascent >= top - 0.5);
                let bottom = self.balance(f, top, &mut out);
                let start = if had {
                    Cursor { frame: f, column: 0, y: bottom, baseline: Some(bottom), top: false }
                } else {
                    Cursor { frame: f, column: 0, y: top, baseline: None, top: true }
                };
                self.span_frame.set(Some(f));
                let r = self.place_para(pi, &built, start, &mut out);
                self.span_frame.set(None);
                r.map(|c| {
                    let y = c.y.max(c.baseline.unwrap_or(c.y));
                    self.region_top.borrow_mut().insert(c.frame, y);
                    Cursor { frame: c.frame, column: 0, y, baseline: None, top: false }
                })
            } else if built.input.props.split_columns > 1 {
                self.place_split(pi, &built, cur, &mut out)
            } else {
                self.place_para(pi, &built, cur, &mut out)
            };
            match res {
                Some(c) => {
                    cur = c;
                    out.frames[cur.frame].last_para = Some(pi);
                }
                None => {
                    out.overset = true;
                    if out.overset_at.is_none() {
                        out.overset_at = Some((pi, 0));
                    }
                    break;
                }
            }
        }
        self.finish(&mut out);
        out
    }

    fn line_styled(&self, para: &Para, number: Option<&para::ListNo>, notes: usize, cur: Cursor, built: Built) -> Built {
        let rules = built.input.props.nested_lines.clone();
        let mut built = built;
        let mut prev: Vec<(usize, usize)> = Vec::new();
        for _ in 0..3 {
            let (_, composed) = self.compose_from(&built.input, 0, cur, 0.0);
            let spans: Vec<(usize, usize)> = composed.lines.iter().map(|l| (l.start, l.end)).collect();
            if spans == prev {
                break;
            }
            let mut extra: Vec<(std::ops::Range<usize>, String)> = Vec::new();
            let mut at = 0usize;
            for r in &rules {
                let to = (at + r.lines as usize).min(spans.len());
                if at < to {
                    extra.push((spans[at].0..spans[to - 1].1, r.style.clone()));
                }
                at += r.lines as usize;
            }
            built = para::build_with(self.styles, para, number, notes, self.ctx, cur.frame, &extra);
            prev = spans;
        }
        built
    }

    fn apply_break(&self, cur: Cursor, b: BreakKind) -> Option<Cursor> {
        match b {
            BreakKind::Column => self.next_column(cur),
            BreakKind::Frame => self.next_frame(cur),
            BreakKind::Page => self.page_start(cur, Start::NextPage),
            BreakKind::OddPage => self.page_start(cur, Start::NextOddPage),
            BreakKind::EvenPage => self.page_start(cur, Start::NextEvenPage),
        }
    }

    fn page_start(&self, cur: Cursor, kind: Start) -> Option<Cursor> {
        let here = self.frames[cur.frame].page;
        let mut f = cur.frame;
        loop {
            f += 1;
            if f >= self.frames.len() {
                return None;
            }
            let p = self.frames[f].page;
            let ok = p != here
                && match kind {
                    Start::NextOddPage => p.is_multiple_of(2),
                    Start::NextEvenPage => !p.is_multiple_of(2),
                    _ => true,
                };
            if ok {
                return Some(Cursor { frame: f, column: 0, y: self.frames[f].inner.y0, baseline: None, top: true });
            }
        }
    }

    fn compose_from(&self, input: &ParaInput, from_byte: usize, cur: Cursor, space_before: f64) -> (Vec<SlotPos>, crate::compose::Composed) {
        let sliced;
        let inp = if from_byte == 0 {
            input
        } else {
            sliced = slice_input(input, from_byte);
            &sliced
        };
        let first = inp.runs.first().map(|r| r.props.clone());
        let size = first.as_ref().map(|p| p.size).unwrap_or(12.0);
        let lead = first.as_ref().and_then(|p| p.leading).unwrap_or(size * inp.props.auto_leading);
        let est = (lead, size * 0.8, size * 0.25);
        let mut c = cur;
        if !c.top {
            c.baseline = c.baseline.map(|b| b + space_before);
            c.y += space_before;
        }
        let slots = RefCell::new(self.plan(c, est, 8, inp.props.grid_align));
        let get = |i: usize| -> Slot {
            let mut s = slots.borrow_mut();
            if i >= s.len() {
                let want = (i + 1).max(s.len() * 2);
                *s = self.plan(c, est, want, inp.props.grid_align);
            }
            s.get(i).or_else(|| s.last()).map(|p| Slot { left: p.left, width: p.width }).unwrap_or(Slot { left: 0.0, width: 1.0 })
        };
        let composer = Composer { fonts: self.fonts };
        let right = self.frames[cur.frame].right_page;
        let composed = composer.compose(inp, &get, usize::MAX / 2, right);
        let n = composed.lines.len();
        let mut planned = slots.into_inner();
        if planned.len() < n {
            planned = self.plan(c, est, n, inp.props.grid_align);
        }
        (planned, composed)
    }

    fn place_para(&self, pi: usize, built: &Built, cur: Cursor, out: &mut Flow) -> Option<Cursor> {
        let props = &built.input.props;
        let mut from_byte = 0usize;
        let mut cur = cur;
        let mut first_line = true;
        let mut placed_here: Vec<usize> = Vec::new();
        let mut anchor_marks: Vec<usize> = Vec::new();
        let mut widow_fixed = false;
        let mut tries = 0;
        loop {
            tries += 1;
            if tries > 400 {
                return None;
            }
            let sb = if first_line { props.space_before } else { 0.0 };
            let (slots, composed) = self.compose_from(&built.input, from_byte, cur, sb);
            if composed.lines.is_empty() {
                break;
            }
            if first_line && !cur.top && (props.keep_together || props.keep_first > 1) {
                let need = if props.keep_together { composed.lines.len() } else { (props.keep_first as usize).min(composed.lines.len()) };
                let same = slots.iter().take(need).all(|s| s.frame == cur.frame && s.column == cur.column);
                if !same
                    && slots.len() >= need
                    && let Some(nc) = self.next_column(cur)
                {
                    cur = nc;
                    continue;
                }
            }
            if slots.is_empty() {
                out.overset_at = Some((pi, built.char_of(from_byte)));
                return None;
            }
            let mut baseline_of_band: HashMap<(usize, usize, usize), f64> = HashMap::new();
            let mut prev: Option<(usize, usize, f64)> = cur.baseline.map(|b| (cur.frame, cur.column, b));
            let mut restart: Option<(usize, Cursor)> = None;
            for (k, mut line) in composed.lines.into_iter().enumerate() {
                let Some(slot) = slots.get(k).copied() else {
                    out.overset_at = Some((pi, built.char_of(from_byte + line.start)));
                    self.record_boxes(pi, &placed_here, out);
                    return None;
                };
                if from_byte > 0 {
                    shift_line(&mut line, from_byte);
                }
                let key = (slot.frame, slot.column, slot.band);
                let baseline = if let Some(b) = baseline_of_band.get(&key) {
                    *b
                } else {
                    let mut b = match prev {
                        Some((f, c, pb)) if f == slot.frame && c == slot.column => pb + line.leading + if k == 0 && first_line && !cur.top { props.space_before } else { 0.0 },
                        _ => {
                            let top = if slot.frame == cur.frame && slot.column == cur.column { cur.y + if k == 0 && first_line && !cur.top { props.space_before } else { 0.0 } } else { self.frames[slot.frame].inner.y0 };
                            self.first_baseline(slot.frame, line.ascent, line.cap, line.xheight, line.leading, top)
                        }
                    };
                    if props.grid_align {
                        b = self.snap(slot.frame, b);
                    }
                    if slot.baseline - b > line.leading * 0.75 {
                        b = slot.baseline;
                    }
                    baseline_of_band.insert(key, b);
                    b
                };
                if baseline + line.descent > self.bottom(slot.frame, slot.column) + 0.5 {
                    let next = Cursor { frame: slot.frame, column: slot.column, y: 0.0, baseline: None, top: true };
                    match self.next_column(next) {
                        Some(nc) => restart = Some((line.start, nc)),
                        None => {
                            out.overset_at = Some((pi, built.char_of(line.start)));
                            return None;
                        }
                    }
                    break;
                }
                let has_note = built.specials.iter().any(|(at, s)| matches!(s, Special::Footnote { .. }) && *at >= line.start && *at < line.end.max(line.start + 1));
                if has_note {
                    let mut added: Vec<f64> = Vec::new();
                    for (at, s) in &built.specials {
                        if let Special::Footnote { ordinal, paras } = s
                            && *at >= line.start
                            && *at < line.end.max(line.start + 1)
                        {
                            let h = self.place_note(*ordinal, paras, slot.frame, slot.column, out);
                            *self.note_height.borrow_mut().entry(self.note_key(slot.frame, slot.column)).or_insert(0.0) += h;
                            added.push(h);
                        }
                    }
                    let not_first = prev.is_some_and(|(f, c, _)| f == slot.frame && c == slot.column);
                    if not_first && baseline + line.descent > self.bottom(slot.frame, slot.column) + 0.5 {
                        for h in added.iter().rev() {
                            if let Some(n) = out.notes.pop()
                                && let Some(cols) = self.note_cols.borrow_mut().get_mut(&n.frame)
                                && let Some(v) = cols.get_mut(n.sub)
                            {
                                *v = n.top;
                            }
                            *self.note_height.borrow_mut().entry(self.note_key(slot.frame, slot.column)).or_insert(0.0) -= h;
                        }
                        let next = Cursor { frame: slot.frame, column: slot.column, y: 0.0, baseline: None, top: true };
                        match self.next_column(next) {
                            Some(nc) => restart = Some((line.start, nc)),
                            None => {
                                out.overset_at = Some((pi, built.char_of(line.start)));
                                return None;
                            }
                        }
                        break;
                    }
                }
                if first_line
                    && k == 0
                    && let Some(d) = &composed.drop
                {
                    let lead = line.leading;
                    let drop_x = if props.rtl { slot.left + slot.width - props.left_indent - d.width } else { slot.left + props.left_indent };
                    out.drops.push(PlacedDrop { para: pi, frame: slot.frame, x: drop_x, baseline: baseline + lead * (d.lines as f64 - 1.0), drop: d.clone() });
                }
                anchor_marks.push(out.anchors.len());
                for (at, s) in &built.specials {
                    if let Special::Anchored { item, inline, attrs } = s
                        && *at >= line.start
                        && *at < line.end.max(line.start + 1)
                    {
                        let x_obj = line.objects.iter().find(|o| o.0 == *at).map(|o| o.1).unwrap_or(slot.left);
                        let (x, y) = if *inline {
                            (x_obj, baseline)
                        } else {
                            let xo = attrs.get("xOffset").and_then(|v| v.as_f64()).unwrap_or(0.0);
                            let yo = attrs.get("yOffset").and_then(|v| v.as_f64()).unwrap_or(0.0);
                            (slot.left + xo, baseline + yo)
                        };
                        out.anchors.push(AnchorOut { item: *item, frame: slot.frame, x, y, inline: *inline });
                    }
                }
                prev = Some((slot.frame, slot.column, baseline));
                placed_here.push(out.lines.len());
                out.lines.push(PlacedLine { para: pi, frame: slot.frame, column: slot.column, baseline, line });
            }
            first_line = false;
            match restart {
                Some((byte, nc)) => {
                    from_byte = byte;
                    cur = nc;
                    if from_byte >= built.input.text.len() {
                        break;
                    }
                }
                None => {
                    if !widow_fixed && let Some(cut) = self.widow_cut(props.keep_last as usize, &placed_here, out) {
                        widow_fixed = true;
                        let line_idx = placed_here[cut];
                        let (f, c, start) = (out.lines[line_idx].frame, out.lines[line_idx].column, out.lines[line_idx].line.start);
                        out.lines.truncate(line_idx);
                        out.anchors.truncate(anchor_marks[cut]);
                        placed_here.truncate(cut);
                        anchor_marks.truncate(cut);
                        if let Some(nc) = self.next_column(Cursor { frame: f, column: c, y: 0.0, baseline: None, top: true }) {
                            from_byte = start;
                            cur = nc;
                            continue;
                        }
                        return None;
                    }
                    let (f, c, b) = prev?;
                    let last_desc = out.lines.last().map(|l| l.line.descent).unwrap_or(0.0);
                    self.record_boxes(pi, &placed_here, out);
                    return Some(Cursor { frame: f, column: c, y: b + last_desc + props.space_after, baseline: Some(b + props.space_after), top: false });
                }
            }
        }
        self.record_boxes(pi, &placed_here, out);
        let l = placed_here.last().map(|i| &out.lines[*i])?;
        Some(Cursor { frame: l.frame, column: l.column, y: l.baseline + l.line.descent, baseline: Some(l.baseline), top: false })
    }

    fn place_split(&self, pi: usize, built: &Built, cur: Cursor, out: &mut Flow) -> Option<Cursor> {
        let props = &built.input.props;
        let k = props.split_columns.max(1) as usize;
        let composer = Composer { fonts: self.fonts };
        let mut c = cur;
        for _ in 0..64 {
            let (x0, w) = self.column_x(c.frame, c.column);
            let g = props.split_gutter;
            let sw = ((w - g * (k as f64 - 1.0)) / k as f64).max(4.0);
            let composed = composer.compose(&built.input, &|_| Slot { left: 0.0, width: sw }, 1, self.frames[c.frame].right_page);
            let n = composed.lines.len();
            if n == 0 {
                return Some(c);
            }
            let rows = n.div_ceil(k);
            let first = &composed.lines[0];
            let lead = composed.lines.iter().map(|l| l.leading).fold(0.0, f64::max);
            let desc = composed.lines.iter().map(|l| l.descent).fold(0.0, f64::max);
            let top_base = match c.baseline {
                Some(b) if !c.top => b + props.space_before + lead,
                _ => self.first_baseline(c.frame, first.ascent, first.cap, first.xheight, lead, c.y),
            };
            let bottom = top_base + (rows as f64 - 1.0) * lead + desc;
            if bottom > self.bottom(c.frame, c.column) + 0.5 && !c.top {
                c = self.next_column(c)?;
                continue;
            }
            let mut placed = Vec::new();
            for (i, mut line) in composed.lines.into_iter().enumerate() {
                let sub = i / rows;
                offset_line(&mut line, x0 + sub as f64 * (sw + g));
                placed.push(out.lines.len());
                out.lines.push(PlacedLine { para: pi, frame: c.frame, column: c.column, baseline: top_base + (i % rows) as f64 * lead, line });
            }
            self.record_boxes(pi, &placed, out);
            let last = top_base + (rows as f64 - 1.0) * lead;
            return Some(Cursor { frame: c.frame, column: c.column, y: last + desc + props.space_after, baseline: Some(last + props.space_after), top: false });
        }
        None
    }

    fn widow_cut(&self, keep_last: usize, placed: &[usize], out: &Flow) -> Option<usize> {
        if keep_last < 2 || placed.len() <= keep_last {
            return None;
        }
        let last = &out.lines[*placed.last()?];
        let in_last = placed.iter().rev().take_while(|i| out.lines[**i].frame == last.frame && out.lines[**i].column == last.column).count();
        if in_last >= keep_last || in_last == placed.len() {
            return None;
        }
        Some(placed.len() - keep_last)
    }

    fn record_boxes(&self, pi: usize, placed: &[usize], out: &mut Flow) {
        let mut groups: Vec<(usize, usize, Vec<usize>)> = Vec::new();
        for i in placed {
            let l = &out.lines[*i];
            match groups.last_mut() {
                Some(g) if g.0 == l.frame && g.1 == l.column => g.2.push(*i),
                _ => groups.push((l.frame, l.column, vec![*i])),
            }
        }
        let n = groups.len();
        for (k, (f, c, idx)) in groups.into_iter().enumerate() {
            let first = &out.lines[idx[0]];
            let last = &out.lines[*idx.last().expect("group")];
            let (x, w) = self.column_x(f, c);
            out.boxes.push(ParaBox {
                para: pi,
                frame: f,
                column: c,
                top: first.baseline - first.line.ascent,
                bottom: last.baseline + last.line.descent,
                left: x,
                right: x + w,
                first: k == 0,
                last: k + 1 == n,
            });
        }
    }

    fn place_note(&self, ordinal: usize, paras: &[Para], f: usize, c: usize, out: &mut Flow) -> f64 {
        let span = self.frames[f].span_notes;
        let (mut x, mut w) = if span { (self.frames[f].inner.x0, self.frames[f].inner.width().max(1.0)) } else { self.column_x(f, c) };
        let c = if span { 0 } else { c };
        let n_sub = if span { self.frames[f].note_columns.max(1) as usize } else { 1 };
        let mut sub = 0;
        let mut top = 0.0;
        if n_sub > 1 {
            let gutter = self.frames[f].gutter;
            let sw = ((w - gutter * (n_sub - 1) as f64) / n_sub as f64).max(1.0);
            let mut cols = self.note_cols.borrow_mut();
            let hs = cols.entry(f).or_insert_with(|| vec![0.0; n_sub]);
            let placed: Vec<usize> = out.notes.iter().filter(|n| n.frame == f).map(|n| n.sub).collect();
            let last = placed.last().copied().unwrap_or(0);
            sub = match self.frames[f].note_split.get(placed.len()) {
                Some(s) if *s < n_sub && *s >= last => *s,
                _ => (last..n_sub).fold(last, |b, k| if hs[k] < hs[b] - 0.01 { k } else { b }),
            };
            top = hs[sub];
            x += sub as f64 * (sw + gutter);
            w = sw;
        }
        let mut lines = Vec::new();
        let mut y = 0.0;
        let composer = Composer { fonts: self.fonts };
        for (k, p) in paras.iter().enumerate() {
            let mut b = para::build(self.styles, p, None, 0, self.ctx, f);
            if k == 0 {
                let mark = self.ctx.footnote_label(ordinal);
                let len = mark.len();
                b.input.text.insert_str(0, &mark);
                for r in &mut b.input.runs {
                    r.range = r.range.start + len..r.range.end + len;
                }
                if let Some(r) = b.input.runs.first_mut() {
                    r.range.start = 0;
                }
            }
            let composed = composer.compose(&b.input, &|_| Slot { left: x, width: w }, 1, self.frames[f].right_page);
            for line in composed.lines {
                y += line.leading;
                lines.push(PlacedLine { para: k, frame: f, column: c, baseline: y, line });
            }
        }
        out.notes.push(NoteOut { ordinal, frame: f, column: c, lines, sub, top, height: y + 4.0 });
        if n_sub > 1 {
            let mut cols = self.note_cols.borrow_mut();
            let hs = cols.entry(f).or_insert_with(|| vec![0.0; n_sub]);
            hs[sub] = top + y + 4.0;
            let mut all: Vec<f64> = out.notes.iter().filter(|n| n.frame == f).map(|n| n.height).collect();
            let after = split_height(&all, n_sub);
            all.pop();
            return after - split_height(&all, n_sub);
        }
        y + 4.0
    }

    fn place_table(&self, pi: usize, t: &Table, cur: Cursor, out: &mut Flow) -> Option<Cursor> {
        let composer = Composer { fonts: self.fonts };
        let mut widths = t.widths.clone();
        for cell in t.cells.iter().filter(|cl| cl.cols == 1) {
            if let Some(w) = t.cell_attrs(self.styles, cell).get("width").and_then(|v| v.as_f64()).filter(|w| *w > 0.0)
                && let Some(slot) = widths.get_mut(cell.col)
            {
                *slot = w;
            }
        }
        let mut col_x = vec![0.0];
        for w in &widths {
            col_x.push(col_x.last().copied().unwrap_or(0.0) + w);
        }
        let mut cell_lines: HashMap<(usize, usize), (Vec<Line>, f64)> = HashMap::new();
        let mut need: HashMap<(usize, usize), f64> = HashMap::new();
        let compose_cell = |cell: &formdes_model::story::TableCell, attrs: &formdes_model::attrs::Attrs, w: f64| -> Vec<Line> {
            let para_style = attrs.get("paraStyle").and_then(|v| v.as_str()).map(formdes_model::style::style_key).filter(|s| self.styles.exists(formdes_model::style::StyleKind::Paragraph, s));
            let mut lines = Vec::new();
            for p in &cell.paras {
                let styled;
                let p = match &para_style {
                    Some(ps) if p.style == formdes_model::style::BASIC_PARA || p.style == "No Paragraph Style" => {
                        let mut q = p.clone();
                        q.style = ps.clone();
                        styled = q;
                        &styled
                    }
                    _ => p,
                };
                let b = para::build(self.styles, p, None, 0, self.ctx, cur.frame);
                let c = composer.compose(&b.input, &|_| Slot { left: 0.0, width: w }, 1, self.frames[cur.frame].right_page);
                lines.extend(c.lines);
            }
            lines
        };
        let mut eff: HashMap<(usize, usize), formdes_model::attrs::Attrs> = HashMap::new();
        for cell in &t.cells {
            let attrs = t.cell_attrs(self.styles, cell);
            let ins = cell_insets_of(&attrs);
            let rot = cell_rotation(&attrs);
            let w = (col_x[(cell.col + cell.cols).min(t.cols())] - col_x[cell.col] - ins[1] - ins[3]).max(4.0);
            if rot == 90 || rot == 270 {
                let nat = compose_cell(cell, &attrs, 1.0e5).iter().map(|l| l.width).fold(0.0, f64::max);
                need.insert((cell.row, cell.col), (nat + ins[1] + ins[3]).min(600.0));
            } else if rot >= 1000 {
                let a = ((rot - 1000) as f64).to_radians();
                let lines = compose_cell(cell, &attrs, 1.0e5);
                let long = lines.iter().map(|l| l.width).fold(0.0, f64::max);
                let h: f64 = lines.iter().map(|l| l.leading).sum();
                let lean = h * a.tan();
                let total = ((long + lean) * a.sin() + h * a.cos() + ins[0] + ins[2] + 4.0).min(600.0);
                cell_lines.insert((cell.row, cell.col), (lines, total));
            } else {
                let lines = compose_cell(cell, &attrs, w);
                let h: f64 = lines.iter().map(|l| l.leading).sum();
                cell_lines.insert((cell.row, cell.col), (lines, h + ins[0] + ins[2]));
            }
            eff.insert((cell.row, cell.col), attrs);
        }
        let rows = t.rows();
        let mut heights = t.heights.clone();
        for cell in &t.cells {
            if cell.rows == 1 && !t.exact.get(cell.row).copied().unwrap_or(false) {
                let h = cell_lines.get(&(cell.row, cell.col)).map(|(_, h)| *h).or_else(|| need.get(&(cell.row, cell.col)).copied());
                if let Some(h) = h {
                    heights[cell.row] = heights[cell.row].max(h);
                }
            }
        }
        for cell in t.cells.iter().filter(|cl| cl.rows == 1) {
            let Some(attrs) = eff.get(&(cell.row, cell.col)) else { continue };
            let num = |k: &str| attrs.get(k).and_then(|v| v.as_f64()).filter(|v| *v > 0.0);
            if let Some(mh) = num("minHeight") {
                heights[cell.row] = heights[cell.row].max(mh);
            }
            if let Some(h) = num("height") {
                heights[cell.row] = h;
            }
        }
        for cell in &t.cells {
            let Some(attrs) = eff.get(&(cell.row, cell.col)) else { continue };
            if need.contains_key(&(cell.row, cell.col)) {
                let ins = cell_insets_of(attrs);
                let span: f64 = (cell.row..(cell.row + cell.rows).min(rows)).map(|r| heights[r]).sum();
                let lines = compose_cell(cell, attrs, (span - ins[1] - ins[3]).max(4.0));
                let h: f64 = lines.iter().map(|l| l.leading).sum();
                cell_lines.insert((cell.row, cell.col), (lines, h + ins[0] + ins[2]));
            }
        }
        let mut c = cur;
        let mut y = if c.top { self.frames[c.frame].inner.y0 } else { c.y + 4.0 };
        let mut frag = TableFrag { para: pi, frame: c.frame, column: c.column, rows: Vec::new(), cells: Vec::new() };
        let headers: Vec<usize> = (0..t.header_rows.min(rows)).collect();
        let split_rows = t.attrs.get("splitRows").and_then(|v| v.as_bool()).unwrap_or(false);
        let continued = t.attrs.get("continued").and_then(|v| v.as_str()).map(str::to_string).filter(|s| !s.trim().is_empty());
        let mut r = 0;
        let mut guard = 0;
        while r < rows {
            guard += 1;
            if guard > rows * 4 + 16 {
                return None;
            }
            let h = heights[r];
            let room = self.bottom(c.frame, c.column) - y;
            if h > room + 0.5
                && split_rows
                && !t.cells.iter().any(|cl| cl.rows > 1 && cl.row <= r && r < cl.row + cl.rows)
                && !t.cells.iter().any(|cl| cl.row == r && need.contains_key(&(cl.row, cl.col)))
            {
                let mut first: HashMap<(usize, usize), (Vec<Line>, f64)> = HashMap::new();
                let mut rest: HashMap<(usize, usize), (Vec<Line>, f64)> = HashMap::new();
                let mut moved = false;
                let mut kept = false;
                for cell in t.cells.iter().filter(|cl| cl.row == r) {
                    let ins = eff.get(&(cell.row, cell.col)).map(cell_insets_of).unwrap_or([0.0; 4]);
                    let lines = cell_lines.get(&(cell.row, cell.col)).map(|(ls, _)| ls.clone()).unwrap_or_default();
                    let mut used = ins[0];
                    let mut k = 0;
                    while k < lines.len() && used + lines[k].leading + lines[k].descent + ins[2] <= room {
                        used += lines[k].leading;
                        k += 1;
                    }
                    let (a, b) = lines.split_at(k);
                    kept |= k > 0;
                    moved |= !b.is_empty();
                    let sum = |ls: &[Line]| ls.iter().map(|l| l.leading).sum::<f64>() + ins[0] + ins[2];
                    first.insert((cell.row, cell.col), (a.to_vec(), sum(a)));
                    rest.insert((cell.row, cell.col), (b.to_vec(), sum(b)));
                }
                if kept && moved && room > 0.0 {
                    let mut part = cell_lines.clone();
                    part.extend(first);
                    self.table_row(t, &heights, r, y, room, &col_x, &part, c, &mut frag);
                    let rest_h = rest.values().map(|(_, h)| *h).fold(0.0, f64::max);
                    cell_lines.extend(rest);
                    heights[r] = rest_h.max(4.0);
                    out.tables.push(std::mem::replace(&mut frag, TableFrag { para: pi, frame: 0, column: 0, rows: Vec::new(), cells: Vec::new() }));
                    c = self.next_column(c)?;
                    frag.frame = c.frame;
                    frag.column = c.column;
                    y = self.frames[c.frame].inner.y0;
                    if let Some(label) = &continued {
                        y += self.continued_row(label, &col_x, c, y, &mut frag);
                    }
                    if r >= headers.len() {
                        for hr in &headers {
                            self.table_row(t, &heights, *hr, y, heights[*hr], &col_x, &cell_lines, c, &mut frag);
                            y += heights[*hr];
                        }
                    }
                    continue;
                }
            }
            if y + h > self.bottom(c.frame, c.column) + 0.5 && !frag.rows.is_empty() {
                out.tables.push(std::mem::replace(&mut frag, TableFrag { para: pi, frame: 0, column: 0, rows: Vec::new(), cells: Vec::new() }));
                c = self.next_column(c)?;
                frag.frame = c.frame;
                frag.column = c.column;
                y = self.frames[c.frame].inner.y0;
                if let Some(label) = &continued {
                    y += self.continued_row(label, &col_x, c, y, &mut frag);
                }
                if r >= headers.len() {
                    for hr in &headers {
                        self.table_row(t, &heights, *hr, y, heights[*hr], &col_x, &cell_lines, c, &mut frag);
                        y += heights[*hr];
                    }
                }
                continue;
            }
            self.table_row(t, &heights, r, y, h, &col_x, &cell_lines, c, &mut frag);
            y += h;
            r += 1;
        }
        out.tables.push(frag);
        Some(Cursor { frame: c.frame, column: c.column, y: y + 4.0, baseline: Some(y + 4.0), top: false })
    }

    /// The "continued" row at the top of a table's later frames: one cell across the table holding `label`,
    /// recorded with row index usize::MAX so styles, fills and edges of real rows never apply to it.
    fn continued_row(&self, label: &str, col_x: &[f64], c: Cursor, y: f64, frag: &mut TableFrag) -> f64 {
        let (x0, _) = self.column_x(c.frame, c.column);
        let width = col_x.last().copied().unwrap_or(0.0);
        let ins = 4.0;
        let para = Para::from_text(formdes_model::style::BASIC_PARA, label);
        let b = para::build(self.styles, &para, None, 0, self.ctx, c.frame);
        let composer = Composer { fonts: self.fonts };
        let composed = composer.compose(&b.input, &|_| Slot { left: 0.0, width: (width - 2.0 * ins).max(4.0) }, 1, self.frames[c.frame].right_page);
        let h: f64 = composed.lines.iter().map(|l| l.leading).sum::<f64>() + 2.0 * ins;
        let rect = Rect::new(x0, y, x0 + width, y + h);
        let mut base = y + ins;
        let mut lines = Vec::new();
        for l in composed.lines {
            base += l.leading;
            let mut l = l;
            for run in l.runs.iter_mut().chain(l.adorn.iter_mut()) {
                for g in &mut run.glyphs {
                    g.x += x0 + ins;
                }
                run.x0 += x0 + ins;
                run.x1 += x0 + ins;
            }
            lines.push(PlacedLine { para: 0, frame: c.frame, column: c.column, baseline: base, line: l });
        }
        frag.rows.push((usize::MAX, rect));
        frag.cells.push(CellOut { row: usize::MAX, col: 0, rect, lines, rotate: 0 });
        h
    }

    #[allow(clippy::too_many_arguments)]
    fn table_row(&self, t: &Table, eff: &[f64], r: usize, y: f64, h: f64, col_x: &[f64], cells: &HashMap<(usize, usize), (Vec<Line>, f64)>, c: Cursor, frag: &mut TableFrag) {
        let (x0, _) = self.column_x(c.frame, c.column);
        frag.rows.push((r, Rect::new(x0, y, x0 + col_x.last().copied().unwrap_or(0.0), y + h)));
        for cell in t.cells.iter().filter(|cl| cl.row == r) {
            let span_h: f64 = h + (1..cell.rows).map(|k| eff.get(r + k).copied().unwrap_or(0.0)).sum::<f64>();
            let rect = Rect::new(x0 + col_x[cell.col], y, x0 + col_x[(cell.col + cell.cols).min(col_x.len() - 1)], y + span_h);
            let mut lines = Vec::new();
            let attrs = t.cell_attrs(self.styles, cell);
            let ins = cell_insets_of(&attrs);
            let rotate = cell_rotation(&attrs);
            let boxr = match rotate {
                90 | 270 => Rect::new(rect.y0, rect.x0, rect.y1, rect.x1),
                r if r >= 1000 => {
                    let h: f64 = cells.get(&(cell.row, cell.col)).map(|(ls, _)| ls.iter().map(|l| l.leading).sum()).unwrap_or(0.0);
                    let lean = (h * ((r - 1000) as f64).to_radians().tan()).min(rect.width() / 2.0);
                    Rect::new(rect.x0 + 2.0 + lean - ins[1], rect.y0 - h - 2.0 - ins[0], rect.x1, rect.y0 - 2.0)
                }
                _ => rect,
            };
            if let Some((ls, total)) = cells.get(&(cell.row, cell.col)) {
                let spare = (boxr.height() - total).max(0.0);
                let shift = match attrs.get("vj").and_then(|v| v.as_str()) {
                    Some("center") => spare / 2.0,
                    Some("bottom") => spare,
                    _ => 0.0,
                };
                let mut b = boxr.y0 + ins[0] + shift;
                for l in ls {
                    b += l.leading;
                    let mut l = l.clone();
                    for run in l.runs.iter_mut().chain(l.adorn.iter_mut()) {
                        for g in &mut run.glyphs {
                            g.x += boxr.x0 + ins[1];
                        }
                        run.x0 += boxr.x0 + ins[1];
                        run.x1 += boxr.x0 + ins[1];
                    }
                    for m in &mut l.marks {
                        m.x += boxr.x0 + ins[1];
                    }
                    lines.push(PlacedLine { para: 0, frame: c.frame, column: c.column, baseline: b, line: l });
                }
            }
            frag.cells.push(CellOut { row: cell.row, col: cell.col, rect, lines, rotate });
        }
    }

    fn finish(&self, out: &mut Flow) {
        for (fi, fr) in self.frames.iter().enumerate() {
            let mut used: f64 = fr.inner.y0;
            for l in out.lines.iter().filter(|l| l.frame == fi) {
                used = used.max(l.baseline + l.line.descent);
            }
            for t in out.tables.iter().filter(|t| t.frame == fi) {
                for (_, r) in &t.rows {
                    used = used.max(r.y1);
                }
            }
            out.frames[fi].used = used - fr.inner.y0;
            if fr.grow {
                out.frames[fi].grown_height = Some(used - fr.inner.y0);
            }
            if fr.span_notes {
                out.frames[fi].note_room = self.note_height.borrow().get(&(fi, 0)).copied().unwrap_or(0.0);
            }
            let mut stacked: HashMap<usize, f64> = HashMap::new();
            for n in out.notes.iter_mut().filter(|n| n.frame == fi) {
                let total = self.note_height.borrow().get(&(fi, n.column)).copied().unwrap_or(0.0);
                let above = stacked.entry(n.column).or_insert(0.0);
                let top = fr.inner.y1 - total + if fr.span_notes && fr.note_columns > 1 { n.top } else { *above };
                *above += n.lines.last().map(|l| l.baseline).unwrap_or(0.0) + 4.0;
                for l in &mut n.lines {
                    l.baseline += top;
                }
            }
            if fr.grow || fr.exclusions.iter().any(|e| e.mode != WrapMode::None) {
                continue;
            }
            for c in 0..fr.columns.max(1) as usize {
                let idx: Vec<usize> = out.lines.iter().enumerate().filter(|(_, l)| l.frame == fi && l.column == c).map(|(i, _)| i).collect();
                if idx.is_empty() {
                    continue;
                }
                let bottom = idx.iter().map(|i| out.lines[*i].baseline + out.lines[*i].line.descent).fold(f64::MIN, f64::max);
                let slack = self.bottom(fi, c) - bottom;
                if slack <= 0.0 {
                    continue;
                }
                match fr.vjust {
                    VJust::Top => {}
                    VJust::Center | VJust::Bottom => {
                        let d = if fr.vjust == VJust::Center { slack / 2.0 } else { slack };
                        for i in &idx {
                            out.lines[*i].baseline += d;
                        }
                    }
                    VJust::Justify => {
                        if idx.len() > 1 && out.overset_at.map(|_| true).unwrap_or(true) {
                            let step = slack / (idx.len() - 1) as f64;
                            for (k, i) in idx.iter().enumerate() {
                                out.lines[*i].baseline += step * k as f64;
                            }
                        }
                    }
                }
            }
            if fr.balance && fr.columns > 1 {
                let top = self.region_top(fi);
                self.balance(fi, top, out);
            }
        }
    }

    fn balance(&self, fi: usize, top: f64, out: &mut Flow) -> f64 {
        let cols = self.frames[fi].columns.max(1) as usize;
        let idx: Vec<usize> = out.lines.iter().enumerate().filter(|(_, l)| l.frame == fi && l.baseline - l.line.ascent >= top - 0.5).map(|(i, _)| i).collect();
        let lowest = idx.iter().map(|i| out.lines[*i].baseline + out.lines[*i].line.descent).fold(top, f64::max);
        if idx.len() < 2 || self.frames[fi].exclusions.iter().any(|e| e.mode != WrapMode::None) || self.shapes[fi].is_some() {
            return lowest;
        }
        let mut rows: Vec<Vec<usize>> = Vec::new();
        for i in idx {
            let l = &out.lines[i];
            match rows.last_mut() {
                Some(r) if out.lines[r[0]].column == l.column && (out.lines[r[0]].baseline - l.baseline).abs() < 0.01 => r.push(i),
                _ => rows.push(vec![i]),
            }
        }
        let metrics: Vec<(f64, f64)> = rows
            .iter()
            .map(|r| {
                (
                    r.iter().map(|i| out.lines[*i].line.leading).fold(0.0, f64::max),
                    r.iter().map(|i| out.lines[*i].line.ascent).fold(0.0, f64::max),
                )
            })
            .collect();
        let total: f64 = metrics.iter().map(|m| m.0).sum();
        let target = total / cols as f64;
        let mut col = 0usize;
        let mut acc = 0.0;
        let mut prev_base: Option<f64> = None;
        let mut bottom = top;
        for (r, &(h, asc)) in rows.iter().zip(&metrics) {
            if col + 1 < cols && acc > 0.0 && acc + h / 2.0 > target * (col as f64 + 1.0) {
                col += 1;
                prev_base = None;
            }
            acc += h;
            let base = match prev_base {
                Some(b) => b + h,
                None => top + asc,
            };
            prev_base = Some(base);
            for i in r {
                let (x, _) = self.column_x(fi, col);
                let (ox, _) = self.column_x(fi, out.lines[*i].column);
                let l = &mut out.lines[*i];
                offset_line(&mut l.line, x - ox);
                l.column = col;
                l.baseline = base;
                bottom = bottom.max(base + l.line.descent);
            }
        }
        bottom
    }
}

/// Maps the text of a rotated table cell (composed in a box whose width is the cell's height for 90 and 270
/// degrees) onto the cell rectangle.
pub fn cell_xf(rect: Rect, rotate: i32) -> kurbo::Affine {
    if rotate >= 1000 {
        let a = ((rotate - 1000) as f64).to_radians();
        return kurbo::Affine::translate((rect.x0, rect.y1)) * kurbo::Affine::rotate(-a) * kurbo::Affine::translate((-rect.x0, -rect.y0));
    }
    match rotate {
        90 => kurbo::Affine::new([0.0, 1.0, -1.0, 0.0, rect.x0 + rect.x1, 0.0]),
        270 => kurbo::Affine::new([0.0, -1.0, 1.0, 0.0, 0.0, rect.y0 + rect.y1]),
        180 => kurbo::Affine::new([-1.0, 0.0, 0.0, -1.0, rect.x0 + rect.x1, rect.y0 + rect.y1]),
        _ => kurbo::Affine::IDENTITY,
    }
}

/// A cell's text rotation: 0, 90, 180 or 270, or 1000 plus the angle for slanted text (10 to 80 degrees).
pub fn cell_rotation(attrs: &formdes_model::attrs::Attrs) -> i32 {
    if let Some(s) = attrs.get("slant").and_then(|v| v.as_f64()).filter(|s| *s >= 10.0 && *s <= 80.0) {
        return 1000 + s.round() as i32;
    }
    match attrs.get("rotate").and_then(|v| v.as_f64()).map(|d| d.round() as i64) {
        Some(90 | -270) => 90,
        Some(180 | -180) => 180,
        Some(270 | -90) => 270,
        _ => 0,
    }
}

pub fn cell_insets(cell: &formdes_model::story::TableCell) -> [f64; 4] {
    cell_insets_of(&cell.attrs)
}

pub fn cell_insets_of(attrs: &formdes_model::attrs::Attrs) -> [f64; 4] {
    match attrs.get("insets") {
        Some(serde_json::Value::Number(n)) => [n.as_f64().unwrap_or(4.0); 4],
        Some(serde_json::Value::Array(a)) if a.len() == 4 => {
            let g = |i: usize| a[i].as_f64().unwrap_or(4.0);
            [g(0), g(1), g(2), g(3)]
        }
        _ => [4.0; 4],
    }
}

pub fn offset_line(l: &mut Line, dx: f64) {
    for r in l.runs.iter_mut().chain(l.adorn.iter_mut()) {
        r.x0 += dx;
        r.x1 += dx;
        for g in &mut r.glyphs {
            g.x += dx;
        }
    }
    for m in &mut l.marks {
        m.x += dx;
    }
    for t in &mut l.tabs {
        t.x0 += dx;
        t.x1 += dx;
    }
    for c in &mut l.carets {
        c.1 += dx;
    }
    for o in &mut l.objects {
        o.1 += dx;
    }
    l.left += dx;
}
