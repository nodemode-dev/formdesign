use std::sync::Arc;

use formdesign_model::doc::{Doc, Home};
use formdesign_model::geom::Vec2;
use formdesign_model::ids::{ItemId, StoryId};
use formdesign_render::Renderer;
use formdesign_text::layout::layout_doc;

const MAX_PAGES: usize = 40;

fn fills_margins(doc: &Doc, frame: ItemId) -> Option<usize> {
    let page = doc.page_of(frame)?;
    let m = crate::cmd::object::margin_rect(doc, page)?;
    let b = doc.bounds(frame)?;
    let inter = b.intersect(m);
    let area = |r: formdesign_model::geom::Rect| (r.width().max(0.0)) * (r.height().max(0.0));
    if area(m) <= 0.0 || area(inter) / area(m) < 0.6 {
        return None;
    }
    Some(page)
}

pub fn smart_reflow(doc: &mut Doc, renderer: &Renderer) -> usize {
    if !doc.settings.smart_reflow {
        return 0;
    }
    let mut added = 0;
    for _ in 0..MAX_PAGES {
        let lay = layout_doc(doc, &renderer.fonts);
        let target: Option<(StoryId, ItemId, usize)> = doc.stories.iter().find_map(|(sid, st)| {
            if !lay.overset(*sid) {
                return None;
            }
            let last = *st.frames.last()?;
            let it = doc.item(last)?;
            if it.text().is_some_and(|t| t.on_path.is_some()) || it.group.is_some() || it.anchored_in.is_some() {
                return None;
            }
            let page = fills_margins(doc, last)?;
            Some((*sid, last, page))
        });
        let Some((sid, last, page)) = target else { break };
        let parent = doc.page(page).and_then(|p| p.parent);
        let Some(old) = doc.page_rect(page) else { break };
        crate::cmd::layout::insert_pages(doc, page + 1, 1, parent);
        let Some(new) = doc.page_rect(page + 1) else { break };
        let Some((si, _)) = doc.page_slot(page + 1) else { break };
        let Some(src) = doc.item(last).cloned() else { break };
        let mut it = src.clone();
        it.id = ItemId(0);
        let world = doc.world_xf(last);
        let shift = Vec2::new(new.x0 - old.x0, new.y0 - old.y0);
        it.xf = kurbo::Affine::translate(shift) * world;
        let nid = doc.insert_item(it, Home::Spread(si));
        if let Some(st) = doc.stories.get_mut(&sid).map(Arc::make_mut) {
            let pos = st.frames.iter().position(|f| *f == last).map(|i| i + 1).unwrap_or(st.frames.len());
            st.frames.insert(pos, nid);
        }
        added += 1;
    }
    added
}
