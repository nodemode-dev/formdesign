use std::ops::Range;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use formdesign_model::doc::Doc;
use formdesign_model::ids::StoryId;
use formdesign_model::story::{Seg, Story};
use kurbo::{Point, Rect};
use serde_json::{Value, json};

use crate::core::Core;
use crate::textloc::{self, byte_to_char, marker_text};

type Key = (u64, usize, u64);
type Miss = (Range<usize>, Option<&'static str>);
type Found = Arc<Vec<(StoryId, Vec<Miss>)>>;

static CACHE: Mutex<Option<(Key, Found)>> = Mutex::new(None);
type Loaded = Vec<(&'static str, Option<Arc<spellbook::Dictionary>>)>;

static DICTS: Mutex<Loaded> = Mutex::new(Vec::new());
static BUILTIN: OnceLock<Option<Arc<spellbook::Dictionary>>> = OnceLock::new();

const LANGS: [(&str, &str); 22] = [
    ("English: UK", "en_GB"),
    ("English: Canadian", "en_CA"),
    ("German: 2006 Reform", "de_DE"),
    ("German: Swiss 2006 Reform", "de_CH"),
    ("French", "fr_FR"),
    ("French: Canadian", "fr_CA"),
    ("Spanish", "es_ES"),
    ("Italian", "it_IT"),
    ("Portuguese", "pt_PT"),
    ("Portuguese: Brazilian", "pt_BR"),
    ("Dutch: 2005 Reform", "nl_NL"),
    ("Danish", "da_DK"),
    ("Swedish", "sv_SE"),
    ("Norwegian: Bokmål", "nb_NO"),
    ("Finnish", "fi_FI"),
    ("Polish", "pl_PL"),
    ("Czech", "cs_CZ"),
    ("Hungarian", "hu_HU"),
    ("Greek", "el_GR"),
    ("Russian", "ru_RU"),
    ("Ukrainian", "uk_UA"),
    ("Turkish", "tr_TR"),
];

fn builtin() -> Option<Arc<spellbook::Dictionary>> {
    BUILTIN
        .get_or_init(|| spellbook::Dictionary::new(include_str!("../dict/en_US.aff"), include_str!("../dict/en_US.dic")).ok().map(Arc::new))
        .clone()
}

fn code_of(language: &str) -> Option<&'static str> {
    let norm = language.replace('-', "_");
    LANGS.iter().find(|(n, c)| *n == language || c.eq_ignore_ascii_case(&norm)).map(|(_, c)| *c)
}

fn dirs() -> Vec<PathBuf> {
    std::env::var_os("FORMDESIGN_DICT_DIRS").map(|v| std::env::split_paths(&v).filter(|p| p.is_dir()).collect()).unwrap_or_default()
}

fn files(code: &str) -> Option<(PathBuf, PathBuf)> {
    let lang = code.split('_').next().unwrap_or(code);
    let mut near = None;
    for dir in dirs() {
        let (a, d) = (dir.join(format!("{code}.aff")), dir.join(format!("{code}.dic")));
        if a.is_file() && d.is_file() {
            return Some((a, d));
        }
        if near.is_none()
            && let Ok(rd) = std::fs::read_dir(&dir)
        {
            let mut stems: Vec<String> = rd.filter_map(|e| e.ok()?.file_name().into_string().ok()?.strip_suffix(".aff").map(str::to_string)).collect();
            stems.sort();
            near = stems
                .into_iter()
                .find(|s| {
                    let s = s.replace('-', "_");
                    s == lang || s.starts_with(&format!("{lang}_"))
                })
                .map(|s| (dir.join(format!("{s}.aff")), dir.join(format!("{s}.dic"))))
                .filter(|(_, d)| d.is_file());
        }
    }
    near
}

fn read_text(p: &PathBuf) -> Option<String> {
    let b = std::fs::read(p).ok()?;
    Some(match String::from_utf8(b) {
        Ok(s) => s,
        Err(e) => e.into_bytes().into_iter().map(char::from).collect(),
    })
}

fn dictionary(code: &'static str) -> Option<Arc<spellbook::Dictionary>> {
    let mut l = DICTS.lock().ok()?;
    if let Some((_, d)) = l.iter().find(|(c, _)| *c == code) {
        return d.clone();
    }
    let d = files(code).and_then(|(a, d)| spellbook::Dictionary::new(&read_text(&a)?, &read_text(&d)?).ok()).map(Arc::new);
    l.push((code, d.clone()));
    d
}

fn words(text: &str, range: Range<usize>) -> Vec<Range<usize>> {
    let base = range.start;
    let ch: Vec<(usize, char)> = text[range].char_indices().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < ch.len() {
        if !ch[i].1.is_alphabetic() {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < ch.len() && (ch[j].1.is_alphabetic() || ((ch[j].1 == '\'' || ch[j].1 == '\u{2019}') && j + 1 < ch.len() && ch[j + 1].1.is_alphabetic())) {
            j += 1;
        }
        let end = ch.get(j).map(|c| c.0).unwrap_or(ch[ch.len() - 1].0 + ch[ch.len() - 1].1.len_utf8());
        out.push(base + ch[i].0..base + end);
        i = j.max(i + 1);
    }
    out
}

fn user_words(doc: &Doc) -> Vec<String> {
    doc.prefs.get("userWords").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_lowercase)).collect()).unwrap_or_default()
}

fn hidden(doc: &Doc, props: &formdesign_model::attrs::CharProps) -> bool {
    props.change.as_deref() == Some("deleted") || (!props.conditions.is_empty() && props.conditions.iter().all(|c| doc.conditions.iter().any(|x| x.name == *c && !x.visible)))
}

fn misses(doc: &Doc, story: &Story, user: &[String]) -> Vec<Miss> {
    let text = marker_text(&story.paras);
    let mut spans: Vec<(Range<usize>, Option<&'static str>, Arc<spellbook::Dictionary>)> = Vec::new();
    let mut byte = 0usize;
    for (pi, para) in story.paras.iter().enumerate() {
        if pi > 0 {
            byte += 1;
        }
        for seg in &para.segs {
            let len = match seg {
                Seg::Text { text, .. } => text.len(),
                Seg::Inline { .. } => 3,
            };
            let r = byte..byte + len;
            byte += len;
            if !matches!(seg, Seg::Text { .. }) || len == 0 {
                continue;
            }
            let props = doc.styles.char_props(&para.style, &para.over, seg.style(), seg.over());
            if hidden(doc, &props) || props.language.is_empty() {
                continue;
            }
            let (code, dict) = match code_of(&props.language).and_then(|c| dictionary(c).map(|d| (Some(c), d))) {
                Some(x) => x,
                None if props.language.to_ascii_lowercase().starts_with("en") => match builtin() {
                    Some(d) => (None, d),
                    None => continue,
                },
                None => continue,
            };
            match spans.last_mut() {
                Some(last) if last.1 == code && last.0.end == r.start => last.0.end = r.end,
                _ => spans.push((r, code, dict)),
            }
        }
    }
    let mut out = Vec::new();
    for (r, c, d) in &spans {
        for w in words(&text, r.clone()) {
            let word = text[w.clone()].replace('\u{2019}', "'");
            let n = word.chars().count();
            if n <= 1 || (word.chars().all(char::is_uppercase) && n <= 5) || d.check(&word) || user.contains(&word.to_lowercase()) {
                continue;
            }
            out.push((w, *c));
        }
    }
    out.sort_by_key(|m| m.0.start);
    out
}

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn found(core: &Core) -> Option<Found> {
    let st = core.ses.active()?;
    let key = (st.uid, Arc::as_ptr(&st.doc) as usize, st.revision);
    let mut cache = CACHE.lock().ok()?;
    if let Some((k, v)) = cache.as_ref()
        && *k == key
    {
        return Some(v.clone());
    }
    let d = &st.doc;
    let user = user_words(d);
    let v: Found = Arc::new(
        d.stories
            .values()
            .filter_map(|s| {
                let r = misses(d, s, &user);
                (!r.is_empty()).then_some((s.id, r))
            })
            .collect(),
    );
    *cache = Some((key, v.clone()));
    Some(v)
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x0 <= b.x1 && b.x0 <= a.x1 && a.y0 <= b.y1 && b.y0 <= a.y1
}

pub fn marks(core: &mut Core, p: &Value) -> Result<Value, String> {
    let Some(all) = found(core) else { return Ok(json!({"lines": [], "count": 0})) };
    let r = core.ses.renderer.clone();
    let st = core.ses.active_mut().ok_or("no document open")?;
    let lay = st.layout(&r);
    let cv = st.canvas();
    let doc = st.doc.clone();
    let vr = p.get("rect").and_then(Value::as_array).map(|a| {
        let g = |i: usize| a.get(i).and_then(Value::as_f64).unwrap_or(0.0);
        Rect::new(g(0), g(1), g(2), g(3))
    });
    let mut out = Vec::new();
    for (sid, ranges) in all.iter() {
        let Some(story) = doc.story(*sid) else { continue };
        let visible = story.frames.iter().any(|f| {
            let slot = formdesign_engine::tools::geo::spread_of_item(&doc, doc.top_of(*f)).and_then(|r| cv.slot(r));
            match (slot, doc.bounds(*f)) {
                (Some(s), Some(b)) => vr.is_none_or(|v| overlaps(s.xf.transform_rect_bbox(b), v)),
                _ => false,
            }
        });
        if !visible {
            continue;
        }
        let text = marker_text(&story.paras);
        for (rg, _) in ranges {
            let cr = byte_to_char(&text, rg.start)..byte_to_char(&text, rg.end);
            for sp in textloc::spans(&doc, &lay, &cv, *sid, cr) {
                let y = sp.base + (sp.desc * 0.45).max(1.0);
                let (p0, p1) = (sp.m * Point::new(sp.x0.min(sp.x1), y), sp.m * Point::new(sp.x0.max(sp.x1), y));
                out.push(json!([r2(p0.x), r2(p0.y), r2(p1.x), r2(p1.y)]));
            }
        }
    }
    Ok(json!({"lines": out, "count": all.iter().map(|x| x.1.len()).sum::<usize>()}))
}

fn distance(a: &[char], b: &[char]) -> usize {
    let (n, m) = (a.len(), b.len());
    let mut d = vec![vec![0usize; m + 1]; n + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[n][m]
}

fn fold(s: &str) -> Vec<char> {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ą' => 'a',
            'ç' | 'č' | 'ć' => 'c',
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ě' | 'ę' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ñ' | 'ń' | 'ň' => 'n',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ő' => 'o',
            'ù' | 'ú' | 'û' | 'ü' | 'ű' | 'ů' => 'u',
            'ý' | 'ÿ' => 'y',
            'ž' | 'ź' | 'ż' => 'z',
            'š' | 'ś' => 's',
            'ř' => 'r',
            'ł' => 'l',
            other => other,
        })
        .collect()
}

pub fn check(core: &Core, p: &Value) -> Result<Value, String> {
    let st = core.ses.active().ok_or("no document open")?;
    let want = p.get("story").and_then(Value::as_u64);
    let mut out = Vec::new();
    for (sid, list) in found(core).unwrap_or_default().iter() {
        if want.is_some_and(|w| w != sid.0) {
            continue;
        }
        let Some(story) = st.doc.story(*sid) else { continue };
        let text = marker_text(&story.paras);
        for (r, c) in list {
            out.push(json!({"story": sid.0, "start": r.start, "end": r.end, "word": &text[r.clone()], "lang": c}));
        }
    }
    Ok(Value::Array(out))
}

pub fn languages() -> Value {
    let mut out = vec![json!({"language": "English: USA", "code": "en_US", "installed": true, "builtin": true})];
    for (n, c) in LANGS {
        let file = files(c).and_then(|(a, _)| a.file_stem().map(|s| s.to_string_lossy().into_owned()));
        out.push(json!({"language": n, "code": c, "installed": file.is_some(), "file": file, "builtin": false}));
    }
    json!({"languages": out, "folders": dirs().iter().map(|d| d.display().to_string()).collect::<Vec<_>>()})
}

pub fn reload() -> Value {
    if let Ok(mut d) = DICTS.lock() {
        d.clear();
    }
    if let Ok(mut c) = CACHE.lock() {
        *c = None;
    }
    languages()
}

fn recase(word: &str, list: Vec<String>) -> Vec<String> {
    let cap = word.chars().next().is_some_and(char::is_uppercase);
    let letters: Vec<char> = word.chars().filter(|c| c.is_alphabetic()).collect();
    let upper = letters.len() > 1 && letters.iter().all(|c| c.is_uppercase());
    list.into_iter()
        .map(|s| {
            if upper {
                s.to_uppercase()
            } else if cap {
                let mut c = s.chars();
                c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
            } else {
                s
            }
        })
        .collect()
}

pub fn suggest(p: &Value) -> Result<Value, String> {
    let word = p.get("word").and_then(Value::as_str).ok_or("missing `word`")?;
    let named = p.get("lang").and_then(Value::as_str).and_then(|l| LANGS.iter().find(|(_, c)| *c == l)).and_then(|(_, c)| dictionary(c));
    let english = named.is_none();
    let Some(d) = named.or_else(builtin) else { return Ok(json!({"word": word, "suggestions": []})) };
    let mut list = Vec::new();
    d.suggest(&word.replace('\u{2019}', "'"), &mut list);
    let w = fold(word);
    let mut v: Vec<(usize, String)> = list.into_iter().enumerate().collect();
    let within = |small: &[char], big: &[char]| {
        let mut it = big.iter();
        small.iter().all(|c| it.any(|x| x == c))
    };
    v.sort_by_cached_key(|(i, s)| {
        let f = fold(s);
        (usize::from(s.contains(' ') && !word.contains(' ')), distance(&w, &f), usize::from(!within(&w, &f)), *i)
    });
    let mut list: Vec<String> = v.into_iter().map(|(_, s)| s).collect();
    list.dedup();
    list.truncate(8);
    if english {
        list = recase(word, list);
    }
    Ok(json!({"word": word, "suggestions": list}))
}
