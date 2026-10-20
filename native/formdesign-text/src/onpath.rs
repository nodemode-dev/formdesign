use kurbo::{Affine, BezPath};

use crate::compose::{Composer, GlyphRun, ParaInput, Slot};
use crate::fonts::FontBook;
use crate::region::ArcPath;

pub struct OnPath {
    pub runs: Vec<GlyphRun>,
    pub xforms: Vec<Vec<Affine>>,
    pub width: f64,
    pub length: f64,
    pub overset: bool,
}

pub fn lay(fonts: &FontBook, inputs: &[ParaInput], path: &BezPath, start: f64, flip: bool, align: &str) -> OnPath {
    let composer = Composer { fonts };
    let mut runs: Vec<GlyphRun> = Vec::new();
    let mut x = 0.0;
    for (k, inp) in inputs.iter().enumerate() {
        let mut inp = inp.clone();
        inp.props.single_line = true;
        inp.props.drop_lines = 0;
        inp.props.left_indent = 0.0;
        inp.props.first_indent = 0.0;
        inp.props.right_indent = 0.0;
        inp.props.align = if inp.props.rtl { formdesign_model::attrs::Align::Right } else { formdesign_model::attrs::Align::Left };
        let c = composer.compose(&inp, &|_| Slot { left: 0.0, width: 1.0e9 }, 1, true);
        for line in c.lines {
            for mut r in line.runs {
                for g in &mut r.glyphs {
                    g.x += x;
                }
                r.x0 += x;
                r.x1 += x;
                runs.push(r);
            }
            x += line.width;
        }
        if k + 1 < inputs.len() {
            x += runs.last().map(|r| r.size * 0.25).unwrap_or(3.0);
        }
    }
    let arc = ArcPath::new(path);
    let arc = if flip { arc.reversed() } else { arc };
    let length = arc.length();
    let width = x;
    let offset = match align {
        "center" => (length - width) / 2.0 + start,
        "right" => length - width - start,
        _ => start,
    };
    let mut overset = false;
    let mut xforms = Vec::new();
    for r in &runs {
        let mut v = Vec::new();
        for g in &r.glyphs {
            let mid = offset + g.x + g.adv / 2.0;
            if mid < 0.0 || mid > length {
                overset = true;
            }
            let (p, ang) = arc.at(mid).unwrap_or_default();
            v.push(Affine::translate(p.to_vec2()) * Affine::rotate(ang) * Affine::translate((-g.adv / 2.0, g.y)));
        }
        xforms.push(v);
    }
    OnPath { runs, xforms, width, length, overset }
}
