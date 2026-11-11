use formdesign_model::doc::Doc;
use formdesign_model::ids::ItemId;
use formdesign_model::item::ItemKind;
use formdesign_model::page::LAYER_COLORS;
use formdesign_model::style::style_key;
use quick_xml::Reader;
use quick_xml::events::Event;
use serde_json::{Map, Value, json};

use crate::cmd::object::put_text;
use crate::registry::{Needs, Registry, cmd};
use crate::session::Ctx;
use crate::target::{items, text_targets};
use crate::{EngineError, Result, args, wire};

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn state(doc: &Doc) -> Map<String, Value> {
    doc.xml.as_object().cloned().unwrap_or_default()
}

fn root(doc: &Doc) -> String {
    state(doc).get("root").and_then(Value::as_str).unwrap_or("Root").to_string()
}

fn tags(doc: &Doc) -> Vec<(String, String)> {
    state(doc)
        .get("tags")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|t| Some((t.get("name")?.as_str()?.to_string(), t.get("color").and_then(Value::as_str).unwrap_or("#4f8bff").to_string()))).collect())
        .unwrap_or_default()
}

fn style_map(doc: &Doc) -> Vec<(String, String)> {
    state(doc)
        .get("styleMap")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|p| Some((p.get(0)?.as_str()?.to_string(), p.get(1)?.as_str()?.to_string()))).collect())
        .unwrap_or_default()
}

fn put(doc: &mut Doc, k: &str, v: Value) {
    let mut s = state(doc);
    s.insert(k.into(), v);
    doc.xml = Value::Object(s);
}

fn ensure_tag(doc: &mut Doc, name: &str) {
    let mut t = tags(doc);
    if !t.iter().any(|(n, _)| n == name) {
        let color = LAYER_COLORS[t.len() % LAYER_COLORS.len()].to_string();
        t.push((name.to_string(), color));
        put(doc, "tags", Value::Array(t.into_iter().map(|(n, c)| json!({"name": n, "color": c})).collect()));
    }
}

fn valid_name(n: &str) -> bool {
    let mut cs = n.chars();
    cs.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && cs.all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
}

fn tagged_items(doc: &Doc) -> Vec<(ItemId, String)> {
    let mut out: Vec<(usize, f64, f64, ItemId, String)> = doc
        .items
        .values()
        .filter_map(|it| {
            let tag = it.extra.get("xmlTag")?.as_str()?.to_string();
            let b = doc.bounds(it.id).unwrap_or_default();
            Some((doc.page_of(it.id).unwrap_or(usize::MAX), b.y0, b.x0, it.id, tag))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.total_cmp(&b.2)));
    out.into_iter().map(|x| (x.3, x.4)).collect()
}

pub fn export(doc: &Doc) -> String {
    let r = root(doc);
    let map = style_map(doc);
    let mut out = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<{r}>\n");
    for (id, tag) in tagged_items(doc) {
        let Some(it) = doc.item(id) else { continue };
        match &it.kind {
            ItemKind::Text(t) => {
                let Some(st) = doc.story(t.story) else { continue };
                out.push_str(&format!("<{tag}>"));
                for (i, p) in st.paras.iter().enumerate() {
                    let text = esc(&p.plain());
                    match map.iter().find(|(s, _)| *s == p.style) {
                        Some((_, ptag)) => out.push_str(&format!("<{ptag}>{text}</{ptag}>")),
                        None => out.push_str(&text),
                    }
                    if i + 1 < st.paras.len() {
                        out.push('\n');
                    }
                }
                out.push_str(&format!("</{tag}>\n"));
            }
            ItemKind::Graphic(g) => {
                let href = g.asset.and_then(|a| doc.assets.get(&a)).map(|a| a.link.clone().map(|l| format!("file://{l}")).unwrap_or_else(|| a.name.clone())).unwrap_or_default();
                out.push_str(&format!("<{tag} href=\"{}\"/>\n", esc(&href)));
            }
            _ => out.push_str(&format!("<{tag}/>\n")),
        }
    }
    out.push_str(&format!("</{r}>\n"));
    out
}

fn tags_cmd(c: &mut Ctx, _: &Value) -> Result<Value> {
    let doc = c.doc()?;
    let s = state(doc);
    Ok(json!({
        "root": root(doc),
        "tags": tags(doc).into_iter().map(|(n, col)| json!({"name": n, "color": wire::rgb255(&col)})).collect::<Vec<_>>(),
        "styleMap": style_map(doc).into_iter().map(|(a, b)| json!([wire::style_name(formdesign_model::style::StyleKind::Paragraph, &a), b])).collect::<Vec<_>>(),
        "dtd": s.get("dtd").and_then(Value::as_str).is_some_and(|d| !d.is_empty()),
    }))
}

fn structure(c: &mut Ctx, _: &Value) -> Result<Value> {
    let doc = c.doc()?;
    let elements: Vec<Value> = tagged_items(doc)
        .into_iter()
        .filter_map(|(id, tag)| {
            let it = doc.item(id)?;
            Some(match &it.kind {
                ItemKind::Text(t) => json!({"tag": tag, "id": id, "kind": "text", "text": doc.story(t.story).map(|s| s.plain().chars().take(60).collect::<String>()).unwrap_or_default()}),
                ItemKind::Graphic(_) => json!({"tag": tag, "id": id, "kind": "graphic"}),
                _ => json!({"tag": tag, "id": id, "kind": "object"}),
            })
        })
        .collect();
    Ok(json!({"root": root(doc), "elements": elements}))
}

fn new_tag(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = args::str_(p, "name", "the tag name")?.to_string();
    if !valid_name(&name) {
        return Err(EngineError::new(format!("{name} is not a valid XML name")));
    }
    let doc = c.doc_mut()?;
    if tags(doc).iter().any(|(n, _)| *n == name) {
        return Err(EngineError::new(format!("there is already a tag named {name}")));
    }
    ensure_tag(doc, &name);
    if let Some(col) = p.get("color").and_then(wire::hex_of) {
        let t: Vec<Value> = tags(doc).into_iter().map(|(n, c0)| json!({"name": n, "color": if n == name { col.clone() } else { c0 }})).collect();
        put(doc, "tags", Value::Array(t));
    }
    ok()
}

fn ok() -> Result<Value> {
    Ok(json!({}))
}

fn delete_tag(c: &mut Ctx, p: &Value) -> Result<Value> {
    let name = args::str_(p, "name", "the tag to delete")?.to_string();
    let doc = c.doc_mut()?;
    let t: Vec<Value> = tags(doc).into_iter().filter(|(n, _)| *n != name).map(|(n, c0)| json!({"name": n, "color": c0})).collect();
    put(doc, "tags", Value::Array(t));
    let m: Vec<Value> = style_map(doc).into_iter().filter(|(_, tg)| *tg != name).map(|(a, b)| json!([a, b])).collect();
    put(doc, "styleMap", Value::Array(m));
    let ids: Vec<ItemId> = doc.items.values().filter(|i| i.extra.get("xmlTag").and_then(Value::as_str) == Some(name.as_str())).map(|i| i.id).collect();
    for id in ids {
        if let Some(it) = doc.item_mut(id) {
            it.extra.remove("xmlTag");
        }
    }
    let n2 = name.clone();
    crate::walk::paras_where(doc, &move |p| p.segs.iter().any(|s| s.over().get("xmlTag").and_then(Value::as_str) == Some(n2.as_str())), &mut |p| {
        for s in p.segs.iter_mut() {
            let (_, over) = s.format_mut();
            if over.get("xmlTag").and_then(Value::as_str) == Some(name.as_str()) {
                over.remove("xmlTag");
            }
        }
        p.normalize();
    });
    ok()
}

fn tag_items(c: &mut Ctx, p: &Value) -> Result<Value> {
    let ids = items(c, p)?;
    let tag = p.get("tag").and_then(Value::as_str).map(str::to_string);
    if let Some(t) = &tag
        && !valid_name(t)
    {
        return Err(EngineError::new(format!("{t} is not a valid XML name")));
    }
    let doc = c.doc_mut()?;
    if let Some(t) = &tag {
        ensure_tag(doc, t);
    }
    for id in ids {
        if let Some(it) = doc.item_mut(id) {
            match &tag {
                Some(t) => {
                    it.extra.insert("xmlTag".into(), json!(t));
                }
                None => {
                    it.extra.remove("xmlTag");
                }
            }
        }
    }
    c.label(if tag.is_some() { "Tag Frame" } else { "Untag" });
    ok()
}

fn tag_text(c: &mut Ctx, p: &Value) -> Result<Value> {
    let tag = p.get("tag").and_then(Value::as_str).map(str::to_string);
    if let Some(t) = &tag
        && !valid_name(t)
    {
        return Err(EngineError::new(format!("{t} is not a valid XML name")));
    }
    let targets = text_targets(c)?;
    let doc = c.doc_mut()?;
    if let Some(t) = &tag {
        ensure_tag(doc, t);
    }
    let mut m = Map::new();
    m.insert("xmlTag".into(), tag.map(Value::String).unwrap_or(Value::Null));
    for (sid, range) in targets {
        if range.is_empty() {
            continue;
        }
        if let Some(st) = doc.story_mut(sid) {
            st.set_char_attrs(range, &m)?;
        }
    }
    ok()
}

fn map_style(c: &mut Ctx, p: &Value) -> Result<Value> {
    let style = style_key(args::str_(p, "style", "a paragraph style name")?);
    let tag = p.get("tag").and_then(Value::as_str).map(str::to_string);
    if let Some(t) = &tag
        && !valid_name(t)
    {
        return Err(EngineError::new(format!("{t} is not a valid XML name")));
    }
    let doc = c.doc_mut()?;
    let mut m: Vec<(String, String)> = style_map(doc).into_iter().filter(|(s, _)| *s != style).collect();
    if let Some(t) = tag {
        ensure_tag(doc, &t);
        m.push((style, t));
    }
    put(doc, "styleMap", Value::Array(m.into_iter().map(|(a, b)| json!([a, b])).collect()));
    c.label("Map Styles to Tags");
    ok()
}

fn dtd_elements(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("<!ELEMENT") {
        rest = &rest[i + 9..];
        let end = rest.find('>').unwrap_or(rest.len());
        let decl = rest[..end].trim();
        let name: String = decl.chars().take_while(|c| !c.is_whitespace()).collect();
        let model = decl[name.len()..].trim().to_string();
        if !name.is_empty() {
            out.push((name, model));
        }
        rest = &rest[end..];
    }
    out
}

fn load_dtd(c: &mut Ctx, p: &Value) -> Result<Value> {
    let text = args::str_(p, "text", "the DTD file's text")?.to_string();
    let els = dtd_elements(&text);
    if els.is_empty() {
        return Err(EngineError::new("the DTD declares no elements"));
    }
    let doc = c.doc_mut()?;
    for (n, _) in &els {
        ensure_tag(doc, n);
    }
    put(doc, "dtd", json!(text));
    if let Some((first, _)) = els.first() {
        put(doc, "root", json!(first));
    }
    Ok(json!({"elements": els.len()}))
}

fn delete_dtd(c: &mut Ctx, _: &Value) -> Result<Value> {
    let doc = c.doc_mut()?;
    put(doc, "dtd", Value::Null);
    ok()
}

fn validate(c: &mut Ctx, _: &Value) -> Result<Value> {
    let doc = c.doc()?;
    let dtd = state(doc).get("dtd").and_then(Value::as_str).map(str::to_string).filter(|d| !d.is_empty()).ok_or_else(|| EngineError::new("no DTD is loaded: load one first"))?;
    let els = dtd_elements(&dtd);
    let r = root(doc);
    let mut problems = Vec::new();
    let root_model = els.iter().find(|(n, _)| *n == r).map(|(_, m)| m.clone());
    if root_model.is_none() {
        problems.push(json!({"path": format!("/{r}"), "message": format!("the DTD does not declare the root element {r}")}));
    }
    let allowed = |m: &str, child: &str| m.contains("ANY") || m.split(|ch: char| !(ch.is_ascii_alphanumeric() || "_.-".contains(ch))).any(|w| w == child);
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (_, tag) in tagged_items(doc) {
        let k = counts.entry(tag.clone()).or_insert(0);
        *k += 1;
        let path = format!("/{r}/{tag}[{k}]");
        if !els.iter().any(|(n, _)| *n == tag) {
            problems.push(json!({"path": path, "message": format!("element {tag} is not declared in the DTD")}));
        } else if let Some(m) = &root_model
            && !allowed(m, &tag)
        {
            problems.push(json!({"path": path, "message": format!("element {tag} not allowed here")}));
        }
    }
    Ok(json!({"valid": problems.is_empty(), "problems": problems}))
}

struct XmlEl {
    tag: String,
    text: String,
    href: Option<String>,
}

fn parse_children(text: &str) -> Result<(String, Vec<XmlEl>)> {
    let mut r = Reader::from_str(text);
    let mut depth = 0;
    let mut root = String::new();
    let mut out: Vec<XmlEl> = Vec::new();
    loop {
        match r.read_event() {
            Ok(Event::Start(e)) => {
                depth += 1;
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if depth == 1 {
                    root = name;
                } else if depth == 2 {
                    let href = e.attributes().flatten().find(|a| a.key.as_ref() == b"href").and_then(|a| a.unescape_value().ok()).map(|v| v.to_string());
                    out.push(XmlEl { tag: name, text: String::new(), href });
                } else if let Some(cur) = out.last_mut()
                    && !cur.text.is_empty()
                    && !cur.text.ends_with('\n')
                {
                    cur.text.push('\n');
                }
            }
            Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if depth == 0 {
                    root = name;
                } else if depth == 1 {
                    let href = e.attributes().flatten().find(|a| a.key.as_ref() == b"href").and_then(|a| a.unescape_value().ok()).map(|v| v.to_string());
                    out.push(XmlEl { tag: name, text: String::new(), href });
                }
            }
            Ok(Event::Text(t)) if depth >= 2 => {
                if let Some(cur) = out.last_mut() {
                    let s = t.decode().map(|c| c.into_owned()).unwrap_or_default();
                    cur.text.push_str(&quick_xml::escape::unescape(&s).map(|c| c.into_owned()).unwrap_or(s));
                }
            }
            Ok(Event::GeneralRef(g)) if depth >= 2 => {
                if let Some(cur) = out.last_mut() {
                    if let Ok(Some(ch)) = g.resolve_char_ref() {
                        cur.text.push(ch);
                        continue;
                    }
                    let n = g.decode().map(|c| c.into_owned()).unwrap_or_default();
                    let s = match n.as_str() {
                        "amp" => "&",
                        "lt" => "<",
                        "gt" => ">",
                        "quot" => "\"",
                        "apos" => "'",
                        _ => "",
                    };
                    cur.text.push_str(s);
                }
            }
            Ok(Event::End(_)) => depth -= 1,
            Ok(Event::Eof) => break,
            Err(e) => return Err(EngineError::new(format!("XML: {e}"))),
            _ => {}
        }
    }
    Ok((root, out))
}

fn import_xml(c: &mut Ctx, p: &Value) -> Result<Value> {
    let text = args::str_(p, "text", "the XML file's text")?.to_string();
    let (root_name, els) = parse_children(&text)?;
    let doc = c.doc_mut()?;
    let slots = tagged_items(doc);
    let mut used: Vec<ItemId> = Vec::new();
    let (mut placed, mut unmatched) = (0, 0);
    for el in els {
        let Some((id, _)) = slots.iter().find(|(id, t)| *t == el.tag && !used.contains(id)) else {
            unmatched += 1;
            continue;
        };
        used.push(*id);
        let it = doc.item(*id).cloned();
        match it.as_ref().map(|i| &i.kind) {
            Some(ItemKind::Text(t)) => {
                let sid = t.story;
                if let Some(st) = doc.story_mut(sid) {
                    let style = st.paras.first().map(|p| p.style.clone()).unwrap_or_else(|| formdesign_model::style::BASIC_PARA.into());
                    st.paras = vec![formdesign_model::story::Para::empty(&style)];
                    put_text(st, 0, el.text.trim());
                }
                placed += 1;
            }
            Some(ItemKind::Graphic(_)) | Some(ItemKind::Plain) => {
                if let Some(h) = &el.href {
                    let path = h.strip_prefix("file://").unwrap_or(h);
                    if let Ok(a) = crate::assets::load(path, 0) {
                        crate::cmd::file::place_image(doc, *id, a)?;
                        placed += 1;
                        continue;
                    }
                }
                unmatched += 1;
            }
            _ => unmatched += 1,
        }
    }
    if !root_name.is_empty() {
        put(doc, "root", json!(root_name));
    }
    if placed > 0 && c.selection().text.is_some() {
        c.set_selection(crate::session::Selection::default());
    }
    c.label("Import XML");
    Ok(json!({"placed": placed, "unmatched": unmatched}))
}

pub fn register(r: &mut Registry) {
    cmd("xml.tags", "Tags").query().sig("{}").reg(r, tags_cmd);
    cmd("xml.structure", "Structure").query().sig("{}").reg(r, structure);
    cmd("xml.newTag", "New Tag").sig("{name, color?}").reg(r, new_tag);
    cmd("xml.deleteTag", "Delete Tag").sig("{name}").reg(r, delete_tag);
    cmd("xml.tag", "Tag Frame").needs(Needs::Items).sig("{tag | null, ids?}").reg(r, tag_items);
    cmd("xml.tagText", "Tag Text").needs(Needs::Text).sig("{tag | null}").reg(r, tag_text);
    cmd("xml.mapStyle", "Map Styles to Tags").sig("{style, tag | null}").reg(r, map_style);
    cmd("xml.loadDtd", "Load DTD").sig("{text}").reg(r, load_dtd);
    cmd("xml.deleteDtd", "Delete DTD").sig("{}").reg(r, delete_dtd);
    cmd("xml.validate", "Validate").query().sig("{}").reg(r, validate);
    cmd("file.importXml", "Import XML...").menu(&["File", "Import XML..."]).sig("{text}").reg(r, import_xml);
}
