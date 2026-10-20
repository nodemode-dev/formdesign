use std::ops::Range;
use std::sync::Arc;

use fancy_regex::{Captures, Regex, RegexBuilder};
use formdesign_engine::canvas::Canvas;
use formdesign_engine::tools::geo;
use formdesign_engine::{Selection, TextSel, wire};
use formdesign_model::doc::Doc;
use formdesign_model::geom::Rect;
use formdesign_model::ids::{ItemId, StoryId};
use formdesign_model::item::{Item, ItemKind, Shape};
use formdesign_model::story::Story;
use formdesign_model::style::{StyleKind, style_key};
use formdesign_text::layout::DocLayout;
use serde_json::{Map, Value, json};

use crate::core::Core;
use crate::textloc::{self, Loc, Sub, byte_to_char, char_to_byte, marker_text, structural};

const HSPACE: &str = r"\t \x{A0}\x{202F}\x{2000}-\x{200A}\x{205F}\x{3000}";

fn s<'a>(p: &'a Value, k: &str) -> Option<&'a str> {
    p.get(k).and_then(Value::as_str)
}

fn flag(p: &Value, k: &str) -> bool {
    p.get(k).and_then(Value::as_bool).unwrap_or(false)
}

#[derive(Default)]
struct Fmt {
    para: Option<String>,
    chr: Option<String>,
    attrs: Map<String, Value>,
}

impl Fmt {
    fn parse(v: Option<&Value>) -> Result<Fmt, String> {
        let mut f = Fmt::default();
        let mut rest = Map::new();
        for (k, v) in v.and_then(Value::as_object).into_iter().flatten() {
            if v.is_null() || v.as_str() == Some("") {
                continue;
            }
            match k.as_str() {
                "paragraphStyle" => f.para = v.as_str().map(style_key),
                "characterStyle" => f.chr = v.as_str().map(style_key),
                _ => {
                    rest.insert(k.clone(), v.clone());
                }
            }
        }
        if !rest.is_empty() {
            f.attrs = wire::char_in(&rest).map_err(|e| e.0)?;
        }
        Ok(f)
    }

    fn is_empty(&self) -> bool {
        self.para.is_none() && self.chr.is_none() && self.attrs.is_empty()
    }
}

struct Spec {
    re: Option<Regex>,
    grep: bool,
    literal: Option<String>,
    change: String,
    find: Fmt,
    to: Fmt,
}

impl Spec {
    fn parse(p: &Value) -> Result<Spec, String> {
        let mode = s(p, "mode").unwrap_or("text");
        let mut find_raw = p.get("findFormat").cloned().unwrap_or(Value::Null);
        let mut to_raw = p.get("changeFormat").cloned().unwrap_or(Value::Null);
        let mut literal = None;
        let pat = match mode {
            "grep" => grep_pattern(s(p, "find").unwrap_or("")),
            "glyph" => {
                let g = p.get("glyph").ok_or("missing `glyph`")?;
                let c = glyph_char(g).ok_or("Choose the glyph to find.")?;
                copy_font(g, &mut find_raw);
                if let Some(t) = p.get("toGlyph").filter(|t| t.is_object()) {
                    literal = glyph_char(t).map(String::from);
                    copy_font(t, &mut to_raw);
                }
                esc(&c.to_string())
            }
            _ => text_pattern(s(p, "find").unwrap_or("")),
        };
        let find = Fmt::parse(Some(&find_raw))?;
        let to = Fmt::parse(Some(&to_raw))?;
        let re = if pat.is_empty() {
            if find.is_empty() {
                return Err("Type something to find, or set a Find Format.".into());
            }
            None
        } else {
            let pat = if mode == "text" && flag(p, "wholeWord") { format!(r"(?<![\p{{L}}\p{{N}}_])(?:{pat})(?![\p{{L}}\p{{N}}_])") } else { pat };
            let ci = mode != "glyph" && !flag(p, "caseSensitive");
            Some(RegexBuilder::new(&pat).case_insensitive(ci).multi_line(true).build().map_err(|e| format!("This GREP does not parse: {e}"))?)
        };
        Ok(Spec { re, grep: mode == "grep", literal, change: s(p, "change").unwrap_or("").to_string(), find, to })
    }

    fn replaces_text(&self) -> bool {
        self.literal.is_some() || !self.change.is_empty() || self.to.is_empty()
    }

    fn replacement(&self, caps: Option<&Captures<'_, str>>) -> String {
        match &self.literal {
            Some(l) => l.clone(),
            None => expand(&self.change, self.grep, caps),
        }
    }
}

fn glyph_char(v: &Value) -> Option<char> {
    let t = v.get("char").and_then(Value::as_str)?.trim();
    if let Some(h) = t.strip_prefix("U+").or_else(|| t.strip_prefix("u+")) {
        return u32::from_str_radix(h, 16).ok().and_then(char::from_u32);
    }
    t.chars().next()
}

fn copy_font(v: &Value, f: &mut Value) {
    if !f.is_object() {
        *f = json!({});
    }
    for k in ["fontFamily", "fontStyle"] {
        if let Some(x) = v.get(k).and_then(Value::as_str).filter(|x| !x.is_empty()) {
            f[k] = json!(x);
        }
    }
}

fn esc(t: &str) -> String {
    let mut o = String::with_capacity(t.len() + 4);
    for c in t.chars() {
        if r"\.+*?()|[]{}^$".contains(c) {
            o.push('\\');
        }
        o.push(c);
    }
    o
}

fn special(n: char, grep: bool) -> Option<char> {
    Some(match n {
        's' => {
            if grep {
                '\u{202F}'
            } else {
                '\u{A0}'
            }
        }
        'S' => {
            if grep {
                '\u{A0}'
            } else {
                '\u{202F}'
            }
        }
        'N' => '\u{E000}',
        'X' => '\u{E007}',
        'V' => '\u{E008}',
        'x' => '\u{E001}',
        'M' => '\u{E002}',
        'R' => '\u{E003}',
        'P' => '\u{E004}',
        'i' => '\u{E005}',
        'y' => '\u{E006}',
        'F' => textloc::FOOTNOTE_REF,
        'a' => '\u{E00E}',
        'I' => '\u{E00D}',
        '_' => '\u{2014}',
        '=' => '\u{2013}',
        '-' => '\u{AD}',
        '~' => '\u{2011}',
        'm' => '\u{2003}',
        '>' => '\u{2002}',
        'f' => '\u{2001}',
        '|' => '\u{200A}',
        '<' => '\u{2009}',
        '/' => '\u{2007}',
        '.' => '\u{2008}',
        '3' => '\u{2004}',
        '4' => '\u{2005}',
        '%' => '\u{2006}',
        'k' => '\u{200B}',
        'j' => '\u{200C}',
        '8' => '\u{2022}',
        'e' => '\u{2026}',
        '2' => '\u{A9}',
        'r' => '\u{AE}',
        'd' => '\u{2122}',
        '6' => '\u{A7}',
        '7' => '\u{B6}',
        '{' => '\u{201C}',
        '}' => '\u{201D}',
        '[' => '\u{2018}',
        ']' => '\u{2019}',
        _ => return None,
    })
}

fn hex(c: char) -> String {
    format!(r"\x{{{:X}}}", c as u32)
}

fn text_pattern(f: &str) -> String {
    let mut o = String::new();
    let mut it = f.chars().peekable();
    while let Some(c) = it.next() {
        if c == '^'
            && let Some(&n) = it.peek()
        {
            let w = match n {
                'p' => Some(r"\n".to_string()),
                't' => Some(r"\t".into()),
                'n' => Some(r"\x{2028}".into()),
                '^' => Some(r"\^".into()),
                '#' => Some(r"[\x{E000}\x{E007}\x{E008}]".into()),
                '9' => Some(r"\d".into()),
                '$' => Some(r"\p{L}".into()),
                '?' => Some(r"[^\n]".into()),
                'w' => Some(format!("[{HSPACE}]")),
                '"' => Some(r#"["\x{201C}\x{201D}]"#.into()),
                '\'' => Some(r"['\x{2018}\x{2019}]".into()),
                _ => special(n, false).map(hex),
            };
            if let Some(w) = w {
                it.next();
                o.push_str(&w);
                continue;
            }
        }
        o.push_str(&esc(&c.to_string()));
    }
    o
}

fn grep_pattern(f: &str) -> String {
    let mut o = String::new();
    let mut it = f.chars().peekable();
    let mut class = false;
    while let Some(c) = it.next() {
        match c {
            '\\' => {
                let Some(n) = it.next() else {
                    o.push_str(r"\\");
                    break;
                };
                match n {
                    'r' => o.push_str(r"\n"),
                    'n' => o.push_str(r"\x{2028}"),
                    'h' if class => o.push_str(HSPACE),
                    'h' => o.push_str(&format!("[{HSPACE}]")),
                    'v' if class => o.push_str(r"\n\x{2028}"),
                    'v' => o.push_str(r"[\n\x{2028}]"),
                    'l' => o.push_str(r"\p{Ll}"),
                    'u' => o.push_str(r"\p{Lu}"),
                    '<' if !class => o.push_str(r"\b(?=\w)"),
                    '>' if !class => o.push_str(r"\b(?<=\w)"),
                    _ => {
                        o.push('\\');
                        o.push(n);
                    }
                }
            }
            '~' => {
                let set = |inner: &str| if class { inner.to_string() } else { format!("[{inner}]") };
                let w = match it.peek().copied() {
                    Some('#') => Some(set(r"\x{E000}\x{E007}\x{E008}")),
                    Some('"') => Some(set(r#""\x{201C}\x{201D}"#)),
                    Some('\'') => Some(set(r"'\x{2018}\x{2019}")),
                    Some('b') => Some(r"\n".to_string()),
                    Some(n) => special(n, true).map(hex),
                    None => None,
                };
                match w {
                    Some(w) => {
                        it.next();
                        o.push_str(&w);
                    }
                    None => o.push('~'),
                }
            }
            '[' if !class => {
                class = true;
                o.push('[');
                if it.peek() == Some(&'^') {
                    it.next();
                    o.push('^');
                }
                if it.peek() == Some(&']') {
                    it.next();
                    o.push_str(r"\]");
                }
            }
            ']' if class => {
                class = false;
                o.push(']');
            }
            _ => o.push(c),
        }
    }
    o
}

fn expand(rep: &str, grep: bool, caps: Option<&Captures<'_, str>>) -> String {
    let mut o = String::new();
    let mut it = rep.chars().peekable();
    while let Some(c) = it.next() {
        if grep {
            match c {
                '$' => {
                    if let Some(d) = it.peek().and_then(|d| d.to_digit(10)) {
                        it.next();
                        if let Some(m) = caps.and_then(|c| c.get(d as usize)) {
                            o.extend(m.as_str().chars().filter(|ch| !structural(*ch)));
                        }
                        continue;
                    }
                }
                '\\' => {
                    match it.next() {
                        Some('r') => o.push('\n'),
                        Some('n') => o.push('\u{2028}'),
                        Some('t') => o.push('\t'),
                        Some(x) => o.push(x),
                        None => o.push('\\'),
                    }
                    continue;
                }
                '~' => {
                    if let Some(ch) = it.peek().and_then(|n| special(*n, true)) {
                        it.next();
                        if !structural(ch) {
                            o.push(ch);
                        }
                        continue;
                    }
                }
                _ => {}
            }
        } else if c == '^'
            && let Some(&n) = it.peek()
        {
            let ch = match n {
                'p' => Some('\n'),
                't' => Some('\t'),
                'n' => Some('\u{2028}'),
                '^' => Some('^'),
                _ => special(n, false),
            };
            if let Some(ch) = ch {
                it.next();
                if !structural(ch) {
                    o.push(ch);
                }
                continue;
            }
        }
        o.push(c);
    }
    o
}

fn same(have: &Value, want: &Value) -> bool {
    match (have, want) {
        (Value::Number(a), Value::Number(b)) => (a.as_f64().unwrap_or(0.0) - b.as_f64().unwrap_or(0.0)).abs() < 1e-3,
        (h, w) => {
            let (pa, pb) = (formdesign_model::color::Paint::from_value(h), formdesign_model::color::Paint::from_value(w));
            h == w || (pa.is_some() && pa == pb)
        }
    }
}

fn fmt_at(doc: &Doc, story: &Story, pos: usize, f: &Fmt) -> bool {
    let (pi, off) = story.locate(pos);
    let para = &story.paras[pi];
    if f.para.as_ref().is_some_and(|ps| para.style != *ps) {
        return false;
    }
    let (cs, over) = para.format_at(off + 1);
    if let Some(want) = &f.chr {
        let have = cs.clone().unwrap_or_else(|| "None".into());
        if have != *want {
            return false;
        }
    }
    if f.attrs.is_empty() {
        return true;
    }
    let stack = doc.styles.char_stack(&para.style, &para.over, cs.as_deref(), &over);
    f.attrs.iter().all(|(k, want)| same(&stack.get(k), want))
}

fn cuts(story: &Story, a: usize, b: usize) -> Vec<usize> {
    let mut out = vec![a];
    let mut pos = 0;
    for (i, p) in story.paras.iter().enumerate() {
        if i > 0 && pos > a && pos < b {
            out.push(pos);
        }
        for seg in &p.segs {
            if pos > a && pos < b {
                out.push(pos);
            }
            pos += seg.len();
        }
        pos += 1;
    }
    out.sort();
    out.dedup();
    out
}

fn fmt_ok(doc: &Doc, story: &Story, r: Range<usize>, f: &Fmt) -> bool {
    f.is_empty() || cuts(story, r.start, r.end).into_iter().all(|p| fmt_at(doc, story, p, f))
}

fn format_runs(doc: &Doc, story: &Story, r: Range<usize>, f: &Fmt, mut emit: impl FnMut(Range<usize>)) {
    for pi in 0..story.paras.len() {
        let pr = story.para_range(pi);
        let (a, b) = (pr.start.max(r.start), pr.end.min(r.end));
        if a >= b {
            continue;
        }
        let mut cs = cuts(story, a, b);
        cs.push(b);
        let mut cur: Option<usize> = None;
        for w in cs.windows(2) {
            if fmt_at(doc, story, w[0], f) {
                cur.get_or_insert(w[0]);
            } else if let Some(s0) = cur.take() {
                emit(s0..w[0]);
            }
        }
        if let Some(s0) = cur {
            emit(s0..b);
        }
    }
}

#[derive(Clone, Copy)]
struct Incl {
    locked: bool,
    hidden: bool,
    parents: bool,
}

impl Incl {
    fn parse(p: &Value) -> Incl {
        let i = p.get("include").cloned().unwrap_or(Value::Null);
        Incl { locked: flag(&i, "locked"), hidden: flag(&i, "hidden"), parents: flag(&i, "parents") }
    }
}

struct Flags {
    parent: bool,
    hidden: bool,
    locked: bool,
    key: (usize, f64, f64),
}

fn item_flags(doc: &Doc, id: ItemId) -> Option<Flags> {
    let it = doc.item(id)?;
    let top = doc.top_of(id);
    let (parent, si) = match (doc.spread_of(top), doc.parent_of(top)) {
        (Some(i), _) => (false, i),
        (None, Some(i)) => (true, 1_000_000 + i),
        _ => (false, usize::MAX),
    };
    let layer = doc.layer(it.layer);
    let b = doc.bounds(id).unwrap_or_default();
    Some(Flags { parent, hidden: it.hidden || layer.is_some_and(|l| !l.visible), locked: it.locked || layer.is_some_and(|l| l.locked), key: (si, b.y0, b.x0) })
}

fn story_locked(doc: &Doc, sid: StoryId) -> bool {
    doc.story(sid).and_then(|s| s.frames.first().copied()).and_then(|f| item_flags(doc, f)).is_some_and(|f| f.locked)
}

fn allowed(f: &Flags, inc: Incl) -> bool {
    !(f.parent && !inc.parents || f.hidden && !inc.hidden || f.locked && !inc.locked)
}

struct Target {
    loc: Loc,
    text: String,
    range: Range<usize>,
    locked: bool,
}

fn target(doc: &Doc, loc: Loc, locked: bool) -> Option<Target> {
    let st = textloc::view(doc, loc)?;
    let text = marker_text(&st.paras);
    let n = text.len();
    Some(Target { loc, text, range: 0..n, locked })
}

fn places(doc: &Doc, inc: Incl) -> Vec<Target> {
    let mut keyed = Vec::new();
    for st in doc.stories.values() {
        let Some(f) = st.frames.first().and_then(|f| item_flags(doc, *f)) else { continue };
        if allowed(&f, inc) {
            keyed.push((f.key, st.id, f.locked));
        }
    }
    keyed.sort_by(|a, b| a.0.0.cmp(&b.0.0).then(a.0.1.total_cmp(&b.0.1)).then(a.0.2.total_cmp(&b.0.2)));
    let mut out = Vec::new();
    for (_, sid, locked) in keyed {
        for loc in textloc::locs(doc, sid) {
            if let Some(t) = target(doc, loc, locked) {
                out.push(t);
            }
        }
    }
    out
}

fn sel_loc(doc: &Doc, sel: &Selection) -> Option<(Loc, Range<usize>)> {
    let t = sel.text?;
    let st = doc.story(t.story)?;
    let text = marker_text(&st.paras);
    let r = t.range();
    Some((Loc::main(t.story), char_to_byte(&text, r.start)..char_to_byte(&text, r.end)))
}

fn targets(doc: &Doc, sel: &Selection, p: &Value, inc: Incl) -> Result<Vec<Target>, String> {
    if let Some(w) = p.get("within").filter(|w| w.is_object()) {
        let loc = Loc::parse(w).ok_or("bad `within`")?;
        let mut t = target(doc, loc, story_locked(doc, loc.story)).ok_or("bad `within`")?;
        let n = t.text.len();
        let a = (w.get("start").and_then(Value::as_u64).unwrap_or(0) as usize).min(n);
        let b = w.get("end").and_then(Value::as_u64).map_or(n, |e| e as usize).clamp(a, n);
        t.range = a..b;
        return Ok(vec![t]);
    }
    let sel_story = || {
        sel.text.map(|t| t.story).or_else(|| match sel.items.as_slice() {
            [id] => doc.item(*id).and_then(Item::story),
            _ => None,
        })
    };
    match s(p, "scope").unwrap_or("document") {
        "story" => {
            let sid = sel_story().ok_or("Select text or a text frame to search its story.")?;
            let all = Incl { locked: true, hidden: true, parents: true };
            Ok(places(doc, all).into_iter().filter(|t| t.loc.story == sid).collect())
        }
        "toEnd" => {
            let (loc, r) = sel_loc(doc, sel).ok_or("Click in a story to search to its end.")?;
            let mut t = target(doc, loc, story_locked(doc, loc.story)).ok_or("Click in a story to search to its end.")?;
            t.range = r.start..t.text.len();
            Ok(vec![t])
        }
        "selection" => {
            let (loc, r) = sel_loc(doc, sel).filter(|(_, r)| !r.is_empty()).ok_or("Select some text to search within it.")?;
            let mut t = target(doc, loc, story_locked(doc, loc.story)).ok_or("Select some text to search within it.")?;
            t.range = r;
            Ok(vec![t])
        }
        _ => Ok(places(doc, inc)),
    }
}

struct Hit {
    loc: Loc,
    range: Range<usize>,
    rep: String,
    locked: bool,
}

fn hits(doc: &Doc, spec: &Spec, t: &Target, limit: usize, out: &mut Vec<Hit>) {
    let Some(story) = textloc::view(doc, t.loc) else { return };
    let text = t.text.as_str();
    let r = t.range.start.min(text.len())..t.range.end.min(text.len());
    let chars = |b: Range<usize>| byte_to_char(text, b.start)..byte_to_char(text, b.end);
    match &spec.re {
        Some(re) => {
            let mut pos = r.start;
            while pos <= r.end && out.len() < limit {
                let Ok(Some(c)) = re.captures_from_pos(text, pos) else { break };
                let Some(m) = c.get(0) else { break };
                let (a, b) = (m.start(), m.end());
                if b > r.end {
                    break;
                }
                if a == b {
                    match text[a..].chars().next() {
                        Some(ch) => {
                            pos = a + ch.len_utf8();
                            continue;
                        }
                        None => break,
                    }
                }
                if fmt_ok(doc, &story, chars(a..b), &spec.find) {
                    out.push(Hit { loc: t.loc, range: a..b, rep: spec.replacement(Some(&c)), locked: t.locked });
                }
                pos = b;
            }
        }
        None => format_runs(doc, &story, chars(r), &spec.find, |x| {
            if out.len() < limit {
                out.push(Hit { loc: t.loc, range: char_to_byte(text, x.start)..char_to_byte(text, x.end), rep: spec.replacement(None), locked: t.locked });
            }
        }),
    }
}

fn tail(t: &str, n: usize) -> String {
    let v: Vec<char> = t.chars().collect();
    v[v.len().saturating_sub(n)..].iter().collect()
}

fn head(t: &str, n: usize) -> String {
    t.chars().take(n).collect()
}

struct View {
    doc: Arc<Doc>,
    lay: Arc<DocLayout>,
    cv: Canvas,
}

fn view_of(core: &mut Core) -> Result<View, String> {
    let r = core.ses.renderer.clone();
    let st = core.ses.active_mut().ok_or("no document open")?;
    let lay = st.layout(&r);
    Ok(View { doc: st.doc.clone(), lay, cv: st.canvas() })
}

fn locate(v: &View, loc: Loc, range: Range<usize>) -> (Option<ItemId>, Option<[f64; 4]>) {
    let doc = &v.doc;
    let Some(main) = doc.story(loc.story) else { return (None, None) };
    let mt = marker_text(&main.paras);
    let cr = match loc.sub {
        None => byte_to_char(&mt, range.start)..byte_to_char(&mt, range.end),
        Some(_) => match textloc::anchor_of(doc, loc) {
            Some(a) => a..a + 1,
            None => return (None, None),
        },
    };
    let spans = textloc::spans(doc, &v.lay, &v.cv, loc.story, cr);
    let mut out: Option<Rect> = None;
    let frame = spans.first().map(|s| s.frame);
    for sp in &spans {
        let x1 = sp.x1.max(sp.x0 + 2.0);
        for (x, y) in [(sp.x0, sp.base - sp.asc), (x1, sp.base + sp.desc), (sp.x0, sp.base + sp.desc), (x1, sp.base - sp.asc)] {
            let q = sp.m * kurbo::Point::new(x, y);
            let r = Rect::new(q.x, q.y, q.x, q.y);
            out = Some(out.map_or(r, |o| o.union(r)));
        }
    }
    (frame, out.map(|r| [r.x0, r.y0, r.x1, r.y1]))
}

fn hit_json(v: &View, h: &Hit, context: bool) -> Value {
    let text = textloc::view(&v.doc, h.loc).map(|s| marker_text(&s.paras)).unwrap_or_default();
    let r = h.range.start.min(text.len())..h.range.end.min(text.len());
    let mut out = h.loc.json();
    out["start"] = json!(h.range.start);
    out["end"] = json!(h.range.end);
    out["text"] = json!(text.get(r.clone()).unwrap_or(""));
    out["locked"] = json!(h.locked);
    let (frame, rect) = locate(v, h.loc, h.range.clone());
    out["page"] = json!(frame.and_then(|f| v.doc.page_of(f)));
    out["overset"] = json!(frame.is_none() && h.loc.sub.is_none());
    if context {
        out["before"] = json!(tail(text.get(..r.start).unwrap_or(""), 32));
        out["after"] = json!(head(text.get(r.end..).unwrap_or(""), 32));
    } else {
        out["rect"] = json!(rect);
    }
    out
}

fn select(core: &mut Core, loc: Loc, range: Range<usize>) -> Result<Value, String> {
    let v = view_of(core)?;
    let main = v.doc.story(loc.story).ok_or("no such story")?;
    let mt = marker_text(&main.paras);
    let (a, b) = match loc.sub {
        None => (byte_to_char(&mt, range.start), byte_to_char(&mt, range.end)),
        Some(_) => {
            let at = textloc::anchor_of(&v.doc, loc).unwrap_or(0);
            (at, at + 1)
        }
    };
    let locked = story_locked(&v.doc, loc.story);
    core.ses.select(Selection::text(TextSel { story: loc.story, anchor: a, focus: b }));
    Ok(hit_json(&v, &Hit { loc, range, rep: String::new(), locked }, false))
}

fn next(core: &mut Core, spec: &Spec, p: &Value, inc: Incl) -> Result<Value, String> {
    let v = view_of(core)?;
    let sel = core.ses.active().map(|s| s.selection.clone()).unwrap_or_default();
    let ts = targets(&v.doc, &sel, p, inc)?;
    let n = ts.len();
    if n == 0 {
        return Ok(json!({"match": null, "wrapped": false}));
    }
    let at = sel_loc(&v.doc, &sel).and_then(|(l, r)| ts.iter().position(|x| x.loc == l).map(|i| (i, r.end)));
    let (start, pos) = at.unwrap_or((0, ts[0].range.start));
    let rounds = if at.is_some() { n + 1 } else { n };
    for k in 0..rounds {
        let i = (start + k) % n;
        let t = &ts[i];
        let mut range = t.range.clone();
        if k == 0 {
            range.start = pos.clamp(range.start, range.end);
        }
        let mut hs = Vec::new();
        hits(&v.doc, spec, &Target { loc: t.loc, text: t.text.clone(), range, locked: t.locked }, 1, &mut hs);
        let Some(h) = hs.pop() else { continue };
        if k == n && h.range.start >= pos {
            break;
        }
        let m = select(core, h.loc, h.range)?;
        return Ok(json!({"match": m, "wrapped": start + k >= n || k == n}));
    }
    Ok(json!({"match": null, "wrapped": false}))
}

fn apply(core: &mut Core, label: &str, mut hs: Vec<Hit>, spec: &Spec, select_last: bool) -> Result<Value, String> {
    let replace = spec.replaces_text();
    let over = spec.to.attrs.clone();
    let para = spec.to.para.clone();
    let chr = spec.to.chr.clone();
    let key = |l: &Loc| {
        let sub = match l.sub {
            None => (0, 0, 0, 0),
            Some(Sub::Cell { at, row, col }) => (1, at, row, col),
            Some(Sub::Note { n }) => (2, n, 0, 0),
        };
        (l.story.0, sub)
    };
    hs.sort_by(|a, b| key(&a.loc).cmp(&key(&b.loc)).then(b.range.start.cmp(&a.range.start)));
    let mut count = 0usize;
    let mut last: Option<(Loc, Range<usize>)> = None;
    let mut failure: Option<String> = None;
    core.ses.edit_now(label, |d, sel| {
        let mut i = 0;
        while i < hs.len() {
            let loc = hs[i].loc;
            let Some(mut st) = textloc::view(d, loc) else {
                i += 1;
                continue;
            };
            while i < hs.len() && hs[i].loc == loc {
                let h = &hs[i];
                let text = marker_text(&st.paras);
                let (a, b) = (byte_to_char(&text, h.range.start), byte_to_char(&text, h.range.end));
                let end = if replace {
                    st.delete(a..b);
                    let before = st.len();
                    formdesign_engine::cmd::object::put_text(&mut st, a, &h.rep);
                    a + (st.len() - before)
                } else {
                    b
                };
                if end > a {
                    if let Some(ps) = &para {
                        let r = st.paras_in(a..end);
                        st.set_para_style(r, ps, false);
                    }
                    if let Some(cs) = &chr {
                        st.set_char_style(a..end, if cs == "None" { None } else { Some(cs.as_str()) }, false);
                    }
                    if !over.is_empty()
                        && let Err(e) = st.set_char_attrs(a..end, &over)
                    {
                        failure = Some(e.0);
                    }
                }
                count += 1;
                if loc.sub.is_none() {
                    last = Some((loc, a..end));
                }
                i += 1;
            }
            textloc::put(d, loc, st);
        }
        match (&last, select_last) {
            (Some((l, r)), true) => *sel = Selection::text(TextSel { story: l.story, anchor: r.start, focus: r.end }),
            _ => {
                if let Some(t) = sel.text.as_mut() {
                    let n = d.story(t.story).map_or(0, Story::len);
                    t.anchor = t.anchor.min(n);
                    t.focus = t.focus.min(n);
                }
            }
        }
    });
    if let Some(e) = failure {
        return Err(e);
    }
    Ok(json!({"changed": count}))
}

fn guarded(doc: &Doc, spec: &Spec, h: &Hit) -> bool {
    spec.replaces_text() && textloc::view(doc, h.loc).is_some_and(|s| marker_text(&s.paras).get(h.range.clone()).is_some_and(|t| t.chars().any(structural)))
}

fn change_current(core: &mut Core, spec: &Spec) -> Result<Value, String> {
    let st = core.ses.active().ok_or("no document open")?;
    let doc = st.doc.clone();
    let sel = st.selection.clone();
    let Some((loc, r)) = sel_loc(&doc, &sel).filter(|(_, r)| !r.is_empty()) else { return Ok(json!({"changed": 0})) };
    let Some(mut t) = target(&doc, loc, false) else { return Ok(json!({"changed": 0})) };
    t.range = r.clone();
    let mut hs = Vec::new();
    hits(&doc, spec, &t, 64, &mut hs);
    let Some(h) = hs.into_iter().find(|h| h.range == r) else { return Ok(json!({"changed": 0})) };
    if story_locked(&doc, loc.story) {
        return Err("That text is on a locked layer or in a locked frame, so it cannot be changed.".into());
    }
    if guarded(&doc, spec, &h) {
        return Err("This match holds a footnote, table, anchored object or marker, so Change leaves it alone.".into());
    }
    apply(core, "Change", vec![h], spec, true)
}

fn change_all(core: &mut Core, spec: &Spec, p: &Value, inc: Incl) -> Result<Value, String> {
    let st = core.ses.active().ok_or("no document open")?;
    let doc = st.doc.clone();
    let sel = st.selection.clone();
    let mut hs = Vec::new();
    for t in targets(&doc, &sel, p, inc)? {
        if !t.locked {
            hits(&doc, spec, &t, usize::MAX, &mut hs);
        }
    }
    let (keep, skip): (Vec<Hit>, Vec<Hit>) = hs.into_iter().partition(|h| !guarded(&doc, spec, h));
    if keep.is_empty() {
        return Ok(json!({"changed": 0, "skipped": skip.len()}));
    }
    let mut r = apply(core, "Change All", keep, spec, false)?;
    r["skipped"] = json!(skip.len());
    Ok(r)
}

struct ObjQuery {
    kind: Option<String>,
    fill: Option<String>,
    stroke: Option<String>,
    weight: Option<f64>,
    opacity: Option<f64>,
    layer: Option<String>,
    style: Option<String>,
}

impl ObjQuery {
    fn parse(v: Option<&Value>) -> ObjQuery {
        let v = v.cloned().unwrap_or(Value::Null);
        let st = |k: &str| v.get(k).and_then(Value::as_str).filter(|x| !x.is_empty()).map(str::to_string);
        ObjQuery {
            kind: st("kind"),
            fill: st("fill"),
            stroke: st("stroke"),
            weight: v.get("strokeWeight").and_then(Value::as_f64),
            opacity: v.get("opacity").and_then(Value::as_f64),
            layer: st("layer"),
            style: st("objectStyle"),
        }
    }

    fn is_empty(&self) -> bool {
        self.kind.is_none() && self.fill.is_none() && self.stroke.is_none() && self.weight.is_none() && self.opacity.is_none() && self.layer.is_none() && self.style.is_none()
    }

    fn matches(&self, doc: &Doc, it: &Item) -> bool {
        self.kind.as_deref().is_none_or(|k| kind(it) == k)
            && self.fill.as_ref().is_none_or(|f| wire::paint_name(&it.fill) == wire::swatch_name(f))
            && self.stroke.as_ref().is_none_or(|c| wire::paint_name(&it.stroke.paint) == wire::swatch_name(c) && it.stroke.weight > 0.0)
            && self.weight.is_none_or(|w| (it.stroke.weight - w).abs() < 1e-6)
            && self.opacity.is_none_or(|o| (it.opacity - o).abs() < 1e-3)
            && self.layer.as_ref().is_none_or(|n| doc.layer(it.layer).is_some_and(|l| l.name == *n))
            && self.style.as_ref().is_none_or(|n| wire::style_name(StyleKind::Object, it.style.as_deref().unwrap_or("None")) == wire::style_name(StyleKind::Object, &style_key(n)))
    }
}

fn kind(it: &Item) -> &'static str {
    match (&it.kind, &it.shape) {
        (ItemKind::Group { .. } | ItemKind::States { .. }, _) => "group",
        (ItemKind::Text(_), _) => "text",
        (ItemKind::Graphic(_), _) => "graphic",
        (_, Shape::Line { .. }) => "line",
        _ => "shape",
    }
}

fn object_hits(doc: &Doc, q: &ObjQuery, inc: Incl) -> Vec<(ItemId, bool)> {
    let mut tops: Vec<ItemId> = doc.spreads.iter().flat_map(|s| s.items.iter().copied()).collect();
    if inc.parents {
        tops.extend(doc.parents.iter().flat_map(|p| p.items.iter().copied()));
    }
    let mut out = Vec::new();
    for id in doc.walk(&tops) {
        let Some(it) = doc.item(id) else { continue };
        if q.matches(doc, it)
            && let Some(f) = item_flags(doc, id)
            && allowed(&f, inc)
        {
            out.push((id, f.locked));
        }
    }
    out
}

fn object_json(v: &View, id: ItemId, locked: bool) -> Value {
    let doc = &v.doc;
    let Some(it) = doc.item(id) else { return Value::Null };
    let rect = geo::spread_of_item(doc, doc.top_of(id)).and_then(|r| v.cv.slot(r)).zip(doc.bounds(id)).map(|(s, b)| {
        let r = s.xf.transform_rect_bbox(b);
        [r.x0, r.y0, r.x1, r.y1]
    });
    json!({"id": id.0, "kind": kind(it), "name": it.name, "style": wire::style_name(StyleKind::Object, it.style.as_deref().unwrap_or("None")), "page": doc.page_of(id), "locked": locked, "rect": rect})
}

fn objects(core: &mut Core, p: &Value) -> Result<Value, String> {
    let q = ObjQuery::parse(p.get("object"));
    if q.is_empty() {
        return Err("Set at least one attribute to find objects by.".into());
    }
    let inc = Incl::parse(p);
    let v = view_of(core)?;
    let sel = core.ses.active().map(|s| s.selection.clone()).unwrap_or_default();
    let found = object_hits(&v.doc, &q, inc);
    let op = s(p, "op").unwrap_or("all");
    match op {
        "all" => Ok(json!({"total": found.len(), "matches": found.iter().map(|(id, l)| object_json(&v, *id, *l)).collect::<Vec<_>>()})),
        "next" => {
            if found.is_empty() {
                return Ok(json!({"match": null}));
            }
            let cur = sel.items.first().and_then(|id| found.iter().position(|(x, _)| x == id));
            let i = cur.map_or(0, |c| (c + 1) % found.len());
            let (id, locked) = found[i];
            core.ses.select(Selection::items(vec![id]));
            Ok(json!({"match": object_json(&v, id, locked), "wrapped": cur.is_some_and(|c| i <= c)}))
        }
        "change" | "changeAll" | "changeFind" => {
            let ids: Vec<ItemId> = found.iter().filter(|(id, l)| !*l && (op == "changeAll" || sel.items.contains(id))).map(|(id, _)| *id).collect();
            let ch = p.get("objectChange").cloned().unwrap_or(Value::Null);
            let style = ch.get("objectStyle").and_then(Value::as_str).filter(|x| !x.is_empty()).map(style_key);
            let style_attrs = match &style {
                Some(n) => {
                    if !v.doc.styles.exists(StyleKind::Object, n) {
                        return Err(format!("no object style `{n}`"));
                    }
                    let mut a = formdesign_model::attrs::Attrs::new();
                    for x in v.doc.styles.chain(StyleKind::Object, n) {
                        a.extend(x.clone());
                    }
                    Some(a)
                }
                None => None,
            };
            let n = ids.len();
            if n > 0 {
                core.ses.edit_now(if op == "changeAll" { "Change All" } else { "Change" }, |d, _| {
                    for id in &ids {
                        if let (Some(a), Some(name)) = (&style_attrs, &style) {
                            formdesign_engine::cmd::text::apply_object_style(d, *id, a);
                            if let Some(it) = d.item_mut(*id) {
                                it.style = (name != "None").then(|| name.clone());
                            }
                        }
                        let Some(it) = d.item_mut(*id) else { continue };
                        if let Some(f) = ch.get("fill").and_then(Value::as_str).filter(|x| !x.is_empty()) {
                            it.fill = wire::paint_in(f, None);
                        }
                        if let Some(c) = ch.get("stroke").and_then(Value::as_str).filter(|x| !x.is_empty()) {
                            it.stroke.paint = wire::paint_in(c, None);
                        }
                        if let Some(w) = ch.get("strokeWeight").and_then(Value::as_f64) {
                            it.stroke.weight = w.max(0.0);
                        }
                        if let Some(o) = ch.get("opacity").and_then(Value::as_f64) {
                            it.opacity = o.clamp(0.0, 1.0);
                        }
                    }
                });
            }
            let mut r = json!({"changed": n});
            if op == "changeFind" {
                let nx = objects(core, &with_op(p, "next"))?;
                r["match"] = nx["match"].clone();
            }
            Ok(r)
        }
        _ => Err(format!("unknown op `{op}`")),
    }
}

fn with_op(p: &Value, op: &str) -> Value {
    let mut v = p.clone();
    v["op"] = json!(op);
    v
}

pub fn run(core: &mut Core, p: &Value) -> Result<Value, String> {
    if s(p, "mode") == Some("object") {
        return objects(core, p);
    }
    let op = s(p, "op").unwrap_or("all");
    if op == "select" {
        let t = p.get("target").ok_or("missing `target`")?;
        let loc = Loc::parse(t).ok_or("bad `target`")?;
        let st = core.ses.active().ok_or("no document open")?;
        let n = textloc::view(&st.doc, loc).map(|s| marker_text(&s.paras).len()).ok_or("bad `target`")?;
        let a = (t.get("start").and_then(Value::as_u64).unwrap_or(0) as usize).min(n);
        let b = (t.get("end").and_then(Value::as_u64).unwrap_or(0) as usize).clamp(a, n);
        return Ok(json!({"match": select(core, loc, a..b)?}));
    }
    let spec = Spec::parse(p)?;
    let inc = Incl::parse(p);
    match op {
        "all" => {
            let v = view_of(core)?;
            let sel = core.ses.active().map(|s| s.selection.clone()).unwrap_or_default();
            let limit = p.get("limit").and_then(Value::as_u64).unwrap_or(2000).clamp(1, 20_000) as usize;
            let mut hs = Vec::new();
            for t in targets(&v.doc, &sel, p, inc)? {
                hits(&v.doc, &spec, &t, limit + 1, &mut hs);
                if hs.len() > limit {
                    break;
                }
            }
            let truncated = hs.len() > limit;
            hs.truncate(limit);
            let list: Vec<Value> = hs.iter().map(|h| hit_json(&v, h, true)).collect();
            Ok(json!({"total": list.len(), "truncated": truncated, "matches": list}))
        }
        "next" => next(core, &spec, p, inc),
        "change" => change_current(core, &spec),
        "changeAll" => change_all(core, &spec, p, inc),
        "changeFind" => {
            let mut r = change_current(core, &spec)?;
            let n = next(core, &spec, p, inc)?;
            r["match"] = n["match"].clone();
            r["wrapped"] = n.get("wrapped").cloned().unwrap_or(Value::Null);
            Ok(r)
        }
        _ => Err(format!("unknown op `{op}`")),
    }
}
