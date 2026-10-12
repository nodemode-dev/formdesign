//! Separations: turning a scene's colours into the density of one plate, so a plate can be previewed, rasterised or
//! written as a vector PDF with the same logic. Plate numbers index `Doc::plate_names`: 0 to 3 are Cyan, Magenta,
//! Yellow and Black, the rest spot inks.

use formdes_model::color::Color;
use formdes_model::doc::Doc;

use crate::scene::{Brush, Op};

/// A colour as the density of one plate: a grey where white is no ink and black is full ink.
pub fn plate_color(doc: &Doc, c: Color, plate: u8) -> Color {
    Color::Gray(doc.plate_density(c, plate as usize))
}

pub fn plate_brush(doc: &Doc, b: &Brush, plate: u8) -> Brush {
    let grey = |c: &Color| plate_color(doc, *c, plate);
    match b {
        Brush::Solid(c, a) => Brush::Solid(grey(c), *a),
        Brush::Linear { start, end, stops, alpha } => Brush::Linear { start: *start, end: *end, stops: stops.iter().map(|(o, c, a)| (*o, grey(c), *a)).collect(), alpha: *alpha },
        Brush::Radial { center, radius, stops, alpha } => Brush::Radial { center: *center, radius: *radius, stops: stops.iter().map(|(o, c, a)| (*o, grey(c), *a)).collect(), alpha: *alpha },
    }
}

/// The scene's ops with every colour replaced by its density on `plate` (images are separated when drawn, see
/// `plate_rgba`). Soft masks are left alone: they hold greys that must stay as they are.
pub fn plate_ops(doc: &Doc, ops: &[Op], plate: u8) -> Vec<Op> {
    ops.iter()
        .map(|op| match op {
            Op::Fill { path, brush, even_odd } => Op::Fill { path: path.clone(), brush: plate_brush(doc, brush, plate), even_odd: *even_odd },
            Op::Stroke { path, brush, style } => Op::Stroke { path: path.clone(), brush: plate_brush(doc, brush, plate), style: style.clone() },
            Op::Glyphs(g) => {
                let mut g = g.clone();
                g.fill = g.fill.as_ref().map(|b| plate_brush(doc, b, plate));
                g.stroke = g.stroke.as_ref().map(|(b, w)| (plate_brush(doc, b, plate), *w));
                Op::Glyphs(g)
            }
            other => other.clone(),
        })
        .collect()
}

/// Whether any fill, stroke or glyph of the scene puts ink on `plate` (images count as ink on the process plates).
pub fn plate_used(doc: &Doc, ops: &[Op], plate: u8) -> bool {
    let hit = |b: &Brush| -> bool {
        let any = |c: &Color| doc.plate_density(*c, plate as usize) > 0.0005;
        match b {
            Brush::Solid(c, _) => any(c),
            Brush::Linear { stops, .. } | Brush::Radial { stops, .. } => stops.iter().any(|(_, c, _)| any(c)),
        }
    };
    ops.iter().any(|op| match op {
        Op::Fill { brush, .. } | Op::Stroke { brush, .. } => hit(brush),
        Op::Glyphs(g) => g.fill.as_ref().is_some_and(&hit) || g.stroke.as_ref().is_some_and(|(b, _)| hit(b)),
        Op::Image { .. } => plate < 4,
        _ => false,
    })
}

/// An image's pixels as the plate's ink density (RGB to process by the same conversion colours use; spot plates
/// get nothing from a picture), as grey RGBA with the picture's own alpha.
pub fn plate_rgba(rgba: &[u8], plate: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
        let (r, g, b) = (px[0] as f64 / 255.0, px[1] as f64 / 255.0, px[2] as f64 / 255.0);
        let k = 1.0 - r.max(g).max(b);
        let d = (1.0 - k).max(1e-6);
        let v = match plate {
            0 => (1.0 - r - k) / d,
            1 => (1.0 - g - k) / d,
            2 => (1.0 - b - k) / d,
            3 => k,
            _ => 0.0,
        };
        let grey = ((1.0 - v.clamp(0.0, 1.0)) * 255.0).round() as u8;
        out.extend_from_slice(&[grey, grey, grey, px[3]]);
    }
    out
}
