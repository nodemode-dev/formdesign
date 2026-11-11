use std::ops::Range;
use std::sync::Arc;

use formdesign_model::attrs::{Align, Caps, CharProps, Kerning, Kinsoku, Mojikumi, ParaProps, Position, Ruby, RubyAlign, TabAlign};
use unicode_bidi::BidiInfo;
use unicode_linebreak::{BreakOpportunity, linebreaks};

use crate::fonts::{Face, FontBook};
use crate::kp::{self, FILL, INF, Item};
use crate::lang;
use crate::shape::{ShapeReq, shape};

pub const OBJ: char = '\u{FFFC}';
pub const LINE_SEP: char = '\u{2028}';
pub const SHY: char = '\u{00AD}';
const NO_GLYPH: u32 = u32::MAX;
pub const GENERATED: usize = usize::MAX;
const TAB_GLYPH: u32 = u32::MAX - 1;

#[derive(Clone, Debug)]
pub struct TextRun {
    pub range: Range<usize>,
    pub props: Arc<CharProps>,
}

#[derive(Clone, Debug)]
pub struct ObjBox {
    pub at: usize,
    pub width: f64,
    pub ascent: f64,
    pub descent: f64,
}

#[derive(Clone, Debug)]
pub struct ParaInput {
    pub text: String,
    pub runs: Vec<TextRun>,
    pub objects: Vec<ObjBox>,
    pub props: ParaProps,
    pub prefix: usize,
    pub exceptions: Exceptions,
    pub vertical: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Orient {
    #[default]
    Side,
    Upright,
    Tcy,
}

pub fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x1100..=0x11FF | 0x2E80..=0x2FDF | 0x2FF0..=0x2FFF | 0x3001..=0x30FF | 0x3100..=0x318F | 0x3190..=0x4DBF | 0x4E00..=0x9FFF | 0xA960..=0xA97F | 0xAC00..=0xD7FF | 0xF900..=0xFAFF | 0xFE10..=0xFE1F | 0xFE30..=0xFE4F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6 | 0x1B000..=0x1B16F | 0x20000..=0x3FFFF)
}

const CLOSING: &str = "\u{FF09}\u{3015}\u{FF3D}\u{FF5D}\u{3009}\u{300B}\u{300D}\u{300F}\u{3011}\u{3019}\u{3017}\u{FF60}\u{301E}\u{301F}";
const OPENING: &str = "\u{FF08}\u{3014}\u{FF3B}\u{FF5B}\u{3008}\u{300A}\u{300C}\u{300E}\u{3010}\u{3018}\u{3016}\u{FF5F}\u{301D}";
const STOPS: &str = "\u{3001}\u{3002}\u{FF0C}\u{FF0E}";
const MIDDLE: &str = "\u{30FB}\u{FF1A}\u{FF1B}";
const HARD_START: &str = "\u{FF01}\u{FF1F}\u{203C}\u{2047}\u{30A0}\u{30FC}\u{3005}\u{303B}\u{309D}\u{309E}\u{30FD}\u{30FE}\u{309B}\u{309C}\u{301C}\u{3041}\u{3043}\u{3045}\u{3047}\u{3049}\u{3063}\u{3083}\u{3085}\u{3087}\u{308E}\u{3095}\u{3096}\u{30A1}\u{30A3}\u{30A5}\u{30A7}\u{30A9}\u{30C3}\u{30E3}\u{30E5}\u{30E7}\u{30EE}\u{30F5}\u{30F6}";

fn no_start(c: char, k: Kinsoku) -> bool {
    match k {
        Kinsoku::None => false,
        Kinsoku::Soft => CLOSING.contains(c) || STOPS.contains(c),
        Kinsoku::Hard => CLOSING.contains(c) || STOPS.contains(c) || MIDDLE.contains(c) || HARD_START.contains(c),
    }
}

fn no_end(c: char, k: Kinsoku) -> bool {
    k != Kinsoku::None && OPENING.contains(c)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Punct {
    Open,
    Close,
    Stop,
    Middle,
}

fn punct(c: char) -> Option<Punct> {
    if OPENING.contains(c) {
        Some(Punct::Open)
    } else if CLOSING.contains(c) {
        Some(Punct::Close)
    } else if STOPS.contains(c) {
        Some(Punct::Stop)
    } else if MIDDLE.contains(c) {
        Some(Punct::Middle)
    } else {
        None
    }
}

pub type Exceptions = Arc<std::collections::HashMap<String, Vec<usize>>>;

pub fn parse_exceptions(words: &[String]) -> Exceptions {
    let mut m = std::collections::HashMap::new();
    for w in words {
        let mut breaks = Vec::new();
        let mut plain = String::new();
        for ch in w.chars() {
            if ch == '~' {
                breaks.push(plain.chars().count());
            } else {
                plain.push(ch);
            }
        }
        breaks.dedup();
        breaks.retain(|b| *b > 0 && *b < plain.chars().count());
        m.insert(plain.to_lowercase(), breaks);
    }
    Arc::new(m)
}

#[derive(Clone)]
pub struct Seg {
    pub face: Arc<Face>,
    pub size: f64,
    pub shift: f64,
    pub props: Arc<CharProps>,
    pub level: u8,
    pub orient: Orient,
    pub tcy_scale: f64,
}

#[derive(Clone, Copy, Debug)]
struct SGlyph {
    id: u32,
    cluster: usize,
    adv: f64,
    dx: f64,
    dy: f64,
    seg: usize,
    squeezed: bool,
}

struct RubyPiece {
    base: Range<usize>,
    text: String,
    glyphs: Vec<crate::shape::Glyph>,
    face: Arc<Face>,
    size: f64,
    props: Arc<CharProps>,
    ruby: Ruby,
    base_size: f64,
    base_shift: f64,
    width: f64,
}

pub struct Shaped {
    pub text: String,
    glyphs: Vec<SGlyph>,
    pub segs: Vec<Seg>,
    levels: Vec<u8>,
    rtl_para: bool,
    pub base_size: f64,
    objects: Vec<ObjBox>,
    exceptions: Exceptions,
    vertical: bool,
    rubies: Vec<RubyPiece>,
    fixed: Vec<Range<usize>>,
    mojikumi: Mojikumi,
}

fn caps_text(s: &str, base: usize) -> (String, Vec<usize>) {
    let mut out = String::new();
    let mut map = Vec::new();
    for (i, c) in s.char_indices() {
        for u in c.to_uppercase() {
            let mut b = [0u8; 4];
            let enc = u.encode_utf8(&mut b);
            for _ in 0..enc.len() {
                map.push(base + i);
            }
            out.push_str(enc);
        }
    }
    map.push(base + s.len());
    (out, map)
}

fn identity(s: &str, base: usize) -> (String, Vec<usize>) {
    let mut map: Vec<usize> = (0..s.len()).map(|i| base + i).collect();
    map.push(base + s.len());
    (s.to_string(), map)
}

fn is_special(c: char) -> bool {
    matches!(c, '\t' | OBJ | LINE_SEP | '\n')
}

pub fn shape_para(fonts: &FontBook, input: &ParaInput) -> Shaped {
    let text = input.text.clone();
    let rtl_para = input.props.rtl;
    let has_rtl = rtl_para || text.chars().any(|c| matches!(c as u32, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF));
    let levels: Vec<u8> = if has_rtl && !text.is_empty() {
        let info = BidiInfo::new(&text, Some(if rtl_para { unicode_bidi::Level::rtl() } else { unicode_bidi::Level::ltr() }));
        info.levels.iter().map(|l| l.number()).collect()
    } else {
        vec![0; text.len()]
    };
    let mut segs: Vec<Seg> = Vec::new();
    let mut glyphs: Vec<SGlyph> = Vec::new();
    let base_size = input.runs.first().map(|r| r.props.size).unwrap_or(12.0);
    let fallback_face = fonts.any_face();
    let mut tcy: Vec<Range<usize>> = Vec::new();
    if input.vertical {
        for run in &input.runs {
            if run.props.tcy && !run.range.is_empty() {
                tcy.push(run.range.clone());
            }
        }
        if input.props.auto_tcy > 0 {
            let mut start: Option<usize> = None;
            for (i, c) in text.char_indices().chain(std::iter::once((text.len(), ' '))) {
                if c.is_ascii_digit() {
                    start.get_or_insert(i);
                } else if let Some(s) = start.take()
                    && text[s..i].len() <= input.props.auto_tcy as usize
                    && !tcy.iter().any(|r| r.start <= s && i <= r.end)
                {
                    tcy.push(s..i);
                }
            }
        }
    }
    let mut fixed: Vec<Range<usize>> = tcy.clone();
    for run in &input.runs {
        let p = &run.props;
        let Some(main) = fonts.resolve(&p.font, &p.font_style).or_else(|| fallback_face.clone()) else {
            continue;
        };
        let main = fonts.varied(&main, &p.variations);
        let mut pieces: Vec<(Range<usize>, Arc<Face>, u8, bool, Orient)> = Vec::new();
        let slice = &text[run.range.clone()];
        let mut last_special = false;
        for (off, c) in slice.char_indices() {
            let at = run.range.start + off;
            let lvl = levels.get(at).copied().unwrap_or(0);
            let face = if is_special(c) || main.covers(c) { main.clone() } else { fonts.fallback(&main, c).unwrap_or_else(|| main.clone()) };
            let lower = matches!(p.caps, Caps::Small) && c.is_lowercase() && !main.has_feature(b"smcp");
            let special = is_special(c);
            let orient = if !input.vertical || special {
                Orient::Side
            } else if tcy.iter().any(|r| r.contains(&at)) {
                Orient::Tcy
            } else if is_cjk(c) {
                Orient::Upright
            } else {
                Orient::Side
            };
            match pieces.last_mut() {
                Some((r, f, l, lw, o)) if f.id == face.id && *l == lvl && *lw == lower && *o == orient && !special && !last_special => {
                    r.end = at + c.len_utf8();
                }
                _ => pieces.push((at..at + c.len_utf8(), face, lvl, lower, orient)),
            }
            last_special = special;
        }
        for (range, face, lvl, synth_small, orient) in pieces {
            let piece = &text[range.clone()];
            let first = piece.chars().next().unwrap_or(' ');
            let mut size = p.size;
            let mut shift = p.baseline_shift;
            match p.position {
                Position::Superscript => {
                    shift += p.size * 0.333;
                    size *= 0.583;
                }
                Position::Subscript => {
                    shift -= p.size * 0.333;
                    size *= 0.583;
                }
                _ => {}
            }
            if synth_small {
                size *= 0.7;
            }
            let seg_index = segs.len();
            segs.push(Seg { face: face.clone(), size, shift, props: p.clone(), level: lvl, orient, tcy_scale: 1.0 });
            if is_special(first) {
                let (id, adv) = match first {
                    OBJ => (NO_GLYPH, input.objects.iter().find(|o| o.at == range.start).map(|o| o.width).unwrap_or(0.0)),
                    '\t' => (TAB_GLYPH, 0.0),
                    _ => (NO_GLYPH, 0.0),
                };
                glyphs.push(SGlyph { id, cluster: range.start, adv, dx: 0.0, dy: 0.0, seg: seg_index, squeezed: false });
                continue;
            }
            let upper = matches!(p.caps, Caps::All | Caps::AllSmall) || synth_small;
            let (shaped_text, map) = if upper { caps_text(piece, range.start) } else { identity(piece, range.start) };
            let mut feats = p.features.clone();
            if matches!(p.caps, Caps::Small) && face.has_feature(b"smcp") {
                feats.push("smcp".into());
            }
            if matches!(p.caps, Caps::AllSmall) && face.has_feature(b"c2sc") {
                feats.push("c2sc".into());
            }
            if matches!(p.position, Position::Numerator) {
                feats.push("numr".into());
            }
            if matches!(p.position, Position::Denominator) {
                feats.push("dnom".into());
            }
            if orient == Orient::Upright {
                feats.push("vert".into());
                feats.push("vrt2".into());
            }
            let req = ShapeReq {
                text: &shaped_text,
                face: &face,
                size,
                kerning: !matches!(p.kerning, Kerning::Off),
                ligatures: p.ligatures,
                features: &feats,
                rtl: lvl % 2 == 1,
                language: &lang::bcp47(&p.language),
            };
            let track = p.tracking / 1000.0 * p.size;
            let kern = p.kern / 1000.0 * p.size;
            let out = shape(&req);
            let n = out.len();
            let (mut fit, mut pad) = (1.0, 0.0);
            if orient == Orient::Tcy {
                let nat: f64 = out.iter().map(|g| g.advance * p.h_scale).sum();
                if nat > size {
                    fit = size / nat;
                } else {
                    pad = (size - nat) / 2.0;
                }
                segs[seg_index].tcy_scale = fit;
            }
            for (gi, g) in out.into_iter().enumerate() {
                let cluster = map.get(g.cluster).copied().unwrap_or(range.start);
                let mut adv = g.advance * p.h_scale * fit + if orient == Orient::Tcy { 0.0 } else { track };
                let mut dx = g.dx * p.h_scale * fit;
                if gi + 1 == n {
                    adv += if orient == Orient::Tcy { pad } else { kern };
                }
                if gi == 0 && pad > 0.0 {
                    dx += pad;
                    adv += pad;
                }
                glyphs.push(SGlyph { id: g.id, cluster, adv, dx, dy: g.dy, seg: seg_index, squeezed: false });
            }
        }
    }
    glyphs.sort_by_key(|g| g.cluster);
    if input.props.mojikumi != Mojikumi::None {
        squeeze_punctuation(&text, &segs, &mut glyphs, input.props.mojikumi);
    }
    let rubies = ruby_pieces(fonts, input, &segs, &mut glyphs, &mut fixed);
    Shaped { text, glyphs, segs, levels, rtl_para, base_size, objects: input.objects.clone(), exceptions: input.exceptions.clone(), vertical: input.vertical, rubies, fixed, mojikumi: input.props.mojikumi }
}

fn squeeze_one(g: &mut SGlyph, c: char) {
    if g.squeezed {
        return;
    }
    let q = g.adv * 0.5;
    if punct(c) == Some(Punct::Open) {
        g.dx -= q;
    }
    g.adv -= q;
    g.squeezed = true;
}

fn squeeze_punctuation(text: &str, segs: &[Seg], glyphs: &mut [SGlyph], set: Mojikumi) {
    let mut at = 0;
    while at < glyphs.len() {
        let g = glyphs[at];
        let next_same = glyphs.get(at + 1).is_some_and(|n| n.cluster == g.cluster);
        let prev_same = at > 0 && glyphs[at - 1].cluster == g.cluster;
        let c = text.get(g.cluster..).and_then(|t| t.chars().next());
        at += 1;
        let (Some(c), false, false) = (c, next_same, prev_same) else { continue };
        let Some(class) = punct(c) else { continue };
        let seg = &segs[g.seg];
        if g.adv < seg.size * seg.props.h_scale * 0.9 {
            continue;
        }
        let before = text[..g.cluster].chars().next_back().and_then(punct);
        let after = text[g.cluster + c.len_utf8()..].chars().next().and_then(punct);
        let squeeze = match set {
            Mojikumi::None => false,
            Mojikumi::Tight => true,
            Mojikumi::Standard => match class {
                Punct::Open => before.is_some(),
                Punct::Close => after.is_some(),
                Punct::Stop => matches!(after, Some(Punct::Open | Punct::Close | Punct::Stop)),
                Punct::Middle => false,
            },
        };
        if squeeze {
            squeeze_one(&mut glyphs[at - 1], c);
        }
    }
}

fn ruby_pieces(fonts: &FontBook, input: &ParaInput, segs: &[Seg], glyphs: &mut [SGlyph], fixed: &mut Vec<Range<usize>>) -> Vec<RubyPiece> {
    let mut groups: Vec<(Range<usize>, &Ruby)> = Vec::new();
    for run in &input.runs {
        let Some(r) = run.props.ruby.as_ref() else { continue };
        if run.range.is_empty() {
            continue;
        }
        match groups.last_mut() {
            Some((g, prev)) if g.end == run.range.start && *prev == r => g.end = run.range.end,
            _ => groups.push((run.range.clone(), r)),
        }
    }
    let mut out = Vec::new();
    for (range, ruby) in groups {
        let base: Vec<(usize, char)> = input.text[range.clone()].char_indices().map(|(i, c)| (range.start + i, c)).filter(|(_, c)| !is_special(*c)).collect();
        if base.is_empty() {
            continue;
        }
        let first = glyphs.partition_point(|g| g.cluster < base[0].0);
        let Some(g0) = glyphs.get(first) else { continue };
        let seg = &segs[g0.seg];
        let rsize = (seg.props.size * ruby.scale).max(1.0);
        let parts: Vec<String> = if ruby.per_char {
            let mut parts: Vec<String> = if ruby.text.contains('|') { ruby.text.split('|').map(str::to_string).collect() } else { ruby.text.chars().map(|c| c.to_string()).collect() };
            if parts.len() > base.len() {
                let rest = parts.split_off(base.len() - 1).join("");
                parts.push(rest);
            }
            parts
        } else {
            vec![ruby.text.clone()]
        };
        let mut props = (*seg.props).clone();
        props.size = rsize;
        props.underline = false;
        props.strike = false;
        props.ruby = None;
        props.kenten = None;
        props.tcy = false;
        props.tracking = 0.0;
        props.kern = 0.0;
        props.h_scale = 1.0;
        props.v_scale = 1.0;
        props.baseline_shift = 0.0;
        props.position = Position::Normal;
        let props = Arc::new(props);
        for (pi, part) in parts.iter().enumerate() {
            if part.is_empty() {
                continue;
            }
            let sub = if ruby.per_char { base[pi].0..base[pi].0 + base[pi].1.len_utf8() } else { range.clone() };
            let face = match part.chars().find(|c| !seg.face.covers(*c)) {
                Some(c) => fonts.fallback(&seg.face, c).unwrap_or_else(|| seg.face.clone()),
                None => seg.face.clone(),
            };
            let shaped = shape(&ShapeReq { text: part, face: &face, size: rsize, kerning: true, ligatures: false, features: &[], rtl: false, language: &lang::bcp47(&seg.props.language) });
            let width: f64 = shaped.iter().map(|g| g.advance).sum();
            let lo = glyphs.partition_point(|g| g.cluster < sub.start);
            let hi = glyphs.partition_point(|g| g.cluster < sub.end);
            if hi > lo {
                let base_w: f64 = glyphs[lo..hi].iter().map(|g| g.adv).sum();
                if width > base_w + 0.01 {
                    let half = (width - base_w) / 2.0;
                    glyphs[lo].dx += half;
                    glyphs[lo].adv += half;
                    glyphs[hi - 1].adv += half;
                }
            }
            fixed.push(sub.clone());
            out.push(RubyPiece { base: sub, text: part.clone(), glyphs: shaped, face, size: rsize, props: props.clone(), ruby: ruby.clone(), base_size: seg.size, base_shift: seg.shift, width });
        }
    }
    out
}

impl Shaped {
    fn width(&self, r: Range<usize>) -> f64 {
        let lo = self.glyphs.partition_point(|g| g.cluster < r.start);
        let hi = self.glyphs.partition_point(|g| g.cluster < r.end);
        self.glyphs[lo..hi].iter().map(|g| g.adv).sum()
    }

    fn seg_at(&self, at: usize) -> Option<&Seg> {
        let i = self.glyphs.partition_point(|g| g.cluster < at);
        self.glyphs.get(i).or_else(|| self.glyphs.last()).map(|g| &self.segs[g.seg])
    }

    fn hyphen_width(&self, at: usize) -> f64 {
        self.seg_at(at.saturating_sub(1)).map(|s| hyphen_glyph(s).1).unwrap_or(0.0)
    }
}

fn hang(c: char, right: bool) -> f64 {
    match c {
        '"' | '\'' | '\u{201C}' | '\u{201D}' | '\u{2018}' | '\u{2019}' | '\u{201E}' | '\u{00AB}' | '\u{00BB}' => 1.0,
        '.' | ',' if right => 1.0,
        '-' | '\u{2010}' | '\u{2011}' => 0.75,
        '\u{2013}' => 0.5,
        '\u{2014}' => 0.25,
        ':' | ';' if right => 0.5,
        '!' | '?' if right => 0.2,
        'A' | 'T' | 'V' | 'W' | 'Y' => 0.05,
        _ => 0.0,
    }
}

fn optical(sh: &Shaped, start: usize, end: usize, hyphenated: bool) -> (f64, f64) {
    let first = sh.text[start..end].chars().next();
    let left = first.map(|c| hang(c, false) * sh.width(start..start + c.len_utf8())).unwrap_or(0.0);
    let right = if hyphenated {
        0.75 * sh.hyphen_width(end)
    } else {
        sh.text[start..end].chars().next_back().map(|c| hang(c, true) * sh.width(end - c.len_utf8()..end)).unwrap_or(0.0)
    };
    (left, right)
}

fn hyphen_glyph(seg: &Seg) -> (u32, f64) {
    let g = seg.face.glyph('-').or_else(|| seg.face.glyph('\u{2010}')).unwrap_or(0);
    let adv = seg.face.advance(g) * seg.size / seg.face.metrics.upem * seg.props.h_scale;
    (g, adv)
}

fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\u{3000}' | '\u{2002}'..='\u{200A}' | '\u{205F}')
}

#[derive(Clone, Copy, Debug)]
enum Mark {
    Word(usize, usize),
    Space(usize, usize),
    Hyphen(usize),
    Break(usize, bool),
    Forced(usize),
    End(usize),
}

pub struct Prepared {
    items: Vec<Item>,
    marks: Vec<Mark>,
    has_tabs: bool,
}

fn syllables(word: &str, props: &ParaProps, language: &str, exc: &Exceptions) -> Vec<usize> {
    let letters: Vec<(usize, char)> = word.char_indices().collect();
    if let Some(breaks) = exc.get(&word.to_lowercase()) {
        return breaks.iter().filter_map(|b| letters.get(*b).map(|(i, _)| *i)).collect();
    }
    if letters.len() < props.hyph_min_word.max(1) || !letters.iter().all(|(_, c)| c.is_alphabetic()) {
        return Vec::new();
    }
    if !props.hyph_caps && letters.first().map(|(_, c)| c.is_uppercase()).unwrap_or(false) {
        return Vec::new();
    }
    let Some(l) = lang::hyphen_lang(language) else {
        return Vec::new();
    };
    let lower = word.to_lowercase();
    if lower.len() != word.len() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut pos = 0;
    let syl: Vec<&str> = hypher::hyphenate(&lower, l).collect();
    for s in syl.iter().take(syl.len().saturating_sub(1)) {
        pos += s.len();
        let before = word[..pos].chars().count();
        let after = word[pos..].chars().count();
        if before >= props.hyph_after.max(1) && after >= props.hyph_before.max(1) {
            out.push(pos);
        }
    }
    out
}

const HYPHEN_PENALTY: f64 = 50.0;

fn cjk_opportunities(text: &str, opps: Vec<(usize, BreakOpportunity)>, k: Kinsoku) -> Vec<(usize, BreakOpportunity)> {
    let mut map: std::collections::BTreeMap<usize, BreakOpportunity> = opps.into_iter().collect();
    let mut prev: Option<char> = None;
    for (i, c) in text.char_indices() {
        if let Some(p) = prev
            && is_cjk(p)
            && is_cjk(c)
        {
            let allowed = !(no_start(c, k) || no_end(p, k));
            if allowed {
                map.entry(i).or_insert(BreakOpportunity::Allowed);
            } else if map.get(&i) == Some(&BreakOpportunity::Allowed) {
                map.remove(&i);
            }
        }
        prev = Some(c);
    }
    map.into_iter().collect()
}

pub fn prepare(sh: &Shaped, props: &ParaProps) -> Prepared {
    let text = &sh.text;
    let justified = props.align.justified();
    let mut items = Vec::new();
    let mut marks = Vec::new();
    let mut opps: Vec<(usize, BreakOpportunity)> = linebreaks(text).collect();
    let inside_no_break = |i: usize| sh.seg_at(i).is_some_and(|s| s.props.no_break) && text[..i].char_indices().next_back().and_then(|(p, _)| sh.seg_at(p)).is_some_and(|s| s.props.no_break);
    opps.retain(|(i, _)| *i >= text.len() || !inside_no_break(*i));
    if text.chars().any(is_cjk) {
        opps = cjk_opportunities(text, opps, props.kinsoku);
    }
    if !sh.fixed.is_empty() {
        opps.retain(|(i, _)| !sh.fixed.iter().any(|r| r.start < *i && *i < r.end));
    }
    let has_tabs = text.contains('\t');
    let mut start = 0;
    for (at, kind) in opps {
        if at <= start && at < text.len() {
            continue;
        }
        let piece = &text[start..at];
        let trimmed_len = piece.trim_end_matches(|c: char| is_space(c) || c == LINE_SEP || c == '\n').len();
        let word_end = start + trimmed_len;
        let ws_end = start + piece.trim_end_matches([LINE_SEP, '\n']).len();
        if word_end > start {
            let word = &text[start..word_end];
            let points = if props.hyphenate { hyph_points(word, start, sh, props).into_iter().filter(|p| !inside_no_break(*p)).collect() } else { Vec::new() };
            let mut s = start;
            for p in points {
                items.push(Item::Box { w: sh.width(s..p) });
                marks.push(Mark::Word(s, p));
                items.push(Item::Penalty { w: sh.hyphen_width(p), p: HYPHEN_PENALTY, flagged: true });
                marks.push(Mark::Hyphen(p));
                s = p;
            }
            items.push(Item::Box { w: sh.width(s..word_end) });
            marks.push(Mark::Word(s, word_end));
        }
        let mandatory = matches!(kind, BreakOpportunity::Mandatory);
        if at >= text.len() {
            if ws_end > word_end {
                items.push(Item::Glue { w: 0.0, stretch: 0.0, shrink: 0.0 });
                marks.push(Mark::Space(word_end, ws_end));
            }
            items.push(Item::Glue { w: 0.0, stretch: FILL, shrink: 0.0 });
            marks.push(Mark::End(text.len()));
            items.push(Item::Penalty { w: 0.0, p: -INF, flagged: false });
            marks.push(Mark::End(text.len()));
            start = at;
            break;
        }
        if ws_end > word_end {
            let natural = sh.width(word_end..ws_end);
            let ws = props.word_space;
            let (w, y, z) = if justified {
                (natural * ws[1], natural * (ws[2] - ws[1]).max(0.0), natural * (ws[1] - ws[0]).max(0.0))
            } else {
                (natural * ws[1], 0.0, 0.0)
            };
            items.push(Item::Glue { w, stretch: y, shrink: z });
            marks.push(Mark::Space(word_end, ws_end));
        } else if !mandatory {
            let prev = text[..word_end].chars().next_back();
            let soft = prev == Some(SHY);
            let flagged = soft || matches!(prev, Some('-' | '\u{2010}' | '\u{2013}'));
            let w = if soft { sh.hyphen_width(word_end) } else { 0.0 };
            items.push(Item::Penalty { w, p: if soft { HYPHEN_PENALTY } else { 0.0 }, flagged });
            marks.push(Mark::Break(word_end, soft));
        }
        if mandatory {
            items.push(Item::Glue { w: 0.0, stretch: FILL, shrink: 0.0 });
            marks.push(Mark::Forced(at));
            items.push(Item::Penalty { w: 0.0, p: -INF, flagged: false });
            marks.push(Mark::Forced(at));
        }
        start = at;
    }
    if items.is_empty() || start < text.len() {
        items.push(Item::Glue { w: 0.0, stretch: FILL, shrink: 0.0 });
        marks.push(Mark::End(text.len()));
        items.push(Item::Penalty { w: 0.0, p: -INF, flagged: false });
        marks.push(Mark::End(text.len()));
    }
    Prepared { items, marks, has_tabs }
}

fn hyph_points(word: &str, base: usize, sh: &Shaped, props: &ParaProps) -> Vec<usize> {
    let mut out = Vec::new();
    let mut tok_start: Option<usize> = None;
    let chars: Vec<(usize, char)> = word.char_indices().chain(std::iter::once((word.len(), ' '))).collect();
    for (i, c) in chars {
        if c.is_alphabetic() {
            tok_start.get_or_insert(i);
        } else if let Some(s) = tok_start.take() {
            let tok = &word[s..i];
            let language = sh.seg_at(base + s).map(|g| g.props.language.clone()).unwrap_or_default();
            for p in syllables(tok, props, &language, &sh.exceptions) {
                out.push(base + s + p);
            }
        }
    }
    out
}

#[derive(Clone, Debug)]
pub struct PlacedGlyph {
    pub id: u32,
    pub x: f64,
    pub y: f64,
    pub adv: f64,
    pub cluster: usize,
}

#[derive(Clone)]
pub struct GlyphRun {
    pub face: Arc<Face>,
    pub size: f64,
    pub shift: f64,
    pub props: Arc<CharProps>,
    pub glyphs: Vec<PlacedGlyph>,
    pub x0: f64,
    pub x1: f64,
    pub rtl: bool,
    pub hscale: f64,
    pub orient: Orient,
    pub text: String,
}

/// A kenten (emphasis mark) drawn as a vector shape centred on (x, baseline + y).
#[derive(Clone, Debug)]
pub struct EmphasisMark {
    pub x: f64,
    pub y: f64,
    pub size: f64,
    pub kind: char,
    pub props: Arc<CharProps>,
}

#[derive(Clone, Debug)]
pub struct TabFill {
    pub x0: f64,
    pub x1: f64,
    pub leader: String,
}

#[derive(Clone)]
pub struct Line {
    pub start: usize,
    pub end: usize,
    pub left: f64,
    pub avail: f64,
    pub width: f64,
    pub ascent: f64,
    pub descent: f64,
    pub cap: f64,
    pub xheight: f64,
    pub leading: f64,
    pub runs: Vec<GlyphRun>,
    pub adorn: Vec<GlyphRun>,
    pub marks: Vec<EmphasisMark>,
    pub hyphen: bool,
    pub forced: bool,
    pub last: bool,
    pub tabs: Vec<TabFill>,
    pub objects: Vec<(usize, f64)>,
    pub carets: Vec<(usize, f64)>,
    pub ratio: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct Slot {
    pub left: f64,
    pub width: f64,
}

pub struct Composed {
    pub lines: Vec<Line>,
    pub drop: Option<DropCap>,
}

#[derive(Clone)]
pub struct DropCap {
    pub run: GlyphRun,
    pub chars: usize,
    pub lines: u32,
    pub width: f64,
    pub height: f64,
}

pub struct Composer<'a> {
    pub fonts: &'a FontBook,
}

fn tab_stops(props: &ParaProps) -> Vec<(f64, TabAlign, String, String)> {
    props.tabs.iter().map(|t| (t.position, t.align, t.leader.clone(), t.align_on.clone())).collect()
}

impl Composer<'_> {
    pub fn compose(&self, input: &ParaInput, slot: &dyn Fn(usize) -> Slot, distinct: usize, right_page: bool) -> Composed {
        let sh = shape_para(self.fonts, input);
        let props = &input.props;
        let drop = self.drop_cap(&sh, input);
        let drop_w = drop.as_ref().map(|d| d.width).unwrap_or(0.0);
        let drop_lines = drop.as_ref().map(|d| d.lines as usize).unwrap_or(0);
        let skip = drop.as_ref().map(|d| d.chars).unwrap_or(0);
        let geom = |line: usize| -> Slot {
            let s = slot(line);
            let mut left = s.left + if props.rtl { props.right_indent } else { props.left_indent };
            let mut width = s.width - props.left_indent - props.right_indent;
            if line == 0 {
                if !props.rtl {
                    left += props.first_indent;
                }
                width -= props.first_indent;
            }
            if line < drop_lines {
                if !props.rtl {
                    left += drop_w;
                }
                width -= drop_w;
            }
            Slot { left, width: width.max(1.0) }
        };
        let mut prepared = prepare(&sh, props);
        if skip > 0 {
            trim_front(&mut prepared, &sh, input.prefix, skip);
        }
        let em = sh.base_size;
        let justified = props.align.justified();
        let end_stretch = if justified { 0.0 } else { 3.0 * em };
        let run_breaks = |cap: f64| {
            let width_of = |l: usize| geom(l).width.min(cap);
            if props.single_line || prepared.has_tabs {
                kp::greedy(&prepared.items, &width_of, end_stretch)
            } else {
                let mut params = kp::Params { end_stretch, max_hyphens: props.hyph_limit, ..kp::Params::default() };
                kp::breaks(&prepared.items, &width_of, distinct.max(drop_lines + 2), &params)
                    .or_else(|| {
                        params.tolerance = 10.0;
                        params.emergency_stretch = 1.5 * em;
                        kp::breaks(&prepared.items, &width_of, distinct.max(drop_lines + 2), &params)
                    })
                    .unwrap_or_else(|| kp::greedy(&prepared.items, &width_of, end_stretch))
            }
        };
        let mut breaks = run_breaks(f64::MAX);
        if props.balance && !justified && breaks.len() > 1 {
            let total: f64 = prepared.items.iter().filter(|i| !matches!(i, Item::Penalty { .. })).map(|i| i.width()).sum();
            let target = total / breaks.len() as f64 * 1.06;
            let again = run_breaks(target);
            if again.len() == breaks.len() {
                breaks = again;
            }
        }
        let mut lines = Vec::new();
        let mut from = 0usize;
        let n = breaks.len();
        for (li, b) in breaks.iter().enumerate() {
            let line = self.build_line(&sh, &prepared, (from, b.at), geom(li), slot(li).left, li + 1 == n, props, right_page, drop.is_some() && li == 0);
            lines.push(line);
            from = b.at + 1;
        }
        if lines.is_empty() {
            let last = prepared.items.len().saturating_sub(1);
            lines.push(self.build_line(&sh, &prepared, (0, last), geom(0), slot(0).left, true, props, right_page, false));
        }
        Composed { lines, drop }
    }

    fn drop_cap(&self, sh: &Shaped, input: &ParaInput) -> Option<DropCap> {
        let p = &input.props;
        if p.drop_lines < 2 || p.drop_chars == 0 || sh.text.is_empty() {
            return None;
        }
        let end = sh.text[input.prefix..].char_indices().nth(p.drop_chars as usize).map(|(i, _)| input.prefix + i).unwrap_or(sh.text.len());
        let seg = sh.seg_at(input.prefix)?.clone();
        let leading = seg.props.leading.unwrap_or(seg.size * p.auto_leading);
        let cap_ratio = seg.face.metrics.cap_height / seg.face.metrics.upem;
        let height = (p.drop_lines as f64 - 1.0) * leading + seg.size * cap_ratio;
        let size = height / cap_ratio.max(0.1);
        let text = &sh.text[input.prefix..end];
        let req = ShapeReq {
            text,
            face: &seg.face,
            size,
            kerning: true,
            ligatures: seg.props.ligatures,
            features: &seg.props.features,
            rtl: false,
            language: &lang::bcp47(&seg.props.language),
        };
        let mut x = 0.0;
        let glyphs: Vec<PlacedGlyph> = shape(&req)
            .into_iter()
            .map(|g| {
                let pg = PlacedGlyph { id: g.id, x: x + g.dx, y: -g.dy, adv: g.advance, cluster: input.prefix + g.cluster };
                x += g.advance;
                pg
            })
            .collect();
        let width = x + seg.size * 0.15;
        Some(DropCap {
            run: GlyphRun { face: seg.face.clone(), size, shift: 0.0, props: seg.props.clone(), glyphs, x0: 0.0, x1: x, rtl: false, hscale: 1.0, orient: Orient::Side, text: String::new() },
            chars: end,
            lines: p.drop_lines,
            width,
            height,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn build_line(&self, sh: &Shaped, pr: &Prepared, span: (usize, usize), slot: Slot, origin: f64, last: bool, props: &ParaProps, right_page: bool, after_drop: bool) -> Line {
        let (from, to) = span;
        let mut start = usize::MAX;
        let mut end = 0usize;
        let mut hyphen = false;
        let mut forced = false;
        let mut stretch = 0.0;
        let mut shrink = 0.0;
        let mut natural = 0.0;
        let mut spaces: Vec<(usize, usize, f64, f64, f64)> = Vec::new();
        let upto = to.min(pr.items.len().saturating_sub(1));
        for k in from..=upto {
            let it = pr.items[k];
            let mk = pr.marks[k];
            let at_break = k == upto;
            match mk {
                Mark::Word(a, b) => {
                    start = start.min(a);
                    end = end.max(b);
                    natural += it.width();
                }
                Mark::Space(a, b) => {
                    if !at_break {
                        start = start.min(a);
                        end = end.max(b);
                        if let Item::Glue { w, stretch: y, shrink: z } = it {
                            natural += w;
                            stretch += y;
                            shrink += z;
                            spaces.push((a, b, w, y, z));
                        }
                    }
                }
                Mark::Hyphen(p) => {
                    if at_break {
                        hyphen = true;
                        natural += it.width();
                        end = end.max(p);
                    }
                }
                Mark::Break(p, soft) => {
                    end = end.max(p);
                    if at_break && soft {
                        hyphen = true;
                        natural += it.width();
                    }
                }
                Mark::Forced(p) => {
                    if at_break {
                        forced = true;
                        end = end.max(p);
                    }
                }
                Mark::End(p) => {
                    end = end.max(p);
                }
            }
        }
        if start == usize::MAX {
            start = end.min(sh.text.len());
        }
        if after_drop {
            start = start.max(pr.marks.iter().find_map(|m| if let Mark::Word(a, _) = m { Some(*a) } else { None }).unwrap_or(start));
        }
        let content_end = if forced {
            sh.text[..end].trim_end_matches([LINE_SEP, '\n']).len().max(start)
        } else {
            end
        };
        let (mut edge_l, mut edge_r) = (0.0, 0.0);
        let edge_lo = sh.glyphs.partition_point(|g| g.cluster < start);
        let edge_hi = sh.glyphs.partition_point(|g| g.cluster < content_end);
        if sh.mojikumi != Mojikumi::None && edge_hi > edge_lo {
            let g = &sh.glyphs[edge_lo];
            if !g.squeezed && sh.text.get(g.cluster..).and_then(|t| t.chars().next()).and_then(punct) == Some(Punct::Open) && g.adv >= sh.segs[g.seg].size * 0.9 {
                edge_l = g.adv * 0.5;
            }
            let g = &sh.glyphs[edge_hi - 1];
            if !g.squeezed && matches!(sh.text.get(g.cluster..).and_then(|t| t.chars().next()).and_then(punct), Some(Punct::Close | Punct::Stop)) && g.adv >= sh.segs[g.seg].size * 0.9 {
                edge_r = g.adv * 0.5;
            }
            if edge_hi - edge_lo == 1 {
                edge_r = if edge_l > 0.0 { 0.0 } else { edge_r };
            }
            natural -= edge_l + edge_r;
        }
        let align = props.align.resolve(right_page);
        let align = if props.rtl {
            match align {
                Align::Left => Align::Right,
                Align::Right => Align::Left,
                Align::Justify => Align::JustifyRight,
                Align::JustifyRight => Align::Justify,
                a => a,
            }
        } else {
            align
        };
        let (hang_l, hang_r) = if props.optical_margin && content_end > start { optical(sh, start, content_end, hyphen) } else { (0.0, 0.0) };
        let slot = Slot { left: slot.left - hang_l, width: slot.width + hang_l + hang_r };
        let justify_this = match align {
            Align::JustifyAll => true,
            a if a.justified() => !(last || forced),
            _ => false,
        };
        let r = if justify_this { kp::ratio_for(natural, stretch, shrink, 0.0, slot.width) } else { 0.0 };
        let mut space_extra: Vec<(usize, usize, f64)> = Vec::new();
        let mut letter_extra = 0.0;
        let mut gscale = props.glyph_scale[1].max(0.01);
        if justify_this {
            let rr = r.clamp(-1.0, 1.0);
            for (a, b, _w, y, z) in &spaces {
                let d = if rr >= 0.0 { y * rr } else { z * rr };
                space_extra.push((*a, *b, d));
            }
            if r > 1.0 {
                let used: f64 = space_extra.iter().map(|s| s.2).sum();
                let mut left = slot.width - natural - used;
                let gaps = sh.text[start..content_end].chars().filter(|c| !is_space(*c)).count().saturating_sub(1).max(1) as f64;
                let cjk_line = sh.text[start..content_end].chars().any(is_cjk);
                let max_ls = (props.letter_space[2] - props.letter_space[1]).max(if cjk_line { 0.3 } else { 0.0 }) * sh.base_size;
                letter_extra = (left / gaps).min(max_ls);
                left -= letter_extra * gaps;
                let space_nat: f64 = spaces.iter().map(|s| s.2).sum();
                let glyph_nat = (natural - space_nat).max(1.0);
                let max_gs = props.glyph_scale[2].max(gscale) / gscale;
                if left > 0.01 && max_gs > 1.0 {
                    let k = (1.0 + left / glyph_nat).min(max_gs);
                    left -= glyph_nat * (k - 1.0);
                    gscale *= k;
                }
                if left > 0.01 && !space_extra.is_empty() {
                    let each = left / space_extra.len() as f64;
                    for s in &mut space_extra {
                        s.2 += each;
                    }
                }
            }
        }
        let base_ls = props.letter_space[1] * sh.base_size;
        let level_of = |at: usize| sh.levels.get(at).copied().unwrap_or(0);
        let mut order: Vec<Range<usize>> = Vec::new();
        if sh.levels.iter().any(|l| *l > 0) && content_end > start {
            let info = BidiInfo::new(&sh.text, Some(if sh.rtl_para { unicode_bidi::Level::rtl() } else { unicode_bidi::Level::ltr() }));
            if let Some(para) = info.paragraphs.first() {
                let (_, runs) = info.visual_runs(para, start..content_end);
                order = runs;
            }
        }
        if order.is_empty() {
            order.push(start..content_end);
        }
        let lo = sh.glyphs.partition_point(|g| g.cluster < start);
        let hi = sh.glyphs.partition_point(|g| g.cluster < content_end);
        let stops = tab_stops(props);
        let mut x = 0.0;
        let mut runs: Vec<GlyphRun> = Vec::new();
        let mut tabs = Vec::new();
        let mut objects = Vec::new();
        let mut carets: Vec<(usize, f64)> = Vec::new();
        let mut rspans: Vec<(usize, f64, f64)> = sh.rubies.iter().enumerate().filter(|(_, r)| r.base.start >= start && r.base.end <= content_end).map(|(i, _)| (i, f64::MAX, f64::MIN)).collect();
        let mut kent: Vec<(usize, usize, f64)> = Vec::new();
        let mut last_seg = usize::MAX;
        for vr in &order {
            let rtl = level_of(vr.start) % 2 == 1;
            let mut idx: Vec<usize> = (lo..hi).filter(|&i| vr.contains(&sh.glyphs[i].cluster)).collect();
            if rtl {
                idx.reverse();
            }
            for i in idx {
                let g = sh.glyphs[i];
                let seg = &sh.segs[g.seg];
                if g.id == TAB_GLYPH {
                    let rel = x + slot.left - origin;
                    let (target, kind, leader, on) = stops
                        .iter()
                        .find(|s| s.0 > rel + 0.01)
                        .cloned()
                        .unwrap_or_else(|| (((rel / 36.0).floor() + 1.0) * 36.0, TabAlign::Left, String::new(), String::new()));
                    let follow_end = sh.text[g.cluster + 1..content_end].find('\t').map(|p| g.cluster + 1 + p).unwrap_or(content_end);
                    let follow = sh.width(g.cluster + 1..follow_end);
                    let pre = match kind {
                        TabAlign::Left => 0.0,
                        TabAlign::Right => follow,
                        TabAlign::Center => follow / 2.0,
                        TabAlign::Decimal => {
                            let ch = on.chars().next().unwrap_or('.');
                            let upto = sh.text[g.cluster + 1..follow_end].find(ch).map(|p| g.cluster + 1 + p).unwrap_or(follow_end);
                            sh.width(g.cluster + 1..upto)
                        }
                    };
                    let nx = (x + (target - rel) - pre).max(x);
                    carets.push((g.cluster, x));
                    tabs.push(TabFill { x0: x, x1: nx, leader });
                    x = nx;
                    continue;
                }
                if g.id == NO_GLYPH {
                    if sh.text[g.cluster..].starts_with(OBJ) {
                        objects.push((g.cluster, x));
                    }
                    carets.push((g.cluster, x));
                    x += g.adv;
                    continue;
                }
                let mut extra = base_ls + letter_extra;
                let mut adv = g.adv * gscale;
                let mut gdx = g.dx * gscale;
                if let Some(&(a, b, w, _, _)) = spaces.iter().find(|s| g.cluster >= s.0 && g.cluster < s.1) {
                    let count = sh.text[a..b].chars().count().max(1) as f64;
                    let nat = sh.width(a..b);
                    let scale = if nat > 0.0 { w / nat } else { 1.0 };
                    let just = space_extra.iter().find(|s| s.0 == a).map(|s| s.2).unwrap_or(0.0);
                    extra = g.adv * (scale - 1.0) + just / count;
                    adv = g.adv;
                    gdx = g.dx;
                }
                if i == lo && edge_l > 0.0 {
                    gdx -= edge_l;
                    adv -= edge_l;
                }
                if i + 1 == hi && edge_r > 0.0 {
                    adv -= edge_r;
                }
                let new_group = seg.orient == Orient::Tcy && last_seg != g.seg;
                last_seg = g.seg;
                let push_new = match runs.last() {
                    Some(r) => new_group || !Arc::ptr_eq(&r.props, &seg.props) || r.face.id != seg.face.id || (r.size - seg.size).abs() > 1e-9 || r.rtl != rtl || r.orient != seg.orient,
                    None => true,
                };
                if push_new {
                    runs.push(GlyphRun {
                        face: seg.face.clone(),
                        size: seg.size,
                        shift: seg.shift,
                        props: seg.props.clone(),
                        glyphs: Vec::new(),
                        x0: x,
                        x1: x,
                        rtl,
                        hscale: seg.props.h_scale * gscale * seg.tcy_scale,
                        orient: seg.orient,
                        text: String::new(),
                    });
                }
                let run = runs.last_mut().expect("run pushed");
                if !carets.iter().any(|c| c.0 == g.cluster) {
                    carets.push((g.cluster, if rtl { x + adv + extra } else { x }));
                }
                run.glyphs.push(PlacedGlyph { id: g.id, x: x + gdx, y: -(g.dy + seg.shift), adv: adv + extra, cluster: g.cluster });
                for sp in rspans.iter_mut() {
                    if sh.rubies[sp.0].base.contains(&g.cluster) {
                        sp.1 = sp.1.min(x);
                        sp.2 = sp.2.max(x + adv + extra);
                    }
                }
                if seg.props.kenten.is_some() && kent.last().is_none_or(|k| k.1 != g.cluster) && sh.text.get(g.cluster..).and_then(|t| t.chars().next()).is_some_and(|c| !c.is_whitespace() && !is_special(c)) {
                    kent.push((g.seg, g.cluster, x + (adv + extra) / 2.0));
                }
                x += adv + extra;
                run.x1 = x;
            }
        }
        if hyphen {
            let seg = sh.seg_at(content_end.saturating_sub(1)).cloned();
            if let Some(seg) = seg {
                let (gid, adv) = hyphen_glyph(&seg);
                let needs_new = runs.last().map(|r| r.face.id != seg.face.id || !Arc::ptr_eq(&r.props, &seg.props)).unwrap_or(true);
                if needs_new {
                    runs.push(GlyphRun { face: seg.face.clone(), size: seg.size, shift: seg.shift, props: seg.props.clone(), glyphs: Vec::new(), x0: x, x1: x, rtl: false, hscale: seg.props.h_scale, orient: Orient::Side, text: String::new() });
                }
                if let Some(run) = runs.last_mut() {
                    run.glyphs.push(PlacedGlyph { id: gid, x, y: -seg.shift, adv, cluster: GENERATED });
                    x += adv;
                    run.x1 = x;
                }
            }
        }
        let mut adorn: Vec<GlyphRun> = Vec::new();
        let (mut up, mut down, mut ruby_lead) = (0.0f64, 0.0f64, 0.0f64);
        let adorn_orient = if sh.vertical { Orient::Upright } else { Orient::Side };
        for (pi, lo_x, hi_x) in &rspans {
            if *lo_x > *hi_x {
                continue;
            }
            let rp = &sh.rubies[*pi];
            let n = rp.glyphs.len();
            let span = hi_x - lo_x;
            let free = span - rp.width;
            let centered = (lo_x + free / 2.0, 0.0);
            let (first_x, gap) = match rp.ruby.align {
                RubyAlign::Center => centered,
                RubyAlign::Left => (*lo_x, 0.0),
                RubyAlign::Right => (hi_x - rp.width, 0.0),
                RubyAlign::Justify if n > 1 && free > 0.0 => (*lo_x, free / (n - 1) as f64),
                RubyAlign::Jis if n > 1 && free > 0.0 => (lo_x + free / (2 * n) as f64, free / n as f64),
                RubyAlign::EqualAki if n > 1 && free > 0.0 => (lo_x + free / (2 * n) as f64, free / n as f64),
                _ => centered,
            };
            let top = rp.base_size * 0.88 + rp.base_shift;
            let bottom = rp.base_size * 0.12 - rp.base_shift;
            let y = if rp.ruby.below { bottom + rp.ruby.offset + rp.size * 0.88 } else { -(top + rp.ruby.offset + rp.size * 0.12) };
            if rp.ruby.below {
                down = down.max(bottom + rp.ruby.offset + rp.size);
            } else {
                up = up.max(top + rp.ruby.offset + rp.size);
            }
            if rp.props.leading.is_none() {
                ruby_lead = ruby_lead.max(rp.base_size + rp.size + rp.ruby.offset.max(0.0));
            }
            let mut pen = first_x;
            let mut placed = Vec::with_capacity(n);
            for g in &rp.glyphs {
                placed.push(PlacedGlyph { id: g.id, x: pen + g.dx, y: y - g.dy, adv: g.advance, cluster: g.cluster });
                pen += g.advance + gap;
            }
            adorn.push(GlyphRun { face: rp.face.clone(), size: rp.size, shift: 0.0, props: rp.props.clone(), glyphs: placed, x0: first_x, x1: pen - gap, rtl: false, hscale: 1.0, orient: adorn_orient, text: rp.text.clone() });
        }
        let mut mark_props: Vec<(*const CharProps, Arc<CharProps>)> = Vec::new();
        let mut kent_idx: Option<usize> = None;
        let mut marks: Vec<EmphasisMark> = Vec::new();
        for (si, _cluster, cx) in &kent {
            let seg = &sh.segs[*si];
            let Some(k) = seg.props.kenten.as_ref() else { continue };
            let msize = (seg.size * k.scale).max(1.0);
            let face = if seg.face.covers(k.mark) { seg.face.clone() } else { self.fonts.fallback(&seg.face, k.mark).unwrap_or_else(|| seg.face.clone()) };
            let mut buf = [0u8; 4];
            let shaped = shape(&ShapeReq { text: k.mark.encode_utf8(&mut buf), face: &face, size: msize, kerning: false, ligatures: false, features: &[], rtl: false, language: "en" });
            let Some(mg) = shaped.first() else { continue };
            let props = match mark_props.iter().find(|(p, _)| std::ptr::eq(*p, Arc::as_ptr(&seg.props))) {
                Some((_, a)) => a.clone(),
                None => {
                    let mut p = (*seg.props).clone();
                    p.size = msize;
                    p.underline = false;
                    p.strike = false;
                    p.ruby = None;
                    p.kenten = None;
                    p.tcy = false;
                    p.tracking = 0.0;
                    p.kern = 0.0;
                    p.h_scale = 1.0;
                    p.v_scale = 1.0;
                    p.baseline_shift = 0.0;
                    p.position = Position::Normal;
                    if let Some(c) = &k.color {
                        p.fill = c.clone();
                        p.fill_tint = k.tint;
                    }
                    let a = Arc::new(p);
                    mark_props.push((Arc::as_ptr(&seg.props), a.clone()));
                    a
                }
            };
            let top = seg.size * 0.88 + seg.shift;
            let bottom = seg.size * 0.12 - seg.shift;
            if matches!(k.mark, '\u{FE45}' | '\u{FE46}' | '\u{2022}' | '\u{25CB}' | '\u{25C9}' | '\u{25B2}' | '\u{25B3}') {
                let s = msize * 0.6;
                let gap = seg.size * 0.06;
                let cy = if k.below { bottom + gap + s / 2.0 } else { -(top + gap + s / 2.0) };
                if k.below {
                    down = down.max(bottom + gap + s);
                } else {
                    up = up.max(top + gap + s);
                }
                marks.push(EmphasisMark { x: *cx, y: cy, size: s, kind: k.mark, props });
                continue;
            }
            let y = if k.below { bottom + seg.size * 0.04 + msize * 0.88 } else { -(top + seg.size * 0.04 + msize * 0.12) };
            if k.below {
                down = down.max(bottom + seg.size * 0.04 + msize);
            } else {
                up = up.max(top + seg.size * 0.04 + msize);
            }
            let mx = if sh.vertical { cx - msize * 0.5 } else { cx - mg.advance / 2.0 };
            let fresh = match kent_idx {
                Some(ix) => !Arc::ptr_eq(&adorn[ix].props, &props) || adorn[ix].face.id != face.id,
                None => true,
            };
            if fresh {
                adorn.push(GlyphRun { face: face.clone(), size: msize, shift: 0.0, props: props.clone(), glyphs: Vec::new(), x0: mx, x1: mx, rtl: false, hscale: 1.0, orient: adorn_orient, text: String::new() });
                kent_idx = Some(adorn.len() - 1);
            }
            if let Some(ix) = kent_idx {
                let r = &mut adorn[ix];
                r.glyphs.push(PlacedGlyph { id: mg.id, x: mx + mg.dx, y: y - mg.dy, adv: mg.advance, cluster: GENERATED });
                r.x1 = mx + mg.advance;
            }
        }
        carets.push((content_end, x));
        carets.sort_by_key(|c| c.0);
        carets.dedup_by_key(|c| c.0);
        let used = x;
        let offset = match align {
            Align::Right => slot.width - used,
            Align::Center => (slot.width - used) / 2.0,
            Align::JustifyRight if last || forced => slot.width - used,
            Align::JustifyCenter if last || forced => (slot.width - used) / 2.0,
            _ => 0.0,
        }
        .max(if matches!(align, Align::Right | Align::Center) { f64::MIN } else { 0.0 });
        let dx = slot.left + offset;
        for r in &mut runs {
            r.x0 += dx;
            r.x1 += dx;
            for g in &mut r.glyphs {
                g.x += dx;
            }
        }
        for r in &mut adorn {
            r.x0 += dx;
            r.x1 += dx;
            for g in &mut r.glyphs {
                g.x += dx;
            }
        }
        for m in &mut marks {
            m.x += dx;
        }
        for t in &mut tabs {
            t.x0 += dx;
            t.x1 += dx;
        }
        for o in &mut objects {
            o.1 += dx;
        }
        for c in &mut carets {
            c.1 += dx;
        }
        let mut ascent: f64 = 0.0;
        let mut descent: f64 = 0.0;
        let mut cap: f64 = 0.0;
        let mut xh: f64 = 0.0;
        let mut leading: f64 = 0.0;
        let mut measure = |seg_face: &Face, size: f64, shift: f64, p: &CharProps| {
            let m = &seg_face.metrics;
            let k = size / m.upem;
            ascent = ascent.max(m.ascent * k + shift);
            descent = descent.max(m.descent * k - shift);
            cap = cap.max(m.cap_height * k);
            xh = xh.max(m.x_height * k);
            leading = leading.max(p.leading.unwrap_or(p.size * props.auto_leading));
        };
        for r in &runs {
            measure(&r.face, r.size, r.shift, &r.props);
        }
        if runs.is_empty() {
            if let Some(seg) = sh.seg_at(start) {
                measure(&seg.face, seg.size, seg.shift, &seg.props);
            } else {
                leading = 12.0 * props.auto_leading;
                ascent = 9.0;
                descent = 3.0;
            }
        }
        ascent = ascent.max(up);
        descent = descent.max(down);
        leading = leading.max(ruby_lead);
        for (at, _) in &objects {
            if let Some(o) = sh.objects.iter().find(|o| o.at == *at) {
                ascent = ascent.max(o.ascent);
                descent = descent.max(o.descent);
                leading = leading.max((o.ascent + o.descent) * props.auto_leading.max(1.0));
            }
        }
        Line {
            start,
            end,
            left: slot.left,
            avail: slot.width,
            width: used,
            ascent,
            descent,
            cap,
            xheight: xh,
            leading,
            runs,
            adorn,
            marks,
            hyphen,
            forced,
            last,
            tabs,
            objects,
            carets,
            ratio: r,
        }
    }
}

pub fn slice_input(input: &ParaInput, from: usize) -> ParaInput {
    let from = from.min(input.text.len());
    let runs = input
        .runs
        .iter()
        .filter(|r| r.range.end > from || (r.range.is_empty() && r.range.start >= from))
        .map(|r| TextRun { range: r.range.start.max(from) - from..r.range.end.max(from) - from, props: r.props.clone() })
        .collect::<Vec<_>>();
    let mut props = input.props.clone();
    props.first_indent = 0.0;
    props.drop_lines = 0;
    props.drop_chars = 0;
    ParaInput {
        text: input.text[from..].to_string(),
        runs: if runs.is_empty() { input.runs.last().map(|r| vec![TextRun { range: 0..0, props: r.props.clone() }]).unwrap_or_default() } else { runs },
        objects: input.objects.iter().filter(|o| o.at >= from).map(|o| ObjBox { at: o.at - from, ..o.clone() }).collect(),
        props,
        prefix: input.prefix.saturating_sub(from),
        exceptions: input.exceptions.clone(),
        vertical: input.vertical,
    }
}

pub fn shift_line(line: &mut Line, by: usize) {
    line.start += by;
    line.end += by;
    for r in &mut line.runs {
        for g in &mut r.glyphs {
            g.cluster += by;
        }
    }
    for o in &mut line.objects {
        o.0 += by;
    }
    for c in &mut line.carets {
        c.0 += by;
    }
}

fn trim_front(pr: &mut Prepared, sh: &Shaped, from: usize, upto: usize) {
    for (k, m) in pr.marks.iter_mut().enumerate() {
        if let Mark::Word(a, b) = m {
            if *b <= upto {
                pr.items[k] = Item::Box { w: 0.0 };
                let p = if *a >= from { upto } else { *b };
                *a = p;
                *b = p;
            } else if *a < upto {
                *a = upto;
                pr.items[k] = Item::Box { w: sh.width(upto..*b) };
            }
        } else if let Mark::Space(a, b) = m
            && *a >= from
            && *b <= upto
        {
            pr.items[k] = Item::Glue { w: 0.0, stretch: 0.0, shrink: 0.0 };
            *a = upto;
            *b = upto;
        }
    }
}
