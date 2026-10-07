use std::collections::BTreeMap;
use std::sync::Arc;

use kurbo::{Affine, Rect};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::attrs::Attrs;
use crate::color::{Color, Paint, Swatch, SwatchValue, swatch_key};
use crate::geom::Edges;
use crate::ids::{AssetId, ItemId, LayerId, PageId, ParentId, SpreadId, StoryId};
use crate::item::{Asset, Item, ItemKind};
use crate::page::{LAYER_COLORS, Layer, Page, Parent, Section, Spread, format_number};
use crate::story::Story;
use crate::style::Styles;
use crate::{Result, err};

pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Intent {
    Print,
    Web,
    Mobile,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Grid {
    pub start: f64,
    pub increment: f64,
    pub color: String,
    #[serde(default)]
    pub from_margin: bool,
    #[serde(default)]
    pub visible: bool,
}

impl Default for Grid {
    fn default() -> Self {
        Grid { start: 36.0, increment: 12.0, color: "#9fc2ff".into(), from_margin: false, visible: false }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocSettings {
    pub page_width: f64,
    pub page_height: f64,
    pub facing: bool,
    pub intent: Intent,
    pub bleed: Edges,
    pub slug: Edges,
    pub margins: Edges,
    pub columns: u32,
    pub gutter: f64,
    #[serde(default)]
    pub baseline: Grid,
    #[serde(default = "pt")]
    pub units: String,
    #[serde(default)]
    pub start_left: bool,
    #[serde(default)]
    pub primary_text_frame: bool,
    #[serde(default = "yes")]
    pub smart_reflow: bool,
}

fn pt() -> String {
    "pt".into()
}

fn yes() -> bool {
    true
}

impl Default for DocSettings {
    fn default() -> Self {
        DocSettings {
            page_width: 612.0,
            page_height: 792.0,
            facing: true,
            intent: Intent::Print,
            bleed: Edges::default(),
            slug: Edges::default(),
            margins: Edges::all(36.0),
            columns: 1,
            gutter: 12.0,
            baseline: Grid::default(),
            units: pt(),
            start_left: false,
            primary_text_frame: false,
            smart_reflow: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LinkSource {
    Text { story: StoryId, start: usize, end: usize },
    Item { item: ItemId },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum LinkDest {
    Url(String),
    Email(String),
    Page(usize),
    Anchor(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hyperlink {
    pub id: u64,
    pub name: String,
    pub source: LinkSource,
    pub dest: LinkDest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Article {
    pub name: String,
    pub items: Vec<ItemId>,
    #[serde(default = "yes")]
    pub export: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Condition {
    pub name: String,
    pub visible: bool,
    pub color: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Variable {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub value: Attrs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Home {
    Spread(usize),
    Parent(usize),
    Group(ItemId),
    Anchored(StoryId),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Doc {
    pub format: u32,
    pub title: String,
    pub settings: DocSettings,
    pub spreads: Vec<Spread>,
    pub parents: Vec<Parent>,
    pub layers: Vec<Layer>,
    pub items: BTreeMap<ItemId, Arc<Item>>,
    pub stories: BTreeMap<StoryId, Arc<Story>>,
    #[serde(default)]
    pub assets: BTreeMap<AssetId, Asset>,
    pub styles: Styles,
    pub swatches: Vec<Swatch>,
    #[serde(default)]
    pub sections: Vec<Section>,
    #[serde(default)]
    pub hyperlinks: Vec<Hyperlink>,
    #[serde(default)]
    pub articles: Vec<Article>,
    #[serde(default)]
    pub conditions: Vec<Condition>,
    #[serde(default)]
    pub variables: Vec<Variable>,
    #[serde(default)]
    pub prefs: Attrs,
    #[serde(default)]
    pub xml: Value,
    next_id: u64,
}

impl Doc {
    pub fn new(title: &str, settings: DocSettings, pages: usize) -> Doc {
        let mut d = Doc {
            format: FORMAT_VERSION,
            title: title.into(),
            settings,
            spreads: Vec::new(),
            parents: Vec::new(),
            layers: Vec::new(),
            items: BTreeMap::new(),
            stories: BTreeMap::new(),
            assets: BTreeMap::new(),
            styles: Styles::default(),
            swatches: Swatch::defaults(),
            sections: vec![Section { start: 0, start_at: Some(1), style: "arabic".into(), prefix: String::new(), marker: String::new(), include_prefix: false }],
            hyperlinks: Vec::new(),
            articles: Vec::new(),
            conditions: Vec::new(),
            variables: default_variables(),
            prefs: Attrs::new(),
            xml: Value::Null,
            next_id: 1,
        };
        let layer = LayerId(d.alloc());
        d.layers.push(Layer::new(layer, "Layer 1", LAYER_COLORS[0]));
        let parent_pages = if d.settings.facing { 2 } else { 1 };
        let pid = ParentId(d.alloc());
        let ppages = (0..parent_pages).map(|_| d.blank_page()).collect();
        d.parents.push(Parent { id: pid, prefix: "A".into(), name: "Parent".into(), based_on: None, pages: ppages, items: Vec::new(), guides: Vec::new() });
        let mut flat = Vec::new();
        for _ in 0..pages.max(1) {
            let mut p = d.blank_page();
            p.parent = Some(pid);
            flat.push(p);
        }
        d.arrange(flat, Vec::new());
        d
    }

    pub fn alloc(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn bump_ids(&mut self) {
        let mut max = 0;
        max = max.max(self.items.keys().map(|k| k.0).max().unwrap_or(0));
        max = max.max(self.stories.keys().map(|k| k.0).max().unwrap_or(0));
        max = max.max(self.assets.keys().map(|k| k.0).max().unwrap_or(0));
        for s in &self.spreads {
            max = max.max(s.id.0);
            for p in &s.pages {
                max = max.max(p.id.0);
            }
        }
        for p in &self.parents {
            max = max.max(p.id.0);
        }
        for l in &self.layers {
            max = max.max(l.id.0);
        }
        self.next_id = self.next_id.max(max + 1);
    }

    pub fn blank_page(&mut self) -> Page {
        let id = PageId(self.alloc());
        let s = &self.settings;
        Page { id, width: s.page_width, height: s.page_height, margins: s.margins, columns: s.columns, gutter: s.gutter, parent: None, detached: Vec::new(), layout_rule: None }
    }

    pub fn arrange(&mut self, pages: Vec<Page>, carried: Vec<(ItemId, usize, Affine)>) {
        let mut spreads: Vec<Vec<Page>> = Vec::new();
        let facing = self.settings.facing;
        let mut cur: Vec<Page> = Vec::new();
        let first_alone = facing && !self.settings.start_left;
        for (i, p) in pages.into_iter().enumerate() {
            cur.push(p);
            let close = !facing || (first_alone && i == 0) || cur.len() == 2;
            if close {
                spreads.push(std::mem::take(&mut cur));
            }
        }
        if !cur.is_empty() {
            spreads.push(cur);
        }
        let mut new_spreads = Vec::new();
        for pages in spreads {
            let id = self.spreads.iter().find(|s| s.pages.first().map(|p| p.id) == pages.first().map(|p| p.id)).map(|s| s.id).unwrap_or_else(|| SpreadId(self.alloc()));
            new_spreads.push(Spread { id, pages, items: Vec::new(), guides: Vec::new(), transition: None });
        }
        let mut abs = 0;
        let mut page_slot: BTreeMap<usize, (usize, usize)> = BTreeMap::new();
        for (si, s) in new_spreads.iter().enumerate() {
            for pi in 0..s.pages.len() {
                page_slot.insert(abs, (si, pi));
                abs += 1;
            }
        }
        for (id, page_abs, local) in carried {
            let Some(&(si, pi)) = page_slot.get(&page_abs).or_else(|| page_slot.values().last()) else { continue };
            let x = new_spreads[si].page_x(pi);
            new_spreads[si].items.push(id);
            if let Some(it) = self.items.get_mut(&id) {
                Arc::make_mut(it).xf = Affine::translate((x, 0.0)) * local;
            }
        }
        self.spreads = new_spreads;
    }

    pub fn repaginate(&mut self, edit: impl FnOnce(&mut Vec<Page>, &mut Vec<(ItemId, usize, Affine)>, &mut Doc)) {
        let mut pages = Vec::new();
        let mut carried = Vec::new();
        let mut abs = 0;
        for s in std::mem::take(&mut self.spreads) {
            let first_abs = abs;
            abs += s.pages.len();
            for id in &s.items {
                let Some(it) = self.items.get(id) else { continue };
                let b = it.own_bounds();
                let pi = s.nearest_page(b.center().x);
                let x = s.page_x(pi);
                carried.push((*id, first_abs + pi, Affine::translate((-x, 0.0)) * it.xf));
            }
            pages.extend(s.pages);
            self.spreads.push(Spread { id: s.id, pages: Vec::new(), items: Vec::new(), guides: s.guides, transition: s.transition });
        }
        edit(&mut pages, &mut carried, self);
        let guides: Vec<_> = self.spreads.iter().map(|s| (s.id, s.guides.clone(), s.transition.clone())).collect();
        self.arrange(pages, carried);
        for s in self.spreads.iter_mut() {
            if let Some((_, g, t)) = guides.iter().find(|(id, _, _)| *id == s.id) {
                s.guides = g.clone();
                s.transition = t.clone();
            }
        }
    }

    pub fn page_count(&self) -> usize {
        self.spreads.iter().map(|s| s.pages.len()).sum()
    }

    pub fn first_page_of_spread(&self, si: usize) -> usize {
        self.spreads.iter().take(si).map(|s| s.pages.len()).sum()
    }

    pub fn page_slot(&self, abs: usize) -> Option<(usize, usize)> {
        let mut n = abs;
        for (si, s) in self.spreads.iter().enumerate() {
            if n < s.pages.len() {
                return Some((si, n));
            }
            n -= s.pages.len();
        }
        None
    }

    pub fn page(&self, abs: usize) -> Option<&Page> {
        let (si, pi) = self.page_slot(abs)?;
        self.spreads.get(si)?.pages.get(pi)
    }

    pub fn page_mut(&mut self, abs: usize) -> Option<&mut Page> {
        let (si, pi) = self.page_slot(abs)?;
        self.spreads.get_mut(si)?.pages.get_mut(pi)
    }

    pub fn page_rect(&self, abs: usize) -> Option<Rect> {
        let (si, pi) = self.page_slot(abs)?;
        let s = &self.spreads[si];
        let x = s.page_x(pi);
        let p = &s.pages[pi];
        Some(Rect::new(x, 0.0, x + p.width, p.height))
    }

    pub fn is_right_page(&self, abs: usize) -> bool {
        if !self.settings.facing {
            return true;
        }
        let Some((si, pi)) = self.page_slot(abs) else { return true };
        let s = &self.spreads[si];
        if s.pages.len() >= 2 {
            return pi == s.pages.len() - 1;
        }
        abs % 2 == usize::from(self.settings.start_left)
    }

    pub fn section_for(&self, abs: usize) -> Option<&Section> {
        self.sections.iter().filter(|s| s.start <= abs).max_by_key(|s| s.start)
    }

    pub fn page_number(&self, abs: usize) -> u32 {
        let Some(sec) = self.section_for(abs) else { return abs as u32 + 1 };
        let base = match sec.start_at {
            Some(n) => n,
            None => {
                let prev = self.sections.iter().filter(|s| s.start < sec.start).max_by_key(|s| s.start);
                match prev {
                    Some(_) => self.page_number(sec.start.saturating_sub(1)) + 1,
                    None => sec.start as u32 + 1,
                }
            }
        };
        base + (abs - sec.start) as u32
    }

    pub fn page_name(&self, abs: usize) -> String {
        let n = self.page_number(abs);
        match self.section_for(abs) {
            Some(sec) => {
                let num = format_number(n, &sec.style);
                if sec.include_prefix { format!("{}{num}", sec.prefix) } else { num }
            }
            None => n.to_string(),
        }
    }

    pub fn find_page_by_name(&self, name: &str) -> Option<usize> {
        (0..self.page_count()).find(|&i| self.page_name(i) == name)
    }

    pub fn item(&self, id: ItemId) -> Option<&Item> {
        self.items.get(&id).map(|a| a.as_ref())
    }

    pub fn item_mut(&mut self, id: ItemId) -> Option<&mut Item> {
        self.items.get_mut(&id).map(Arc::make_mut)
    }

    pub fn story(&self, id: StoryId) -> Option<&Story> {
        self.stories.get(&id).map(|a| a.as_ref())
    }

    pub fn story_mut(&mut self, id: StoryId) -> Option<&mut Story> {
        self.stories.get_mut(&id).map(Arc::make_mut)
    }

    pub fn new_story(&mut self, style: &str) -> StoryId {
        let id = StoryId(self.alloc());
        self.stories.insert(id, Arc::new(Story::new(id, style)));
        id
    }

    pub fn insert_item(&mut self, mut item: Item, home: Home) -> ItemId {
        if item.id.0 == 0 {
            item.id = ItemId(self.alloc());
        }
        let id = item.id;
        if let Home::Group(g) = home {
            item.group = Some(g);
        }
        if let Home::Anchored(s) = home {
            item.anchored_in = Some(s);
        }
        self.items.insert(id, Arc::new(item));
        match home {
            Home::Spread(si) => {
                if let Some(s) = self.spreads.get_mut(si) {
                    s.items.push(id);
                }
            }
            Home::Parent(pi) => {
                if let Some(p) = self.parents.get_mut(pi) {
                    p.items.push(id);
                }
            }
            Home::Group(g) => {
                if let Some(ItemKind::Group { children }) = self.item_mut(g).map(|i| &mut i.kind) {
                    children.push(id);
                }
            }
            Home::Anchored(_) => {}
        }
        id
    }

    pub fn home(&self, id: ItemId) -> Option<Home> {
        let it = self.item(id)?;
        if let Some(g) = it.group {
            return Some(Home::Group(g));
        }
        if let Some(s) = it.anchored_in {
            return Some(Home::Anchored(s));
        }
        if let Some(si) = self.spreads.iter().position(|s| s.items.contains(&id)) {
            return Some(Home::Spread(si));
        }
        self.parents.iter().position(|p| p.items.contains(&id)).map(Home::Parent)
    }

    pub fn top_of(&self, id: ItemId) -> ItemId {
        let mut cur = id;
        let mut guard = 0;
        while let Some(g) = self.item(cur).and_then(|i| i.group) {
            cur = g;
            guard += 1;
            if guard > 64 {
                break;
            }
        }
        cur
    }

    pub fn spread_of(&self, id: ItemId) -> Option<usize> {
        let top = self.top_of(id);
        if let Some(st) = self.item(top).and_then(|i| i.anchored_in) {
            let frame = self.story(st)?.frames.first().copied()?;
            return self.spread_of(frame);
        }
        self.spreads.iter().position(|s| s.items.contains(&top))
    }

    pub fn parent_of(&self, id: ItemId) -> Option<usize> {
        let top = self.top_of(id);
        self.parents.iter().position(|p| p.items.contains(&top))
    }

    pub fn page_of(&self, id: ItemId) -> Option<usize> {
        let si = self.spread_of(id)?;
        let b = self.bounds(id)?;
        let s = &self.spreads[si];
        let pi = s.page_at_x(b.center().x)?;
        Some(self.first_page_of_spread(si) + pi)
    }

    pub fn world_xf(&self, id: ItemId) -> Affine {
        let mut xf = Affine::IDENTITY;
        let mut cur = Some(id);
        let mut guard = 0;
        while let Some(c) = cur {
            let Some(it) = self.item(c) else { break };
            xf = it.xf * xf;
            cur = it.group;
            guard += 1;
            if guard > 64 {
                break;
            }
        }
        xf
    }

    pub fn bounds(&self, id: ItemId) -> Option<Rect> {
        let it = self.item(id)?;
        let parent_xf = match it.group {
            Some(g) => self.world_xf(g),
            None => Affine::IDENTITY,
        };
        match &it.kind {
            ItemKind::Group { .. } | ItemKind::States { .. } => {
                let kids = it.children();
                let mut r: Option<Rect> = None;
                for k in kids {
                    if let Some(b) = self.bounds(k) {
                        r = Some(r.map_or(b, |a| a.union(b)));
                    }
                }
                r.or_else(|| Some((parent_xf * it.xf).transform_rect_bbox(it.shape.bbox())))
            }
            _ => Some((parent_xf * it.xf).transform_rect_bbox(it.shape.bbox())),
        }
    }

    pub fn walk(&self, ids: &[ItemId]) -> Vec<ItemId> {
        let mut out = Vec::new();
        for id in ids {
            out.push(*id);
            if let Some(it) = self.item(*id) {
                out.extend(self.walk(&it.all_children()));
            }
        }
        out
    }

    pub fn all_items(&self) -> Vec<ItemId> {
        self.items.keys().copied().collect()
    }

    pub fn detach_from_home(&mut self, id: ItemId) {
        match self.home(id) {
            Some(Home::Spread(si)) => self.spreads[si].items.retain(|x| *x != id),
            Some(Home::Parent(pi)) => self.parents[pi].items.retain(|x| *x != id),
            Some(Home::Group(g)) => {
                if let Some(ItemKind::Group { children }) = self.item_mut(g).map(|i| &mut i.kind) {
                    children.retain(|x| *x != id);
                }
                if let Some(it) = self.item_mut(id) {
                    it.group = None;
                }
            }
            Some(Home::Anchored(_)) | None => {}
        }
    }

    pub fn remove_item(&mut self, id: ItemId) {
        let Some(it) = self.item(id).cloned() else { return };
        for c in it.all_children() {
            self.remove_item(c);
        }
        self.detach_from_home(id);
        if let Some(sid) = it.story() {
            let mut drop_story = false;
            if let Some(st) = self.story_mut(sid) {
                st.frames.retain(|f| *f != id);
                drop_story = st.frames.is_empty();
            }
            if drop_story {
                if let Some(st) = self.story(sid) {
                    for a in st.anchored_items() {
                        self.remove_item(a);
                    }
                }
                self.stories.remove(&sid);
            }
        }
        self.items.remove(&id);
        for a in self.articles.iter_mut() {
            a.items.retain(|x| *x != id);
        }
        self.hyperlinks.retain(|h| !matches!(h.source, LinkSource::Item { item } if item == id));
    }

    pub fn add_asset(&mut self, mut asset: Asset) -> AssetId {
        if asset.id.0 == 0 {
            asset.id = AssetId(self.alloc());
        }
        let id = asset.id;
        self.assets.insert(id, asset);
        id
    }

    pub fn swatch(&self, name: &str) -> Option<&Swatch> {
        let k = swatch_key(name);
        self.swatches.iter().find(|s| s.name == k || s.name.eq_ignore_ascii_case(&k))
    }

    pub fn swatch_color(&self, name: &str, tint: f64, depth: u32) -> Option<Color> {
        if depth > 16 {
            return None;
        }
        let k = swatch_key(name);
        if k.eq_ignore_ascii_case("paper") {
            if let Some(s) = self.swatch("Paper")
                && let SwatchValue::Color { color } = &s.value
            {
                return Some(*color);
            }
            return Some(Color::white());
        }
        let s = self.swatch(&k)?;
        match &s.value {
            SwatchValue::Color { color } => Some(color.tinted(tint)),
            SwatchValue::Tint { base, tint: t } => self.swatch_color(base, t * tint, depth + 1),
            SwatchValue::Gradient { gradient } => gradient.stops.first().map(|s| s.color.tinted(tint)),
        }
    }

    pub fn paint_color(&self, paint: &Paint) -> Option<Color> {
        match paint {
            Paint::None => None,
            Paint::Swatch { name, tint } => self.swatch_color(name, *tint, 0),
            Paint::Color { color } => Some(*color),
            Paint::Gradient { gradient } => gradient.stops.first().map(|s| s.color),
        }
    }

    pub fn gradient_of(&self, paint: &Paint) -> Option<crate::color::Gradient> {
        match paint {
            Paint::Gradient { gradient } => Some(gradient.clone()),
            Paint::Swatch { name, .. } => match &self.swatch(name)?.value {
                SwatchValue::Gradient { gradient } => Some(gradient.clone()),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn layer(&self, id: LayerId) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id == id)
    }

    pub fn layer_index(&self, id: LayerId) -> usize {
        self.layers.iter().position(|l| l.id == id).unwrap_or(0)
    }

    pub fn parent(&self, id: ParentId) -> Option<&Parent> {
        self.parents.iter().find(|p| p.id == id)
    }

    pub fn parent_by_label(&self, label: &str) -> Option<&Parent> {
        let l = label.trim();
        self.parents.iter().find(|p| p.label() == l || p.prefix == l || p.name == l || format!("{}-{}", p.prefix, p.name).eq_ignore_ascii_case(l))
    }

    pub fn variable_text(&self, name: &str, page: Option<usize>) -> String {
        let Some(v) = self.variables.iter().find(|v| v.name == name) else { return String::new() };
        let text = |k: &str| v.value.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        match v.kind.as_str() {
            "custom" => text("text"),
            "fileName" => self.title.clone(),
            "chapterNumber" => text("number"),
            "lastPageNumber" => self.page_name(self.page_count().saturating_sub(1)),
            "modificationDate" | "creationDate" | "outputDate" => text("date"),
            "runningHeader" => page.map(|p| self.running_header(p, &text("style"))).unwrap_or_default(),
            _ => text("text"),
        }
    }

    pub fn running_header(&self, page: usize, style: &str) -> String {
        let mut found = String::new();
        for st in self.stories.values() {
            let Some(first) = st.frames.first() else { continue };
            let Some(p) = self.page_of(*first) else { continue };
            if p > page {
                continue;
            }
            for para in &st.paras {
                if para.style == style {
                    found = para.plain();
                }
            }
        }
        found
    }

    pub fn check(&self) -> Result<()> {
        if self.spreads.is_empty() {
            return err("a document needs at least one page");
        }
        if self.layers.is_empty() {
            return err("a document needs a layer");
        }
        for (id, st) in &self.stories {
            if st.id != *id {
                return err(format!("story {id} is filed under the wrong id"));
            }
            st.check()?;
        }
        Ok(())
    }
}

fn default_variables() -> Vec<Variable> {
    let mk = |name: &str, kind: &str| Variable { name: name.into(), kind: kind.into(), value: Attrs::new() };
    vec![
        mk("Chapter Number", "chapterNumber"),
        mk("Creation Date", "creationDate"),
        mk("File Name", "fileName"),
        mk("Last Page Number", "lastPageNumber"),
        mk("Modification Date", "modificationDate"),
        mk("Output Date", "outputDate"),
        mk("Running Header", "runningHeader"),
    ]
}
