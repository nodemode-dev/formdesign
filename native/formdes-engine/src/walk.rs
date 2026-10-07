use std::sync::Arc;

use formdes_model::doc::Doc;
use formdes_model::ids::StoryId;
use formdes_model::story::{Inline, Para, Seg};

fn visit(p: &Para, f: &mut dyn FnMut(&Para)) {
    f(p);
    for seg in &p.segs {
        if let Seg::Inline { inline, .. } = seg {
            match inline {
                Inline::Footnote { paras } => paras.iter().for_each(|q| visit(q, f)),
                Inline::Table { table } => table.cells.iter().flat_map(|c| c.paras.iter()).for_each(|q| visit(q, f)),
                _ => {}
            }
        }
    }
}

fn visit_mut(p: &mut Para, f: &mut dyn FnMut(&mut Para)) {
    f(p);
    for seg in p.segs.iter_mut() {
        if let Seg::Inline { inline, .. } = seg {
            match inline {
                Inline::Footnote { paras } => paras.iter_mut().for_each(|q| visit_mut(q, f)),
                Inline::Table { table } => table.cells.iter_mut().flat_map(|c| c.paras.iter_mut()).for_each(|q| visit_mut(q, f)),
                _ => {}
            }
        }
    }
}

pub fn any_para(doc: &Doc, sid: StoryId, pred: &dyn Fn(&Para) -> bool) -> bool {
    let Some(st) = doc.story(sid) else { return false };
    let mut hit = false;
    for p in &st.paras {
        visit(p, &mut |q| hit |= pred(q));
    }
    hit
}

pub fn paras_where(doc: &mut Doc, pred: &dyn Fn(&Para) -> bool, f: &mut dyn FnMut(&mut Para)) -> usize {
    let ids: Vec<StoryId> = doc.stories.keys().copied().filter(|s| any_para(doc, *s, pred)).collect();
    for sid in &ids {
        if let Some(st) = doc.stories.get_mut(sid).map(Arc::make_mut) {
            for p in st.paras.iter_mut() {
                visit_mut(p, &mut |q| {
                    if pred(q) {
                        f(q)
                    }
                });
            }
        }
    }
    ids.len()
}

pub fn all_paras(doc: &Doc, f: &mut dyn FnMut(StoryId, &Para)) {
    for (sid, st) in &doc.stories {
        for p in &st.paras {
            visit(p, &mut |q| f(*sid, q));
        }
    }
}
