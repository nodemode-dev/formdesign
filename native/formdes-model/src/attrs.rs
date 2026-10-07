use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::color::Paint;
use crate::{Result, err};

pub type Attrs = BTreeMap<String, Value>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Char,
    Para,
}

#[derive(Debug, Clone, Copy)]
pub enum Kind {
    Num,
    Int,
    Bool,
    Text,
    Choice(&'static [&'static str]),
    List,
    Paint,
    Leading,
    Object,
}

pub struct KeyDef {
    pub name: &'static str,
    pub level: Level,
    pub kind: Kind,
    pub about: &'static str,
}

const CAPS: &[&str] = &["normal", "allCaps", "smallCaps", "allSmallCaps"];
const POSITION: &[&str] = &["normal", "superscript", "subscript", "numerator", "denominator"];
const KERNING: &[&str] = &["metrics", "optical", "none"];
const ALIGN: &[&str] = &["left", "center", "right", "justify", "justifyCenter", "justifyRight", "justifyAll", "towardsSpine", "awayFromSpine"];
const COMPOSER: &[&str] = &["paragraph", "singleLine"];
const START: &[&str] = &["anywhere", "nextColumn", "nextFrame", "nextPage", "nextOddPage", "nextEvenPage"];
const LIST: &[&str] = &["none", "bullet", "number"];
const DIRECTION: &[&str] = &["ltr", "rtl"];
const CHANGE: &[&str] = &["none", "inserted", "deleted"];

macro_rules! keys {
    ($($lvl:ident $name:literal $kind:expr, $about:literal;)*) => {
        pub static KEYS: &[KeyDef] = &[$(KeyDef { name: $name, level: Level::$lvl, kind: $kind, about: $about }),*];
    };
}

keys! {
    Char "font" Kind::Text, "font family";
    Char "fontStyle" Kind::Text, "font style within the family (Regular, Bold Italic)";
    Char "size" Kind::Num, "point size";
    Char "leading" Kind::Leading, "line spacing in points, or \"auto\"";
    Char "kerning" Kind::Choice(KERNING), "pair kerning method";
    Char "kern" Kind::Num, "manual kerning after the character, 1/1000 em";
    Char "tracking" Kind::Num, "letter spacing, 1/1000 em";
    Char "hScale" Kind::Num, "horizontal scale, 1 = 100%";
    Char "vScale" Kind::Num, "vertical scale, 1 = 100%";
    Char "baselineShift" Kind::Num, "baseline shift in points";
    Char "skew" Kind::Num, "false italic angle in degrees";
    Char "fill" Kind::Paint, "text colour: a swatch name, \"none\", or a colour";
    Char "fillTint" Kind::Num, "tint of the fill, 0 to 1";
    Char "stroke" Kind::Paint, "text outline colour";
    Char "strokeWeight" Kind::Num, "text outline weight in points";
    Char "caps" Kind::Choice(CAPS), "case: normal, allCaps, smallCaps, allSmallCaps";
    Char "position" Kind::Choice(POSITION), "normal, superscript, subscript, numerator, denominator";
    Char "underline" Kind::Bool, "underline";
    Char "strike" Kind::Bool, "strikethrough";
    Char "ligatures" Kind::Bool, "standard ligatures";
    Char "noBreak" Kind::Bool, "keep the run on one line";
    Char "language" Kind::Text, "language tag for hyphenation and spelling (en-US, de-DE)";
    Char "features" Kind::List, "OpenType features, e.g. [\"onum\", \"ss01\"]";
    Char "conditions" Kind::List, "conditional text conditions applied";
    Char "change" Kind::Choice(CHANGE), "tracked change mark";
    Char "xmlTag" Kind::Text, "XML element of this text";
    Para "align" Kind::Choice(ALIGN), "alignment";
    Para "leftIndent" Kind::Num, "left indent in points";
    Para "rightIndent" Kind::Num, "right indent in points";
    Para "firstIndent" Kind::Num, "first-line indent in points";
    Para "lastIndent" Kind::Num, "last-line right indent in points";
    Para "spaceBefore" Kind::Num, "space before in points";
    Para "spaceAfter" Kind::Num, "space after in points";
    Para "autoLeading" Kind::Num, "auto leading as a multiple of size";
    Para "dropLines" Kind::Int, "drop cap height in lines";
    Para "dropChars" Kind::Int, "drop cap character count";
    Para "dropStyle" Kind::Text, "character style of the drop cap";
    Para "hyphenate" Kind::Bool, "hyphenation";
    Para "hyphMinWord" Kind::Int, "shortest word to hyphenate";
    Para "hyphAfter" Kind::Int, "letters before a hyphen";
    Para "hyphBefore" Kind::Int, "letters after a hyphen";
    Para "hyphLimit" Kind::Int, "most consecutive hyphenated lines, 0 = no limit";
    Para "hyphZone" Kind::Num, "hyphenation zone in points (single-line composer)";
    Para "hyphCaps" Kind::Bool, "hyphenate capitalised words";
    Para "wordSpace" Kind::List, "word spacing [min, desired, max] as fractions of a space";
    Para "letterSpace" Kind::List, "letter spacing [min, desired, max] in fractions of an em";
    Para "glyphScale" Kind::List, "glyph scaling [min, desired, max]";
    Para "composer" Kind::Choice(COMPOSER), "paragraph or singleLine";
    Para "keepNext" Kind::Int, "keep with the next n lines";
    Para "keepTogether" Kind::Bool, "keep all lines together";
    Para "keepFirst" Kind::Int, "lines kept together at the start";
    Para "keepLast" Kind::Int, "lines kept together at the end";
    Para "start" Kind::Choice(START), "where the paragraph starts";
    Para "ruleAbove" Kind::Object, "{weight, color, tint, offset, left, right, width: column|text}";
    Para "ruleBelow" Kind::Object, "{weight, color, tint, offset, left, right, width: column|text}";
    Para "shading" Kind::Object, "{color, tint, top, bottom, left, right}";
    Para "tabs" Kind::List, "[{position, align: left|center|right|decimal, leader, alignOn}]";
    Para "list" Kind::Choice(LIST), "none, bullet or number";
    Para "bullet" Kind::Text, "bullet character";
    Para "numberFormat" Kind::Text, "number pattern: ^# is the number, ^t a tab";
    Para "numberStyle" Kind::Text, "arabic, roman, ROMAN, alpha, ALPHA";
    Para "listName" Kind::Text, "named list numbering continues across stories";
    Para "startAt" Kind::Int, "first number of the list";
    Para "spanColumns" Kind::Int, "columns this paragraph spans, 0 = one, -1 = all";
    Para "splitColumns" Kind::Int, "sub-columns this paragraph splits into inside its column, 0 = none";
    Para "splitGutter" Kind::Num, "gap between split sub-columns in points";
    Para "gridAlign" Kind::Bool, "align to the baseline grid";
    Para "direction" Kind::Choice(DIRECTION), "paragraph direction";
    Para "balance" Kind::Bool, "balance ragged lines";
    Para "opticalMargin" Kind::Bool, "hang punctuation outside the margin";
    Para "nested" Kind::List, "nested styles: [{style, through, count, inclusive}]";
    Para "grep" Kind::List, "GREP styles: [{style, pattern}]";
}

pub fn key(name: &str) -> Option<&'static KeyDef> {
    KEYS.iter().find(|k| k.name == name)
}

pub fn default_value(name: &str) -> Value {
    match name {
        "font" => json!("Source Serif 4"),
        "fontStyle" => json!("Regular"),
        "size" => json!(12.0),
        "leading" => json!("auto"),
        "kerning" => json!("metrics"),
        "kern" | "tracking" | "baselineShift" | "skew" => json!(0.0),
        "hScale" | "vScale" | "fillTint" => json!(1.0),
        "fill" => json!("Black"),
        "stroke" => json!("none"),
        "strokeWeight" => json!(1.0),
        "caps" | "position" => json!("normal"),
        "underline" | "strike" | "noBreak" => json!(false),
        "ligatures" => json!(true),
        "language" => json!("en-US"),
        "features" | "conditions" | "tabs" | "nested" | "grep" => json!([]),
        "change" => json!("none"),
        "xmlTag" | "dropStyle" | "listName" => json!(""),
        "align" => json!("left"),
        "leftIndent" | "rightIndent" | "firstIndent" | "lastIndent" | "spaceBefore" | "spaceAfter" => json!(0.0),
        "autoLeading" => json!(1.2),
        "dropLines" | "dropChars" | "keepNext" | "spanColumns" | "splitColumns" => json!(0),
        "splitGutter" => json!(12.0),
        "hyphenate" | "hyphCaps" => json!(true),
        "hyphMinWord" => json!(5),
        "hyphAfter" | "hyphBefore" | "keepFirst" | "keepLast" => json!(2),
        "hyphLimit" => json!(3),
        "hyphZone" => json!(36.0),
        "wordSpace" => json!([0.8, 1.0, 1.33]),
        "letterSpace" => json!([0.0, 0.0, 0.0]),
        "glyphScale" => json!([1.0, 1.0, 1.0]),
        "composer" => json!("paragraph"),
        "keepTogether" | "gridAlign" | "balance" | "opticalMargin" => json!(false),
        "start" => json!("anywhere"),
        "ruleAbove" | "ruleBelow" | "shading" => Value::Null,
        "list" => json!("none"),
        "bullet" => json!("\u{2022}"),
        "numberFormat" => json!("^#.^t"),
        "numberStyle" => json!("arabic"),
        "startAt" => json!(1),
        "direction" => json!("ltr"),
        _ => Value::Null,
    }
}

pub fn validate(name: &str, v: &Value) -> Result<Value> {
    let Some(k) = key(name) else { return err(format!("unknown attribute `{name}`")) };
    let bad = || err(format!("`{name}` expects {}", describe(k.kind)));
    Ok(match k.kind {
        Kind::Num => match v.as_f64() {
            Some(n) if n.is_finite() => json!(n),
            _ => return bad(),
        },
        Kind::Int => match v.as_f64() {
            Some(n) if n.is_finite() => json!(n.round() as i64),
            _ => return bad(),
        },
        Kind::Bool => match v.as_bool() {
            Some(b) => json!(b),
            None => return bad(),
        },
        Kind::Text => match v.as_str() {
            Some(s) => json!(s),
            None => return bad(),
        },
        Kind::Choice(opts) => match v.as_str().and_then(|s| opts.iter().find(|o| o.eq_ignore_ascii_case(s))) {
            Some(o) => json!(o),
            None => return err(format!("`{name}` is one of {}", opts.join(", "))),
        },
        Kind::List => match v {
            Value::Array(_) => v.clone(),
            _ => return bad(),
        },
        Kind::Paint => match Paint::from_value(v) {
            Some(_) => v.clone(),
            None => return bad(),
        },
        Kind::Leading => match v {
            Value::String(s) if s.eq_ignore_ascii_case("auto") => json!("auto"),
            Value::Number(n) if n.as_f64().is_some_and(|f| f > 0.0) => v.clone(),
            _ => return bad(),
        },
        Kind::Object => match v {
            Value::Object(_) | Value::Null => v.clone(),
            _ => return bad(),
        },
    })
}

fn describe(k: Kind) -> &'static str {
    match k {
        Kind::Num => "a number",
        Kind::Int => "a whole number",
        Kind::Bool => "true or false",
        Kind::Text => "text",
        Kind::Choice(_) => "one of its choices",
        Kind::List => "a list",
        Kind::Paint => "a swatch name, \"none\" or a colour",
        Kind::Leading => "points or \"auto\"",
        Kind::Object => "an object",
    }
}

pub fn set_checked(target: &mut Attrs, src: &serde_json::Map<String, Value>, level: Option<Level>) -> Result<()> {
    for (k, v) in src {
        if let (Some(def), Some(l)) = (key(k), level)
            && def.level != l
        {
            return err(format!("`{k}` is a {} attribute", if def.level == Level::Char { "character" } else { "paragraph" }));
        }
        if v.is_null() {
            target.remove(k);
        } else {
            target.insert(k.clone(), validate(k, v)?);
        }
    }
    Ok(())
}

pub struct Stack<'a>(pub Vec<&'a Attrs>);

impl Stack<'_> {
    pub fn get(&self, name: &str) -> Value {
        for layer in self.0.iter().rev() {
            if let Some(v) = layer.get(name) {
                return v.clone();
            }
        }
        default_value(name)
    }

    pub fn num(&self, name: &str) -> f64 {
        self.get(name).as_f64().unwrap_or_else(|| default_value(name).as_f64().unwrap_or(0.0))
    }

    pub fn int(&self, name: &str) -> i64 {
        self.get(name).as_f64().map(|f| f as i64).unwrap_or(0)
    }

    pub fn bool(&self, name: &str) -> bool {
        self.get(name).as_bool().unwrap_or(false)
    }

    pub fn text(&self, name: &str) -> String {
        match self.get(name) {
            Value::String(s) => s,
            _ => String::new(),
        }
    }

    pub fn triple(&self, name: &str) -> [f64; 3] {
        let v = self.get(name);
        let a: Vec<f64> = v.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
        if a.len() == 3 { [a[0], a[1], a[2]] } else { let d = default_value(name); let d: Vec<f64> = d.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default(); [d[0], d[1], d[2]] }
    }

    pub fn strings(&self, name: &str) -> Vec<String> {
        self.get(name).as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Caps {
    Normal,
    All,
    Small,
    AllSmall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Position {
    Normal,
    Superscript,
    Subscript,
    Numerator,
    Denominator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kerning {
    Metrics,
    Optical,
    Off,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CharProps {
    pub font: String,
    pub font_style: String,
    pub size: f64,
    pub leading: Option<f64>,
    pub kerning: Kerning,
    pub kern: f64,
    pub tracking: f64,
    pub h_scale: f64,
    pub v_scale: f64,
    pub baseline_shift: f64,
    pub skew: f64,
    pub fill: Paint,
    pub fill_tint: f64,
    pub stroke: Paint,
    pub stroke_weight: f64,
    pub caps: Caps,
    pub position: Position,
    pub underline: bool,
    pub strike: bool,
    pub ligatures: bool,
    pub no_break: bool,
    pub language: String,
    pub features: Vec<String>,
    pub conditions: Vec<String>,
    pub change: Option<String>,
    pub xml_tag: String,
}

impl CharProps {
    pub fn resolve(stack: &Stack) -> CharProps {
        CharProps {
            font: stack.text("font"),
            font_style: stack.text("fontStyle"),
            size: stack.num("size").max(0.1),
            leading: stack.get("leading").as_f64(),
            kerning: match stack.text("kerning").as_str() {
                "optical" => Kerning::Optical,
                "none" => Kerning::Off,
                _ => Kerning::Metrics,
            },
            kern: stack.num("kern"),
            tracking: stack.num("tracking"),
            h_scale: stack.num("hScale").max(0.01),
            v_scale: stack.num("vScale").max(0.01),
            baseline_shift: stack.num("baselineShift"),
            skew: stack.num("skew"),
            fill: Paint::from_value(&stack.get("fill")).unwrap_or(Paint::None),
            fill_tint: stack.num("fillTint").clamp(0.0, 1.0),
            stroke: Paint::from_value(&stack.get("stroke")).unwrap_or(Paint::None),
            stroke_weight: stack.num("strokeWeight"),
            caps: match stack.text("caps").as_str() {
                "allCaps" => Caps::All,
                "smallCaps" => Caps::Small,
                "allSmallCaps" => Caps::AllSmall,
                _ => Caps::Normal,
            },
            position: match stack.text("position").as_str() {
                "superscript" => Position::Superscript,
                "subscript" => Position::Subscript,
                "numerator" => Position::Numerator,
                "denominator" => Position::Denominator,
                _ => Position::Normal,
            },
            underline: stack.bool("underline"),
            strike: stack.bool("strike"),
            ligatures: stack.bool("ligatures"),
            no_break: stack.bool("noBreak"),
            language: stack.text("language"),
            features: stack.strings("features"),
            conditions: stack.strings("conditions"),
            change: Some(stack.text("change")).filter(|c| c != "none" && !c.is_empty()),
            xml_tag: stack.text("xmlTag"),
        }
    }

    pub fn line_height(&self, auto: f64) -> f64 {
        self.leading.unwrap_or(self.size * auto)
    }

    pub fn weight(&self) -> u16 {
        style_weight(&self.font_style)
    }

    pub fn italic(&self) -> bool {
        let s = self.font_style.to_ascii_lowercase();
        s.contains("italic") || s.contains("oblique")
    }
}

pub fn style_weight(style: &str) -> u16 {
    let s = style.to_ascii_lowercase().replace([' ', '-'], "");
    for (k, w) in [("thin", 100), ("hairline", 100), ("extralight", 200), ("ultralight", 200), ("semilight", 350), ("light", 300), ("book", 400), ("medium", 500), ("semibold", 600), ("demibold", 600), ("extrabold", 800), ("ultrabold", 800), ("black", 900), ("heavy", 900), ("bold", 700)] {
        if s.contains(k) {
            return w;
        }
    }
    400
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Align {
    Left,
    Center,
    Right,
    Justify,
    JustifyCenter,
    JustifyRight,
    JustifyAll,
    TowardsSpine,
    AwayFromSpine,
}

impl Align {
    pub fn justified(self) -> bool {
        matches!(self, Align::Justify | Align::JustifyCenter | Align::JustifyRight | Align::JustifyAll)
    }

    pub fn resolve(self, right_page: bool) -> Align {
        match self {
            Align::TowardsSpine => if right_page { Align::Left } else { Align::Right },
            Align::AwayFromSpine => if right_page { Align::Right } else { Align::Left },
            a => a,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub weight: f64,
    pub color: Paint,
    pub tint: f64,
    pub offset: f64,
    pub left: f64,
    pub right: f64,
    pub text_width: bool,
}

impl Rule {
    fn from_value(v: &Value) -> Option<Rule> {
        let o = v.as_object()?;
        if o.get("on").and_then(Value::as_bool) == Some(false) {
            return None;
        }
        let n = |k: &str, d: f64| o.get(k).and_then(Value::as_f64).unwrap_or(d);
        Some(Rule {
            weight: n("weight", 1.0),
            color: o.get("color").and_then(Paint::from_value).unwrap_or(Paint::Swatch { name: "Black".into(), tint: 1.0 }),
            tint: n("tint", 1.0),
            offset: n("offset", 0.0),
            left: n("left", 0.0),
            right: n("right", 0.0),
            text_width: o.get("width").and_then(Value::as_str) == Some("text"),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shading {
    pub color: Paint,
    pub tint: f64,
    pub top: f64,
    pub bottom: f64,
    pub left: f64,
    pub right: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TabAlign {
    Left,
    Center,
    Right,
    Decimal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tab {
    pub position: f64,
    pub align: TabAlign,
    pub leader: String,
    pub align_on: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NestedRule {
    pub style: String,
    pub through: String,
    pub count: u32,
    pub inclusive: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GrepRule {
    pub style: String,
    pub pattern: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    None,
    Bullet,
    Number,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    Anywhere,
    NextColumn,
    NextFrame,
    NextPage,
    NextOddPage,
    NextEvenPage,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParaProps {
    pub align: Align,
    pub left_indent: f64,
    pub right_indent: f64,
    pub first_indent: f64,
    pub last_indent: f64,
    pub space_before: f64,
    pub space_after: f64,
    pub auto_leading: f64,
    pub drop_lines: u32,
    pub drop_chars: u32,
    pub drop_style: String,
    pub hyphenate: bool,
    pub hyph_min_word: usize,
    pub hyph_after: usize,
    pub hyph_before: usize,
    pub hyph_limit: usize,
    pub hyph_zone: f64,
    pub hyph_caps: bool,
    pub word_space: [f64; 3],
    pub letter_space: [f64; 3],
    pub glyph_scale: [f64; 3],
    pub single_line: bool,
    pub keep_next: u32,
    pub keep_together: bool,
    pub keep_first: u32,
    pub keep_last: u32,
    pub start: Start,
    pub rule_above: Option<Rule>,
    pub rule_below: Option<Rule>,
    pub shading: Option<Shading>,
    pub tabs: Vec<Tab>,
    pub list: ListKind,
    pub bullet: String,
    pub number_format: String,
    pub number_style: String,
    pub list_name: String,
    pub start_at: i64,
    pub span_columns: u32,
    pub split_columns: u32,
    pub split_gutter: f64,
    pub grid_align: bool,
    pub rtl: bool,
    pub balance: bool,
    pub optical_margin: bool,
    pub nested: Vec<NestedRule>,
    pub grep: Vec<GrepRule>,
}

impl ParaProps {
    pub fn resolve(stack: &Stack) -> ParaProps {
        let tabs = stack
            .get("tabs")
            .as_array()
            .map(|a| {
                let mut t: Vec<Tab> = a
                    .iter()
                    .filter_map(|v| {
                        let o = v.as_object()?;
                        Some(Tab {
                            position: o.get("position").and_then(Value::as_f64)?,
                            align: match o.get("align").and_then(Value::as_str).unwrap_or("left") {
                                "center" => TabAlign::Center,
                                "right" => TabAlign::Right,
                                "decimal" => TabAlign::Decimal,
                                _ => TabAlign::Left,
                            },
                            leader: o.get("leader").and_then(Value::as_str).unwrap_or("").to_string(),
                            align_on: o.get("alignOn").and_then(Value::as_str).unwrap_or(".").to_string(),
                        })
                    })
                    .collect();
                t.sort_by(|a, b| a.position.total_cmp(&b.position));
                t
            })
            .unwrap_or_default();
        let shading = stack.get("shading").as_object().and_then(|o| {
            let color = o.get("color").and_then(Paint::from_value)?;
            let n = |k: &str| o.get(k).and_then(Value::as_f64).unwrap_or(0.0);
            Some(Shading { color, tint: o.get("tint").and_then(Value::as_f64).unwrap_or(1.0), top: n("top"), bottom: n("bottom"), left: n("left"), right: n("right") })
        });
        let nested = stack
            .get("nested")
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| {
                        let o = v.as_object()?;
                        Some(NestedRule {
                            style: o.get("style")?.as_str()?.to_string(),
                            through: o.get("through").and_then(Value::as_str).unwrap_or("sentence").to_string(),
                            count: o.get("count").and_then(Value::as_u64).unwrap_or(1) as u32,
                            inclusive: o.get("inclusive").and_then(Value::as_bool).unwrap_or(true),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let grep = stack
            .get("grep")
            .as_array()
            .map(|a| a.iter().filter_map(|v| Some(GrepRule { style: v.get("style")?.as_str()?.to_string(), pattern: v.get("pattern")?.as_str()?.to_string() })).collect())
            .unwrap_or_default();
        ParaProps {
            align: match stack.text("align").as_str() {
                "center" => Align::Center,
                "right" => Align::Right,
                "justify" => Align::Justify,
                "justifyCenter" => Align::JustifyCenter,
                "justifyRight" => Align::JustifyRight,
                "justifyAll" => Align::JustifyAll,
                "towardsSpine" => Align::TowardsSpine,
                "awayFromSpine" => Align::AwayFromSpine,
                _ => Align::Left,
            },
            left_indent: stack.num("leftIndent"),
            right_indent: stack.num("rightIndent"),
            first_indent: stack.num("firstIndent"),
            last_indent: stack.num("lastIndent"),
            space_before: stack.num("spaceBefore").max(0.0),
            space_after: stack.num("spaceAfter").max(0.0),
            auto_leading: stack.num("autoLeading").max(0.1),
            drop_lines: stack.int("dropLines").max(0) as u32,
            drop_chars: stack.int("dropChars").max(0) as u32,
            drop_style: stack.text("dropStyle"),
            hyphenate: stack.bool("hyphenate"),
            hyph_min_word: stack.int("hyphMinWord").max(1) as usize,
            hyph_after: stack.int("hyphAfter").max(1) as usize,
            hyph_before: stack.int("hyphBefore").max(1) as usize,
            hyph_limit: stack.int("hyphLimit").max(0) as usize,
            hyph_zone: stack.num("hyphZone"),
            hyph_caps: stack.bool("hyphCaps"),
            word_space: stack.triple("wordSpace"),
            letter_space: stack.triple("letterSpace"),
            glyph_scale: stack.triple("glyphScale"),
            single_line: stack.text("composer") == "singleLine",
            keep_next: stack.int("keepNext").max(0) as u32,
            keep_together: stack.bool("keepTogether"),
            keep_first: stack.int("keepFirst").max(1) as u32,
            keep_last: stack.int("keepLast").max(1) as u32,
            start: match stack.text("start").as_str() {
                "nextColumn" => Start::NextColumn,
                "nextFrame" => Start::NextFrame,
                "nextPage" => Start::NextPage,
                "nextOddPage" => Start::NextOddPage,
                "nextEvenPage" => Start::NextEvenPage,
                _ => Start::Anywhere,
            },
            rule_above: Rule::from_value(&stack.get("ruleAbove")),
            rule_below: Rule::from_value(&stack.get("ruleBelow")),
            shading,
            tabs,
            list: match stack.text("list").as_str() {
                "bullet" => ListKind::Bullet,
                "number" => ListKind::Number,
                _ => ListKind::None,
            },
            bullet: stack.text("bullet"),
            number_format: stack.text("numberFormat"),
            number_style: stack.text("numberStyle"),
            list_name: stack.text("listName"),
            start_at: stack.int("startAt"),
            span_columns: match stack.int("spanColumns") {
                n if n < 0 => u32::MAX,
                n => n as u32,
            },
            split_columns: stack.int("splitColumns").max(0) as u32,
            split_gutter: stack.num("splitGutter").max(0.0),
            grid_align: stack.bool("gridAlign"),
            rtl: stack.text("direction") == "rtl",
            balance: stack.bool("balance"),
            optical_margin: stack.bool("opticalMargin"),
            nested,
            grep,
        }
    }
}

pub fn list_number(n: i64, style: &str) -> String {
    match style {
        "roman" => roman(n).to_lowercase(),
        "ROMAN" => roman(n),
        "alpha" => alpha(n).to_lowercase(),
        "ALPHA" => alpha(n),
        _ => n.to_string(),
    }
}

pub fn roman(mut n: i64) -> String {
    if n <= 0 {
        return n.to_string();
    }
    let table = [(1000, "M"), (900, "CM"), (500, "D"), (400, "CD"), (100, "C"), (90, "XC"), (50, "L"), (40, "XL"), (10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I")];
    let mut s = String::new();
    for (v, r) in table {
        while n >= v {
            s.push_str(r);
            n -= v;
        }
    }
    s
}

pub fn alpha(n: i64) -> String {
    if n <= 0 {
        return n.to_string();
    }
    let mut n = n;
    let mut s = Vec::new();
    while n > 0 {
        n -= 1;
        s.push((b'A' + (n % 26) as u8) as char);
        n /= 26;
    }
    s.iter().rev().collect()
}
