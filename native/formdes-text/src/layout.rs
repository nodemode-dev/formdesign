use std::collections::HashMap;

use formdes_model::doc::Doc;
use formdes_model::geom::{Affine, BezPath, KShape, Point, Rect};
use formdes_model::ids::{ItemId, StoryId};
use formdes_model::item::{AutoSize, ItemKind, Shape, WrapMode};
use formdes_model::page::format_number;
use formdes_model::story::Marker;

use crate::compose::ParaInput;
use crate::flow::{Exclusion, Flow, FrameSpec, Flower};
use crate::fonts::FontBook;
use crate::onpath::{self, OnPath};
use crate::para::{self, Context};

pub struct StoryLayout {
    pub story: StoryId,
    pub frames: Vec<ItemId>,
    pub specs: Vec<FrameSpec>,
    pub flow: Flow,
    pub on_path: Option<OnPath>,
}

#[derive(Default)]
pub struct DocLayout {
    pub stories: HashMap<StoryId, StoryLayout>,
    pub frame_of: HashMap<ItemId, (StoryId, usize)>,
    pub on_page: HashMap<(StoryId, usize), StoryLayout>,
}

impl DocLayout {
    pub fn frame(&self, item: ItemId) -> Option<(&StoryLayout, usize)> {
        let (s, i) = self.frame_of.get(&item)?;
        Some((self.stories.get(s)?, *i))
    }

    pub fn frame_on(&self, item: ItemId, page: Option<usize>) -> Option<(&StoryLayout, usize)> {
        if let Some(p) = page
            && let Some((s, i)) = self.frame_of.get(&item)
            && let Some(sl) = self.on_page.get(&(*s, p))
        {
            return Some((sl, *i));
        }
        self.frame(item)
    }

    pub fn overset(&self, story: StoryId) -> bool {
        self.stories.get(&story).map(|s| s.flow.overset || s.on_path.as_ref().map(|p| p.overset).unwrap_or(false)).unwrap_or(false)
    }
}

struct DocCtx<'a> {
    doc: &'a Doc,
    pages: Vec<Option<usize>>,
    frame_items: Vec<ItemId>,
    exc: crate::compose::Exceptions,
}

impl Context for DocCtx<'_> {
    fn inline_size(&self, item: ItemId) -> Option<(f64, f64)> {
        let it = self.doc.item(item)?;
        let b = it.xf.transform_rect_bbox(it.shape.bbox());
        Some((b.width(), b.height()))
    }

    fn variable(&self, name: &str, frame: usize) -> String {
        self.doc.variable_text_in(name, self.pages.get(frame).copied().flatten(), self.frame_items.get(frame).copied())
    }

    fn marker(&self, marker: Marker, frame: usize) -> String {
        let page = self.pages.get(frame).copied().flatten();
        match (marker, page) {
            (Marker::PageNumber, Some(p)) => self.doc.page_name(p),
            (Marker::NextPageNumber, Some(p)) => {
                let next = self.pages.iter().skip(frame + 1).flatten().find(|q| **q != p).copied().unwrap_or(p + 1);
                self.doc.page_name(next.min(self.doc.page_count().saturating_sub(1)))
            }
            (Marker::PrevPageNumber, Some(p)) => {
                let prev = self.pages.iter().take(frame).flatten().rev().find(|q| **q != p).copied().unwrap_or(p.saturating_sub(1));
                self.doc.page_name(prev)
            }
            (Marker::SectionName, Some(p)) => self.doc.section_for(p).map(|s| s.marker.clone()).unwrap_or_default(),
            (Marker::PageNumber, None) => "#".into(),
            _ => String::new(),
        }
    }

    fn footnote_number(&self, ordinal: usize) -> String {
        format_number(ordinal as u32, "arabic")
    }

    fn exceptions(&self) -> crate::compose::Exceptions {
        self.exc.clone()
    }

    fn hidden(&self, over: &formdes_model::attrs::Attrs) -> bool {
        if over.get("change").and_then(|v| v.as_str()) == Some("deleted") {
            return true;
        }
        let Some(conds) = over.get("conditions").and_then(|v| v.as_array()).filter(|a| !a.is_empty()) else { return false };
        conds.iter().filter_map(|c| c.as_str()).all(|c| self.doc.conditions.iter().any(|d| d.name == c && !d.visible))
    }
}

fn frame_inner(shape: &Shape, insets: &formdes_model::geom::Edges) -> Rect {
    let b = shape.bbox();
    Rect::new(b.x0 + insets.inside, b.y0 + insets.top, (b.x1 - insets.outside).max(b.x0 + insets.inside + 1.0), (b.y1 - insets.bottom).max(b.y0 + insets.top + 1.0))
}

fn exclusions(doc: &Doc, frame: ItemId, inv: Affine) -> Vec<Exclusion> {
    let Some(si) = doc.spread_of(frame) else { return Vec::new() };
    let spread = &doc.spreads[si];
    let mut out = Vec::new();
    let own_story = match doc.item(frame).map(|i| &i.kind) {
        Some(ItemKind::Text(t)) => Some(t.story),
        _ => None,
    };
    for id in doc.walk(&spread.items) {
        if id == frame {
            continue;
        }
        let Some(it) = doc.item(id) else { continue };
        let Some(w) = &it.wrap else { continue };
        if w.mode == WrapMode::None || it.hidden || (own_story.is_some() && it.anchored_in == own_story) {
            continue;
        }
        if doc.layer(it.layer).map(|l| !l.visible).unwrap_or(false) {
            continue;
        }
        let xf = inv * doc.world_xf(id);
        let local = if w.mode == WrapMode::Contour { it.contour_path().unwrap_or_else(|| it.shape.to_path(&it.corner)) } else { it.shape.to_path(&it.corner) };
        let path = xf * local;
        let b = path.bounding_box();
        let o = &w.offsets;
        let rect = Rect::new(b.x0 - o.inside, b.y0 - o.top, b.x1 + o.outside, b.y1 + o.bottom);
        out.push(Exclusion {
            rect,
            mode: w.mode,
            side: w.side.clone(),
            path: (w.mode == WrapMode::Contour).then_some(path),
            pad: o.max(),
            invert: w.invert,
        });
    }
    out
}

pub fn frame_spec(doc: &Doc, id: ItemId) -> Option<FrameSpec> {
    let it = doc.item(id)?;
    let ItemKind::Text(t) = &it.kind else { return None };
    let world = doc.world_xf(id);
    let inv = world.inverse();
    let inner = frame_inner(&it.shape, &t.insets);
    let shape: Option<BezPath> = match it.shape {
        Shape::Rect { .. } if it.corner.radius <= 0.0 => None,
        _ => Some(it.shape.to_path(&it.corner)),
    };
    let page = doc.page_of(id);
    let grid = if doc.settings.baseline.increment > 0.0 {
        let origin = world * Point::new(0.0, 0.0);
        let top = if doc.settings.baseline.from_margin { doc.settings.margins.top } else { 0.0 };
        Some((doc.settings.baseline.start + top - origin.y, doc.settings.baseline.increment))
    } else {
        None
    };
    Some(FrameSpec {
        item: id,
        inner,
        columns: t.column_plan(inner.width()).0,
        gutter: t.gutter,
        fixed_column: t.column_plan(inner.width()).1,
        vjust: t.vjust,
        first_baseline: t.first_baseline,
        first_min: t.first_baseline_min,
        balance: t.balance,
        exclusions: if t.ignore_wrap { Vec::new() } else { exclusions(doc, id, inv) },
        right_page: page.map(|p| doc.is_right_page(p)).unwrap_or(true),
        grow: matches!(t.autosize, AutoSize::Height | AutoSize::Both),
        grid,
        page: page.unwrap_or(0),
        shape,
    })
}

pub fn layout_story(doc: &Doc, fonts: &FontBook, sid: StoryId) -> Option<StoryLayout> {
    layout_story_at(doc, fonts, sid, None)
}

pub fn layout_story_at(doc: &Doc, fonts: &FontBook, sid: StoryId, page: Option<usize>) -> Option<StoryLayout> {
    let story = doc.story(sid)?;
    let frames: Vec<ItemId> = story.frames.iter().copied().filter(|f| doc.item(*f).is_some()).collect();
    let pages: Vec<Option<usize>> = match page {
        Some(p) => vec![Some(p); frames.len()],
        None => frames.iter().map(|f| doc.page_of(*f)).collect(),
    };
    let words: Vec<String> = doc.prefs.get("hyphenationExceptions").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|w| w.as_str().map(str::to_string)).collect()).unwrap_or_default();
    let ctx = DocCtx { doc, pages, frame_items: frames.clone(), exc: crate::compose::parse_exceptions(&words) };
    let path_frame = frames.first().and_then(|f| doc.item(*f)).and_then(|it| match &it.kind {
        ItemKind::Text(t) => t.on_path.clone().map(|p| (it.shape.to_path(&it.corner), p)),
        _ => None,
    });
    if let Some((path, pt)) = path_frame {
        let inputs: Vec<ParaInput> = story.paras.iter().map(|p| para::build(&doc.styles, p, None, 0, &ctx, 0).input).collect();
        let lay = onpath::lay(fonts, &inputs, &path, pt.start, pt.flip, &pt.align);
        return Some(StoryLayout { story: sid, frames, specs: Vec::new(), flow: Flow::default(), on_path: Some(lay) });
    }
    let specs: Vec<FrameSpec> = frames.iter().filter_map(|f| frame_spec(doc, *f)).collect();
    let flower = Flower::new(fonts, &doc.styles, &specs, &ctx);
    let flow = flower.flow(&story.paras);
    Some(StoryLayout { story: sid, frames, specs, flow, on_path: None })
}

pub fn layout_doc(doc: &Doc, fonts: &FontBook) -> DocLayout {
    let mut out = DocLayout::default();
    for sid in doc.stories.keys() {
        if let Some(l) = layout_story(doc, fonts, *sid) {
            for (i, f) in l.frames.iter().enumerate() {
                out.frame_of.insert(*f, (*sid, i));
            }
            out.stories.insert(*sid, l);
        }
    }
    for (sid, st) in &doc.stories {
        let Some(first) = st.frames.first() else { continue };
        let Some(pi) = doc.parent_of(*first) else { continue };
        let paged = st.paras.iter().any(|p| p.segs.iter().any(|s| matches!(s, formdes_model::story::Seg::Inline { inline: formdes_model::story::Inline::Marker { .. } | formdes_model::story::Inline::Variable { .. }, .. })));
        if !paged {
            continue;
        }
        let pid = doc.parents[pi].id;
        for abs in 0..doc.page_count() {
            let mut cur = doc.page(abs).and_then(|p| p.parent);
            let mut hit = false;
            let mut guard = 0;
            while let Some(c) = cur {
                if c == pid {
                    hit = true;
                    break;
                }
                guard += 1;
                cur = if guard < 8 { doc.parent(c).and_then(|p| p.based_on) } else { None };
            }
            if hit && let Some(l) = layout_story_at(doc, fonts, *sid, Some(abs)) {
                out.on_page.insert((*sid, abs), l);
            }
        }
    }
    out
}
