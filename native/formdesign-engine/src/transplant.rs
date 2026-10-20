use std::collections::HashMap;
use std::sync::Arc;

use formdesign_model::color::Paint;
use formdesign_model::doc::{Doc, Home};
use formdesign_model::geom::{Affine, Vec2};
use formdesign_model::ids::{AssetId, ItemId, LayerId, StoryId};
use formdesign_model::item::{Item, ItemKind};
use formdesign_model::story::{Inline, Para, Seg};
use formdesign_model::style::NamedStyle;

pub struct Mapper<'a> {
    pub src: &'a Doc,
    assets: HashMap<AssetId, AssetId>,
    pub layer: Option<LayerId>,
    /// Every text frame carried over, as (source frame, new frame). Each new frame gets a story of its own; a caller
    /// that carries several frames of one story re-threads them from this list.
    pub frames: Vec<(ItemId, ItemId)>,
}

#[derive(Clone, Copy)]
enum Family {
    Paragraph,
    Character,
    Object,
    Table,
    Cell,
}

fn family(doc: &mut Doc, f: Family) -> &mut Vec<NamedStyle> {
    match f {
        Family::Paragraph => &mut doc.styles.paragraph,
        Family::Character => &mut doc.styles.character,
        Family::Object => &mut doc.styles.object,
        Family::Table => &mut doc.styles.table,
        Family::Cell => &mut doc.styles.cell,
    }
}

fn family_ref(doc: &Doc, f: Family) -> &Vec<NamedStyle> {
    match f {
        Family::Paragraph => &doc.styles.paragraph,
        Family::Character => &doc.styles.character,
        Family::Object => &doc.styles.object,
        Family::Table => &doc.styles.table,
        Family::Cell => &doc.styles.cell,
    }
}

impl<'a> Mapper<'a> {
    pub fn new(src: &'a Doc) -> Mapper<'a> {
        Mapper { src, assets: HashMap::new(), layer: None, frames: Vec::new() }
    }

    fn style(&self, dst: &mut Doc, f: Family, name: &str) {
        let mut name = Some(name.to_string());
        let mut guard = 0;
        while let Some(n) = name.take() {
            guard += 1;
            if guard > 32 || family_ref(dst, f).iter().any(|s| s.name == n) {
                return;
            }
            let Some(s) = family_ref(self.src, f).iter().find(|s| s.name == n).cloned() else { return };
            name = s.based_on.clone();
            family(dst, f).push(s);
        }
    }

    fn paint(&self, dst: &mut Doc, p: &Paint) {
        if let Paint::Swatch { name, .. } = p
            && dst.swatch(name).is_none()
            && let Some(s) = self.src.swatch(name).cloned()
        {
            dst.swatches.push(s);
        }
    }

    fn asset(&mut self, dst: &mut Doc, id: AssetId) -> Option<AssetId> {
        if let Some(n) = self.assets.get(&id) {
            return Some(*n);
        }
        let a = self.src.assets.get(&id)?;
        let n = match dst.assets.get(&id) {
            Some(d) if Arc::ptr_eq(&d.data, &a.data) && d.name == a.name => id,
            _ => {
                let mut c = a.clone();
                c.id = AssetId(0);
                dst.add_asset(c)
            }
        };
        self.assets.insert(id, n);
        Some(n)
    }

    fn layer_for(&self, dst: &Doc, l: LayerId) -> LayerId {
        if let Some(t) = self.layer {
            return t;
        }
        let name = self.src.layer(l).map(|x| x.name.clone());
        if let Some(n) = name
            && let Some(d) = dst.layers.iter().find(|x| x.name == n)
        {
            return d.id;
        }
        if dst.layer(l).is_some() { l } else { crate::ops::active_layer(dst) }
    }

    pub fn paras(&mut self, dst: &mut Doc, paras: &[Para], story: StoryId) -> Vec<Para> {
        let mut out = paras.to_vec();
        for p in out.iter_mut() {
            self.style(dst, Family::Paragraph, &p.style.clone());
            for seg in p.segs.iter_mut() {
                if let Some(s) = seg.style().map(str::to_string) {
                    self.style(dst, Family::Character, &s);
                }
                let Seg::Inline { inline, .. } = seg else { continue };
                match inline {
                    Inline::Anchored { item, .. } => {
                        if let Some(n) = self.item(dst, *item, Home::Anchored(story), Vec2::ZERO) {
                            *item = n;
                        }
                    }
                    Inline::Footnote { paras } => {
                        let inner = paras.clone();
                        *paras = self.paras(dst, &inner, story);
                    }
                    Inline::Table { table } => {
                        if let Some(s) = table.style.clone() {
                            self.style(dst, Family::Table, &s);
                        }
                        let cells = table.cells.clone();
                        for (k, cell) in cells.iter().enumerate() {
                            if let Some(s) = &cell.style {
                                self.style(dst, Family::Cell, s);
                            }
                            let ps = self.paras(dst, &cell.paras, story);
                            table.cells[k].paras = ps;
                        }
                    }
                    Inline::Variable { name } => {
                        if !dst.variables.iter().any(|v| &v.name == name)
                            && let Some(v) = self.src.variables.iter().find(|v| &v.name == name).cloned()
                        {
                            dst.variables.push(v);
                        }
                    }
                    _ => {}
                }
            }
        }
        out
    }

    pub fn item(&mut self, dst: &mut Doc, id: ItemId, home: Home, offset: Vec2) -> Option<ItemId> {
        let src = self.src.item(id)?.clone();
        let mut it: Item = src.clone();
        it.id = ItemId(0);
        it.group = None;
        it.anchored_in = None;
        it.layer = self.layer_for(dst, it.layer);
        if matches!(home, Home::Spread(_) | Home::Parent(_)) {
            it.xf = Affine::translate(offset) * it.xf;
        }
        self.paint(dst, &it.fill);
        self.paint(dst, &it.stroke.paint);
        if let Some(s) = it.style.clone() {
            self.style(dst, Family::Object, &s);
        }
        let mut story_paras = None;
        match &mut it.kind {
            ItemKind::Group { children } => children.clear(),
            ItemKind::States { states, .. } => {
                for s in states.iter_mut() {
                    s.items.clear();
                }
            }
            ItemKind::Text(tf) => {
                let old = tf.story;
                let new = dst.new_story(formdesign_model::style::BASIC_PARA);
                let (paras, tag) = self.src.story(old).map(|s| (s.paras.clone(), s.xml_tag.clone())).unwrap_or_default();
                story_paras = Some((new, paras, tag));
                tf.story = new;
            }
            ItemKind::Graphic(g) => {
                if let Some(a) = g.asset {
                    g.asset = self.asset(dst, a);
                }
            }
            ItemKind::Plain => {}
        }
        let nid = dst.insert_item(it, home);
        if story_paras.is_some() {
            self.frames.push((id, nid));
        }
        if let Some((sid, paras, tag)) = story_paras {
            let mapped = self.paras(dst, &paras, sid);
            if let Some(st) = dst.story_mut(sid) {
                st.paras = mapped;
                st.xml_tag = tag;
                st.frames = vec![nid];
            }
        }
        match &src.kind {
            ItemKind::Group { children } => {
                for c in children {
                    self.item(dst, *c, Home::Group(nid), offset);
                }
            }
            ItemKind::States { states, .. } => {
                for (si, s) in states.iter().enumerate() {
                    for c in &s.items {
                        if let Some(cid) = self.item(dst, *c, Home::Group(nid), offset)
                            && let Some(ItemKind::States { states, .. }) = dst.item_mut(nid).map(|i| &mut i.kind)
                            && let Some(st) = states.get_mut(si)
                        {
                            st.items.push(cid);
                        }
                    }
                }
            }
            _ => {}
        }
        Some(nid)
    }
}
