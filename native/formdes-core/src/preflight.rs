use std::collections::BTreeSet;

use formdes_model::color::{Color, Paint, SwatchValue};
use formdes_model::doc::{Doc, Intent};
use formdes_model::ids::ItemId;
use formdes_model::item::ItemKind;
use serde_json::{Value, json};

use crate::core::Core;

struct Profile {
    overset: bool,
    fonts: bool,
    links: bool,
    empty: bool,
    rgb: bool,
    transparency: bool,
    min_ppi: Option<f64>,
    max_ppi: Option<f64>,
    spot_max: Option<f64>,
    bleed: Option<f64>,
    safety: Option<f64>,
    min_type: Option<f64>,
    page_multiple: Option<f64>,
}

impl Profile {
    fn parse(v: Option<&Value>, print: bool) -> Profile {
        let v = v.cloned().unwrap_or(Value::Null);
        let on = |k: &str, d: bool| v.get(k).and_then(Value::as_bool).unwrap_or(d);
        let num = |k: &str, d: Option<f64>| match v.get(k) {
            Some(Value::Null) => None,
            Some(x) => x.as_f64().filter(|n| *n > 0.0),
            None => d,
        };
        Profile {
            overset: on("overset", true),
            fonts: on("fonts", true),
            links: on("links", true),
            empty: on("emptyFrames", true),
            rgb: on("rgb", print),
            transparency: on("transparency", false),
            min_ppi: num("minPpi", print.then_some(150.0)),
            max_ppi: num("maxPpi", None),
            spot_max: v.get("spotMax").and_then(Value::as_f64).filter(|n| *n >= 0.0),
            bleed: num("bleed", None),
            safety: num("safety", None),
            min_type: num("minType", None),
            page_multiple: num("pageMultiple", None),
        }
    }
}

fn issue(severity: &str, kind: &str, message: String, item: Option<ItemId>, page: Option<usize>) -> Value {
    json!({"severity": severity, "kind": kind, "message": message, "item": item.map(|i| i.0), "page": page})
}

fn on_pages(d: &Doc) -> Vec<ItemId> {
    let tops: Vec<ItemId> = d.spreads.iter().flat_map(|s| s.items.iter().copied()).collect();
    d.walk(&tops)
}

fn painted(d: &Doc, id: ItemId) -> bool {
    d.item(id).is_some_and(|it| matches!(it.kind, ItemKind::Graphic(_)) || !matches!(it.fill, Paint::None))
}

fn images(d: &Doc, p: &Profile, out: &mut Vec<Value>) {
    if p.min_ppi.is_none() && p.max_ppi.is_none() {
        return;
    }
    for id in on_pages(d) {
        let Some(it) = d.item(id) else { continue };
        let Some(g) = it.graphic() else { continue };
        let Some(a) = g.asset.and_then(|a| d.assets.get(&a)) else { continue };
        if a.data.is_empty() || a.mime.contains("pdf") || a.mime.contains("svg") || a.is_media() {
            continue;
        }
        let Some((pw, _)) = a.pixels else { continue };
        let k = (d.world_xf(id) * g.content).as_coeffs();
        let (nw, _) = a.natural_size();
        let shown = nw * k[0].hypot(k[1]);
        let ppi = pw as f64 / (shown / 72.0).max(1e-6);
        let page = d.page_of(id);
        if let Some(min) = p.min_ppi
            && ppi < min
        {
            out.push(issue("warning", "lowResolution", format!("{}: effective {ppi:.0} ppi (< {min:.0})", a.name), Some(id), page));
        }
        if let Some(max) = p.max_ppi
            && ppi > max
        {
            out.push(issue("warning", "highResolution", format!("{}: effective {ppi:.0} ppi (> {max:.0}, a heavier file than needed)", a.name), Some(id), page));
        }
    }
}

fn edges(d: &Doc, p: &Profile, out: &mut Vec<Value>) {
    for (si, sp) in d.spreads.iter().enumerate() {
        if sp.pages.is_empty() {
            continue;
        }
        let x0 = 0.0;
        let x1 = sp.page_x(sp.pages.len());
        let y1 = sp.pages.iter().map(|pg| pg.height).fold(0.0, f64::max);
        let first = d.first_page_of_spread(si);
        for &id in &sp.items {
            let Some(it) = d.item(id) else { continue };
            let Some(b) = d.bounds(id) else { continue };
            if let Some(need) = p.bleed
                && painted(d, id)
            {
                let mut short = Vec::new();
                let mut check = |touch: bool, ext: f64, name: &'static str| {
                    if touch && ext < need - 0.01 {
                        short.push(name);
                    }
                };
                check(b.x0 <= x0 + 0.5 && b.x1 > x0, x0 - b.x0, "left");
                check(b.x1 >= x1 - 0.5 && b.x0 < x1, b.x1 - x1, "right");
                check(b.y0 <= 0.5 && b.y1 > 0.0, -b.y0, "top");
                check(b.y1 >= y1 - 0.5 && b.y0 < y1, b.y1 - y1, "bottom");
                if !short.is_empty() {
                    out.push(issue("error", "bleed", format!("Reaches the {} trim without {need:.0} pt of bleed", short.join(", ")), Some(id), d.page_of(id)));
                }
            }
            if let Some(safe) = p.safety
                && matches!(it.kind, ItemKind::Text(_))
                && let Some(pi) = sp.page_at_x(b.center().x)
                && let Some(pg) = sp.pages.get(pi)
            {
                let px = sp.page_x(pi);
                let single = sp.pages.len() == 1;
                let left_out = single || pi == 0;
                let right_out = single || pi == sp.pages.len() - 1;
                let near = (left_out && b.x0 - px < safe - 0.01) || (right_out && px + pg.width - b.x1 < safe - 0.01) || b.y0 < safe - 0.01 || pg.height - b.y1 < safe - 0.01;
                if near {
                    out.push(issue("warning", "trimSafety", format!("Text within {safe:.0} pt of the trim"), Some(id), Some(first + pi)));
                }
            }
        }
    }
}

fn type_size(d: &Doc, p: &Profile, out: &mut Vec<Value>) {
    let Some(min) = p.min_type else { return };
    for story in d.stories.values() {
        let mut smallest: Option<f64> = None;
        for para in &story.paras {
            for seg in &para.segs {
                if let formdes_model::story::Seg::Text { text, .. } = seg
                    && text.trim().is_empty()
                {
                    continue;
                }
                let size = d.styles.char_props(&para.style, &para.over, seg.style(), seg.over()).size;
                if size < min - 0.01 {
                    smallest = Some(smallest.map_or(size, |s: f64| s.min(size)));
                }
            }
        }
        if let Some(sz) = smallest {
            let first = story.frames.first().copied();
            out.push(issue("warning", "smallType", format!("Text at {sz:.1} pt, under {min:.0} pt"), first, first.and_then(|f| d.page_of(f))));
        }
    }
}

fn spot_names(d: &Doc) -> BTreeSet<String> {
    d.swatches.iter().filter(|s| s.spot).map(|s| s.name.clone()).collect()
}

fn colours(d: &Doc, p: &Profile, out: &mut Vec<Value>) {
    if p.rgb {
        for sw in &d.swatches {
            if matches!(sw.value, SwatchValue::Color { color: Color::Rgb(_) | Color::Lab(_) }) {
                out.push(issue("warning", "rgbColor", format!("RGB swatch: {}", formdes_engine::wire::swatch_name(&sw.name)), None, None));
            }
        }
    }
    if let Some(max) = p.spot_max {
        let spots = spot_names(d);
        let mut used = BTreeSet::new();
        let mut note = |paint: &Paint| {
            if let Paint::Swatch { name, .. } = paint
                && spots.contains(name)
            {
                used.insert(name.clone());
            }
        };
        for id in on_pages(d) {
            if let Some(it) = d.item(id) {
                note(&it.fill);
                note(&it.stroke.paint);
            }
        }
        for story in d.stories.values() {
            for para in &story.paras {
                for seg in &para.segs {
                    note(&d.styles.char_props(&para.style, &para.over, seg.style(), seg.over()).fill);
                }
            }
        }
        if used.len() as f64 > max {
            out.push(issue("error", "spotColors", format!("{} spot inks in use ({}), the profile allows {max:.0}", used.len(), used.into_iter().collect::<Vec<_>>().join(", ")), None, None));
        }
    }
}

fn transparency(d: &Doc, p: &Profile, out: &mut Vec<Value>) {
    if !p.transparency {
        return;
    }
    for id in on_pages(d) {
        let Some(it) = d.item(id) else { continue };
        if it.opacity < 0.999 || !it.blend.eq_ignore_ascii_case("normal") {
            let what = if it.opacity < 0.999 { format!("opacity {:.0}%", it.opacity * 100.0) } else { format!("blend mode {}", it.blend) };
            out.push(issue("warning", "transparency", format!("Transparency: {what}"), Some(id), d.page_of(id)));
        }
    }
}

fn empty_text(d: &Doc, out: &mut Vec<Value>) {
    for id in on_pages(d) {
        let Some(it) = d.item(id) else { continue };
        if let Some(sid) = it.story()
            && d.story(sid).is_some_and(|s| s.frames.first() == Some(&id) && s.text().trim().is_empty())
            && it.label.is_empty()
        {
            out.push(issue("warning", "emptyText", "Empty text frame".into(), Some(id), d.page_of(id)));
        }
    }
}

pub fn run(core: &mut Core, p: &Value) -> Result<Value, String> {
    let base = core.exec_value("preflight.run", &json!({}))?;
    let st = core.ses.active().ok_or("no document open")?;
    let d = &st.doc;
    let prof = Profile::parse(p.get("profile"), d.settings.intent == Intent::Print);
    let mut out: Vec<Value> = base
        .get("issues")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| match i.get("kind").and_then(Value::as_str).unwrap_or("") {
            "overset" => prof.overset,
            "missingFont" => prof.fonts,
            "missingLink" | "modifiedLink" => prof.links,
            "lowResolution" | "rgbColor" => false,
            _ => true,
        })
        .collect();
    if prof.empty {
        empty_text(d, &mut out);
    }
    images(d, &prof, &mut out);
    edges(d, &prof, &mut out);
    type_size(d, &prof, &mut out);
    colours(d, &prof, &mut out);
    transparency(d, &prof, &mut out);
    if let Some(m) = prof.page_multiple {
        let n = d.page_count();
        let m = m.round().max(1.0) as usize;
        if !n.is_multiple_of(m) {
            out.push(issue("error", "pageCount", format!("{n} {}; the profile needs a multiple of {m}", if n == 1 { "page" } else { "pages" }), None, None));
        }
    }
    let errors = out.iter().filter(|i| i.get("severity").and_then(Value::as_str) == Some("error")).count();
    Ok(json!({"errors": errors, "warnings": out.len() - errors, "issues": out}))
}
