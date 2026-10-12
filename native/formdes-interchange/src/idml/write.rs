use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{Cursor, Write};

use formdes_model::attrs::Attrs;
use formdes_model::color::{Color, Paint, SwatchValue};
use formdes_model::doc::Doc;
use formdes_model::geom::{Affine, BezPath, Point};
use kurbo::PathEl;
use formdes_model::ids::{ItemId, StoryId};
use formdes_model::item::{AutoSize, FirstBaseline, Item, ItemKind, Shape, StrokeAlign, VJust, WrapMode};
use formdes_model::story::{Inline, Marker, Para, Seg, Table};
use formdes_model::style::{BASIC_PARA, NO_CHAR, StyleKind};
use serde_json::Value;

use super::{DOM, MIME, NS, b64};

pub fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\u{2028}' => o.push('\u{2028}'),
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' => {}
            c => o.push(c),
        }
    }
    o
}

fn uid(n: u64) -> String {
    format!("u{n:x}")
}

fn num(v: f64) -> String {
    let r = (v * 10000.0).round() / 10000.0;
    if r == r.trunc() { format!("{}", r as i64) } else { format!("{r}") }
}

fn matrix(a: Affine) -> String {
    a.as_coeffs().iter().map(|v| num(*v)).collect::<Vec<_>>().join(" ")
}

pub fn style_ref(kind: StyleKind, name: &str) -> String {
    let n = name.replace(':', "%3a");
    match kind {
        StyleKind::Paragraph if name == BASIC_PARA => "ParagraphStyle/$ID/NormalParagraphStyle".into(),
        StyleKind::Paragraph => format!("ParagraphStyle/{n}"),
        StyleKind::Character if name == NO_CHAR || name.is_empty() => "CharacterStyle/$ID/[No character style]".into(),
        StyleKind::Character => format!("CharacterStyle/{n}"),
        StyleKind::Object if name.is_empty() => "ObjectStyle/$ID/[None]".into(),
        StyleKind::Object => format!("ObjectStyle/{n}"),
        StyleKind::Table => format!("TableStyle/{n}"),
        StyleKind::Cell => format!("CellStyle/{n}"),
    }
}

fn style_name(kind: StyleKind, name: &str) -> String {
    match kind {
        StyleKind::Paragraph if name == BASIC_PARA => "$ID/NormalParagraphStyle".into(),
        _ => name.to_string(),
    }
}

struct Colors {
    unnamed: BTreeMap<String, String>,
}

fn color_value(c: Color) -> (&'static str, String) {
    let pct = |v: f64| num((v * 100.0).clamp(0.0, 100.0));
    match c {
        Color::Cmyk([a, b, cc, d]) => ("CMYK", format!("{} {} {} {}", pct(a), pct(b), pct(cc), pct(d))),
        Color::Gray(g) => ("CMYK", format!("0 0 0 {}", pct(g))),
        Color::Lab([l, a, b]) => ("LAB", format!("{} {} {}", num(l), num(a), num(b))),
        Color::Rgb([r, g, b]) => ("RGB", format!("{} {} {}", num(r * 255.0), num(g * 255.0), num(b * 255.0))),
    }
}

impl Colors {
    fn paint_ref(&mut self, doc: &Doc, p: &Paint) -> (String, Option<f64>) {
        match p {
            Paint::None => ("Swatch/None".into(), None),
            Paint::Swatch { name, tint } => {
                let r = match doc.swatch(name).map(|s| &s.value) {
                    Some(SwatchValue::Gradient { .. }) => format!("Gradient/{}", name),
                    Some(SwatchValue::Tint { .. }) => format!("Tint/{}", name),
                    _ => format!("Color/{}", name),
                };
                (r, (*tint < 0.999).then_some(*tint * 100.0))
            }
            Paint::Color { color } => {
                let (space, v) = color_value(*color);
                let key = format!("{space}:{v}");
                let n = self.unnamed.len();
                let r = self.unnamed.entry(key).or_insert_with(|| format!("Color/u{:x}c", 0xc000 + n)).clone();
                (r, None)
            }
            Paint::Gradient { .. } => ("Swatch/None".into(), None),
        }
    }
}

type Anchor = (Point, Point, Point);

fn path_points(p: &BezPath) -> Vec<(Vec<Anchor>, bool)> {
    let mut subs = Vec::new();
    let mut cur: Vec<Anchor> = Vec::new();
    let mut closed = false;
    for el in p.elements() {
        match *el {
            PathEl::MoveTo(a) => {
                if !cur.is_empty() {
                    subs.push((std::mem::take(&mut cur), closed));
                }
                closed = false;
                cur.push((a, a, a));
            }
            PathEl::LineTo(a) => cur.push((a, a, a)),
            PathEl::QuadTo(c, a) => {
                let p0 = cur.last().map(|x| x.0).unwrap_or(a);
                let c1 = p0 + (c - p0) * (2.0 / 3.0);
                let c2 = a + (c - a) * (2.0 / 3.0);
                if let Some(l) = cur.last_mut() {
                    l.2 = c1;
                }
                cur.push((a, c2, a));
            }
            PathEl::CurveTo(c1, c2, a) => {
                if let Some(l) = cur.last_mut() {
                    l.2 = c1;
                }
                cur.push((a, c2, a));
            }
            PathEl::ClosePath => {
                closed = true;
                if cur.len() > 1 && (cur[0].0 - cur[cur.len() - 1].0).hypot() < 1e-6 {
                    let last = cur.pop().expect("len checked");
                    cur[0].1 = last.1;
                }
            }
        }
    }
    if !cur.is_empty() {
        subs.push((cur, closed));
    }
    subs
}

fn geometry(p: &BezPath) -> String {
    let mut s = String::from("<Properties><PathGeometry>");
    for (pts, closed) in path_points(p) {
        let _ = write!(s, "<GeometryPathType PathOpen=\"{}\"><PathPointArray>", !closed);
        for (a, l, r) in pts {
            let _ = write!(s, "<PathPointType Anchor=\"{} {}\" LeftDirection=\"{} {}\" RightDirection=\"{} {}\"/>", num(a.x), num(a.y), num(l.x), num(l.y), num(r.x), num(r.y));
        }
        s.push_str("</PathPointArray></GeometryPathType>");
    }
    s.push_str("</PathGeometry></Properties>");
    s
}

const CHAR_SIMPLE: &[(&str, &str, f64)] = &[
    ("size", "PointSize", 1.0),
    ("tracking", "Tracking", 1.0),
    ("hScale", "HorizontalScale", 100.0),
    ("vScale", "VerticalScale", 100.0),
    ("baselineShift", "BaselineShift", 1.0),
    ("skew", "Skew", 1.0),
    ("fillTint", "FillTint", 100.0),
    ("strokeWeight", "StrokeWeight", 1.0),
];

const PARA_SIMPLE: &[(&str, &str, f64)] = &[
    ("leftIndent", "LeftIndent", 1.0),
    ("rightIndent", "RightIndent", 1.0),
    ("firstIndent", "FirstLineIndent", 1.0),
    ("lastIndent", "LastLineIndent", 1.0),
    ("spaceBefore", "SpaceBefore", 1.0),
    ("spaceAfter", "SpaceAfter", 1.0),
    ("autoLeading", "AutoLeading", 100.0),
    ("dropLines", "DropCapLines", 1.0),
    ("dropChars", "DropCapCharacters", 1.0),
    ("hyphMinWord", "HyphenateWordsLongerThan", 1.0),
    ("hyphAfter", "HyphenateAfterFirst", 1.0),
    ("hyphBefore", "HyphenateBeforeLast", 1.0),
    ("hyphLimit", "HyphenateLadderLimit", 1.0),
    ("hyphZone", "HyphenationZone", 1.0),
    ("keepNext", "KeepWithNext", 1.0),
    ("keepFirst", "KeepFirstLines", 1.0),
    ("keepLast", "KeepLastLines", 1.0),
    ("startAt", "NumberingStartAt", 1.0),
];

const PARA_BOOL: &[(&str, &str)] = &[("hyphenate", "Hyphenation"), ("hyphCaps", "HyphenateCapitalizedWords"), ("keepTogether", "KeepAllLinesTogether"), ("balance", "BalanceRaggedLines")];

fn attr_str(v: &Value) -> Option<String> {
    v.as_str().map(str::to_string)
}

fn char_attrs(doc: &Doc, a: &Attrs, colors: &mut Colors) -> (String, String) {
    let mut at = String::new();
    let mut props = String::new();
    for (k, idml, f) in CHAR_SIMPLE {
        if let Some(v) = a.get(*k).and_then(Value::as_f64) {
            let _ = write!(at, " {idml}=\"{}\"", num(v * f));
        }
    }
    if let Some(v) = a.get("kerning").and_then(Value::as_str) {
        let m = match v {
            "optical" => "$ID/Optical",
            "none" => "$ID/None",
            _ => "$ID/Metrics",
        };
        let _ = write!(at, " KerningMethod=\"{m}\"");
    }
    if let Some(v) = a.get("fontStyle").and_then(attr_str) {
        let _ = write!(at, " FontStyle=\"{}\"", esc(&v));
    }
    for (k, idml) in [("underline", "Underline"), ("strike", "StrikeThru"), ("ligatures", "Ligatures"), ("noBreak", "NoBreak")] {
        if let Some(v) = a.get(k).and_then(Value::as_bool) {
            let _ = write!(at, " {idml}=\"{v}\"");
        }
    }
    if let Some(v) = a.get("caps").and_then(Value::as_str) {
        let c = match v {
            "allCaps" => "AllCaps",
            "smallCaps" => "SmallCaps",
            "allSmallCaps" => "CapToSmallCap",
            _ => "Normal",
        };
        let _ = write!(at, " Capitalization=\"{c}\"");
    }
    if let Some(v) = a.get("position").and_then(Value::as_str) {
        let c = match v {
            "superscript" => "Superscript",
            "subscript" => "Subscript",
            "numerator" => "OTNumerator",
            "denominator" => "OTDenominator",
            _ => "Normal",
        };
        let _ = write!(at, " Position=\"{c}\"");
    }
    for (k, idml) in [("fill", "FillColor"), ("stroke", "StrokeColor")] {
        if let Some(p) = a.get(k).and_then(Paint::from_value) {
            let (r, _) = colors.paint_ref(doc, &p);
            let _ = write!(at, " {idml}=\"{}\"", esc(&r));
        }
    }
    if let Some(v) = a.get("language").and_then(Value::as_str) {
        let _ = write!(at, " AppliedLanguage=\"$ID/{}\"", esc(v).replace(':', "%3a"));
    }
    if let Some(v) = a.get("font").and_then(Value::as_str) {
        let _ = write!(props, "<AppliedFont type=\"string\">{}</AppliedFont>", esc(v));
    }
    match a.get("leading") {
        Some(Value::Number(n)) => {
            let _ = write!(props, "<Leading type=\"unit\">{}</Leading>", num(n.as_f64().unwrap_or(0.0)));
        }
        Some(Value::String(s)) if s == "auto" => props.push_str("<Leading type=\"enumeration\">Auto</Leading>"),
        _ => {}
    }
    (at, props)
}

fn para_attrs(doc: &Doc, a: &Attrs, colors: &mut Colors) -> (String, String) {
    let (mut at, mut props) = char_attrs(doc, a, colors);
    for (k, idml, f) in PARA_SIMPLE {
        if let Some(v) = a.get(*k).and_then(Value::as_f64) {
            let _ = write!(at, " {idml}=\"{}\"", num(v * f));
        }
    }
    for (k, idml) in PARA_BOOL {
        if let Some(v) = a.get(*k).and_then(Value::as_bool) {
            let _ = write!(at, " {idml}=\"{v}\"");
        }
    }
    if let Some(v) = a.get("align").and_then(Value::as_str) {
        let j = match v {
            "center" => "CenterAlign",
            "right" => "RightAlign",
            "justify" => "LeftJustified",
            "justifyCenter" => "CenterJustified",
            "justifyRight" => "RightJustified",
            "justifyAll" => "FullyJustified",
            "towardsSpine" => "ToBindingSide",
            "awayFromSpine" => "AwayFromBindingSide",
            _ => "LeftAlign",
        };
        let _ = write!(at, " Justification=\"{j}\"");
    }
    if let Some(v) = a.get("start").and_then(Value::as_str) {
        let s = match v {
            "nextColumn" => "NextColumn",
            "nextFrame" => "NextFrame",
            "nextPage" => "NextPage",
            "nextOddPage" => "NextOddPage",
            "nextEvenPage" => "NextEvenPage",
            _ => "Anywhere",
        };
        let _ = write!(at, " StartParagraph=\"{s}\"");
    }
    for (k, prefix) in [("wordSpace", "WordSpacing"), ("letterSpace", "LetterSpacing"), ("glyphScale", "GlyphScaling")] {
        if let Some(arr) = a.get(k).and_then(Value::as_array) {
            let n: Vec<f64> = arr.iter().filter_map(Value::as_f64).collect();
            if n.len() == 3 {
                let f = 100.0;
                let _ = write!(at, " Minimum{prefix}=\"{}\" Desired{prefix}=\"{}\" Maximum{prefix}=\"{}\"", num(n[0] * f), num(n[1] * f), num(n[2] * f));
            }
        }
    }
    if let Some(v) = a.get("spanColumns").and_then(Value::as_i64)
        && v != 0
    {
        let count = if v < 0 { "All" } else { "" };
        let _ = write!(at, " SpanColumnType=\"SpanColumns\" SpanSplitColumnCount=\"{}\"", if count.is_empty() { v.to_string() } else { count.to_string() });
    }
    if let Some(v) = a.get("splitColumns").and_then(Value::as_i64)
        && v > 1
    {
        let _ = write!(at, " SpanColumnType=\"SplitColumns\" SpanSplitColumnCount=\"{v}\"");
    }
    if let Some(v) = a.get("gridAlign").and_then(Value::as_bool) {
        let _ = write!(at, " GridAlignment=\"{}\"", if v { "AlignToBaseline" } else { "None" });
    }
    if let Some(v) = a.get("opticalMargin").and_then(Value::as_bool) {
        let _ = write!(at, " IgnoreEdgeAlignment=\"{}\"", !v);
    }
    if let Some(v) = a.get("list").and_then(Value::as_str) {
        let t = match v {
            "bullet" => "BulletList",
            "number" => "NumberedList",
            _ => "NoList",
        };
        let _ = write!(at, " BulletsAndNumberingListType=\"{t}\"");
    }
    if let Some(v) = a.get("numberFormat").and_then(Value::as_str) {
        let _ = write!(at, " NumberingExpression=\"{}\"", esc(v));
    }
    for (k, pre) in [("ruleAbove", "RuleAbove"), ("ruleBelow", "RuleBelow")] {
        if let Some(o) = a.get(k).and_then(Value::as_object) {
            let n = |key: &str, d: f64| o.get(key).and_then(Value::as_f64).unwrap_or(d);
            let color = o.get("color").and_then(Paint::from_value).unwrap_or(Paint::swatch("Black"));
            let (cr, _) = colors.paint_ref(doc, &color);
            let _ = write!(
                at,
                " {pre}=\"true\" {pre}LineWeight=\"{}\" {pre}Color=\"{}\" {pre}Offset=\"{}\" {pre}LeftIndent=\"{}\" {pre}RightIndent=\"{}\" {pre}Width=\"{}\"",
                num(n("weight", 1.0)),
                esc(&cr),
                num(n("offset", 0.0)),
                num(n("left", 0.0)),
                num(n("right", 0.0)),
                if o.get("width").and_then(Value::as_str) == Some("text") { "TextWidth" } else { "ColumnWidth" }
            );
        }
    }
    if let Some(tabs) = a.get("tabs").and_then(Value::as_array) {
        props.push_str("<TabList type=\"list\">");
        for t in tabs {
            let pos = t.get("position").and_then(Value::as_f64).unwrap_or(0.0);
            let al = match t.get("align").and_then(Value::as_str).unwrap_or("left") {
                "center" => "CenterAlign",
                "right" => "RightAlign",
                "decimal" => "CharacterAlign",
                _ => "LeftAlign",
            };
            let leader = t.get("leader").and_then(Value::as_str).unwrap_or("");
            let on = t.get("alignOn").and_then(Value::as_str).unwrap_or(".");
            let _ = write!(
                props,
                "<ListItem type=\"record\"><Alignment type=\"enumeration\">{al}</Alignment><AlignmentCharacter type=\"string\">{}</AlignmentCharacter><Leader type=\"string\">{}</Leader><Position type=\"unit\">{}</Position></ListItem>",
                esc(on),
                esc(leader),
                num(pos)
            );
        }
        props.push_str("</TabList>");
    }
    if let Some(v) = a.get("bullet").and_then(Value::as_str)
        && let Some(c) = v.chars().next()
    {
        let _ = write!(props, "<BulletChar BulletCharacterType=\"UnicodeOnly\" BulletCharacterValue=\"{}\"/>", c as u32);
    }
    (at, props)
}

struct W<'a> {
    doc: &'a Doc,
    colors: Colors,
    stories: Vec<StoryId>,
}

fn props_wrap(p: &str) -> String {
    if p.is_empty() { String::new() } else { format!("<Properties>{p}</Properties>") }
}

impl W<'_> {
    fn item(&mut self, id: ItemId, place: Affine, out: &mut String) {
        let doc = self.doc;
        let Some(it) = doc.item(id) else { return };
        let xf = place * it.xf;
        let common = self.common(it, xf);
        match &it.kind {
            ItemKind::Group { children } => {
                let _ = write!(out, "<Group{common}>");
                for c in children {
                    self.item(*c, Affine::IDENTITY, out);
                }
                out.push_str("</Group>");
            }
            ItemKind::States { states, active } => {
                let _ = write!(out, "<Group{common}>");
                if let Some(s) = states.get(*active) {
                    for c in &s.items {
                        self.item(*c, Affine::IDENTITY, out);
                    }
                }
                out.push_str("</Group>");
            }
            ItemKind::Text(tf) => {
                let frames = doc.story(tf.story).map(|s| s.frames.clone()).unwrap_or_default();
                let at = frames.iter().position(|f| *f == id).unwrap_or(0);
                let prev = at.checked_sub(1).and_then(|i| frames.get(i)).map(|f| uid(f.0)).unwrap_or_else(|| "n".into());
                let next = frames.get(at + 1).map(|f| uid(f.0)).unwrap_or_else(|| "n".into());
                if at == 0 && !self.stories.contains(&tf.story) {
                    self.stories.push(tf.story);
                }
                let geo = geometry(&it.shape.to_path(&it.corner));
                let vj = match tf.vjust {
                    VJust::Center => "CenterAlign",
                    VJust::Bottom => "BottomAlign",
                    VJust::Justify => "JustifyAlign",
                    VJust::Top => "TopAlign",
                };
                let fb = match tf.first_baseline {
                    FirstBaseline::CapHeight => "CapHeight",
                    FirstBaseline::XHeight => "XHeight",
                    FirstBaseline::Leading => "LeadingOffset",
                    FirstBaseline::Fixed => "FixedHeight",
                    FirstBaseline::Ascent => "AscentOffset",
                };
                let auto = match tf.autosize {
                    AutoSize::Height => "HeightOnly",
                    AutoSize::Width => "WidthOnly",
                    AutoSize::Both => "HeightAndWidth",
                    AutoSize::Off => "Off",
                };
                let i = &tf.insets;
                let _ = write!(
                    out,
                    "<TextFrame{common} ParentStory=\"{}\" PreviousTextFrame=\"{prev}\" NextTextFrame=\"{next}\" ContentType=\"TextType\">{geo}{}<TextFramePreference TextColumnCount=\"{}\" TextColumnGutter=\"{}\" InsetSpacing=\"{} {} {} {}\" VerticalJustification=\"{vj}\" FirstBaselineOffset=\"{fb}\" MinimumFirstBaselineOffset=\"{}\" AutoSizingType=\"{auto}\" IgnoreWrap=\"{}\"/>",
                    uid(tf.story.0),
                    self.wrap(it),
                    tf.columns,
                    num(tf.gutter),
                    num(i.top),
                    num(i.inside),
                    num(i.bottom),
                    num(i.outside),
                    num(tf.first_baseline_min),
                    tf.ignore_wrap
                );
                if let Some(pt) = &tf.on_path {
                    let _ = write!(out, "<TextPath Self=\"{}tp\" ParentStory=\"{}\" StartBracket=\"{}\" FlipPathEffect=\"{}\" PathAlignment=\"{}\"/>", uid(id.0), uid(tf.story.0), num(pt.start), if pt.flip { "Flipped" } else { "NotFlipped" }, esc(&pt.align));
                }
                out.push_str("</TextFrame>");
            }
            ItemKind::Graphic(g) => {
                let geo = geometry(&it.shape.to_path(&it.corner));
                let _ = write!(out, "<Rectangle{common} ContentType=\"GraphicType\">{geo}{}", self.wrap(it));
                if let Some(aid) = g.asset
                    && let Some(a) = doc.assets.get(&aid)
                {
                    let (w, h) = a.natural_size();
                    let pdf = a.mime == "application/pdf";
                    let tag = if pdf { "PDF" } else { "Image" };
                    let uri = a.link.clone().unwrap_or_else(|| format!("file:{}", a.name));
                    let contents = if a.data.is_empty() { String::new() } else { format!("<Contents><![CDATA[{}]]></Contents>", b64(&a.data)) };
                    let _ = write!(
                        out,
                        "<{tag} Self=\"{}i\" ItemTransform=\"{}\"><Properties><GraphicBounds Left=\"0\" Top=\"0\" Right=\"{}\" Bottom=\"{}\"/>{contents}</Properties><Link Self=\"{}l\" LinkResourceURI=\"{}\" StoredState=\"{}\"/></{tag}>",
                        uid(id.0),
                        matrix(g.content),
                        num(w),
                        num(h),
                        uid(id.0),
                        esc(&uri),
                        if contents.is_empty() { "Normal" } else { "Embedded" }
                    );
                }
                out.push_str("</Rectangle>");
            }
            ItemKind::Plain => {
                let tag = match &it.shape {
                    Shape::Ellipse { .. } => "Oval",
                    Shape::Polygon { .. } => "Polygon",
                    Shape::Line { .. } => "GraphicLine",
                    Shape::Path { closed: false, .. } => "GraphicLine",
                    Shape::Path { .. } => "Polygon",
                    Shape::Rect { .. } => "Rectangle",
                };
                let geo = geometry(&it.shape.to_path(&it.corner));
                let _ = write!(out, "<{tag}{common}>{geo}{}</{tag}>", self.wrap(it));
            }
        }
    }

    fn wrap(&self, it: &Item) -> String {
        let Some(w) = &it.wrap else { return String::new() };
        let mode = match w.mode {
            WrapMode::BoundingBox => "BoundingBoxTextWrap",
            WrapMode::Contour => "Contour",
            WrapMode::JumpObject => "JumpObjectTextWrap",
            WrapMode::NextColumn => "NextColumnTextWrap",
            WrapMode::None => "None",
        };
        let side = match w.side.as_str() {
            "left" => "LeftSide",
            "right" => "RightSide",
            "largest" => "LargestArea",
            _ => "BothSides",
        };
        let o = &w.offsets;
        format!(
            "<TextWrapPreference Inverse=\"{}\" TextWrapSide=\"{side}\" TextWrapMode=\"{mode}\"><Properties><TextWrapOffset Top=\"{}\" Left=\"{}\" Bottom=\"{}\" Right=\"{}\"/></Properties></TextWrapPreference>",
            w.invert,
            num(o.top),
            num(o.inside),
            num(o.bottom),
            num(o.outside)
        )
    }

    fn common(&mut self, it: &Item, xf: Affine) -> String {
        let (fill, ft) = self.colors.paint_ref(self.doc, &it.fill);
        let (stroke, st) = self.colors.paint_ref(self.doc, &it.stroke.paint);
        let align = match it.stroke.align {
            StrokeAlign::Inside => "InsideAlignment",
            StrokeAlign::Outside => "OutsideAlignment",
            StrokeAlign::Center => "CenterAlignment",
        };
        let mut s = format!(
            " Self=\"{}\" ItemLayer=\"{}\" ItemTransform=\"{}\" FillColor=\"{}\" StrokeColor=\"{}\" StrokeWeight=\"{}\" StrokeAlignment=\"{align}\" Visible=\"{}\" Locked=\"{}\" Nonprinting=\"{}\"",
            uid(it.id.0),
            uid(it.layer.0),
            matrix(xf),
            esc(&fill),
            esc(&stroke),
            num(it.stroke.weight),
            !it.hidden,
            it.locked,
            it.nonprinting
        );
        if let Some(t) = ft {
            let _ = write!(s, " FillTint=\"{}\"", num(t));
        }
        if let Some(t) = st {
            let _ = write!(s, " StrokeTint=\"{}\"", num(t));
        }
        if !it.name.is_empty() {
            let _ = write!(s, " Name=\"{}\"", esc(&it.name));
        }
        if let Some(os) = &it.style {
            let _ = write!(s, " AppliedObjectStyle=\"{}\"", esc(&style_ref(StyleKind::Object, os)));
        }
        if it.corner.radius > 0.0 && matches!(it.shape, Shape::Rect { .. }) {
            let opt = match it.corner.kind.as_str() {
                "bevel" => "BevelCorner",
                "inset" => "InsetCorner",
                "inverse" => "InverseRoundedCorner",
                "fancy" => "FancyCorner",
                _ => "RoundedCorner",
            };
            let _ = write!(s, " TopLeftCornerOption=\"{opt}\" TopRightCornerOption=\"{opt}\" BottomLeftCornerOption=\"{opt}\" BottomRightCornerOption=\"{opt}\" TopLeftCornerRadius=\"{r}\" TopRightCornerRadius=\"{r}\" BottomLeftCornerRadius=\"{r}\" BottomRightCornerRadius=\"{r}\"", r = num(it.corner.radius));
        }
        s
    }

    fn para(&mut self, p: &Para, out: &mut String, last: bool) {
        let (pa, pp) = para_attrs(self.doc, &p.over, &mut self.colors);
        let _ = write!(out, "<ParagraphStyleRange AppliedParagraphStyle=\"{}\"{pa}>{}", esc(&style_ref(StyleKind::Paragraph, &p.style)), props_wrap(&pp));
        let n = p.segs.len();
        for (i, seg) in p.segs.iter().enumerate() {
            let (ca, cp) = char_attrs(self.doc, seg.over(), &mut self.colors);
            let cs = seg.style().unwrap_or(NO_CHAR);
            let _ = write!(out, "<CharacterStyleRange AppliedCharacterStyle=\"{}\"{ca}>{}", esc(&style_ref(StyleKind::Character, cs)), props_wrap(&cp));
            match seg {
                Seg::Text { text, .. } => {
                    for (k, part) in text.split('\u{2028}').enumerate() {
                        if k > 0 {
                            out.push_str("<Br/>");
                        }
                        if !part.is_empty() {
                            let _ = write!(out, "<Content>{}</Content>", esc(part));
                        }
                    }
                }
                Seg::Inline { inline, .. } => self.inline(inline, out),
            }
            if last || i + 1 < n {
                out.push_str("</CharacterStyleRange>");
            } else {
                out.push_str("<Br/></CharacterStyleRange>");
            }
        }
        if n == 0 && !last {
            out.push_str("<CharacterStyleRange><Br/></CharacterStyleRange>");
        }
        out.push_str("</ParagraphStyleRange>");
    }

    fn inline(&mut self, inline: &Inline, out: &mut String) {
        match inline {
            Inline::Anchored { item, .. } => self.item(*item, Affine::IDENTITY, out),
            Inline::Footnote { paras } => {
                out.push_str("<Footnote>");
                let n = paras.len();
                for (i, p) in paras.iter().enumerate() {
                    self.para(p, out, i + 1 == n);
                }
                out.push_str("</Footnote>");
            }
            Inline::Table { table } => self.table(table, out),
            Inline::Variable { name } => {
                let _ = write!(out, "<TextVariableInstance Self=\"tv{}\" Name=\"{}\" ResultText=\"{}\" AssociatedTextVariable=\"dTextVariable{}\"/>", out.len(), esc(name), esc(&self.doc.variable_text(name, None)), esc(name));
            }
            Inline::Marker { marker } => {
                let n = match marker {
                    Marker::PageNumber => 18,
                    Marker::SectionName => 19,
                    Marker::NextPageNumber => 18,
                    Marker::PrevPageNumber => 18,
                };
                let _ = write!(out, "<Content><?ACE {n}?></Content>");
            }
            Inline::Break { .. } => out.push_str("<Br/>"),
            Inline::IndentHere => out.push_str("<Content><?ACE 7?></Content>"),
            Inline::RightTab => out.push_str("<Content><?ACE 8?></Content>"),
            Inline::Note { author, text, .. } => {
                let _ = write!(out, "<Note UserName=\"{}\"><ParagraphStyleRange><CharacterStyleRange><Content>{}</Content></CharacterStyleRange></ParagraphStyleRange></Note>", esc(author), esc(text));
            }
            Inline::Xref { text, .. } if !text.is_empty() => {
                let _ = write!(out, "<Content>{}</Content>", esc(text));
            }
            Inline::Index { .. } | Inline::Anchor { .. } | Inline::Xref { .. } => {}
        }
    }

    fn table(&mut self, t: &Table, out: &mut String) {
        let _ = write!(out, "<Table Self=\"t{}\" HeaderRowCount=\"{}\" FooterRowCount=\"{}\" BodyRowCount=\"{}\" ColumnCount=\"{}\"", out.len(), t.header_rows, t.footer_rows, t.rows().saturating_sub(t.header_rows + t.footer_rows), t.cols());
        if let Some(s) = &t.style {
            let _ = write!(out, " AppliedTableStyle=\"{}\"", esc(&style_ref(StyleKind::Table, s)));
        }
        out.push('>');
        for (r, h) in t.heights.iter().enumerate() {
            let exact = t.exact.get(r).copied().unwrap_or(false);
            let _ = write!(out, "<Row Name=\"{r}\" SingleRowHeight=\"{}\" AutoGrow=\"{}\"/>", num(*h), !exact);
        }
        for (c, w) in t.widths.iter().enumerate() {
            let _ = write!(out, "<Column Name=\"{c}\" SingleColumnWidth=\"{}\"/>", num(*w));
        }
        for cell in &t.cells {
            let _ = write!(out, "<Cell Name=\"{}:{}\" RowSpan=\"{}\" ColumnSpan=\"{}\"", cell.col, cell.row, cell.rows, cell.cols);
            if let Some(s) = &cell.style {
                let _ = write!(out, " AppliedCellStyle=\"{}\"", esc(&style_ref(StyleKind::Cell, s)));
            }
            if let Some(p) = cell.attrs.get("fill").and_then(Paint::from_value) {
                let (r, _) = self.colors.paint_ref(self.doc, &p);
                let _ = write!(out, " FillColor=\"{}\"", esc(&r));
            }
            if let Some(strokes) = cell.attrs.get("strokes").and_then(|s| s.as_object()) {
                for (key, edge) in [("top", "Top"), ("bottom", "Bottom"), ("left", "Left"), ("right", "Right")] {
                    let Some(o) = strokes.get(key).and_then(|s| s.as_object()) else { continue };
                    if let Some(p) = o.get("color").and_then(Paint::from_value) {
                        let (r, _) = self.colors.paint_ref(self.doc, &p);
                        let _ = write!(out, " {edge}EdgeStrokeColor=\"{}\"", esc(&r));
                    }
                    if let Some(w) = o.get("weight").and_then(|v| v.as_f64()) {
                        let _ = write!(out, " {edge}EdgeStrokeWeight=\"{}\"", num(w));
                    }
                    if let Some(t) = o.get("tint").and_then(|v| v.as_f64()) {
                        let _ = write!(out, " {edge}EdgeStrokeTint=\"{}\"", num(t * 100.0));
                    }
                    if let Some(k) = o.get("type").and_then(|v| v.as_str()) {
                        let name = match k {
                            "dashed" => "Dashed",
                            "dotted" => "Dotted",
                            "none" => "None",
                            _ => "Solid",
                        };
                        let _ = write!(out, " {edge}EdgeStrokeType=\"StrokeStyle/$ID/{name}\"");
                    }
                }
            }
            out.push('>');
            let n = cell.paras.len();
            for (i, p) in cell.paras.iter().enumerate() {
                self.para(p, out, i + 1 == n);
            }
            out.push_str("</Cell>");
        }
        out.push_str("</Table>");
    }
}

fn styles_xml(doc: &Doc, colors: &mut Colors) -> String {
    let mut s = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<idPkg:Styles xmlns:idPkg=\"{NS}\" DOMVersion=\"{DOM}\">");
    s.push_str("<RootCharacterStyleGroup Self=\"u79\"><CharacterStyle Self=\"CharacterStyle/$ID/[No character style]\" Imported=\"false\" Name=\"$ID/[No character style]\"/>");
    for st in doc.styles.list(StyleKind::Character) {
        if st.name == NO_CHAR {
            continue;
        }
        let (a, p) = char_attrs(doc, &st.attrs, colors);
        let based = st.based_on.as_ref().map(|b| format!("<BasedOn type=\"object\">{}</BasedOn>", esc(&style_ref(StyleKind::Character, b)))).unwrap_or_default();
        let _ = write!(s, "<CharacterStyle Self=\"{}\" Name=\"{}\"{a}>{}</CharacterStyle>", esc(&style_ref(StyleKind::Character, &st.name)), esc(&st.name), props_wrap(&format!("{p}{based}")));
    }
    s.push_str("</RootCharacterStyleGroup>");
    s.push_str("<RootParagraphStyleGroup Self=\"u78\"><ParagraphStyle Self=\"ParagraphStyle/$ID/[No paragraph style]\" Name=\"$ID/[No paragraph style]\"/>");
    let has_basic = doc.styles.find(StyleKind::Paragraph, BASIC_PARA).is_some();
    if !has_basic {
        s.push_str("<ParagraphStyle Self=\"ParagraphStyle/$ID/NormalParagraphStyle\" Name=\"$ID/NormalParagraphStyle\"/>");
    }
    for st in doc.styles.list(StyleKind::Paragraph) {
        let (a, p) = para_attrs(doc, &st.attrs, colors);
        let based = st.based_on.as_ref().map(|b| format!("<BasedOn type=\"object\">{}</BasedOn>", esc(&style_ref(StyleKind::Paragraph, b)))).unwrap_or_default();
        let next = st.next.as_ref().map(|n| format!(" NextStyle=\"{}\"", esc(&style_ref(StyleKind::Paragraph, n)))).unwrap_or_default();
        let _ = write!(s, "<ParagraphStyle Self=\"{}\" Name=\"{}\"{a}{next}>{}</ParagraphStyle>", esc(&style_ref(StyleKind::Paragraph, &st.name)), esc(&style_name(StyleKind::Paragraph, &st.name)), props_wrap(&format!("{p}{based}")));
    }
    s.push_str("</RootParagraphStyleGroup>");
    s.push_str("<RootCellStyleGroup Self=\"u8a\"><CellStyle Self=\"CellStyle/$ID/[None]\" Name=\"$ID/[None]\"/>");
    for st in doc.styles.list(StyleKind::Cell) {
        let _ = write!(s, "<CellStyle Self=\"{}\" Name=\"{}\"/>", esc(&style_ref(StyleKind::Cell, &st.name)), esc(&st.name));
    }
    s.push_str("</RootCellStyleGroup><RootTableStyleGroup Self=\"u8b\"><TableStyle Self=\"TableStyle/$ID/[No table style]\" Name=\"$ID/[No table style]\"/><TableStyle Self=\"TableStyle/$ID/[Basic Table]\" Name=\"$ID/[Basic Table]\"/>");
    for st in doc.styles.list(StyleKind::Table) {
        let _ = write!(s, "<TableStyle Self=\"{}\" Name=\"{}\"/>", esc(&style_ref(StyleKind::Table, &st.name)), esc(&st.name));
    }
    s.push_str("</RootTableStyleGroup><RootObjectStyleGroup Self=\"u93\"><ObjectStyle Self=\"ObjectStyle/$ID/[None]\" Name=\"$ID/[None]\"/><ObjectStyle Self=\"ObjectStyle/$ID/[Normal Graphics Frame]\" Name=\"$ID/[Normal Graphics Frame]\"/><ObjectStyle Self=\"ObjectStyle/$ID/[Normal Text Frame]\" Name=\"$ID/[Normal Text Frame]\"/>");
    for st in doc.styles.list(StyleKind::Object) {
        let _ = write!(s, "<ObjectStyle Self=\"{}\" Name=\"{}\"/>", esc(&style_ref(StyleKind::Object, &st.name)), esc(&st.name));
    }
    s.push_str("</RootObjectStyleGroup></idPkg:Styles>");
    s
}

fn graphic_xml(doc: &Doc, colors: &Colors) -> String {
    let mut s = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<idPkg:Graphic xmlns:idPkg=\"{NS}\" DOMVersion=\"{DOM}\">");
    for sw in &doc.swatches {
        match &sw.value {
            SwatchValue::Color { color } => {
                let (space, v) = color_value(*color);
                let model = if sw.name == "Registration" { "Registration" } else if sw.spot { "Spot" } else { "Process" };
                let _ = write!(s, "<Color Self=\"Color/{}\" Model=\"{model}\" Space=\"{space}\" ColorValue=\"{v}\" Name=\"{}\" ColorEditable=\"{}\" ColorRemovable=\"{}\"/>", esc(&sw.name), esc(&sw.name), !sw.locked, !sw.locked);
            }
            SwatchValue::Tint { base, tint } => {
                let _ = write!(s, "<Tint Self=\"Tint/{}\" BaseColor=\"Color/{}\" TintValue=\"{}\" Name=\"{}\"/>", esc(&sw.name), esc(base), num(tint * 100.0), esc(&sw.name));
            }
            SwatchValue::Gradient { gradient } => {
                let _ = write!(s, "<Gradient Self=\"Gradient/{}\" Type=\"{}\" Name=\"{}\">", esc(&sw.name), if gradient.radial { "Radial" } else { "Linear" }, esc(&sw.name));
                for st in &gradient.stops {
                    let (space, v) = color_value(st.color);
                    let _ = write!(s, "<GradientStop StopColor=\"{space}:{v}\" Location=\"{}\" Midpoint=\"50\"/>", num(st.at * 100.0));
                }
                s.push_str("</Gradient>");
            }
        }
    }
    for (key, self_) in &colors.unnamed {
        let (space, v) = key.split_once(':').unwrap_or(("CMYK", "0 0 0 100"));
        let _ = write!(s, "<Color Self=\"{}\" Model=\"Process\" Space=\"{space}\" ColorValue=\"{v}\" Name=\"$ID/\" ColorEditable=\"true\" ColorRemovable=\"true\" Visible=\"false\"/>", esc(self_));
    }
    s.push_str("<Swatch Self=\"Swatch/None\" Name=\"None\" ColorEditable=\"false\" ColorRemovable=\"false\"/></idPkg:Graphic>");
    s
}

fn preferences_xml(doc: &Doc) -> String {
    let st = &doc.settings;
    let b = &st.bleed;
    let m = &st.margins;
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<idPkg:Preferences xmlns:idPkg=\"{NS}\" DOMVersion=\"{DOM}\"><DocumentPreference PageHeight=\"{}\" PageWidth=\"{}\" PagesPerDocument=\"{}\" FacingPages=\"{}\" DocumentBleedTopOffset=\"{}\" DocumentBleedBottomOffset=\"{}\" DocumentBleedInsideOrLeftOffset=\"{}\" DocumentBleedOutsideOrRightOffset=\"{}\" SlugTopOffset=\"{}\" SlugBottomOffset=\"{}\" SlugInsideOrLeftOffset=\"{}\" SlugRightOrOutsideOffset=\"{}\" Intent=\"{}\"/><MarginPreference ColumnCount=\"{}\" ColumnGutter=\"{}\" Top=\"{}\" Bottom=\"{}\" Left=\"{}\" Right=\"{}\"/><GridPreference BaselineStart=\"{}\" BaselineDivision=\"{}\" BaselineGridRelativeOption=\"{}\"/></idPkg:Preferences>",
        num(st.page_height),
        num(st.page_width),
        doc.page_count(),
        st.facing,
        num(b.top),
        num(b.bottom),
        num(b.inside),
        num(b.outside),
        num(st.slug.top),
        num(st.slug.bottom),
        num(st.slug.inside),
        num(st.slug.outside),
        match st.intent {
            formdes_model::doc::Intent::Print => "PrintIntent",
            _ => "WebIntent",
        },
        st.columns,
        num(st.gutter),
        num(m.top),
        num(m.bottom),
        num(m.inside),
        num(m.outside),
        num(st.baseline.start),
        num(st.baseline.increment),
        if st.baseline.from_margin { "TopOfMarginOfBaselineGridRelativeOption" } else { "TopOfPageOfBaselineGridRelativeOption" }
    )
}

fn spine_x(doc: &Doc, pages: &[formdes_model::page::Page], first_abs: Option<usize>) -> f64 {
    if !doc.settings.facing {
        return 0.0;
    }
    match pages.len() {
        1 => match first_abs {
            Some(a) if doc.is_right_page(a) => 0.0,
            _ => pages[0].width,
        },
        _ => pages[0].width,
    }
}

pub fn write(doc: &Doc) -> Result<Vec<u8>, String> {
    let mut w = W { doc, colors: Colors { unnamed: BTreeMap::new() }, stories: Vec::new() };
    let mut files: Vec<(String, String)> = Vec::new();
    let mut masters = Vec::new();
    for (pi, parent) in doc.parents.iter().enumerate() {
        let h = parent.pages.iter().map(|p| p.height).fold(0.0, f64::max);
        let spine = spine_x(doc, &parent.pages, None);
        let t = Affine::translate((-spine, -h / 2.0));
        let mut x = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<idPkg:MasterSpread xmlns:idPkg=\"{NS}\" DOMVersion=\"{DOM}\"><MasterSpread Self=\"{}\" NamePrefix=\"{}\" BaseName=\"{}\" Name=\"{}\" PageCount=\"{}\" ItemTransform=\"1 0 0 1 0 0\"{}>",
            uid(parent.id.0),
            esc(&parent.prefix),
            esc(&parent.name),
            esc(&parent.label()),
            parent.pages.len(),
            parent.based_on.map(|b| format!(" BasedOn=\"{}\"", uid(b.0))).unwrap_or_default()
        );
        for (k, p) in parent.pages.iter().enumerate() {
            let px = parent.page_x(k);
            let _ = write!(x, "<Page Self=\"{}\" Name=\"{}\" GeometricBounds=\"0 0 {} {}\" ItemTransform=\"{}\"><MarginPreference ColumnCount=\"{}\" ColumnGutter=\"{}\" Top=\"{}\" Bottom=\"{}\" Left=\"{}\" Right=\"{}\"/></Page>", uid(p.id.0), esc(&parent.prefix), num(p.height), num(p.width), matrix(t * Affine::translate((px, 0.0))), p.columns, num(p.gutter), num(p.margins.top), num(p.margins.bottom), num(p.margins.inside), num(p.margins.outside));
        }
        for id in &parent.items {
            w.item(*id, t, &mut x);
        }
        x.push_str("</MasterSpread></idPkg:MasterSpread>");
        let name = format!("MasterSpreads/MasterSpread_{}.xml", uid(parent.id.0));
        masters.push(name.clone());
        files.push((name, x));
        let _ = pi;
    }
    let mut spreads = Vec::new();
    for (si, s) in doc.spreads.iter().enumerate() {
        let first = doc.first_page_of_spread(si);
        let spine = spine_x(doc, &s.pages, Some(first));
        let t = Affine::translate((-spine, -s.height() / 2.0));
        let mut x = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<idPkg:Spread xmlns:idPkg=\"{NS}\" DOMVersion=\"{DOM}\"><Spread Self=\"{}\" PageCount=\"{}\" BindingLocation=\"{}\" ItemTransform=\"1 0 0 1 0 0\">", uid(s.id.0), s.pages.len(), if spine > 0.0 { 1 } else { 0 });
        for (k, p) in s.pages.iter().enumerate() {
            let px = s.page_x(k);
            let master = p.parent.map(|m| uid(m.0)).unwrap_or_else(|| "n".into());
            let overrides = p.detached.iter().map(|i| uid(i.0)).collect::<Vec<_>>().join(" ");
            let _ = write!(
                x,
                "<Page Self=\"{}\" Name=\"{}\" AppliedMaster=\"{master}\" OverrideList=\"{overrides}\" GeometricBounds=\"0 0 {} {}\" ItemTransform=\"{}\"><MarginPreference ColumnCount=\"{}\" ColumnGutter=\"{}\" Top=\"{}\" Bottom=\"{}\" Left=\"{}\" Right=\"{}\"/></Page>",
                uid(p.id.0),
                esc(&doc.page_name(first + k)),
                num(p.height),
                num(p.width),
                matrix(t * Affine::translate((px, 0.0))),
                p.columns,
                num(p.gutter),
                num(p.margins.top),
                num(p.margins.bottom),
                num(p.margins.inside),
                num(p.margins.outside)
            );
        }
        for id in &s.items {
            w.item(*id, t, &mut x);
        }
        x.push_str("</Spread></idPkg:Spread>");
        let name = format!("Spreads/Spread_{}.xml", uid(s.id.0));
        spreads.push(name.clone());
        files.push((name, x));
    }
    for sid in doc.stories.keys() {
        if !w.stories.contains(sid) {
            w.stories.push(*sid);
        }
    }
    let mut story_files = Vec::new();
    let ids = w.stories.clone();
    for sid in ids {
        let Some(st) = doc.story(sid) else { continue };
        let mut x = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<idPkg:Story xmlns:idPkg=\"{NS}\" DOMVersion=\"{DOM}\"><Story Self=\"{}\" AppliedTOCStyle=\"n\" TrackChanges=\"false\" StoryTitle=\"$ID/\">", uid(sid.0));
        let n = st.paras.len();
        for (i, p) in st.paras.iter().enumerate() {
            w.para(p, &mut x, i + 1 == n);
        }
        x.push_str("</Story></idPkg:Story>");
        let name = format!("Stories/Story_{}.xml", uid(sid.0));
        story_files.push(name.clone());
        files.push((name, x));
    }
    let styles = styles_xml(doc, &mut w.colors);
    let graphic = graphic_xml(doc, &w.colors);
    let mut dm = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<?aid style=\"50\" type=\"document\" readerVersion=\"6.0\" featureSet=\"257\" product=\"{DOM}\" ?>\n<Document xmlns:idPkg=\"{NS}\" DOMVersion=\"{DOM}\" Self=\"d\" StoryList=\"{}\" Name=\"{}\" ZeroPoint=\"0 0\" ActiveLayer=\"{}\">",
        w.stories.iter().map(|s| uid(s.0)).collect::<Vec<_>>().join(" "),
        esc(&doc.title),
        doc.layers.first().map(|l| uid(l.id.0)).unwrap_or_default()
    );
    dm.push_str("<idPkg:Graphic src=\"Resources/Graphic.xml\"/><idPkg:Fonts src=\"Resources/Fonts.xml\"/><idPkg:Styles src=\"Resources/Styles.xml\"/><idPkg:Preferences src=\"Resources/Preferences.xml\"/>");
    for l in &doc.layers {
        let _ = write!(dm, "<Layer Self=\"{}\" Name=\"{}\" Visible=\"{}\" Locked=\"{}\" Printable=\"{}\" UI=\"true\" Expendable=\"true\"/>", uid(l.id.0), esc(&l.name), l.visible, l.locked, l.printable);
    }
    for m in &masters {
        let _ = write!(dm, "<idPkg:MasterSpread src=\"{m}\"/>");
    }
    for s in &spreads {
        let _ = write!(dm, "<idPkg:Spread src=\"{s}\"/>");
    }
    for (i, sec) in doc.sections.iter().enumerate() {
        let start_page = doc.page(sec.start).map(|p| uid(p.id.0)).unwrap_or_default();
        let len = doc.sections.get(i + 1).map(|n| n.start).unwrap_or(doc.page_count()).saturating_sub(sec.start);
        let style = match sec.style.as_str() {
            "roman" => "LowerRoman",
            "ROMAN" => "UpperRoman",
            "alpha" => "LowerLetters",
            "ALPHA" => "UpperLetters",
            _ => "Arabic",
        };
        let _ = write!(
            dm,
            "<Section Self=\"sec{i}\" Length=\"{len}\" Name=\"{}\" ContinueNumbering=\"{}\" IncludeSectionPrefix=\"{}\" PageNumberStyle=\"{style}\" PageStart=\"{start_page}\" SectionPrefix=\"{}\" Marker=\"{}\" PageNumberStart=\"{}\"/>",
            esc(&sec.prefix),
            sec.start_at.is_none(),
            sec.include_prefix,
            esc(&sec.prefix),
            esc(&sec.marker),
            sec.start_at.unwrap_or(1)
        );
    }
    for s in &story_files {
        let _ = write!(dm, "<idPkg:Story src=\"{s}\"/>");
    }
    dm.push_str("</Document>");
    let mut buf = Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let deflate = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let e = |e: zip::result::ZipError| e.to_string();
        z.start_file("mimetype", stored).map_err(e)?;
        z.write_all(MIME.as_bytes()).map_err(|e| e.to_string())?;
        z.start_file("designmap.xml", deflate).map_err(e)?;
        z.write_all(dm.as_bytes()).map_err(|e| e.to_string())?;
        let container = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\"><rootfiles><rootfile full-path=\"designmap.xml\" media-type=\"text/xml\"/></rootfiles></container>";
        let fonts = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<idPkg:Fonts xmlns:idPkg=\"{NS}\" DOMVersion=\"{DOM}\"/>");
        for (name, body) in [
            ("META-INF/container.xml".to_string(), container.to_string()),
            ("Resources/Graphic.xml".to_string(), graphic),
            ("Resources/Fonts.xml".to_string(), fonts),
            ("Resources/Styles.xml".to_string(), styles),
            ("Resources/Preferences.xml".to_string(), preferences_xml(doc)),
        ]
        .into_iter()
        .chain(files)
        {
            z.start_file(name, deflate).map_err(e)?;
            z.write_all(body.as_bytes()).map_err(|e| e.to_string())?;
        }
        z.finish().map_err(e)?;
    }
    Ok(buf.into_inner())
}
