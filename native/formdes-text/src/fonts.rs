use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

use fontdb::{Database, Family, Query, Stretch, Style, Weight};
use skrifa::instance::{LocationRef, NormalizedCoord, Size};
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
    pub root: FaceId,
    pub variations: Vec<(String, f32)>,
    pub location: Vec<NormalizedCoord>,
    instance: Option<harfrust::ShaperInstance>,
    shaper: harfrust::ShaperData,
    glyph_cache: RwLock<HashMap<char, Option<u32>>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AxisInfo {
    pub tag: String,
    pub name: String,
    pub min: f32,
    pub default: f32,
    pub max: f32,
    pub hidden: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InstanceInfo {
    pub name: String,
    pub coords: Vec<(String, f32)>,
}

fn tag_str(t: skrifa::raw::types::Tag) -> String {
    String::from_utf8_lossy(t.to_be_bytes().as_slice()).into_owned()
}

impl Face {
    pub fn location_ref(&self) -> LocationRef<'_> {
        LocationRef::new(&self.location)
    }

    pub fn instance(&self) -> Option<&harfrust::ShaperInstance> {
        self.instance.as_ref()
    }

    pub fn axes(&self) -> Vec<AxisInfo> {
        let font = self.font();
        font.axes()
            .iter()
            .map(|a| AxisInfo {
                tag: tag_str(a.tag()),
                name: font.localized_strings(a.name_id()).english_or_first().map(|s| s.chars().collect::<String>()).unwrap_or_else(|| tag_str(a.tag())),
                min: a.min_value(),
                default: a.default_value(),
                max: a.max_value(),
                hidden: a.is_hidden(),
            })
            .collect()
    }

    pub fn named_instances(&self) -> Vec<InstanceInfo> {
        let font = self.font();
        let axes = font.axes();
        font.named_instances()
            .iter()
            .map(|inst| InstanceInfo {
                name: font.localized_strings(inst.subfamily_name_id()).english_or_first().map(|s| s.chars().collect::<String>()).unwrap_or_default(),
                coords: axes.iter().zip(inst.user_coords()).map(|(a, v)| (tag_str(a.tag()), v)).collect(),
            })
            .collect()
    }

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
        font.glyph_metrics(Size::unscaled(), self.location_ref())
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

fn read_metrics(font: &FontRef, loc: LocationRef) -> Metrics {
    let m = font.metrics(Size::unscaled(), loc);
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
    varied_cache: RwLock<HashMap<(FaceId, String), FaceId>>,
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
            varied_cache: RwLock::new(HashMap::new()),
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
        let mut out: Vec<(u16, bool, String)> = Vec::new();
        for f in ids.into_iter().filter_map(|i| self.open(i)) {
            out.push((f.weight, f.italic, f.style.clone()));
            for inst in f.named_instances() {
                let w = inst.coords.iter().find(|(t, _)| t == "wght").map(|(_, v)| v.round() as u16).unwrap_or(f.weight);
                out.push((w, f.italic, inst.name));
            }
        }
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
        let metrics = read_metrics(&font, LocationRef::default());
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
            root: id,
            variations: Vec::new(),
            location: Vec::new(),
            instance: None,
            shaper,
            glyph_cache: RwLock::new(HashMap::new()),
        });
        self.faces.write().ok()?.insert(id, face.clone());
        self.by_db.write().ok()?.insert(dbid, id);
        Some(face)
    }

    pub fn varied(&self, base: &Arc<Face>, settings: &[(String, f32)]) -> Arc<Face> {
        if settings.is_empty() || !base.is_variable() {
            return base.clone();
        }
        let font = base.font();
        let axes = font.axes();
        let mut merged: Vec<(String, f32)> = axes
            .iter()
            .map(|a| {
                let tag = tag_str(a.tag());
                let v = base.variations.iter().find(|(t, _)| *t == tag).map(|(_, v)| *v).unwrap_or(a.default_value());
                (tag, v)
            })
            .collect();
        for (t, v) in settings {
            if let Some(slot) = merged.iter_mut().find(|(mt, _)| mt == t)
                && let Some(a) = axes.iter().find(|a| tag_str(a.tag()) == *t)
            {
                slot.1 = v.clamp(a.min_value(), a.max_value());
            }
        }
        if merged.iter().zip(axes.iter()).all(|((_, v), a)| (*v - a.default_value()).abs() < 1e-4) {
            return self.face(base.root).unwrap_or_else(|| base.clone());
        }
        let key = (base.root, merged.iter().map(|(t, v)| format!("{t}={v:.3}")).collect::<Vec<_>>().join(","));
        if let Some(id) = self.varied_cache.read().ok().and_then(|m| m.get(&key).copied())
            && let Some(f) = self.face(id)
        {
            return f;
        }
        let location = axes.location(merged.iter().map(|(t, v)| (t.as_str(), *v)));
        let coords: Vec<NormalizedCoord> = location.coords().to_vec();
        let inst = harfrust::ShaperInstance::from_variations(&font, merged.iter().map(|(t, v)| (t.as_str(), *v)));
        let metrics = read_metrics(&font, LocationRef::new(&coords));
        let get = |t: &str| merged.iter().find(|(mt, _)| mt == t).map(|(_, v)| *v);
        let weight = get("wght").map(|w| w.round().clamp(1.0, 1000.0) as u16).unwrap_or(base.weight);
        let italic = get("ital").map(|v| v > 0.5).unwrap_or(base.italic) || get("slnt").is_some_and(|s| s.abs() > 0.5);
        let named = base.named_instances().into_iter().find(|i| i.coords.iter().all(|(t, v)| get(t).is_none_or(|g| (g - v).abs() < 0.01)));
        let style = named.map(|i| i.name).unwrap_or_else(|| merged.iter().map(|(t, v)| format!("{t} {}", v.round())).collect::<Vec<_>>().join(" "));
        let Ok(mut next) = self.next.write() else { return base.clone() };
        let id = *next;
        *next += 1;
        let face = Arc::new(Face {
            id,
            data: base.data.clone(),
            index: base.index,
            family: base.family.clone(),
            style,
            postscript: base.postscript.clone(),
            weight,
            italic,
            metrics,
            synthetic_bold: false,
            synthetic_italic: false,
            root: base.root,
            variations: merged,
            location: coords,
            instance: Some(inst),
            shaper: harfrust::ShaperData::new(&font),
            glyph_cache: RwLock::new(HashMap::new()),
        });
        if let Ok(mut f) = self.faces.write() {
            f.insert(id, face.clone());
        }
        if let Ok(mut m) = self.varied_cache.write() {
            m.insert(key, id);
        }
        face
    }

    fn instance_face(&self, face: &Arc<Face>, want: &str) -> Option<Arc<Face>> {
        if !face.is_variable() {
            return None;
        }
        let inst = face.named_instances().into_iter().find(|i| i.name.trim().eq_ignore_ascii_case(want))?;
        Some(self.varied(face, &inst.coords))
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
            for id in &exact {
                if let Some(face) = self.open(*id)
                    && let Some(inst) = self.instance_face(&face, &want)
                {
                    return Some(inst);
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
