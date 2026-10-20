use formdesign_model::doc::Doc;
use formdesign_model::item::ItemKind;
use serde_json::{Value, json};

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

pub fn slots_json(d: &Doc, prefix: &str) -> Value {
    let mut order = Vec::new();
    for top in d.spreads.iter().flat_map(|s| s.items.iter()).chain(d.parents.iter().flat_map(|p| p.items.iter())) {
        order.extend(d.walk(&[*top]));
    }
    for id in d.items.keys() {
        if !order.contains(id) {
            order.push(*id);
        }
    }
    let mut out = Vec::new();
    for id in order {
        let Some(it) = d.item(id) else { continue };
        let Some(name) = it.label.strip_prefix(prefix) else { continue };
        let b = d.bounds(id).unwrap_or_default();
        let page = if d.spread_of(d.top_of(id)).is_some() { d.page_of(id) } else { None };
        let mut v = json!({"slot": name, "id": id.0, "page": page, "bounds": [r2(b.x0), r2(b.y0), r2(b.x1), r2(b.y1)]});
        match &it.kind {
            ItemKind::Text(tf) => {
                v["kind"] = json!("text");
                v["story"] = json!(tf.story.0);
                if let Some(st) = d.story(tf.story) {
                    let text = st.text();
                    v["text"] = json!(text.chars().take(160).collect::<String>());
                    v["chars"] = json!(text.chars().count());
                    v["frames"] = json!(st.frames.len());
                }
            }
            ItemKind::Graphic(g) => {
                v["kind"] = json!("image");
                if let Some(a) = g.asset.and_then(|a| d.assets.get(&a)) {
                    v["image"] = json!(a.link.clone().unwrap_or_else(|| a.name.clone()));
                }
            }
            _ => {
                v["kind"] = json!("frame");
            }
        }
        out.push(v);
    }
    json!({"slots": out})
}
