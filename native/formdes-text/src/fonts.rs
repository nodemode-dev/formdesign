use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

use fontdb::{Database, Family, Query, Stretch, Style, Weight};
use skrifa::instance::{LocationRef, Size};
use skrifa::raw::FontRef;
use skrifa::MetadataProvider;

pub type FaceId = u32;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StyleWish {
    pub weight: u16,
    pub italic: bool,
    pub stretch: u16,
}

pub fn parse_style(name: &str) -> StyleWish {
    let n = name.to_ascii_lowercase().replace(['-', '_'], " ");
    let compact = n.replace(' ', "");
    let weight = [
        ("hairline", 100),
        ("thin", 100),
        ("extralight", 200),
        ("ultralight", 200),
        ("light", 300),
        ("book", 400),
        ("regular", 400),
        ("normal", 400),
        ("roman", 400),
        ("medium", 500),
        ("semibold", 600),
        ("demibold", 600),
        ("demi", 600),
        ("extrabold", 800),
        ("ultrabold", 800),
        ("heavy", 900),
        ("black", 900),
        ("bold", 700),
    ]
    .iter()
    .find(|(k, _)| compact.contains(k))
    .map(|(_, w)| *w)
    .unwrap_or(400);
    let italic = n.contains("italic") || n.contains("oblique") || n.contains("slanted");
    let stretch = [
        ("ultracondensed", 1),
        ("extracondensed", 2),
        ("semicondensed", 4),
        ("condensed", 3),
        ("narrow", 3),
        ("semiexpanded", 6),
        ("ultraexpanded", 9),
        ("extraexpanded", 8),
        ("expanded", 7),
        ("wide", 7),
    ]
    .iter()
    .find(|(k, _)| compact.contains(k))
    .map(|(_, s)| *s)
    .unwrap_or(5);
    StyleWish { weight, italic, stretch }
}

fn stretch_of(n: u16) -> Stretch {
    match n {
        1 => Stretch::UltraCondensed,
        2 => Stretch::ExtraCondensed,
        3 => Stretch::Condensed,
        4 => Stretch::SemiCondensed,
        6 => Stretch::SemiExpanded,
        7 => Stretch::Expanded,
        8 => Stretch::ExtraExpanded,
        9 => Stretch::UltraExpanded,
        _ => Stretch::Normal,
    }
}

#[derive(Clone, Debug)]
pub struct Metrics {
    pub upem: f64,
    pub ascent: f64,
    pub descent: f64,
    pub line_gap: f64,
    pub cap_height: f64,
    pub x_height: f64,
    pub underline_offset: f64,
    pub underline_thickness: f64,
    pub strike_offset: f64,
    pub strike_thickness: f64,
}

pub struct Face {
    pub id: FaceId,
    pub data: Arc<Vec<u8>>,
    pub index: u32,
    pub family: String,
    pub style: String,
    pub postscript: String,
    pub weight: u16,
    pub italic: bool,
    pub metrics: Metrics,
    pub synthetic_bold: bool,
    pub synthetic_italic: bool,
    shaper: harfrust::ShaperData,
    glyph_cache: RwLock<HashMap<char, Option<u32>>>,
}

impl Face {
    pub fn font(&self) -> FontRef<'_> {
        FontRef::from_index(&self.data, self.index).expect("face parsed on load")
    }

    pub fn shaper_data(&self) -> &harfrust::ShaperData {
        &self.shaper
    }

    pub fn glyph(&self, c: char) -> Option<u32> {
        if let Some(g) = self.glyph_cache.read().ok().and_then(|m| m.get(&c).copied()) {
            return g;
        }
        let g = self.font().charmap().map(c).map(|g| g.to_u32()).filter(|g| *g != 0);
        if let Ok(mut m) = self.glyph_cache.write() {
            m.insert(c, g);
        }
        g
    }

    pub fn chars(&self) -> Vec<u32> {
        self.font().charmap().mappings().map(|(c, _)| c).collect()
    }

    pub fn is_variable(&self) -> bool {
        !self.font().axes().is_empty()
    }

    pub fn covers(&self, c: char) -> bool {
        c.is_control() || matches!(c, '\u{200b}'..='\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2060}'..='\u{2064}' | '\u{feff}') || self.glyph(c).is_some()
    }

    pub fn advance(&self, gid: u32) -> f64 {
        let font = self.font();
        font.glyph_metrics(Size::unscaled(), LocationRef::default())
            .advance_width(skrifa::GlyphId::new(gid))
            .map(|a| a as f64)
            .unwrap_or(0.0)
    }

    pub fn has_feature(&self, tag: &[u8; 4]) -> bool {
        use skrifa::raw::TableProvider;
        let font = self.font();
        let t = skrifa::raw::types::Tag::new(tag);
        let in_gsub = font.gsub().ok().and_then(|g| g.feature_list().ok()).map(|l| l.feature_records().iter().any(|r| r.feature_tag() == t)).unwrap_or(false);
        let in_gpos = font.gpos().ok().and_then(|g| g.feature_list().ok()).map(|l| l.feature_records().iter().any(|r| r.feature_tag() == t)).unwrap_or(false);
        in_gsub || in_gpos
    }
}

fn read_metrics(font: &FontRef) -> Metrics {
    let m = font.metrics(Size::unscaled(), LocationRef::default());
    let upem = m.units_per_em.max(1) as f64;
    let ascent = m.ascent as f64;
    let descent = -(m.descent as f64);
    let cap = m.cap_height.map(|v| v as f64).unwrap_or(ascent * 0.7);
    let xh = m.x_height.map(|v| v as f64).unwrap_or(ascent * 0.5);
    let (uo, ut) = m.underline.map(|d| (d.offset as f64, d.thickness as f64)).unwrap_or((-upem * 0.1, upem * 0.05));
    let (so, st) = m.strikeout.map(|d| (d.offset as f64, d.thickness as f64)).unwrap_or((xh * 0.5, ut));
    Metrics {
        upem,
        ascent,
        descent: descent.abs(),
        line_gap: m.leading as f64,
        cap_height: cap,
        x_height: xh,
        underline_offset: uo,
        underline_thickness: ut.max(upem * 0.02),
        strike_offset: so,
        strike_thickness: st.max(upem * 0.02),
    }
}

pub struct FontBook {
    db: RwLock<Database>,
    faces: RwLock<HashMap<FaceId, Arc<Face>>>,
    by_db: RwLock<HashMap<fontdb::ID, FaceId>>,
    resolved: RwLock<HashMap<(String, String), Option<FaceId>>>,
    fallback_for: RwLock<HashMap<(FaceId, char), Option<FaceId>>>,
    pub fallbacks: Vec<String>,
    next: RwLock<FaceId>,
}

pub const DEFAULT_FALLBACKS: &[&str] = &[
    "Noto Sans",
    "Noto Serif",
    "DejaVu Sans",
    "Liberation Sans",
    "Arial Unicode MS",
    "Segoe UI Symbol",
    "Apple Symbols",
    "Noto Sans Symbols",
    "Noto Sans Symbols 2",
    "Noto Sans Math",
    "Noto Sans CJK SC",
    "Noto Sans CJK JP",
    "Noto Sans Arabic",
    "Noto Sans Hebrew",
    "Noto Sans Devanagari",
    "Noto Color Emoji",
];

impl Default for FontBook {
    fn default() -> Self {
        FontBook::new()
    }
}

impl FontBook {
    pub fn new() -> FontBook {
        FontBook {
            db: RwLock::new(Database::new()),
            faces: RwLock::new(HashMap::new()),
            by_db: RwLock::new(HashMap::new()),
            resolved: RwLock::new(HashMap::new()),
            fallback_for: RwLock::new(HashMap::new()),
            fallbacks: DEFAULT_FALLBACKS.iter().map(|s| s.to_string()).collect(),
            next: RwLock::new(1),
        }
    }

    fn invalidate(&self) {
        if let Ok(mut r) = self.resolved.write() {
            r.clear();
        }
        if let Ok(mut r) = self.fallback_for.write() {
            r.clear();
        }
    }

    pub fn load_system(&self) {
        if let Ok(mut db) = self.db.write() {
            db.load_system_fonts();
        }
        self.invalidate();
    }

    pub fn load_dir(&self, dir: &Path) {
        if let Ok(mut db) = self.db.write() {
            db.load_fonts_dir(dir);
        }
        self.invalidate();
    }

    pub fn load_data(&self, data: Vec<u8>) {
        if let Ok(mut db) = self.db.write() {
            db.load_font_data(data);
        }
        self.invalidate();
    }

    pub fn len(&self) -> usize {
        self.db.read().map(|d| d.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn families(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .db
            .read()
            .map(|d| d.faces().filter_map(|f| f.families.first().map(|(n, _)| n.clone())).collect())
            .unwrap_or_default();
        out.sort_by_key(|s| s.to_lowercase());
        out.dedup();
        out
    }

    pub fn styles_of(&self, family: &str) -> Vec<String> {
        let ids: Vec<fontdb::ID> = self
            .db
            .read()
            .map(|d| d.faces().filter(|f| f.families.iter().any(|(n, _)| n.eq_ignore_ascii_case(family))).map(|f| f.id).collect())
            .unwrap_or_default();
        let mut out: Vec<(u16, bool, String)> = ids.into_iter().filter_map(|i| self.open(i)).map(|f| (f.weight, f.italic, f.style.clone())).collect();
        out.sort();
        out.dedup_by(|a, b| a.2 == b.2);
        out.into_iter().map(|t| t.2).collect()
    }

    pub fn face(&self, id: FaceId) -> Option<Arc<Face>> {
        self.faces.read().ok()?.get(&id).cloned()
    }

    fn open(&self, dbid: fontdb::ID) -> Option<Arc<Face>> {
        if let Some(id) = self.by_db.read().ok()?.get(&dbid).copied() {
            return self.face(id);
        }
        let (data, index, family, ps, weight, italic) = {
            let db = self.db.read().ok()?;
            let info = db.face(dbid)?;
            let family = info.families.first().map(|(n, _)| n.clone()).unwrap_or_default();
            let mut bytes = None;
            db.with_face_data(dbid, |d, _| bytes = Some(d.to_vec()));
            (bytes?, info.index, family, info.post_script_name.clone(), info.weight.0, info.style != Style::Normal)
        };
        let data = Arc::new(data);
        let font = FontRef::from_index(&data, index).ok()?;
        let metrics = read_metrics(&font);
        let style = style_name(&font).unwrap_or_else(|| default_style_name(weight, italic));
        let shaper = harfrust::ShaperData::new(&font);
        let mut next = self.next.write().ok()?;
        let id = *next;
        *next += 1;
        let face = Arc::new(Face {
            id,
            data: data.clone(),
            index,
            family,
            style,
            postscript: ps,
            weight,
            italic,
            metrics,
            synthetic_bold: false,
            synthetic_italic: false,
            shaper,
            glyph_cache: RwLock::new(HashMap::new()),
        });
        self.faces.write().ok()?.insert(id, face.clone());
        self.by_db.write().ok()?.insert(dbid, id);
        Some(face)
    }

    pub fn resolve(&self, family: &str, style: &str) -> Option<Arc<Face>> {
        let key = (family.to_string(), style.to_string());
        if let Some(hit) = self.resolved.read().ok()?.get(&key).copied() {
            return hit.and_then(|id| self.face(id));
        }
        let found = self.lookup(family, style);
        if let Ok(mut r) = self.resolved.write() {
            r.insert(key, found.as_ref().map(|f| f.id));
        }
        found
    }

    fn lookup(&self, family: &str, style: &str) -> Option<Arc<Face>> {
        let wish = parse_style(style);
        let exact: Vec<fontdb::ID> = self
            .db
            .read()
            .ok()?
            .faces()
            .filter(|f| f.families.iter().any(|(n, _)| n.eq_ignore_ascii_case(family)))
            .map(|f| f.id)
            .collect();
        if !exact.is_empty() {
            let want = style.trim().to_ascii_lowercase();
            for id in &exact {
                if let Some(face) = self.open(*id)
                    && face.style.to_ascii_lowercase() == want
                {
                    return Some(face);
                }
            }
        }
        let q = Query {
            families: &[Family::Name(family)],
            weight: Weight(wish.weight),
            stretch: stretch_of(wish.stretch),
            style: if wish.italic { Style::Italic } else { Style::Normal },
        };
        let id = self.db.read().ok()?.query(&q);
        let face = id.and_then(|i| self.open(i))?;
        if !face.family.eq_ignore_ascii_case(family) && !exact.is_empty() {
            return exact.first().and_then(|i| self.open(*i));
        }
        Some(face)
    }

    pub fn any_face(&self) -> Option<Arc<Face>> {
        for f in &self.fallbacks {
            if let Some(face) = self.resolve(f, "Regular") {
                return Some(face);
            }
        }
        let first = self.db.read().ok()?.faces().next().map(|f| f.id)?;
        self.open(first)
    }

    pub fn fallback(&self, base: &Face, c: char) -> Option<Arc<Face>> {
        let key = (base.id, c);
        if let Some(hit) = self.fallback_for.read().ok()?.get(&key).copied() {
            return hit.and_then(|id| self.face(id));
        }
        let style = default_style_name(base.weight, base.italic);
        let mut found = None;
        for fam in &self.fallbacks {
            if let Some(f) = self.resolve(fam, &style)
                && f.covers(c)
            {
                found = Some(f);
                break;
            }
        }
        if found.is_none() {
            let ids: Vec<fontdb::ID> = self.db.read().ok()?.faces().map(|f| f.id).collect();
            for id in ids.into_iter().take(4000) {
                if let Some(f) = self.open(id)
                    && f.covers(c)
                {
                    found = Some(f);
                    break;
                }
            }
        }
        if let Ok(mut m) = self.fallback_for.write() {
            m.insert(key, found.as_ref().map(|f| f.id));
        }
        found
    }
}

fn default_style_name(weight: u16, italic: bool) -> String {
    let w = match weight {
        0..=150 => "Thin",
        151..=250 => "ExtraLight",
        251..=350 => "Light",
        351..=450 => "Regular",
        451..=550 => "Medium",
        551..=650 => "SemiBold",
        651..=750 => "Bold",
        751..=850 => "ExtraBold",
        _ => "Black",
    };
    match (w, italic) {
        ("Regular", true) => "Italic".into(),
        (w, true) => format!("{w} Italic"),
        (w, false) => w.into(),
    }
}

fn style_name(font: &FontRef) -> Option<String> {
    use skrifa::string::StringId;
    for id in [StringId::TYPOGRAPHIC_SUBFAMILY_NAME, StringId::SUBFAMILY_NAME] {
        if let Some(s) = font.localized_strings(id).english_or_first() {
            let v: String = s.chars().collect();
            if !v.trim().is_empty() {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}
