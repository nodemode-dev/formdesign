use formdesign_model::color::Color;
use formdesign_model::doc::Doc;
use formdesign_model::geom::{Point, Rect};
use formdesign_model::story::{Inline, Seg};
use formdesign_render::scene::{Brush, Op};
use formdesign_text::layout::{DocLayout, StoryLayout};
use kurbo::{BezPath, Shape as _};

const NOTE: [f64; 3] = [0.96, 0.77, 0.09];
const INSERTED: [f64; 3] = [0.24, 0.56, 1.0];
const DELETED: [f64; 3] = [0.9, 0.2, 0.2];

struct Span {
    frame: usize,
    x0: f64,
    x1: f64,
    top: f64,
    bottom: f64,
    baseline: f64,
}

fn x_at(carets: &[(usize, f64)], byte: usize, fallback: f64) -> f64 {
    carets.iter().min_by_key(|c| (c.0 as i64 - byte as i64).unsigned_abs()).map(|c| c.1).unwrap_or(fallback)
}

fn spans(sl: &StoryLayout, a: usize, b: usize) -> Vec<Span> {
    let (Some((pa, ba)), Some((pb, bb))) = (sl.flow.para_byte(a), sl.flow.para_byte(b)) else { return Vec::new() };
    let mut out = Vec::new();
    for l in &sl.flow.lines {
        if l.para < pa || l.para > pb {
            continue;
        }
        let lo = if l.para == pa { ba.max(l.line.start) } else { l.line.start };
        let hi = if l.para == pb { bb.min(l.line.end) } else { l.line.end };
        if hi < lo || (hi == lo && a != b) {
            continue;
        }
        if a == b && !(l.line.start..=l.line.end).contains(&lo) {
            continue;
        }
        let x0 = x_at(&l.line.carets, lo, l.line.left);
        let x1 = x_at(&l.line.carets, hi, l.line.left + l.line.width);
        out.push(Span { frame: l.frame, x0, x1, top: l.baseline - l.line.ascent, bottom: l.baseline + l.line.descent, baseline: l.baseline });
        if a == b {
            break;
        }
    }
    out
}

fn solid(c: [f64; 3], a: f64) -> Brush {
    Brush::Solid(Color::Rgb(c), a)
}

pub fn spread_marks(doc: &Doc, lay: &DocLayout, si: usize, notes: bool, changes: bool) -> Vec<Op> {
    if si >= doc.spreads.len() {
        return Vec::new();
    }
    let on_spread = |id| doc.spread_of(id) == Some(si) && doc.parent_of(id).is_none();
    let mut ops = Vec::new();
    for (sid, st) in &doc.stories {
        let Some(sl) = lay.stories.get(sid) else { continue };
        if !sl.frames.iter().any(|f| on_spread(*f)) {
            continue;
        }
        let mut emit = |sp: Span, kind: u8| {
            let Some(frame) = sl.frames.get(sp.frame).copied() else { return };
            if !on_spread(frame) {
                return;
            }
            let xf = doc.world_xf(frame);
            match kind {
                0 => {
                    let s = (sp.bottom - sp.top).clamp(4.0, 12.0) * 0.45;
                    let mut p = BezPath::new();
                    p.move_to(Point::new(sp.x0, sp.top));
                    p.line_to(Point::new(sp.x0 + s, sp.top));
                    p.line_to(Point::new(sp.x0, sp.top + s));
                    p.close_path();
                    ops.push(Op::Fill { path: xf * p, brush: solid(NOTE, 1.0), even_odd: false });
                    ops.push(Op::Fill { path: xf * Rect::new(sp.x0 - 0.3, sp.top, sp.x0 + 0.3, sp.bottom).to_path(0.1), brush: solid(NOTE, 1.0), even_odd: false });
                }
                1 => {
                    let r = Rect::new(sp.x0, sp.top, sp.x1.max(sp.x0 + 0.5), sp.bottom);
                    ops.push(Op::Fill { path: xf * r.to_path(0.1), brush: solid(INSERTED, 0.28), even_odd: false });
                    ops.push(Op::Fill { path: xf * Rect::new(sp.x0, sp.baseline + 1.0, sp.x1.max(sp.x0 + 0.5), sp.baseline + 1.8).to_path(0.1), brush: solid(INSERTED, 1.0), even_odd: false });
                }
                _ => {
                    let w = 0.8;
                    ops.push(Op::Fill { path: xf * Rect::new(sp.x0 - w / 2.0, sp.top, sp.x0 + w / 2.0, sp.bottom).to_path(0.1), brush: solid(DELETED, 1.0), even_odd: false });
                    let mut p = BezPath::new();
                    p.move_to(Point::new(sp.x0 - 2.0, sp.bottom));
                    p.line_to(Point::new(sp.x0 + 2.0, sp.bottom));
                    p.line_to(Point::new(sp.x0, sp.bottom - 2.5));
                    p.close_path();
                    ops.push(Op::Fill { path: xf * p, brush: solid(DELETED, 1.0), even_odd: false });
                }
            }
        };
        let mut pos = 0usize;
        let mut run: Option<(usize, usize, u8)> = None;
        let mut runs: Vec<(usize, usize, u8)> = Vec::new();
        for para in &st.paras {
            for seg in &para.segs {
                let len = seg.len();
                if notes && let Seg::Inline { inline: Inline::Note { .. }, .. } = seg {
                    runs.push((pos, pos, 0));
                }
                let ch = if changes {
                    match seg.over().get("change").and_then(|v| v.as_str()) {
                        Some("inserted") => Some(1u8),
                        Some("deleted") => Some(2u8),
                        _ => None,
                    }
                } else {
                    None
                };
                match (ch, run) {
                    (Some(k), Some((a, _, rk))) if rk == k => run = Some((a, pos + len, k)),
                    (Some(k), r) => {
                        if let Some(r) = r {
                            runs.push(r);
                        }
                        run = Some((pos, pos + len, k));
                    }
                    (None, Some(r)) => {
                        runs.push(r);
                        run = None;
                    }
                    (None, None) => {}
                }
                pos += len;
            }
            pos += 1;
        }
        if let Some(r) = run {
            runs.push(r);
        }
        for (a, b, k) in runs {
            let (a, b) = if k == 2 { (a, a) } else { (a, b) };
            for sp in spans(sl, a, b) {
                emit(sp, k);
            }
        }
    }
    ops
}
