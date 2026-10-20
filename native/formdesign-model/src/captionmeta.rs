use serde_json::Value;

use crate::doc::{Doc, Variable};
use crate::geom::Rect;
use crate::ids::ItemId;

pub const FIELDS: &[&str] = &["name", "path", "title", "description", "creator", "copyright", "dimensions", "ppi"];

const NEAR: f64 = 72.0;

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

fn xmp_field(data: &[u8], tags: &[&str]) -> Option<String> {
    let start = data.windows(10).position(|w| w == b"<x:xmpmeta")?;
    let end = (start + 262_144).min(data.len());
    let text = String::from_utf8_lossy(&data[start..end]);
    for tag in tags {
        let open = format!("<{tag}");
        let Some(at) = text.find(&open) else { continue };
        let rest = &text[at + open.len()..];
        let Some(gt) = rest.find('>') else { continue };
        let close = format!("</{tag}>");
        let Some(stop) = rest.find(&close) else { continue };
        if stop < gt {
            continue;
        }
        let body = &rest[gt + 1..stop];
        let inner = match body.find("<rdf:li") {
            Some(li) => {
                let b = &body[li..];
                let g = b.find('>')?;
                let e = b.find("</rdf:li>").unwrap_or(b.len());
                b[g + 1..e.max(g + 1)].to_string()
            }
            None => body.to_string(),
        };
        let v = unescape(inner.trim());
        if !v.is_empty() {
            return Some(v);
        }
    }
    None
}

fn dist(a: Rect, b: Rect) -> f64 {
    let dx = (a.x0 - b.x1).max(b.x0 - a.x1).max(0.0);
    let dy = (a.y0 - b.y1).max(b.y0 - a.y1).max(0.0);
    dx.hypot(dy)
}

impl Doc {
    fn is_picture(&self, id: ItemId) -> bool {
        self.item(id).and_then(|i| i.graphic()).and_then(|g| g.asset).is_some_and(|a| self.assets.contains_key(&a))
    }

    pub fn live_caption_image(&self, frame: ItemId, bound: Option<ItemId>) -> Option<ItemId> {
        let me = self.item(frame)?;
        if let Some(g) = me.group {
            let found = self.walk(&self.item(g)?.all_children()).into_iter().find(|i| *i != frame && self.is_picture(*i));
            if found.is_some() {
                return found;
            }
        }
        let fb = self.bounds(frame)?;
        let spread = self.spread_of(frame)?;
        let mut best: Option<(f64, ItemId)> = None;
        for id in self.walk(&self.spreads[spread].items) {
            if id == frame || !self.is_picture(id) {
                continue;
            }
            let Some(b) = self.bounds(id) else { continue };
            let d = dist(fb, b);
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, id));
            }
        }
        match best {
            Some((d, id)) if d <= NEAR => Some(id),
            other => bound.filter(|b| self.is_picture(*b)).or(other.map(|(_, id)| id)),
        }
    }

    pub fn live_caption_field(&self, image: ItemId, field: &str) -> String {
        let Some(it) = self.item(image) else { return String::new() };
        let Some(g) = it.graphic() else { return String::new() };
        let Some(asset) = g.asset.and_then(|a| self.assets.get(&a)) else { return String::new() };
        let custom = |k: &str| it.extra.get("metadata").and_then(|m| m.get(k)).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
        let stem = || asset.name.rsplit_once('.').map(|(s, _)| s.to_string()).unwrap_or_else(|| asset.name.clone());
        match field {
            "name" => asset.name.clone(),
            "path" => asset.link.clone().unwrap_or_default(),
            "title" => custom("title").or_else(|| xmp_field(&asset.data, &["dc:title"])).unwrap_or_else(|| if it.name.is_empty() { stem() } else { it.name.clone() }),
            "description" => custom("description").or_else(|| xmp_field(&asset.data, &["dc:description"])).unwrap_or_else(|| it.alt_text.clone()),
            "creator" => custom("creator").or_else(|| xmp_field(&asset.data, &["dc:creator", "photoshop:Credit"])).unwrap_or_default(),
            "copyright" => custom("copyright").or_else(|| xmp_field(&asset.data, &["dc:rights"])).unwrap_or_default(),
            "dimensions" => asset.pixels.map(|(w, h)| format!("{w} x {h} px")).unwrap_or_default(),
            "ppi" => {
                let Some((px, _)) = asset.pixels else { return String::new() };
                let (nat_w, _) = asset.natural_size();
                let [a, b, ..] = g.content.as_coeffs();
                let [wa, wb, ..] = self.world_xf(image).as_coeffs();
                let shown = nat_w * a.hypot(b) * wa.hypot(wb);
                if shown > 0.0 { format!("{}", (px as f64 * 72.0 / shown).round() as i64) } else { String::new() }
            }
            _ => String::new(),
        }
    }

    pub fn live_caption_text(&self, v: &Variable, frame: Option<ItemId>) -> String {
        let bound = v.value.get("image").and_then(Value::as_u64).map(ItemId);
        let image = frame.and_then(|f| self.live_caption_image(f, bound)).or(bound);
        let Some(image) = image else { return String::new() };
        let field = v.value.get("field").and_then(Value::as_str).unwrap_or("name");
        let prefix = v.value.get("prefix").and_then(Value::as_str).unwrap_or("");
        format!("{prefix}{}", self.live_caption_field(image, field))
    }
}
