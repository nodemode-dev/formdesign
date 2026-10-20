use std::collections::{BTreeMap, BTreeSet};

use formdesign_engine::wire;
use formdesign_model::attrs::{Align, Attrs, Caps, CharProps, ListKind, Position};
use formdesign_model::color::Paint;
use formdesign_model::doc::{Doc, LinkDest, LinkSource};
use formdesign_model::ids::{AssetId, ItemId, StoryId};
use formdesign_model::item::ItemKind;
use formdesign_model::story::{Inline, Seg};
use formdesign_model::style::StyleKind;
use serde_json::{Map, Value, json};

use crate::core::Core;
use crate::proto::Reply;

const SLOT: &str = "slot:";

struct Placed {
    item: ItemId,
    page: usize,
    pos: (i64, i64),
}

fn rgb(d: &Doc, paint: &Paint) -> Option<String> {
    d.paint_color(paint).map(|c| c.hex())
}

fn r1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

fn weight_of(style: &str) -> u16 {
    formdesign_model::attrs::style_weight(style)
}

fn italic(style: &str) -> bool {
    let s = style.to_ascii_lowercase();
    s.contains("italic") || s.contains("oblique")
}

fn align_name(a: Align) -> &'static str {
    match a {
        Align::Center => "center",
        Align::Right | Align::AwayFromSpine => "right",
        Align::Left | Align::TowardsSpine => "left",
        _ => "justify",
    }
}

fn caps_name(c: Caps) -> &'static str {
    match c {
        Caps::All => "all",
        Caps::Small | Caps::AllSmall => "small",
        Caps::Normal => "none",
    }
}

fn dest(d: &LinkDest) -> Value {
    match d {
        LinkDest::Url(u) => json!(u),
        LinkDest::Email(e) => json!(format!("mailto:{e}")),
        LinkDest::Page(n) => json!({"page": n}),
        LinkDest::Anchor(a) => json!(format!("#{a}")),
    }
}

fn list_name(l: ListKind) -> Option<&'static str> {
    match l {
        ListKind::Bullet => Some("bullet"),
        ListKind::Number => Some("number"),
        ListKind::None => None,
    }
}

fn para_style_json(d: &Doc, name: &str) -> Value {
    let empty = Attrs::new();
    let pp = d.styles.para_props(name, &empty);
    let cp = d.styles.char_props(name, &empty, None, &empty);
    let mut v = json!({
        "font": cp.font, "fontStyle": cp.font_style, "weight": weight_of(&cp.font_style), "italic": italic(&cp.font_style),
        "size": r1(cp.size), "leading": r1(cp.line_height(pp.auto_leading)), "tracking": cp.tracking, "caps": caps_name(cp.caps),
        "align": align_name(pp.align), "spaceBefore": r1(pp.space_before), "spaceAfter": r1(pp.space_after),
        "firstIndent": r1(pp.first_indent), "leftIndent": r1(pp.left_indent), "rightIndent": r1(pp.right_indent),
        "color": rgb(d, &cp.fill), "underline": cp.underline,
    });
    if pp.drop_lines > 0 {
        v["dropCap"] = json!(pp.drop_lines);
    }
    if let Some(l) = list_name(pp.list) {
        v["list"] = json!(l);
    }
    if let Some(t) = d.styles.find(StyleKind::Paragraph, name).and_then(|s| s.export.as_ref()).filter(|t| !t.tag.is_empty()) {
        v["tag"] = json!(t.tag);
        v["class"] = json!(t.class);
    }
    v
}

fn char_style_json(d: &Doc, name: &str) -> Option<Value> {
    let cs = d.styles.find(StyleKind::Character, name)?;
    let a = &cs.attrs;
    let mut v = Map::new();
    if let Some(f) = a.get("font").and_then(Value::as_str) {
        v.insert("font".into(), json!(f));
    }
    if let Some(f) = a.get("fontStyle").and_then(Value::as_str) {
        v.insert("weight".into(), json!(weight_of(f)));
        v.insert("italic".into(), json!(italic(f)));
    }
    if let Some(s) = a.get("size").and_then(Value::as_f64) {
        v.insert("size".into(), json!(r1(s)));
    }
    if let Some(fill) = a.get("fill").and_then(Paint::from_value)
        && let Some(h) = rgb(d, &fill)
    {
        v.insert("color".into(), json!(h));
    }
    if let Some(u) = a.get("underline").and_then(Value::as_bool) {
        v.insert("underline".into(), json!(u));
    }
    if let Some(k) = a.get("caps").and_then(Value::as_str) {
        let c = match k {
            "all" | "allCaps" => "all",
            "small" | "smallCaps" | "allSmall" => "small",
            _ => "none",
        };
        v.insert("caps".into(), json!(c));
    }
    if let Some(t) = cs.export.as_ref().filter(|t| !t.tag.is_empty()) {
        v.insert("tag".into(), json!(t.tag));
    }
    Some(Value::Object(v))
}

fn clean(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for c in raw.chars() {
        match c {
            '\u{2028}' => out.push('\n'),
            '\t' => out.push(' '),
            '\u{E000}'..='\u{F8FF}' | '\u{FFFC}' => {}
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

fn run_attrs(d: &Doc, pc: &CharProps, rc: &CharProps, style: Option<&str>) -> Map<String, Value> {
    let mut a = Map::new();
    if let Some(s) = style.filter(|s| *s != "None") {
        a.insert("c".into(), json!(wire::style_name(StyleKind::Character, s)));
    }
    let (rw, pw) = (weight_of(&rc.font_style), weight_of(&pc.font_style));
    if rw >= 600 && pw < 600 {
        a.insert("b".into(), json!(true));
    }
    if italic(&rc.font_style) && !italic(&pc.font_style) {
        a.insert("i".into(), json!(true));
    }
    if rc.underline && !pc.underline {
        a.insert("u".into(), json!(true));
    }
    if rc.strike && !pc.strike {
        a.insert("s".into(), json!(true));
    }
    if rc.caps != pc.caps {
        a.insert("caps".into(), json!(caps_name(rc.caps)));
    }
    match rc.position {
        Position::Superscript | Position::Numerator => {
            a.insert("pos".into(), json!("sup"));
        }
        Position::Subscript | Position::Denominator => {
            a.insert("pos".into(), json!("sub"));
        }
        Position::Normal => {}
    }
    let (rcol, pcol) = (rgb(d, &rc.fill), rgb(d, &pc.fill));
    if rcol != pcol
        && let Some(h) = rcol
    {
        a.insert("color".into(), json!(h));
    }
    if rc.font != pc.font {
        a.insert("f".into(), json!(rc.font));
    }
    if pc.size > 0.0 && (rc.size / pc.size - 1.0).abs() > 0.02 {
        a.insert("size".into(), json!((rc.size / pc.size * 100.0).round() / 100.0));
    }
    a
}

fn hidden(d: &Doc, rc: &CharProps) -> bool {
    rc.change.as_deref() == Some("deleted") || (!rc.conditions.is_empty() && rc.conditions.iter().all(|c| d.conditions.iter().any(|x| x.name == *c && !x.visible)))
}

struct Span {
    t: String,
    attrs: Map<String, Value>,
}

fn story_paras(d: &Doc, sid: StoryId, page: Option<usize>) -> Vec<Value> {
    let Some(story) = d.story(sid) else { return Vec::new() };
    let links: Vec<(usize, usize, Value)> = d
        .hyperlinks
        .iter()
        .filter_map(|h| match &h.source {
            LinkSource::Text { story: s, start, end } if *s == sid && start < end => Some((*start, *end, dest(&h.dest))),
            _ => None,
        })
        .collect();
    let mut out = Vec::new();
    let mut pos = 0usize;
    for (pi, para) in story.paras.iter().enumerate() {
        if pi > 0 {
            pos += 1;
        }
        let pp = d.styles.para_props(&para.style, &para.over);
        let empty = Attrs::new();
        let pc = d.styles.char_props(&para.style, &para.over, None, &empty);
        let mut spans: Vec<Span> = Vec::new();
        for seg in &para.segs {
            let (s0, len) = (pos, seg.len());
            pos += len;
            let rc = d.styles.char_props(&para.style, &para.over, seg.style(), seg.over());
            if hidden(d, &rc) {
                continue;
            }
            let raw: String = match seg {
                Seg::Text { text, .. } => text.clone(),
                Seg::Inline { inline: Inline::Variable { name }, .. } => d.variable_text(name, page),
                Seg::Inline { inline: Inline::RightTab, .. } => "\t".into(),
                Seg::Inline { .. } => String::new(),
            };
            if raw.is_empty() {
                continue;
            }
            let base = run_attrs(d, &pc, &rc, seg.style());
            let chars: Vec<char> = raw.chars().collect();
            let mut marks: BTreeSet<usize> = BTreeSet::from([0, chars.len()]);
            if matches!(seg, Seg::Text { .. }) {
                for (ls, le, _) in &links {
                    for m in [*ls, *le] {
                        if m > s0 && m < s0 + len {
                            marks.insert(m - s0);
                        }
                    }
                }
            }
            let marks: Vec<usize> = marks.into_iter().collect();
            for w in marks.windows(2) {
                let piece: String = chars[w[0]..w[1]].iter().collect();
                let t = clean(&piece);
                if t.is_empty() {
                    continue;
                }
                let mut attrs = base.clone();
                let (a, b) = (s0 + w[0], s0 + w[1]);
                if let Some((_, _, href)) = links.iter().find(|(ls, le, _)| *ls <= a && b <= *le) {
                    attrs.insert("href".into(), href.clone());
                }
                match spans.last_mut() {
                    Some(prev) if prev.attrs == attrs => prev.t.push_str(&t),
                    _ => spans.push(Span { t, attrs }),
                }
            }
        }
        let mut v = json!({"style": wire::style_name(StyleKind::Paragraph, &para.style), "align": align_name(pp.align)});
        if let Some(l) = list_name(pp.list) {
            v["list"] = json!(l);
        }
        if pp.rtl {
            v["rtl"] = json!(true);
        }
        v["runs"] = Value::Array(
            spans
                .into_iter()
                .map(|sp| {
                    let mut o = sp.attrs;
                    o.insert("t".into(), json!(sp.t));
                    Value::Object(o)
                })
                .collect(),
        );
        out.push(v);
    }
    out
}

fn placed(d: &Doc) -> Vec<Placed> {
    let mut out = Vec::new();
    for (si, sp) in d.spreads.iter().enumerate() {
        let first = d.first_page_of_spread(si);
        for id in d.walk(&sp.items) {
            let Some(it) = d.item(id) else { continue };
            if it.hidden || it.anchored_in.is_some() || d.layer(it.layer).is_some_and(|l| !l.visible) {
                continue;
            }
            let Some(b) = d.bounds(id) else { continue };
            if let Some(k) = sp.page_at_x(b.center().x) {
                out.push(Placed { item: id, page: first + k, pos: ((b.y0 * 10.0) as i64, (b.x0 * 10.0) as i64) });
            }
        }
    }
    out
}

fn bounds(d: &Doc, id: ItemId) -> [f64; 4] {
    let b = d.bounds(id).unwrap_or_default();
    [r1(b.x0), r1(b.y0), r1(b.x1), r1(b.y1)]
}

fn item_href(d: &Doc, id: ItemId) -> Option<Value> {
    d.hyperlinks.iter().find_map(|h| match h.source {
        LinkSource::Item { item } if item == id => Some(dest(&h.dest)),
        _ => None,
    })
}

fn block(d: &Doc, pl: &Placed, frames_page: &BTreeMap<u64, usize>) -> Option<Value> {
    let it = d.item(pl.item)?;
    let slot = it.label.strip_prefix(SLOT).map(str::to_string);
    let mut v = match &it.kind {
        ItemKind::Text(tf) => {
            let story = d.story(tf.story)?;
            if story.frames.first() != Some(&it.id) || story.text().replace('\u{FFFC}', "").trim().is_empty() {
                return None;
            }
            let pages: BTreeSet<usize> = story.frames.iter().filter_map(|f| frames_page.get(&f.0).copied()).collect();
            json!({"kind": "story", "story": tf.story.0, "frames": story.frames.iter().map(|f| f.0).collect::<Vec<_>>(),
                "pages": pages, "columns": tf.columns, "paras": story_paras(d, tf.story, Some(pl.page))})
        }
        ItemKind::Graphic(g) => {
            let a = d.assets.get(&g.asset?)?;
            if a.data.is_empty() {
                return None;
            }
            let kind = if a.mime.starts_with("video/") {
                "video"
            } else if a.mime.starts_with("audio/") {
                "audio"
            } else {
                "image"
            };
            let mut v = json!({"kind": kind, "asset": a.id.0, "mime": a.mime, "name": a.name, "bytes": a.data.len(),
                "pixels": a.pixels.map(|(w, h)| [w, h]), "alt": it.alt_text});
            if let Some(m) = it.extra.get("media").filter(|_| a.is_media()) {
                let b = |k: &str, dflt: bool| m.get(k).and_then(Value::as_bool).unwrap_or(dflt);
                v["controls"] = json!(b("controls", true));
                v["autoplay"] = json!(b("playOnPageLoad", false));
                v["loop"] = json!(b("loop", false));
                v["poster"] = m.get("poster").cloned().unwrap_or(Value::Null);
            }
            v
        }
        _ => return None,
    };
    v["item"] = json!(it.id.0);
    v["page"] = json!(pl.page);
    v["bounds"] = json!(bounds(d, it.id));
    v["slot"] = json!(slot);
    if let Some(h) = item_href(d, it.id) {
        v["href"] = h;
    }
    Some(v)
}

pub fn content(core: &Core, _p: &Value) -> Result<Value, String> {
    let st = core.ses.active().ok_or("no document open")?;
    let d = &st.doc;
    let mut all = placed(d);
    let frames_page: BTreeMap<u64, usize> = all.iter().map(|p| (p.item.0, p.page)).collect();
    all.sort_by_key(|p| (p.page, p.pos.0, p.pos.1));
    let blocks: Vec<Value> = all.iter().filter_map(|p| block(d, p, &frames_page)).collect();
    let mut pages = Vec::new();
    for (si, sp) in d.spreads.iter().enumerate() {
        let first = d.first_page_of_spread(si);
        for (k, pg) in sp.pages.iter().enumerate() {
            pages.push(json!({"index": first + k, "name": d.page_name(first + k), "spread": si, "x": r1(sp.page_x(k)), "width": r1(pg.width), "height": r1(pg.height)}));
        }
    }
    let hotspots: Vec<Value> = d
        .hyperlinks
        .iter()
        .filter_map(|h| match h.source {
            LinkSource::Item { item } => {
                let pl = all.iter().find(|p| p.item == item)?;
                Some(json!({"page": pl.page, "bounds": bounds(d, item), "href": dest(&h.dest)}))
            }
            LinkSource::Text { .. } => None,
        })
        .collect();
    let articles: Vec<Value> = d.articles.iter().filter(|a| a.export).map(|a| json!({"name": a.name, "items": a.items.iter().map(|i| i.0).collect::<Vec<_>>()})).collect();
    let mut para_styles = Map::new();
    for ps in d.styles.list(StyleKind::Paragraph) {
        para_styles.insert(wire::style_name(StyleKind::Paragraph, &ps.name), para_style_json(d, &ps.name));
    }
    let mut char_styles = Map::new();
    for cs in d.styles.list(StyleKind::Character) {
        if cs.name == "None" {
            continue;
        }
        if let Some(v) = char_style_json(d, &cs.name) {
            char_styles.insert(cs.name.clone(), v);
        }
    }
    let swatches: Map<String, Value> = d
        .swatches
        .iter()
        .filter_map(|w| rgb(d, &Paint::Swatch { name: w.name.clone(), tint: 1.0 }).map(|h| (wire::swatch_name(&w.name), json!(h))))
        .collect();
    let empty = Attrs::new();
    let basic = d.styles.char_props(formdesign_model::style::BASIC_PARA, &empty, None, &empty);
    Ok(json!({
        "title": st.title(), "intent": d.settings.intent, "facing": d.settings.facing,
        "pageWidth": d.settings.page_width, "pageHeight": d.settings.page_height, "language": wire::lang_out(&basic.language),
        "pages": pages, "blocks": blocks, "articles": articles, "hotspots": hotspots,
        "styles": {"paragraph": para_styles, "character": char_styles}, "swatches": swatches,
    }))
}

pub fn asset(core: &Core, p: &Value) -> Result<Reply, String> {
    let id = p.get("asset").and_then(Value::as_u64).ok_or("missing `asset`")?;
    let st = core.ses.active().ok_or("no document open")?;
    let a = st.doc.assets.get(&AssetId(id)).ok_or_else(|| format!("no asset {id}"))?;
    if a.data.is_empty() {
        return Err(format!("asset {id} ({}) has no data: relink it in the Links panel", a.name));
    }
    Ok(Reply { result: json!({"mime": a.mime, "name": a.name, "bytes": a.data.len(), "pixels": a.pixels.map(|(w, h)| [w, h])}), bin: Some(a.data.to_vec()) })
}
