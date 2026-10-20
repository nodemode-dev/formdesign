use std::collections::BTreeSet;
use std::sync::Arc;

use formdesign_engine::canvas::{Canvas, SpreadRef};
use formdesign_model::doc::Doc;
use formdesign_model::geom::Rect;
use formdesign_model::ids::{ItemId, StoryId};

struct Acc<'a> {
    cv: &'a Canvas,
    out: Vec<[f64; 4]>,
    full: bool,
    stories: BTreeSet<StoryId>,
}

impl Acc<'_> {
    fn item(&mut self, doc: &Doc, id: ItemId) {
        let Some(it) = doc.item(id) else { return };
        if let Some(sid) = it.anchored_in {
            self.stories.insert(sid);
            return;
        }
        let top = doc.top_of(id);
        let Some(b) = doc.bounds(id) else { return };
        let pad = if it.effects.is_empty() { it.stroke.weight.max(0.0) + 2.0 } else { 48.0 };
        let b = b.inflate(pad, pad);
        match doc.spread_of(top) {
            Some(si) => self.rect(SpreadRef::Doc(si), b),
            None if doc.parent_of(top).is_some() => self.full = true,
            None => {}
        }
        if let Some(sid) = it.story() {
            self.stories.insert(sid);
        }
    }

    fn rect(&mut self, r: SpreadRef, b: Rect) {
        match self.cv.slot(r) {
            Some(s) => {
                let c = s.xf.transform_rect_bbox(b);
                self.out.push([c.x0, c.y0, c.x1, c.y1]);
            }
            None => self.full = true,
        }
    }
}

fn assets_same(old: &Doc, new: &Doc) -> bool {
    old.assets.len() == new.assets.len()
        && old.assets.iter().zip(new.assets.iter()).all(|((ka, a), (kb, b))| ka == kb && a.link == b.link && a.modified == b.modified && a.embedded == b.embedded && Arc::ptr_eq(&a.data, &b.data))
}

pub fn damage(old: &Doc, new: &Doc, cv: &Canvas) -> Option<Vec<[f64; 4]>> {
    let structural = old.settings != new.settings
        || old.styles != new.styles
        || old.swatches != new.swatches
        || old.layers != new.layers
        || old.sections != new.sections
        || old.conditions != new.conditions
        || old.variables != new.variables
        || old.parents != new.parents
        || old.prefs != new.prefs
        || old.spreads.len() != new.spreads.len()
        || old.spreads.iter().zip(new.spreads.iter()).any(|(a, b)| a.pages != b.pages || a.guides != b.guides)
        || !assets_same(old, new);
    if structural {
        return None;
    }
    let mut acc = Acc { cv, out: Vec::new(), full: false, stories: BTreeSet::new() };
    let mut wrapped = false;
    let ids: BTreeSet<ItemId> = old.items.keys().chain(new.items.keys()).copied().collect();
    for id in ids {
        let (a, b) = (old.items.get(&id), new.items.get(&id));
        let same = matches!((a, b), (Some(x), Some(y)) if Arc::ptr_eq(x, y));
        if same {
            continue;
        }
        wrapped |= a.is_some_and(|i| i.wrap.is_some()) || b.is_some_and(|i| i.wrap.is_some());
        acc.item(old, id);
        acc.item(new, id);
    }
    for (si, (a, b)) in old.spreads.iter().zip(new.spreads.iter()).enumerate() {
        if a.items != b.items {
            let sa: BTreeSet<&ItemId> = a.items.iter().collect();
            let sb: BTreeSet<&ItemId> = b.items.iter().collect();
            let moved: Vec<ItemId> = if sa == sb { b.items.clone() } else { sa.symmetric_difference(&sb).map(|i| **i).collect() };
            for id in moved {
                if let Some(r) = old.bounds(id).or_else(|| new.bounds(id)) {
                    acc.rect(SpreadRef::Doc(si), r.inflate(4.0, 4.0));
                }
            }
        }
    }
    let sids: BTreeSet<StoryId> = old.stories.keys().chain(new.stories.keys()).copied().collect();
    for sid in sids {
        let same = matches!((old.stories.get(&sid), new.stories.get(&sid)), (Some(x), Some(y)) if Arc::ptr_eq(x, y));
        if !same {
            acc.stories.insert(sid);
        }
    }
    if wrapped {
        acc.stories.extend(new.stories.keys().copied());
    }
    let stories: Vec<StoryId> = acc.stories.iter().copied().collect();
    for sid in stories {
        for doc in [old, new] {
            let Some(st) = doc.story(sid) else { continue };
            for f in st.frames.clone() {
                acc.item(doc, f);
            }
            for a in st.anchored_items() {
                acc.item(doc, a);
            }
        }
    }
    if acc.full { None } else { Some(acc.out) }
}
