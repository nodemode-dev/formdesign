use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use formdes_model::attrs::{Attrs, CharProps, ListKind, ParaProps};
use formdes_model::ids::ItemId;
use formdes_model::story::{BreakKind, Inline, Marker, Para, Seg, Table};
use formdes_model::style::Styles;

use crate::compose::{LINE_SEP, OBJ, ObjBox, ParaInput, TextRun};

pub trait Context {
    fn inline_size(&self, item: ItemId) -> Option<(f64, f64)>;
    fn variable(&self, name: &str, frame: usize) -> String;
    fn marker(&self, marker: Marker, frame: usize) -> String;
    fn footnote_number(&self, ordinal: usize) -> String;
    /// The number as it starts the footnote itself (with the document's prefix and suffix) and the space after it.
    fn footnote_label(&self, ordinal: usize) -> String {
        format!("{} ", self.footnote_number(ordinal))
    }
    fn hidden(&self, _over: &Attrs) -> bool {
        false
    }
    fn vertical(&self, _frame: usize) -> bool {
        false
    }
    fn exceptions(&self) -> crate::compose::Exceptions {
        crate::compose::Exceptions::default()
    }
}

#[derive(Clone, Debug)]
pub enum Special {
    Anchored { item: ItemId, inline: bool, attrs: Attrs },
    Footnote { ordinal: usize, paras: Vec<Para> },
    Table(Box<Table>),
}

#[derive(Clone, Debug)]
pub struct Built {
    pub input: ParaInput,
    pub chars: Vec<usize>,
    pub specials: Vec<(usize, Special)>,
    pub brk: Option<BreakKind>,
    pub para_style: String,
}

impl Built {
    pub fn byte_of(&self, ch: usize) -> usize {
        self.chars.get(ch).copied().unwrap_or(self.input.text.len())
    }

    pub fn char_of(&self, byte: usize) -> usize {
        match self.chars.binary_search(&byte) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        }
    }

    pub fn table(&self) -> Option<&Table> {
        self.specials.iter().find_map(|(_, s)| if let Special::Table(t) = s { Some(t.as_ref()) } else { None })
    }
}

fn grep_ranges(text: &str, pattern: &str) -> Vec<Range<usize>> {
    let Ok(re) = fancy_regex::Regex::new(pattern) else {
        return Vec::new();
    };
    re.find_iter(text).filter_map(Result::ok).filter(|m| m.end() > m.start()).map(|m| m.start()..m.end()).collect()
}

fn nested_ranges(text: &str, prefix: usize, rules: &[formdes_model::attrs::NestedRule]) -> Vec<(Range<usize>, String)> {
    let mut out = Vec::new();
    let mut at = prefix;
    for rule in rules {
        if at >= text.len() {
            break;
        }
        let start = at;
        let mut count = 0;
        let mut end = text.len();
        let through = rule.through.as_str();
        for (i, c) in text[at..].char_indices() {
            let abs = at + i;
            let hit = match through {
                "words" | "word" => c.is_whitespace() && abs > start && !text[..abs].ends_with(char::is_whitespace),
                "sentences" | "sentence" => matches!(c, '.' | '!' | '?'),
                "characters" | "character" => true,
                "letters" | "letter" => c.is_alphabetic(),
                "digits" | "digit" => c.is_ascii_digit(),
                "tabs" | "tab" => c == '\t',
                "forcedLineBreak" | "lineBreak" => c == LINE_SEP,
                "emSpace" => c == '\u{2003}',
                "enSpace" => c == '\u{2002}',
                "anchoredObject" => c == OBJ,
                s if s.chars().count() == 1 => s.starts_with(c),
                s => s.contains(c),
            };
            if hit {
                count += 1;
                if count >= rule.count.max(1) {
                    end = if rule.inclusive { abs + c.len_utf8() } else { abs };
                    break;
                }
            }
        }
        out.push((start..end, rule.style.clone()));
        at = if rule.inclusive { end } else { end + text[end..].chars().next().map(char::len_utf8).unwrap_or(0) };
    }
    out
}

pub struct ListNo {
    pub n: i64,
    pub parents: Vec<i64>,
}

pub fn build(styles: &Styles, para: &Para, list_number: Option<&ListNo>, footnote_base: usize, ctx: &dyn Context, frame: usize) -> Built {
    build_with(styles, para, list_number, footnote_base, ctx, frame, &[])
}

pub fn build_with(styles: &Styles, para: &Para, list_number: Option<&ListNo>, footnote_base: usize, ctx: &dyn Context, frame: usize, line_styles: &[(Range<usize>, String)]) -> Built {
    let props: ParaProps = styles.para_props(&para.style, &para.over);
    let mut text = String::new();
    let mut chars = Vec::new();
    let mut runs: Vec<(Range<usize>, Option<String>, Attrs)> = Vec::new();
    let mut objects = Vec::new();
    let mut specials = Vec::new();
    let mut brk = None;
    let mut footnotes = 0;
    let mut prefix = 0;
    let base_props = styles.char_props(&para.style, &para.over, None, &Attrs::new());
    if props.list != ListKind::None {
        let marker = match props.list {
            ListKind::Bullet => props.bullet.replace("^t", "\t"),
            _ => {
                let n = list_number.map(|l| l.n).unwrap_or(props.start_at.max(1));
                let shown = formdes_model::attrs::list_number(n, &props.number_style);
                let mut s = props.number_format.replace("^#", &shown).replace("^t", "\t");
                for lvl in 1..=9usize {
                    let tag = format!("^{lvl}");
                    if s.contains(&tag) {
                        let v = if lvl == props.list_level as usize { n } else { list_number.and_then(|l| l.parents.get(lvl - 1).copied()).unwrap_or(1) };
                        s = s.replace(&tag, &formdes_model::attrs::list_number(v, &props.number_style));
                    }
                }
                s
            }
        };
        let m = if marker.contains('\t') { marker } else { format!("{marker}\t") };
        text.push_str(&m);
        runs.push((0..text.len(), None, Attrs::new()));
        prefix = text.len();
    }
    for seg in &para.segs {
        let start = text.len();
        if ctx.hidden(seg.over()) {
            chars.extend(std::iter::repeat_n(start, seg.len()));
            continue;
        }
        match seg {
            Seg::Text { text: t, .. } => {
                chars.extend(t.char_indices().map(|(i, _)| start + i));
                text.push_str(t);
            }
            Seg::Inline { inline, .. } => {
                chars.push(start);
                match inline {
                    Inline::Anchored { item, attrs } => {
                        let inline_pos = attrs.get("position").and_then(|v| v.as_str()).map(|p| p == "inline").unwrap_or(true);
                        text.push(OBJ);
                        if inline_pos {
                            let (w, h) = ctx.inline_size(*item).unwrap_or((0.0, 0.0));
                            let yoff = attrs.get("yOffset").and_then(|v| v.as_f64()).unwrap_or(0.0);
                            objects.push(ObjBox { at: start, width: w, ascent: (h - yoff).max(0.0), descent: yoff.max(0.0) });
                        }
                        specials.push((start, Special::Anchored { item: *item, inline: inline_pos, attrs: attrs.clone() }));
                    }
                    Inline::Footnote { paras } => {
                        footnotes += 1;
                        let ordinal = footnote_base + footnotes;
                        let n = ctx.footnote_number(ordinal);
                        text.push_str(&n);
                        for _ in 1..n.chars().count() {
                            chars.push(start);
                        }
                        let mut sup = Attrs::new();
                        sup.insert("position".into(), serde_json::json!("superscript"));
                        runs.push((start..text.len(), seg.style().map(str::to_string), merge(seg.over(), &sup)));
                        specials.push((start, Special::Footnote { ordinal, paras: paras.clone() }));
                        continue;
                    }
                    Inline::Table { table } => {
                        text.push(OBJ);
                        specials.push((start, Special::Table(table.clone())));
                    }
                    Inline::Variable { name } => {
                        let v = ctx.variable(name, frame);
                        text.push_str(&v);
                        for _ in 1..v.chars().count() {
                            chars.push(start);
                        }
                    }
                    Inline::Marker { marker } => {
                        let v = ctx.marker(*marker, frame);
                        text.push_str(&v);
                        for _ in 1..v.chars().count() {
                            chars.push(start);
                        }
                    }
                    Inline::Break { brk: kind } => {
                        brk = Some(*kind);
                        text.push(LINE_SEP);
                    }
                    Inline::IndentHere => text.push('\u{200B}'),
                    Inline::RightTab => text.push('\t'),
                    Inline::Note { .. } | Inline::Index { .. } | Inline::Anchor { .. } => {}
                    Inline::Xref { text: v, .. } => {
                        text.push_str(v);
                        for _ in 1..v.chars().count() {
                            chars.push(start);
                        }
                    }
                }
            }
        }
        if text.len() > start {
            runs.push((start..text.len(), seg.style().map(str::to_string), seg.over().clone()));
        }
    }
    let mut over_styles: Vec<(Range<usize>, String)> = line_styles.to_vec();
    over_styles.extend(nested_ranges(&text, prefix, &props.nested));
    for g in &props.grep {
        for r in grep_ranges(&text, &g.pattern) {
            if r.start >= prefix {
                over_styles.push((r, g.style.clone()));
            }
        }
    }
    let mut cache: HashMap<(Option<String>, String, Option<String>), Arc<CharProps>> = HashMap::new();
    let mut out_runs: Vec<TextRun> = Vec::new();
    for (range, style, over) in runs {
        let mut cuts: Vec<usize> = vec![range.start, range.end];
        for (r, _) in &over_styles {
            for p in [r.start, r.end] {
                if p > range.start && p < range.end {
                    cuts.push(p);
                }
            }
        }
        cuts.sort_unstable();
        cuts.dedup();
        for w in cuts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let applied = over_styles.iter().rev().find(|(r, _)| r.start <= a && b <= r.end).map(|(_, s)| s.clone());
            let key = (style.clone(), serde_json::to_string(&over).unwrap_or_default(), applied.clone());
            let cp = cache
                .entry(key)
                .or_insert_with(|| {
                    let mut stack = styles.char_stack(&para.style, &para.over, applied.as_deref(), &over);
                    if let Some(cs) = style.as_deref() {
                        let extra: Vec<_> = formdes_model::style::char_stack_names(cs).iter().flat_map(|n| styles.chain(formdes_model::style::StyleKind::Character, n)).collect();
                        let at = stack.0.len() - 1;
                        for (k, s) in extra.into_iter().enumerate() {
                            stack.0.insert(at + k, s);
                        }
                    }
                    Arc::new(CharProps::resolve(&stack))
                })
                .clone();
            out_runs.push(TextRun { range: a..b, props: cp });
        }
    }
    if out_runs.is_empty() {
        out_runs.push(TextRun { range: 0..0, props: Arc::new(base_props) });
    }
    Built {
        input: ParaInput { text, runs: out_runs, objects, props, prefix, exceptions: ctx.exceptions(), vertical: ctx.vertical(frame) },
        chars,
        specials,
        brk,
        para_style: para.style.clone(),
    }
}

fn merge(a: &Attrs, b: &Attrs) -> Attrs {
    let mut o = a.clone();
    for (k, v) in b {
        o.insert(k.clone(), v.clone());
    }
    o
}
