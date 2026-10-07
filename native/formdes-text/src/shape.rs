use std::str::FromStr;

use harfrust::{Direction, Feature, Language, ShapeOptions, UnicodeBuffer};

use crate::fonts::Face;

#[derive(Clone, Debug, PartialEq)]
pub struct Glyph {
    pub id: u32,
    pub cluster: usize,
    pub advance: f64,
    pub dx: f64,
    pub dy: f64,
}

pub struct ShapeReq<'a> {
    pub text: &'a str,
    pub face: &'a Face,
    pub size: f64,
    pub kerning: bool,
    pub ligatures: bool,
    pub features: &'a [String],
    pub rtl: bool,
    pub language: &'a str,
}

pub fn feature_list(kerning: bool, ligatures: bool, extra: &[String]) -> Vec<Feature> {
    let mut out = Vec::new();
    if !kerning {
        out.extend(Feature::from_str("kern=0"));
    }
    if !ligatures {
        out.extend(Feature::from_str("liga=0"));
        out.extend(Feature::from_str("clig=0"));
    }
    for f in extra {
        let f = f.trim();
        if f.is_empty() {
            continue;
        }
        if let Ok(feat) = Feature::from_str(f) {
            out.push(feat);
        }
    }
    out
}

pub fn shape(req: &ShapeReq) -> Vec<Glyph> {
    if req.text.is_empty() {
        return Vec::new();
    }
    let font = req.face.font();
    let shaper = req.face.shaper_data().shaper(&font).build();
    let mut buf = UnicodeBuffer::new();
    buf.push_str(req.text);
    buf.set_direction(if req.rtl { Direction::RightToLeft } else { Direction::LeftToRight });
    if let Ok(l) = Language::from_str(req.language) {
        buf.set_language(l);
    }
    buf.guess_segment_properties();
    let feats = feature_list(req.kerning, req.ligatures, req.features);
    let out = shaper.shape(buf, ShapeOptions::new().features(&feats));
    let scale = req.size / req.face.metrics.upem;
    let mut glyphs: Vec<Glyph> = out
        .glyph_infos()
        .iter()
        .zip(out.glyph_positions())
        .map(|(i, p)| Glyph {
            id: i.glyph_id,
            cluster: i.cluster as usize,
            advance: p.x_advance as f64 * scale,
            dx: p.x_offset as f64 * scale,
            dy: p.y_offset as f64 * scale,
        })
        .collect();
    if req.rtl {
        glyphs.reverse();
    }
    glyphs
}
