use formdes_model::attrs::Stack;
use formdes_model::color::{Paint, swatch_key};
use serde_json::{Map, Value, json};

use crate::{EngineError, Result};

pub const LOCKED: [&str; 4] = ["None", "Paper", "Black", "Registration"];

pub fn swatch_name(n: &str) -> String {
    let k = swatch_key(n);
    match LOCKED.iter().find(|l| l.eq_ignore_ascii_case(&k)) {
        Some(l) => format!("[{l}]"),
        None => k,
    }
}

pub fn paint_name(p: &Paint) -> String {
    match p {
        Paint::None => "[None]".into(),
        Paint::Swatch { name, .. } => swatch_name(name),
        Paint::Color { color } => color.hex(),
        Paint::Gradient { .. } => "[Gradient]".into(),
    }
}

pub fn paint_tint(p: &Paint) -> f64 {
    match p {
        Paint::Swatch { tint, .. } => *tint,
        _ => 1.0,
    }
}

pub fn paint_in(name: &str, tint: Option<f64>) -> Paint {
    let k = swatch_key(name);
    if k.is_empty() || k.eq_ignore_ascii_case("none") {
        return Paint::None;
    }
    if k.starts_with('#')
        && let Some(color) = formdes_model::color::Color::from_hex(&k)
    {
        return Paint::Color { color };
    }
    Paint::Swatch { name: k, tint: tint.unwrap_or(1.0).clamp(0.0, 1.0) }
}

const LANGS: &[(&str, &str)] = &[
    ("English: USA", "en-US"),
    ("English: UK", "en-GB"),
    ("English: Canadian", "en-CA"),
    ("German: 2006 Reform", "de-DE"),
    ("German: Swiss 2006 Reform", "de-CH"),
    ("French", "fr-FR"),
    ("French: Canadian", "fr-CA"),
    ("Spanish", "es-ES"),
    ("Italian", "it-IT"),
    ("Dutch: 2005 Reform", "nl-NL"),
    ("Portuguese", "pt-PT"),
    ("Portuguese: Brazilian", "pt-BR"),
    ("Swedish", "sv-SE"),
    ("Danish", "da-DK"),
    ("Norwegian: Bokmal", "nb-NO"),
    ("Norwegian: Nynorsk", "nn-NO"),
    ("Finnish", "fi-FI"),
    ("Polish", "pl-PL"),
    ("Czech", "cs-CZ"),
    ("Hungarian", "hu-HU"),
    ("Russian", "ru-RU"),
    ("Greek", "el-GR"),
    ("Turkish", "tr-TR"),
    ("Catalan", "ca-ES"),
    ("[No Language]", ""),
];

pub fn lang_in(s: &str) -> String {
    LANGS.iter().find(|(n, _)| n.eq_ignore_ascii_case(s)).map(|(_, t)| t.to_string()).unwrap_or_else(|| s.to_string())
}

pub fn lang_out(s: &str) -> String {
    LANGS.iter().find(|(_, t)| t.eq_ignore_ascii_case(s)).map(|(n, _)| n.to_string()).unwrap_or_else(|| s.to_string())
}

fn obj(v: &Value) -> Option<&Map<String, Value>> {
    v.as_object()
}

pub fn char_in(src: &Map<String, Value>) -> Result<Map<String, Value>> {
    let mut out = Map::new();
    let mut stroke_tint = None;
    for (k, v) in src {
        match k.as_str() {
            "fontFamily" | "font" => {
                out.insert("font".into(), v.clone());
            }
            "capitalization" | "caps" => {
                let c = match v.as_str() {
                    Some("openTypeAllSmallCaps" | "allSmallCaps") => json!("allSmallCaps"),
                    Some("allCaps") => json!("allCaps"),
                    Some("smallCaps") => json!("smallCaps"),
                    Some(_) => json!("normal"),
                    None => Value::Null,
                };
                out.insert("caps".into(), c);
            }
            "position" => {
                let p = match v.as_str() {
                    Some("otSuperscript" | "superscript") => json!("superscript"),
                    Some("otSubscript" | "subscript") => json!("subscript"),
                    Some("numerator") => json!("numerator"),
                    Some("denominator") => json!("denominator"),
                    Some(_) => json!("normal"),
                    None => Value::Null,
                };
                out.insert("position".into(), p);
            }
            "strikethrough" | "strike" => {
                out.insert("strike".into(), v.clone());
            }
            "leading" => {
                let l = match v {
                    Value::Object(o) => match o.get("kind").and_then(Value::as_str) {
                        Some("auto") => json!("auto"),
                        _ => o.get("value").cloned().unwrap_or(json!("auto")),
                    },
                    other => other.clone(),
                };
                out.insert("leading".into(), l);
            }
            "kerning" => match v {
                Value::Object(o) => match o.get("kind").and_then(Value::as_str) {
                    Some("manual") => {
                        out.insert("kern".into(), o.get("value").cloned().unwrap_or(json!(0.0)));
                    }
                    Some(kind) => {
                        out.insert("kerning".into(), json!(kind));
                        out.insert("kern".into(), Value::Null);
                    }
                    None => {}
                },
                other => {
                    out.insert("kerning".into(), other.clone());
                }
            },
            "language" => {
                out.insert("language".into(), v.as_str().map(|s| json!(lang_in(s))).unwrap_or(Value::Null));
            }
            "strokeTint" => stroke_tint = v.as_f64(),
            "otfFeatures" | "features" => {
                if v.is_array() {
                    out.insert("features".into(), v.clone());
                }
            }
            "ruby" | "kenten" | "tateChuYoko" | "change" | "xmlTag" | "conditions" => {}
            _ => {
                out.insert(k.clone(), v.clone());
            }
        }
    }
    if let Some(t) = stroke_tint
        && let Some(Value::String(name)) = out.get("stroke").cloned()
        && (t - 1.0).abs() > 1e-9
    {
        out.insert("stroke".into(), json!({"swatch": name, "tint": t}));
    }
    Ok(out)
}

pub fn char_out(s: &Stack) -> Map<String, Value> {
    let fill = Paint::from_value(&s.get("fill")).unwrap_or(Paint::None);
    let stroke = Paint::from_value(&s.get("stroke")).unwrap_or(Paint::None);
    let kern = s.num("kern");
    let leading = match s.get("leading").as_f64() {
        Some(v) => json!({"kind": "points", "value": v}),
        None => json!({"kind": "auto"}),
    };
    let kerning = if kern.abs() > 1e-9 { json!({"kind": "manual", "value": kern}) } else { json!({"kind": s.text("kerning")}) };
    let caps = match s.text("caps").as_str() {
        "allSmallCaps" => "openTypeAllSmallCaps",
        "allCaps" => "allCaps",
        "smallCaps" => "smallCaps",
        _ => "normal",
    };
    let mut m = Map::new();
    m.insert("fontFamily".into(), json!(s.text("font")));
    m.insert("fontStyle".into(), json!(s.text("fontStyle")));
    m.insert("size".into(), json!(s.num("size")));
    m.insert("leading".into(), leading);
    m.insert("kerning".into(), kerning);
    m.insert("tracking".into(), json!(s.num("tracking")));
    m.insert("hScale".into(), json!(s.num("hScale")));
    m.insert("vScale".into(), json!(s.num("vScale")));
    m.insert("baselineShift".into(), json!(s.num("baselineShift")));
    m.insert("skew".into(), json!(s.num("skew")));
    m.insert("language".into(), json!(lang_out(&s.text("language"))));
    m.insert("fill".into(), json!(paint_name(&fill)));
    m.insert("fillTint".into(), json!(s.num("fillTint") * paint_tint(&fill)));
    m.insert("stroke".into(), json!(paint_name(&stroke)));
    m.insert("strokeTint".into(), json!(paint_tint(&stroke)));
    m.insert("capitalization".into(), json!(caps));
    m.insert("position".into(), json!(s.text("position")));
    m.insert("underline".into(), json!(s.bool("underline")));
    m.insert("strikethrough".into(), json!(s.bool("strike")));
    m.insert("change".into(), json!(s.text("change")));
    m.insert("xmlTag".into(), json!(s.text("xmlTag")));
    m.insert("conditions".into(), s.get("conditions"));
    m.insert("otfFeatures".into(), s.get("features"));
    m
}

const ALIGN_MAP: &[(&str, &str)] = &[
    ("left", "left"),
    ("center", "center"),
    ("right", "right"),
    ("leftJustified", "justify"),
    ("centerJustified", "justifyCenter"),
    ("rightJustified", "justifyRight"),
    ("fullyJustified", "justifyAll"),
    ("towardsSpine", "towardsSpine"),
    ("awayFromSpine", "awayFromSpine"),
];

fn rule_in(v: &Value) -> Value {
    let Some(o) = obj(v) else { return json!({"on": false}) };
    if o.get("on").and_then(Value::as_bool) == Some(false) {
        return json!({"on": false});
    }
    let mut r = o.clone();
    r.remove("on");
    if let Some(cw) = r.remove("columnWidth").and_then(|v| v.as_bool()) {
        r.insert("width".into(), json!(if cw { "column" } else { "text" }));
    }
    Value::Object(r)
}

fn rule_out(v: &Value) -> Value {
    let Some(o) = obj(v).filter(|o| o.get("on").and_then(Value::as_bool) != Some(false)) else { return json!({"on": false}) };
    let color = o.get("color").and_then(Paint::from_value).unwrap_or_else(|| Paint::swatch("Black"));
    json!({
        "on": true,
        "weight": o.get("weight").and_then(Value::as_f64).unwrap_or(1.0),
        "color": paint_name(&color),
        "tint": o.get("tint").and_then(Value::as_f64).unwrap_or(1.0),
        "offset": o.get("offset").and_then(Value::as_f64).unwrap_or(0.0),
        "columnWidth": o.get("width").and_then(Value::as_str) != Some("text"),
    })
}

pub fn para_in(src: &Map<String, Value>) -> Result<Map<String, Value>> {
    let mut out = Map::new();
    let mut shading: Option<Map<String, Value>> = None;
    let mut shading_off = false;
    for (k, v) in src {
        match k.as_str() {
            "align" => {
                let a = v.as_str().and_then(|s| ALIGN_MAP.iter().find(|(w, m)| w.eq_ignore_ascii_case(s) || m.eq_ignore_ascii_case(s))).map(|(_, m)| json!(m));
                match a {
                    Some(a) => {
                        out.insert("align".into(), a);
                    }
                    None if v.is_null() => {
                        out.insert("align".into(), Value::Null);
                    }
                    None => return Err(EngineError::new(format!("`align` expected one of {}", ALIGN_MAP.iter().map(|(w, _)| *w).collect::<Vec<_>>().join(", ")))),
                }
            }
            "firstLineIndent" | "firstIndent" => {
                out.insert("firstIndent".into(), v.clone());
            }
            "lastLineIndent" | "lastIndent" => {
                out.insert("lastIndent".into(), v.clone());
            }
            "dropCapLines" | "dropLines" => {
                out.insert("dropLines".into(), v.clone());
            }
            "dropCapChars" | "dropChars" => {
                out.insert("dropChars".into(), v.clone());
            }
            "keepWithNext" | "keepNext" => {
                out.insert("keepNext".into(), v.clone());
            }
            "keepLinesTogether" | "keepTogether" => {
                out.insert("keepTogether".into(), v.clone());
            }
            "balanceRagged" | "balance" => {
                out.insert("balance".into(), v.clone());
            }
            "gridAlign" => {
                let b = match v {
                    Value::String(s) => json!(s != "none"),
                    other => other.clone(),
                };
                out.insert("gridAlign".into(), b);
            }
            "listType" | "list" => {
                let l = match v.as_str() {
                    Some("bullets" | "bullet") => json!("bullet"),
                    Some("numbers" | "number") => json!("number"),
                    Some(_) => json!("none"),
                    None => Value::Null,
                };
                out.insert("list".into(), l);
            }
            "tabs" => {
                let tabs = v
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|t| {
                                let mut t = t.as_object().cloned().unwrap_or_default();
                                if t.get("align").and_then(Value::as_str) == Some("char") {
                                    t.insert("align".into(), json!("decimal"));
                                }
                                Value::Object(t)
                            })
                            .collect::<Vec<_>>()
                    })
                    .map(Value::Array)
                    .unwrap_or(Value::Null);
                out.insert("tabs".into(), tabs);
            }
            "ruleAbove" | "ruleBelow" => {
                out.insert(k.clone(), rule_in(v));
            }
            "shadingOn" => {
                if v.as_bool() == Some(false) {
                    shading_off = true;
                } else {
                    shading.get_or_insert_with(Map::new);
                }
            }
            "shadingColor" => {
                shading.get_or_insert_with(Map::new).insert("color".into(), v.clone());
            }
            "shadingTint" => {
                shading.get_or_insert_with(Map::new).insert("tint".into(), v.clone());
            }
            "shadingOffsets" => {
                let a: Vec<f64> = v.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
                if a.len() == 4 {
                    let s = shading.get_or_insert_with(Map::new);
                    s.insert("top".into(), json!(a[0]));
                    s.insert("left".into(), json!(a[1]));
                    s.insert("bottom".into(), json!(a[2]));
                    s.insert("right".into(), json!(a[3]));
                }
            }
            "shading" => {
                out.insert("shading".into(), v.clone());
            }
            _ => {
                out.insert(k.clone(), v.clone());
            }
        }
    }
    if shading_off {
        out.insert("shading".into(), json!({"on": false}));
    } else if let Some(mut s) = shading {
        s.entry("color").or_insert(json!("Black"));
        out.insert("shading".into(), Value::Object(s));
    }
    Ok(out)
}

pub fn para_out(s: &Stack) -> Map<String, Value> {
    let align = s.text("align");
    let align = ALIGN_MAP.iter().find(|(_, m)| *m == align).map(|(w, _)| *w).unwrap_or("left");
    let tabs: Vec<Value> = s
        .get("tabs")
        .as_array()
        .map(|a| {
            a.iter()
                .map(|t| {
                    let mut t = t.as_object().cloned().unwrap_or_default();
                    if t.get("align").and_then(Value::as_str) == Some("decimal") {
                        t.insert("align".into(), json!("char"));
                    }
                    Value::Object(t)
                })
                .collect()
        })
        .unwrap_or_default();
    let list = match s.text("list").as_str() {
        "bullet" => "bullets",
        "number" => "numbers",
        _ => "none",
    };
    let shading = s.get("shading");
    let sh = shading.as_object().filter(|o| o.get("color").is_some());
    let shn = |k: &str| sh.and_then(|o| o.get(k)).and_then(Value::as_f64).unwrap_or(0.0);
    let mut m = Map::new();
    m.insert("align".into(), json!(align));
    m.insert("leftIndent".into(), json!(s.num("leftIndent")));
    m.insert("rightIndent".into(), json!(s.num("rightIndent")));
    m.insert("firstLineIndent".into(), json!(s.num("firstIndent")));
    m.insert("lastLineIndent".into(), json!(s.num("lastIndent")));
    m.insert("spaceBefore".into(), json!(s.num("spaceBefore")));
    m.insert("spaceAfter".into(), json!(s.num("spaceAfter")));
    m.insert("dropCapLines".into(), json!(s.int("dropLines")));
    m.insert("dropCapChars".into(), json!(s.int("dropChars")));
    m.insert("keepWithNext".into(), json!(s.int("keepNext")));
    m.insert("hyphenate".into(), json!(s.bool("hyphenate")));
    m.insert("keepLinesTogether".into(), json!(s.bool("keepTogether")));
    m.insert("gridAlign".into(), json!(if s.bool("gridAlign") { "allLines" } else { "none" }));
    m.insert("listType".into(), json!(list));
    m.insert("autoLeading".into(), json!(s.num("autoLeading")));
    m.insert("tabs".into(), Value::Array(tabs));
    m.insert("balanceRagged".into(), json!(s.bool("balance")));
    m.insert("ruleAbove".into(), rule_out(&s.get("ruleAbove")));
    m.insert("ruleBelow".into(), rule_out(&s.get("ruleBelow")));
    m.insert("shadingOn".into(), json!(sh.is_some()));
    m.insert("shadingColor".into(), json!(sh.and_then(|o| o.get("color")).and_then(Paint::from_value).map(|p| paint_name(&p)).unwrap_or_else(|| "[Black]".into())));
    m.insert("shadingTint".into(), json!(sh.and_then(|o| o.get("tint")).and_then(Value::as_f64).unwrap_or(1.0)));
    m.insert("shadingOffsets".into(), json!([shn("top"), shn("left"), shn("bottom"), shn("right")]));
    let border = s.get("border");
    let bd = border.as_object().filter(|o| o.get("on").and_then(Value::as_bool) != Some(false));
    m.insert("border".into(), match bd {
        Some(o) => {
            let mut o = o.clone();
            o.insert("on".into(), json!(true));
            Value::Object(o)
        }
        None => json!({"on": false}),
    });
    m.insert("nestedLines".into(), Value::Array(s.get("nestedLines").as_array().cloned().unwrap_or_default()));
    m
}

pub fn style_name(kind: formdes_model::style::StyleKind, name: &str) -> String {
    use formdes_model::style::StyleKind as K;
    let builtin = match kind {
        K::Paragraph => ["No Paragraph Style", "Basic Paragraph"].contains(&name),
        K::Character | K::Cell => name == "None",
        K::Object => ["None", "Basic Graphics Frame", "Basic Text Frame"].contains(&name),
        K::Table => ["No Table Style", "Basic Table"].contains(&name),
    };
    if builtin { format!("[{name}]") } else { name.to_string() }
}

pub fn rgb255(hex: &str) -> Value {
    let c = formdes_model::color::Color::from_hex(hex).map(|c| c.to_rgb()).unwrap_or([0.5, 0.5, 0.5]);
    json!([(c[0] * 255.0).round() as u8, (c[1] * 255.0).round() as u8, (c[2] * 255.0).round() as u8])
}

pub fn color_in(v: &Value) -> Option<formdes_model::color::Color> {
    use formdes_model::color::Color;
    let o = match v {
        Value::String(s) => return Color::from_hex(s),
        Value::Object(o) => o,
        _ => return None,
    };
    let n = |k: &str| o.get(k).and_then(Value::as_f64);
    let model = o.get("model").or_else(|| o.get("space")).and_then(Value::as_str).unwrap_or("").to_ascii_lowercase();
    if let (Some(c), Some(m), Some(y), Some(k)) = (n("c"), n("m"), n("y"), n("k")) {
        return Some(Color::Cmyk([c, m, y, k].map(|x| (x / 100.0).clamp(0.0, 1.0))));
    }
    if let (Some(r), Some(g), Some(b)) = (n("r"), n("g"), n("b")) {
        let max = r.max(g).max(b);
        let s = if max > 1.0 { 255.0 } else { 1.0 };
        return Some(Color::Rgb([r / s, g / s, b / s].map(|x| x.clamp(0.0, 1.0))));
    }
    if model == "lab"
        && let (Some(l), Some(a), Some(b)) = (n("l"), n("a"), n("b"))
    {
        return Some(Color::Lab([l, a, b]));
    }
    if let Some(g) = n("gray") {
        return Some(Color::Gray(if g > 1.0 { g / 100.0 } else { g }));
    }
    Color::from_value(v)
}

pub fn color_out(c: formdes_model::color::Color) -> Value {
    use formdes_model::color::Color;
    let r4 = |x: f64| (x * 10000.0).round() / 10000.0;
    match c {
        Color::Cmyk([c, m, y, k]) => json!({"model": "cmyk", "c": r4(c * 100.0), "m": r4(m * 100.0), "y": r4(y * 100.0), "k": r4(k * 100.0)}),
        Color::Rgb([r, g, b]) => json!({"model": "rgb", "r": r4(r), "g": r4(g), "b": r4(b), "hex": c.hex()}),
        Color::Lab([l, a, b]) => json!({"model": "lab", "l": r4(l), "a": r4(a), "b": r4(b)}),
        Color::Gray(g) => json!({"model": "gray", "gray": r4(g * 100.0)}),
    }
}

pub fn rgb_to_cmyk([r, g, b]: [f64; 3]) -> formdes_model::color::Color {
    let k = 1.0 - r.max(g).max(b);
    if k >= 0.9999 {
        return formdes_model::color::Color::Cmyk([0.0, 0.0, 0.0, 1.0]);
    }
    let f = |x: f64| ((1.0 - x - k) / (1.0 - k)).clamp(0.0, 1.0);
    formdes_model::color::Color::Cmyk([f(r), f(g), f(b), k])
}

pub fn hex_of(v: &Value) -> Option<String> {
    match v {
        Value::Array(a) if a.len() >= 3 => {
            let c: Vec<u8> = a.iter().take(3).map(|x| x.as_f64().unwrap_or(0.0).clamp(0.0, 255.0).round() as u8).collect();
            Some(format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]))
        }
        Value::String(s) => formdes_model::color::Color::from_hex(s).map(|c| c.hex()),
        _ => None,
    }
}
