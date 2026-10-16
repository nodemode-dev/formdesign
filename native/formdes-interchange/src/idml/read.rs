use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::sync::Arc;

use formdes_model::attrs::Attrs;
use formdes_model::color::{Color, Gradient, Paint, Stop, Swatch, SwatchValue};
use formdes_model::doc::{Doc, DocSettings, Home, Intent};
use formdes_model::geom::{Affine, BezPath, Edges, Point, Rect};
use formdes_model::ids::{AssetId, ItemId, LayerId, ParentId, StoryId};
use formdes_model::item::{Asset, AutoSize, Corner, FirstBaseline, Graphic, Item, ItemKind, PathText, Shape, Stroke, StrokeAlign, TextFrame, TextWrap, VJust, WrapMode};
use formdes_model::page::{Layer, Page, Parent, Section, Spread};
use formdes_model::story::{Inline, Marker, Para, Seg, Table, TableCell};
use formdes_model::style::{BASIC_PARA, NamedStyle, StyleKind};
use serde_json::{Value, json};

use super::dom::{El, Node, parse};
use super::unb64;

struct Ctx {
    files: HashMap<String, String>,
    paints: HashMap<String, Paint>,
    pstyles: HashMap<String, String>,
    cstyles: HashMap<String, String>,
    ostyles: HashMap<String, String>,
    layers: HashMap<String, LayerId>,
    masters: HashMap<String, ParentId>,
    stories: HashMap<String, StoryId>,
    frames: Vec<(String, ItemId, String, String, String)>,
    pending_anchors: Vec<(ItemId, StoryId)>,
}

fn matrix(s: Option<&str>) -> Affine {
    let n: Vec<f64> = s.unwrap_or("").split_whitespace().filter_map(|v| v.parse().ok()).collect();
    if n.len() == 6 { Affine::new([n[0], n[1], n[2], n[3], n[4], n[5]]) } else { Affine::IDENTITY }
}

fn pt(s: Option<&str>) -> Option<Point> {
    let n: Vec<f64> = s?.split_whitespace().filter_map(|v| v.parse().ok()).collect();
    (n.len() == 2).then(|| Point::new(n[0], n[1]))
}

fn unesc(s: &str) -> String {
    s.replace("%3a", ":").replace("%3A", ":")
}

fn style_name(r: &str) -> String {
    let n = r.split_once('/').map(|(_, n)| n).unwrap_or(r);
    let n = unesc(n);
    match n.as_str() {
        "$ID/NormalParagraphStyle" | "$ID/[Basic Paragraph]" => BASIC_PARA.into(),
        "$ID/[No paragraph style]" => "No Paragraph Style".into(),
        "$ID/[No character style]" => "None".into(),
        "$ID/[None]" => "None".into(),
        "$ID/[Basic Table]" => "Basic Table".into(),
        "$ID/[No table style]" => "No Table Style".into(),
        "$ID/[Normal Graphics Frame]" => "Basic Graphics Frame".into(),
        "$ID/[Normal Text Frame]" => "Basic Text Frame".into(),
        other => {
            let o = other.strip_prefix("$ID/").unwrap_or(other);
            o.rsplit(':').next().unwrap_or(o).to_string()
        }
    }
}

fn color_of(space: &str, v: &str) -> Option<Color> {
    let n: Vec<f64> = v.split_whitespace().filter_map(|x| x.parse().ok()).collect();
    match (space, n.len()) {
        ("CMYK", 4) => Some(Color::Cmyk([n[0] / 100.0, n[1] / 100.0, n[2] / 100.0, n[3] / 100.0])),
        ("RGB", 3) => Some(Color::Rgb([n[0] / 255.0, n[1] / 255.0, n[2] / 255.0])),
        ("LAB", 3) => Some(Color::Lab([n[0], n[1], n[2]])),
        _ => None,
    }
}

fn path_of(el: &El) -> Option<(BezPath, bool)> {
    let geo = el.prop("PathGeometry")?;
    let mut p = BezPath::new();
    let mut closed_all = true;
    for g in geo.all("GeometryPathType") {
        let open = g.attr("PathOpen") == Some("true");
        closed_all &= !open;
        let pts: Vec<(Point, Point, Point)> = g
            .child("PathPointArray")
            .map(|a| a.all("PathPointType").iter().filter_map(|pp| Some((pt(pp.attr("Anchor"))?, pt(pp.attr("LeftDirection"))?, pt(pp.attr("RightDirection"))?))).collect())
            .unwrap_or_default();
        if pts.is_empty() {
            continue;
        }
        p.move_to(pts[0].0);
        let n = pts.len();
        let segs = if open { n - 1 } else { n };
        for i in 0..segs {
            let a = pts[i];
            let b = pts[(i + 1) % n];
            if (a.2 - a.0).hypot() < 1e-6 && (b.1 - b.0).hypot() < 1e-6 {
                p.line_to(b.0);
            } else {
                p.curve_to(a.2, b.1, b.0);
            }
        }
        if !open {
            p.close_path();
        }
    }
    (!p.elements().is_empty()).then_some((p, closed_all))
}

fn shape_of(el: &El) -> Shape {
    let Some((path, closed)) = path_of(el) else { return Shape::Rect { rect: Rect::ZERO } };
    let bb = kurbo::Shape::bounding_box(&path);
    let curves = path.elements().iter().any(|e| matches!(e, kurbo::PathEl::CurveTo(..) | kurbo::PathEl::QuadTo(..)));
    let pts: Vec<Point> = path.elements().iter().filter_map(|e| match e {
        kurbo::PathEl::MoveTo(p) | kurbo::PathEl::LineTo(p) => Some(*p),
        _ => None,
    }).collect();
    match el.name.as_str() {
        "Oval" => Shape::Ellipse { rect: bb },
        "GraphicLine" if pts.len() == 2 && !curves => Shape::Line { from: (pts[0].x, pts[0].y), to: (pts[1].x, pts[1].y) },
        _ if closed && !curves && pts.len() == 4 && pts.iter().all(|p| ((p.x - bb.x0).abs() < 1e-3 || (p.x - bb.x1).abs() < 1e-3) && ((p.y - bb.y0).abs() < 1e-3 || (p.y - bb.y1).abs() < 1e-3)) => Shape::Rect { rect: bb },
        _ => Shape::Path { path, closed },
    }
}

impl Ctx {
    fn file(&self, name: &str) -> Option<El> {
        self.files.get(name).and_then(|s| parse(s).ok())
    }

    fn paint(&self, r: Option<&str>) -> Option<Paint> {
        let r = r?;
        if r == "n" || r.is_empty() {
            return None;
        }
        Some(self.paints.get(r).cloned().unwrap_or_else(|| {
            let n = unesc(r.split_once('/').map(|(_, n)| n).unwrap_or(r));
            if n == "None" { Paint::None } else { Paint::Swatch { name: n, tint: 1.0 } }
        }))
    }

    fn paint_value(&self, r: Option<&str>) -> Option<Value> {
        self.paint(r).map(|p| p.to_value())
    }

    fn char_attrs(&self, el: &El, a: &mut Attrs) {
        let num = |k: &str| el.num(k);
        for (key, idml, f) in [("size", "PointSize", 1.0), ("tracking", "Tracking", 1.0), ("hScale", "HorizontalScale", 0.01), ("vScale", "VerticalScale", 0.01), ("baselineShift", "BaselineShift", 1.0), ("skew", "Skew", 1.0), ("strokeWeight", "StrokeWeight", 1.0)] {
            if let Some(v) = num(idml) {
                a.insert(key.into(), json!(v * f));
            }
        }
        if let Some(v) = num("FillTint").filter(|v| *v >= 0.0) {
            a.insert("fillTint".into(), json!(v / 100.0));
        }
        if let Some(v) = el.attr("KerningMethod") {
            a.insert("kerning".into(), json!(if v.contains("Optical") { "optical" } else if v.ends_with("None") { "none" } else { "metrics" }));
        }
        if let Some(v) = el.attr("FontStyle") {
            a.insert("fontStyle".into(), json!(v));
        }
        for (k, idml) in [("underline", "Underline"), ("strike", "StrikeThru"), ("ligatures", "Ligatures"), ("noBreak", "NoBreak")] {
            if let Some(v) = el.flag(idml) {
                a.insert(k.into(), json!(v));
            }
        }
        if let Some(v) = el.attr("Capitalization") {
            a.insert("caps".into(), json!(match v {
                "AllCaps" => "allCaps",
                "SmallCaps" => "smallCaps",
                "CapToSmallCap" => "allSmallCaps",
                _ => "normal",
            }));
        }
        if let Some(v) = el.attr("Position") {
            a.insert("position".into(), json!(match v {
                "Superscript" | "OTSuperscript" => "superscript",
                "Subscript" | "OTSubscript" => "subscript",
                "OTNumerator" => "numerator",
                "OTDenominator" => "denominator",
                _ => "normal",
            }));
        }
        if let Some(v) = self.paint_value(el.attr("FillColor")) {
            a.insert("fill".into(), v);
        }
        if let Some(v) = self.paint_value(el.attr("StrokeColor")) {
            a.insert("stroke".into(), v);
        }
        if let Some(v) = el.attr("AppliedLanguage") {
            let l = unesc(v.trim_start_matches("$ID/"));
            a.insert("language".into(), json!(lang(&l)));
        }
        if let Some(p) = el.child("Properties") {
            if let Some(f) = p.child("AppliedFont") {
                a.insert("font".into(), json!(f.text()));
            }
            if let Some(l) = p.child("Leading") {
                let t = l.text();
                match t.trim().parse::<f64>() {
                    Ok(v) => {
                        a.insert("leading".into(), json!(v));
                    }
                    Err(_) => {
                        a.insert("leading".into(), json!("auto"));
                    }
                }
            }
        }
    }

    fn para_attrs(&self, el: &El, a: &mut Attrs) {
        self.char_attrs(el, a);
        for (key, idml, f) in [
            ("leftIndent", "LeftIndent", 1.0),
            ("rightIndent", "RightIndent", 1.0),
            ("firstIndent", "FirstLineIndent", 1.0),
            ("lastIndent", "LastLineIndent", 1.0),
            ("spaceBefore", "SpaceBefore", 1.0),
            ("spaceAfter", "SpaceAfter", 1.0),
            ("autoLeading", "AutoLeading", 0.01),
            ("hyphZone", "HyphenationZone", 1.0),
        ] {
            if let Some(v) = el.num(idml) {
                a.insert(key.into(), json!(v * f));
            }
        }
        for (key, idml) in [("dropLines", "DropCapLines"), ("dropChars", "DropCapCharacters"), ("hyphMinWord", "HyphenateWordsLongerThan"), ("hyphAfter", "HyphenateAfterFirst"), ("hyphBefore", "HyphenateBeforeLast"), ("hyphLimit", "HyphenateLadderLimit"), ("keepNext", "KeepWithNext"), ("keepFirst", "KeepFirstLines"), ("keepLast", "KeepLastLines"), ("startAt", "NumberingStartAt")] {
            if let Some(v) = el.num(idml) {
                a.insert(key.into(), json!(v.round() as i64));
            }
        }
        for (key, idml) in [("hyphenate", "Hyphenation"), ("hyphCaps", "HyphenateCapitalizedWords"), ("keepTogether", "KeepAllLinesTogether"), ("balance", "BalanceRaggedLines")] {
            if let Some(v) = el.flag(idml) {
                a.insert(key.into(), json!(v));
            }
        }
        if let Some(v) = el.attr("Justification") {
            a.insert("align".into(), json!(match v {
                "CenterAlign" => "center",
                "RightAlign" => "right",
                "LeftJustified" => "justify",
                "CenterJustified" => "justifyCenter",
                "RightJustified" => "justifyRight",
                "FullyJustified" => "justifyAll",
                "ToBindingSide" => "towardsSpine",
                "AwayFromBindingSide" => "awayFromSpine",
                _ => "left",
            }));
        }
        if let Some(v) = el.attr("StartParagraph") {
            a.insert("start".into(), json!(match v {
                "NextColumn" => "nextColumn",
                "NextFrame" => "nextFrame",
                "NextPage" => "nextPage",
                "NextOddPage" => "nextOddPage",
                "NextEvenPage" => "nextEvenPage",
                _ => "anywhere",
            }));
        }
        if let Some(v) = el.attr("GridAlignment") {
            a.insert("gridAlign".into(), json!(v != "None"));
        }
        if let Some(v) = el.attr("BulletsAndNumberingListType") {
            a.insert("list".into(), json!(match v {
                "BulletList" => "bullet",
                "NumberedList" => "number",
                _ => "none",
            }));
        }
        for (key, pre) in [("ruleAbove", "RuleAbove"), ("ruleBelow", "RuleBelow")] {
            if el.flag(pre) == Some(true) {
                let color = self.paint_value(el.attr(&format!("{pre}Color"))).unwrap_or(json!("Black"));
                a.insert(key.into(), json!({
                    "weight": el.num(&format!("{pre}LineWeight")).unwrap_or(1.0), "color": color,
                    "offset": el.num(&format!("{pre}Offset")).unwrap_or(0.0), "left": el.num(&format!("{pre}LeftIndent")).unwrap_or(0.0),
                    "right": el.num(&format!("{pre}RightIndent")).unwrap_or(0.0),
                    "width": if el.attr(&format!("{pre}Width")) == Some("TextWidth") { "text" } else { "column" },
                }));
            }
        }
        if let Some(tl) = el.prop("TabList") {
            let tabs: Vec<Value> = tl
                .all("ListItem")
                .iter()
                .filter_map(|li| {
                    let pos: f64 = li.child("Position")?.text().trim().parse().ok()?;
                    let al = match li.child("Alignment").map(|x| x.text()).as_deref() {
                        Some("CenterAlign") => "center",
                        Some("RightAlign") => "right",
                        Some("CharacterAlign") => "decimal",
                        _ => "left",
                    };
                    Some(json!({"position": pos, "align": al, "leader": li.child("Leader").map(|x| x.text()).unwrap_or_default(), "alignOn": li.child("AlignmentCharacter").map(|x| x.text()).unwrap_or_else(|| ".".into())}))
                })
                .collect();
            a.insert("tabs".into(), Value::Array(tabs));
        }
    }
}

fn lang(l: &str) -> String {
    match l {
        "English: USA" | "English: USA Medical" => "en-US",
        "English: UK" => "en-GB",
        "English: Canadian" => "en-CA",
        "German: 2006 Reform" | "German: Reformed" | "German" => "de-DE",
        "French" => "fr-FR",
        "Spanish" | "Spanish: Castilian" => "es-ES",
        "Italian" => "it-IT",
        "Dutch: 2005 Reform" | "Dutch" => "nl-NL",
        "Portuguese" => "pt-PT",
        "Portuguese: Brazilian" => "pt-BR",
        "Swedish" => "sv-SE",
        "Danish" => "da-DK",
        "Norwegian: Bokmal" => "nb-NO",
        "Finnish" => "fi-FI",
        "Polish" => "pl-PL",
        "[No Language]" => "",
        _ => "en-US",
    }
    .to_string()
}

fn mime(name: &str) -> String {
    match name.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "gif" => "image/gif",
        "tif" | "tiff" => "image/tiff",
        "webp" => "image/webp",
        "pdf" | "ai" => "application/pdf",
        "psd" => "image/vnd.adobe.photoshop",
        "eps" => "application/postscript",
        _ => "image/jpeg",
    }
    .to_string()
}

fn uri_path(u: &str) -> String {
    let p = u.strip_prefix("file://").or_else(|| u.strip_prefix("file:")).unwrap_or(u);
    let mut out = String::new();
    let b = p.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) = u8::from_str_radix(&p[i + 1..i + 3], 16)
        {
            out.push(v as char);
            i += 3;
            continue;
        }
        out.push(b[i] as char);
        i += 1;
    }
    out
}

const ITEM_TAGS: &[&str] = &["TextFrame", "Rectangle", "Oval", "Polygon", "GraphicLine", "Group"];

fn build_item(c: &mut Ctx, doc: &mut Doc, el: &El, xf: Affine, home: Home) -> Option<ItemId> {
    if !ITEM_TAGS.contains(&el.name.as_str()) {
        return None;
    }
    let layer = el.attr("ItemLayer").and_then(|l| c.layers.get(l)).copied().or_else(|| doc.layers.first().map(|l| l.id)).unwrap_or(LayerId(0));
    let shape = if el.name == "Group" { Shape::Rect { rect: Rect::ZERO } } else { shape_of(el) };
    let kind = match el.name.as_str() {
        "Group" => ItemKind::Group { children: Vec::new() },
        "TextFrame" => ItemKind::Text(TextFrame::new(StoryId(0))),
        _ => {
            let media = el.child("Image").or_else(|| el.child("PDF")).or_else(|| el.child("EPS")).or_else(|| el.child("ImportedPage"));
            match (media, el.attr("ContentType")) {
                (Some(_), _) | (None, Some("GraphicType")) => ItemKind::Graphic(Graphic { asset: None, content: Affine::IDENTITY, fitting: None }),
                _ => ItemKind::Plain,
            }
        }
    };
    let mut it = Item::new(ItemId(0), kind, shape, layer);
    it.xf = xf;
    it.name = el.attr("Name").filter(|n| !n.starts_with("$ID/")).unwrap_or("").to_string();
    it.fill = c.paint(el.attr("FillColor")).unwrap_or(Paint::None);
    if let (Some(t), Paint::Swatch { tint, .. }) = (el.num("FillTint").filter(|t| *t >= 0.0), &mut it.fill) {
        *tint = t / 100.0;
    }
    it.stroke = Stroke {
        paint: c.paint(el.attr("StrokeColor")).unwrap_or(Paint::None),
        weight: el.num("StrokeWeight").unwrap_or(if el.attr("StrokeColor").is_some_and(|s| s != "Swatch/None") { 1.0 } else { 0.0 }),
        align: match el.attr("StrokeAlignment") {
            Some("InsideAlignment") => StrokeAlign::Inside,
            Some("OutsideAlignment") => StrokeAlign::Outside,
            _ => StrokeAlign::Center,
        },
        ..Stroke::default()
    };
    it.hidden = el.flag("Visible") == Some(false);
    it.locked = el.flag("Locked") == Some(true);
    it.nonprinting = el.flag("Nonprinting") == Some(true);
    if let Some(o) = el.child("TransparencySetting").and_then(|t| t.child("BlendingSetting")) {
        if let Some(op) = o.num("Opacity") {
            it.opacity = (op / 100.0).clamp(0.0, 1.0);
        }
        if let Some(b) = o.attr("BlendMode") {
            let mut s = b.to_string();
            if let Some(f) = s.get_mut(0..1) {
                f.make_ascii_lowercase();
            }
            it.blend = s;
        }
    }
    if let Some(os) = el.attr("AppliedObjectStyle") {
        let n = c.ostyles.get(os).cloned().unwrap_or_else(|| style_name(os));
        if n != "None" {
            it.style = Some(n);
        }
    }
    if let Some(r) = el.num("TopLeftCornerRadius").filter(|r| *r > 0.0)
        && el.attr("TopLeftCornerOption").is_some_and(|o| o != "None")
    {
        it.corner = Corner {
            radius: r,
            kind: match el.attr("TopLeftCornerOption") {
                Some("BevelCorner") => "bevel",
                Some("InsetCorner") => "inset",
                Some("InverseRoundedCorner") => "inverse",
                Some("FancyCorner") => "fancy",
                _ => "rounded",
            }
            .into(),
            each: Vec::new(),
        };
    }
    if let Some(w) = el.child("TextWrapPreference") {
        let mode = match w.attr("TextWrapMode") {
            Some("BoundingBoxTextWrap") => Some(WrapMode::BoundingBox),
            Some("Contour") => Some(WrapMode::Contour),
            Some("JumpObjectTextWrap") => Some(WrapMode::JumpObject),
            Some("NextColumnTextWrap") => Some(WrapMode::NextColumn),
            _ => None,
        };
        if let Some(m) = mode {
            let o = w.prop("TextWrapOffset");
            let g = |k: &str| o.and_then(|o| o.num(k)).unwrap_or(0.0);
            it.wrap = Some(TextWrap { mode: m, offsets: Edges { top: g("Top"), bottom: g("Bottom"), inside: g("Left"), outside: g("Right") }, invert: w.flag("Inverse") == Some(true), side: "both".into() });
        }
    }
    if let ItemKind::Text(tf) = &mut it.kind
        && let Some(p) = el.child("TextFramePreference")
    {
        if let Some(n) = p.num("TextColumnCount") {
            tf.columns = n.max(1.0) as u32;
        }
        if let Some(g) = p.num("TextColumnGutter") {
            tf.gutter = g;
        }
        if let Some(w) = p.num("TextColumnFixedWidth").filter(|w| *w > 0.0) {
            tf.fixed_column_width = Some(w);
        }
        if let Some(w) = p.num("TextColumnMaxWidth").filter(|w| *w > 0.0) {
            tf.max_column_width = w;
        }
        if p.flag("UseFixedColumnWidth") == Some(true) {
            tf.column_rule = formdes_model::item::ColumnRule::Fixed;
        } else if p.flag("UseFlexibleColumnWidth") == Some(true) {
            tf.column_rule = formdes_model::item::ColumnRule::Flexible;
        }
        let ins: Vec<f64> = p.attr("InsetSpacing").unwrap_or("").split_whitespace().filter_map(|v| v.parse().ok()).collect();
        if ins.len() == 4 {
            tf.insets = Edges { top: ins[0], inside: ins[1], bottom: ins[2], outside: ins[3] };
        } else if ins.len() == 1 {
            tf.insets = Edges::all(ins[0]);
        }
        tf.vjust = match p.attr("VerticalJustification") {
            Some("CenterAlign") => VJust::Center,
            Some("BottomAlign") => VJust::Bottom,
            Some("JustifyAlign") => VJust::Justify,
            _ => VJust::Top,
        };
        tf.first_baseline = match p.attr("FirstBaselineOffset") {
            Some("CapHeight") => FirstBaseline::CapHeight,
            Some("XHeight") => FirstBaseline::XHeight,
            Some("LeadingOffset") => FirstBaseline::Leading,
            Some("FixedHeight") => FirstBaseline::Fixed,
            _ => FirstBaseline::Ascent,
        };
        tf.autosize = match p.attr("AutoSizingType") {
            Some("HeightOnly") => AutoSize::Height,
            Some("WidthOnly") => AutoSize::Width,
            Some("HeightAndWidth" | "HeightAndWidthProportionally") => AutoSize::Both,
            _ => AutoSize::Off,
        };
        tf.ignore_wrap = p.flag("IgnoreWrap") == Some(true);
    }
    if let ItemKind::Text(tf) = &mut it.kind
        && let Some(tp) = el.child("TextPath")
    {
        tf.on_path = Some(PathText { start: tp.num("StartBracket").unwrap_or(0.0), flip: tp.attr("FlipPathEffect") == Some("Flipped"), align: tp.attr("PathAlignment").unwrap_or("baseline").to_string() });
    }
    let media = el.child("Image").or_else(|| el.child("PDF")).or_else(|| el.child("EPS"));
    let id = doc.insert_item(it, home);
    if el.name == "TextFrame" {
        c.frames.push((el.attr("ParentStory").unwrap_or("").to_string(), id, el.attr("Self").unwrap_or("").to_string(), el.attr("PreviousTextFrame").unwrap_or("n").to_string(), el.attr("NextTextFrame").unwrap_or("n").to_string()));
    }
    if el.name == "Group" {
        for k in el.els() {
            build_item(c, doc, k, matrix(k.attr("ItemTransform")), Home::Group(id));
        }
    }
    if let Some(m) = media {
        let content = matrix(m.attr("ItemTransform"));
        let gb = m.prop("GraphicBounds");
        let (w, h) = gb.map(|g| (g.num("Right").unwrap_or(100.0) - g.num("Left").unwrap_or(0.0), g.num("Bottom").unwrap_or(100.0) - g.num("Top").unwrap_or(0.0))).unwrap_or((100.0, 100.0));
        let ppi: Vec<f64> = m.attr("EffectivePpi").or(m.attr("ActualPpi")).unwrap_or("").split_whitespace().filter_map(|v| v.parse().ok()).collect();
        let link = m.child("Link").and_then(|l| l.attr("LinkResourceURI")).map(uri_path);
        let name = link.as_deref().and_then(|l| l.rsplit(['/', '\\']).next()).unwrap_or("image").to_string();
        let mut data = m.prop("Contents").map(|c| unb64(&c.text())).unwrap_or_default();
        if data.is_empty()
            && let Some(l) = &link
        {
            data = std::fs::read(l).unwrap_or_default();
        }
        let (px, py) = if ppi.len() == 2 && m.name == "Image" { (ppi[0], ppi[1]) } else { (72.0, 72.0) };
        let mut mt = mime(&name);
        if m.name == "PDF" || data.starts_with(b"%PDF") {
            mt = "application/pdf".into();
        }
        let embedded = !data.is_empty() && m.prop("Contents").is_some();
        let asset = Asset {
            id: AssetId(0),
            name,
            mime: mt,
            link: if embedded { None } else { link },
            data: Arc::new(data),
            pixels: Some(((w * px / 72.0).round().max(1.0) as u32, (h * py / 72.0).round().max(1.0) as u32)),
            ppi: Some((px, py)),
            page: m.child("PDFAttribute").and_then(|a| a.num("PageNumber")).map(|n| (n.max(1.0) - 1.0) as u32).unwrap_or(0),
            embedded,
            modified: 0,
        };
        let aid = doc.add_asset(asset);
        if let Some(ItemKind::Graphic(g)) = doc.item_mut(id).map(|i| &mut i.kind) {
            g.asset = Some(aid);
            g.content = content;
        }
    }
    Some(id)
}

fn pages_of(el: &El) -> (Vec<(f64, f64, Affine, &El)>, Affine) {
    let pages: Vec<(f64, f64, Affine, &El)> = el
        .all("Page")
        .into_iter()
        .map(|p| {
            let gb: Vec<f64> = p.attr("GeometricBounds").unwrap_or("0 0 792 612").split_whitespace().filter_map(|v| v.parse().ok()).collect();
            let (t, l, b, r) = if gb.len() == 4 { (gb[0], gb[1], gb[2], gb[3]) } else { (0.0, 0.0, 792.0, 612.0) };
            let xf = matrix(p.attr("ItemTransform")) * Affine::translate((l, t));
            (r - l, b - t, xf, p)
        })
        .collect();
    let minx = pages.iter().map(|p| p.2.translation().x).fold(f64::INFINITY, f64::min);
    let miny = pages.iter().map(|p| p.2.translation().y).fold(f64::INFINITY, f64::min);
    let off = if minx.is_finite() { Affine::translate((-minx, -miny)) } else { Affine::IDENTITY };
    (pages, off)
}

fn make_page(doc: &mut Doc, w: f64, h: f64, p: &El) -> Page {
    let mut pg = doc.blank_page();
    pg.width = w;
    pg.height = h;
    if let Some(m) = p.child("MarginPreference") {
        pg.columns = m.num("ColumnCount").unwrap_or(1.0).max(1.0) as u32;
        pg.gutter = m.num("ColumnGutter").unwrap_or(12.0);
        pg.margins = Edges { top: m.num("Top").unwrap_or(36.0), bottom: m.num("Bottom").unwrap_or(36.0), inside: m.num("Left").unwrap_or(36.0), outside: m.num("Right").unwrap_or(36.0) };
    }
    pg
}

fn story_content(c: &mut Ctx, doc: &mut Doc, sid: StoryId, root: &El, paras: &mut Vec<Para>) {
    for psr in root.els() {
        match psr.name.as_str() {
            "ParagraphStyleRange" => paragraph_range(c, doc, sid, psr, paras),
            "XMLElement" => story_content(c, doc, sid, psr, paras),
            _ => {}
        }
    }
}

fn cur<'a>(paras: &'a mut Vec<Para>, style: &str, over: &Attrs) -> &'a mut Para {
    if paras.is_empty() {
        paras.push(Para { style: style.into(), over: over.clone(), segs: Vec::new() });
    }
    paras.last_mut().expect("just pushed")
}

fn paragraph_range(c: &mut Ctx, doc: &mut Doc, sid: StoryId, psr: &El, paras: &mut Vec<Para>) {
    let style = psr.attr("AppliedParagraphStyle").map(|s| c.pstyles.get(s).cloned().unwrap_or_else(|| style_name(s))).unwrap_or_else(|| BASIC_PARA.into());
    let mut pover = Attrs::new();
    c.para_attrs(psr, &mut pover);
    {
        let p = cur(paras, &style, &pover);
        if p.segs.is_empty() {
            p.style = style.clone();
            p.over = pover.clone();
        }
    }
    for csr in psr.els() {
        if csr.name == "XMLElement" {
            paragraph_range(c, doc, sid, csr, paras);
            continue;
        }
        if csr.name != "CharacterStyleRange" {
            continue;
        }
        let cstyle = csr.attr("AppliedCharacterStyle").map(|s| c.cstyles.get(s).cloned().unwrap_or_else(|| style_name(s))).filter(|s| s != "None");
        let mut over = Attrs::new();
        c.char_attrs(csr, &mut over);
        for k in &csr.kids {
            let Node::El(e) = k else { continue };
            let push_inline = |paras: &mut Vec<Para>, inline: Inline| {
                cur(paras, &style, &pover).segs.push(Seg::Inline { inline, style: cstyle.clone(), over: over.clone() });
            };
            match e.name.as_str() {
                "Content" => {
                    for n in &e.kids {
                        match n {
                            Node::Text(t) => cur(paras, &style, &pover).segs.push(Seg::Text { text: t.replace('\r', "").replace('\n', "\u{2028}"), style: cstyle.clone(), over: over.clone() }),
                            Node::Pi(pi) => {
                                let code: String = pi.chars().filter(|ch| ch.is_ascii_digit()).collect();
                                let inline = match code.as_str() {
                                    "18" => Some(Inline::Marker { marker: Marker::PageNumber }),
                                    "19" => Some(Inline::Marker { marker: Marker::SectionName }),
                                    "7" => Some(Inline::IndentHere),
                                    "8" => Some(Inline::RightTab),
                                    _ => None,
                                };
                                if let Some(i) = inline {
                                    push_inline(paras, i);
                                }
                            }
                            Node::El(_) => {}
                        }
                    }
                }
                "Br" => {
                    let p = cur(paras, &style, &pover);
                    if p.segs.is_empty() {
                        p.segs.push(Seg::text(""));
                    }
                    paras.push(Para { style: style.clone(), over: pover.clone(), segs: Vec::new() });
                }
                "TextVariableInstance" => {
                    let name = e.attr("AssociatedTextVariable").map(|v| unesc(v.trim_start_matches("dTextVariable"))).or_else(|| e.attr("Name").map(str::to_string)).unwrap_or_default();
                    push_inline(paras, Inline::Variable { name });
                }
                "Note" => {
                    let text = e.text();
                    let id = doc.alloc();
                    push_inline(paras, Inline::Note { id, author: e.attr("UserName").unwrap_or("").to_string(), text });
                }
                "Footnote" => {
                    let mut fp = Vec::new();
                    story_content(c, doc, sid, e, &mut fp);
                    if fp.is_empty() {
                        fp.push(Para::empty(BASIC_PARA));
                    }
                    for p in fp.iter_mut() {
                        if p.segs.is_empty() {
                            p.segs.push(Seg::text(""));
                        }
                    }
                    push_inline(paras, Inline::Footnote { paras: fp });
                }
                "Table" => {
                    let t = table_of(c, doc, sid, e);
                    push_inline(paras, Inline::Table { table: Box::new(t) });
                }
                n if ITEM_TAGS.contains(&n) => {
                    if let Some(id) = build_item(c, doc, e, matrix(e.attr("ItemTransform")), Home::Anchored(sid)) {
                        c.pending_anchors.push((id, sid));
                        push_inline(paras, Inline::Anchored { item: id, attrs: Attrs::new() });
                    }
                }
                _ => {}
            }
        }
    }
}

fn table_of(c: &mut Ctx, doc: &mut Doc, sid: StoryId, e: &El) -> Table {
    let rows = e.all("Row");
    let cols = e.all("Column");
    let mut t = Table::new(rows.len().max(1), cols.len().max(1), 100.0 * cols.len().max(1) as f64);
    t.cells.clear();
    t.heights = rows.iter().map(|r| r.num("SingleRowHeight").unwrap_or(18.0)).collect();
    t.exact = rows.iter().map(|r| r.flag("AutoGrow") == Some(false)).collect();
    t.widths = cols.iter().map(|r| r.num("SingleColumnWidth").unwrap_or(100.0)).collect();
    if t.heights.is_empty() {
        t.heights = vec![18.0];
        t.exact = vec![false];
    }
    if t.widths.is_empty() {
        t.widths = vec![100.0];
    }
    t.header_rows = e.num("HeaderRowCount").unwrap_or(0.0) as usize;
    t.footer_rows = e.num("FooterRowCount").unwrap_or(0.0) as usize;
    t.style = e.attr("AppliedTableStyle").map(style_name).filter(|s| s != "No Table Style");
    for cell in e.all("Cell") {
        let (col, row) = cell.attr("Name").and_then(|n| n.split_once(':')).map(|(a, b)| (a.parse().unwrap_or(0), b.parse().unwrap_or(0))).unwrap_or((0, 0));
        let mut paras = Vec::new();
        story_content(c, doc, sid, cell, &mut paras);
        for p in paras.iter_mut() {
            if p.segs.is_empty() {
                p.segs.push(Seg::text(""));
            }
        }
        if paras.is_empty() {
            paras.push(Para::empty(BASIC_PARA));
        }
        let mut attrs = Attrs::new();
        if let Some(f) = c.paint_value(cell.attr("FillColor")) {
            attrs.insert("fill".into(), f);
        }
        let mut strokes = serde_json::Map::new();
        for (key, edge) in [("top", "Top"), ("bottom", "Bottom"), ("left", "Left"), ("right", "Right")] {
            let mut one = serde_json::Map::new();
            if let Some(v) = c.paint_value(cell.attr(&format!("{edge}EdgeStrokeColor"))) {
                one.insert("color".into(), v);
            }
            if let Some(w) = cell.num(&format!("{edge}EdgeStrokeWeight")) {
                one.insert("weight".into(), json!(w));
            }
            if let Some(t) = cell.num(&format!("{edge}EdgeStrokeTint")).filter(|t| *t >= 0.0) {
                one.insert("tint".into(), json!(t / 100.0));
            }
            if let Some(k) = cell.attr(&format!("{edge}EdgeStrokeType")) {
                let k = k.rsplit('/').next().unwrap_or("Solid").to_ascii_lowercase();
                one.insert("type".into(), json!(if k.contains("dash") { "dashed" } else if k.contains("dot") { "dotted" } else if k == "none" { "none" } else { "solid" }));
            }
            if !one.is_empty() {
                strokes.insert(key.into(), serde_json::Value::Object(one));
            }
        }
        if !strokes.is_empty() {
            attrs.insert("strokes".into(), serde_json::Value::Object(strokes));
        }
        t.cells.push(TableCell { row, col, rows: cell.num("RowSpan").unwrap_or(1.0).max(1.0) as usize, cols: cell.num("ColumnSpan").unwrap_or(1.0).max(1.0) as usize, paras, attrs, style: cell.attr("AppliedCellStyle").map(style_name).filter(|s| s != "None") });
    }
    for r in 0..t.rows() {
        for col in 0..t.cols() {
            if t.cell(r, col).is_none() {
                t.cells.push(TableCell { row: r, col, rows: 1, cols: 1, paras: vec![Para::empty(BASIC_PARA)], attrs: Attrs::new(), style: None });
            }
        }
    }
    t
}

fn styles(c: &mut Ctx, doc: &mut Doc, root: &El) {
    let mut ps = Vec::new();
    root.find_all("ParagraphStyle", &mut ps);
    let mut cs = Vec::new();
    root.find_all("CharacterStyle", &mut cs);
    let mut os = Vec::new();
    root.find_all("ObjectStyle", &mut os);
    for (kind, list, map) in [(StyleKind::Paragraph, &ps, 0), (StyleKind::Character, &cs, 1), (StyleKind::Object, &os, 2)] {
        for s in list.iter() {
            let selfr = s.attr("Self").unwrap_or("");
            let name = style_name(s.attr("Name").map(|n| format!("x/{n}")).as_deref().unwrap_or(selfr));
            match map {
                0 => c.pstyles.insert(selfr.to_string(), name.clone()),
                1 => c.cstyles.insert(selfr.to_string(), name.clone()),
                _ => c.ostyles.insert(selfr.to_string(), name.clone()),
            };
            if name == "No Paragraph Style" || name == "None" {
                continue;
            }
            let mut attrs = Attrs::new();
            match kind {
                StyleKind::Paragraph => c.para_attrs(s, &mut attrs),
                StyleKind::Character => c.char_attrs(s, &mut attrs),
                _ => {}
            }
            let based = s.prop("BasedOn").map(|b| b.text()).filter(|b| !b.contains("[No ")).map(|b| style_name(&b));
            let next = s.attr("NextStyle").map(style_name).filter(|n| *n != name);
            let list = doc.styles.list_mut(kind);
            let st = match list.iter_mut().find(|x| x.name == name) {
                Some(x) => x,
                None => {
                    list.push(NamedStyle::new(&name));
                    list.last_mut().expect("pushed")
                }
            };
            st.attrs.extend(attrs);
            st.based_on = based;
            st.next = next;
        }
    }
}

fn graphic(c: &mut Ctx, doc: &mut Doc, root: &El) {
    for col in root.all("Color") {
        let selfr = col.attr("Self").unwrap_or("").to_string();
        let name = col.attr("Name").unwrap_or("");
        let Some(color) = color_of(col.attr("Space").unwrap_or("CMYK"), col.attr("ColorValue").unwrap_or("")) else { continue };
        if name.is_empty() || name.starts_with("$ID/") || col.flag("Visible") == Some(false) {
            c.paints.insert(selfr, Paint::Color { color });
            continue;
        }
        let spot = col.attr("Model") == Some("Spot");
        match doc.swatches.iter_mut().find(|s| s.name == name) {
            Some(s) => {
                if !s.locked {
                    s.value = SwatchValue::Color { color };
                    s.spot = spot;
                }
            }
            None => doc.swatches.push(Swatch::color(name, color, spot)),
        }
        c.paints.insert(selfr, Paint::Swatch { name: name.to_string(), tint: 1.0 });
    }
    for t in root.all("Tint") {
        let name = t.attr("Name").unwrap_or("").to_string();
        let base = t.attr("BaseColor").map(|b| match c.paints.get(b) {
            Some(Paint::Swatch { name, .. }) => name.clone(),
            _ => unesc(b.split_once('/').map(|(_, n)| n).unwrap_or(b)),
        });
        let (Some(base), Some(v)) = (base, t.num("TintValue")) else { continue };
        if name.is_empty() || name.starts_with("$ID/") {
            c.paints.insert(t.attr("Self").unwrap_or("").to_string(), Paint::Swatch { name: base, tint: v / 100.0 });
            continue;
        }
        if !doc.swatches.iter().any(|s| s.name == name) {
            doc.swatches.push(Swatch { name: name.clone(), value: SwatchValue::Tint { base, tint: v / 100.0 }, spot: false, group: None, locked: false });
        }
        c.paints.insert(t.attr("Self").unwrap_or("").to_string(), Paint::Swatch { name, tint: 1.0 });
    }
    for g in root.all("Gradient") {
        let name = g.attr("Name").unwrap_or("").to_string();
        let stops: Vec<Stop> = g
            .all("GradientStop")
            .iter()
            .filter_map(|s| {
                let sc = s.attr("StopColor")?;
                let color = match sc.split_once(':') {
                    Some((sp, v)) if ["CMYK", "RGB", "LAB"].contains(&sp) => color_of(sp, v),
                    _ => match c.paints.get(sc) {
                        Some(Paint::Color { color }) => Some(*color),
                        Some(Paint::Swatch { name, .. }) => doc.swatch_color(name, 1.0, 0),
                        _ => None,
                    },
                }?;
                Some(Stop { at: s.num("Location").unwrap_or(0.0) / 100.0, color, opacity: 1.0 })
            })
            .collect();
        if stops.len() < 2 || name.is_empty() {
            continue;
        }
        if !doc.swatches.iter().any(|s| s.name == name) {
            doc.swatches.push(Swatch { name: name.clone(), value: SwatchValue::Gradient { gradient: Gradient { radial: g.attr("Type") == Some("Radial"), stops, angle: 0.0 } }, spot: false, group: None, locked: false });
        }
        c.paints.insert(g.attr("Self").unwrap_or("").to_string(), Paint::Swatch { name, tint: 1.0 });
    }
}

pub fn read(bytes: &[u8]) -> Result<Doc, String> {
    let mut z = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("not an IDML package: {e}"))?;
    let mut files = HashMap::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).map_err(|e| e.to_string())?;
        if !f.name().ends_with(".xml") {
            continue;
        }
        let mut s = String::new();
        if f.read_to_string(&mut s).is_ok() {
            files.insert(f.name().to_string(), s);
        }
    }
    let mut c = Ctx { files, paints: HashMap::new(), pstyles: HashMap::new(), cstyles: HashMap::new(), ostyles: HashMap::new(), layers: HashMap::new(), masters: HashMap::new(), stories: HashMap::new(), frames: Vec::new(), pending_anchors: Vec::new() };
    let dm = c.file("designmap.xml").ok_or("not an IDML package: no designmap.xml")?;
    let dm = dm.child("Document").cloned().ok_or("designmap.xml has no Document")?;
    let prefs = c.file("Resources/Preferences.xml");
    let dp = prefs.as_ref().and_then(|p| p.find("DocumentPreference").cloned());
    let mut s = DocSettings::default();
    if let Some(d) = &dp {
        s.page_width = d.num("PageWidth").unwrap_or(612.0);
        s.page_height = d.num("PageHeight").unwrap_or(792.0);
        s.facing = d.flag("FacingPages").unwrap_or(true);
        s.bleed = Edges { top: d.num("DocumentBleedTopOffset").unwrap_or(0.0), bottom: d.num("DocumentBleedBottomOffset").unwrap_or(0.0), inside: d.num("DocumentBleedInsideOrLeftOffset").unwrap_or(0.0), outside: d.num("DocumentBleedOutsideOrRightOffset").unwrap_or(0.0) };
        s.slug = Edges { top: d.num("SlugTopOffset").unwrap_or(0.0), bottom: d.num("SlugBottomOffset").unwrap_or(0.0), inside: d.num("SlugInsideOrLeftOffset").unwrap_or(0.0), outside: d.num("SlugRightOrOutsideOffset").unwrap_or(0.0) };
        s.intent = if d.attr("Intent") == Some("WebIntent") { Intent::Web } else { Intent::Print };
    }
    if let Some(m) = prefs.as_ref().and_then(|p| p.find("MarginPreference")) {
        s.margins = Edges { top: m.num("Top").unwrap_or(36.0), bottom: m.num("Bottom").unwrap_or(36.0), inside: m.num("Left").unwrap_or(36.0), outside: m.num("Right").unwrap_or(36.0) };
        s.columns = m.num("ColumnCount").unwrap_or(1.0).max(1.0) as u32;
        s.gutter = m.num("ColumnGutter").unwrap_or(12.0);
    }
    if let Some(g) = prefs.as_ref().and_then(|p| p.find("GridPreference")) {
        s.baseline.start = g.num("BaselineStart").unwrap_or(36.0);
        s.baseline.increment = g.num("BaselineDivision").unwrap_or(12.0);
    }
    let mut doc = Doc::new(dm.attr("Name").unwrap_or("Untitled").trim_end_matches(".indd"), s, 1);
    doc.layers.clear();
    doc.parents.clear();
    doc.spreads.clear();
    doc.sections.clear();
    for l in dm.all("Layer") {
        let id = LayerId(doc.alloc());
        let mut layer = Layer::new(id, l.attr("Name").unwrap_or("Layer"), formdes_model::page::LAYER_COLORS[doc.layers.len() % formdes_model::page::LAYER_COLORS.len()]);
        layer.visible = l.flag("Visible").unwrap_or(true);
        layer.locked = l.flag("Locked").unwrap_or(false);
        layer.printable = l.flag("Printable").unwrap_or(true);
        c.layers.insert(l.attr("Self").unwrap_or("").to_string(), id);
        doc.layers.push(layer);
    }
    if doc.layers.is_empty() {
        let id = LayerId(doc.alloc());
        doc.layers.push(Layer::new(id, "Layer 1", formdes_model::page::LAYER_COLORS[0]));
    }
    if let Some(g) = c.file("Resources/Graphic.xml") {
        let g = g.els().next().cloned().unwrap_or_default();
        graphic(&mut c, &mut doc, &g);
    }
    if let Some(st) = c.file("Resources/Styles.xml") {
        styles(&mut c, &mut doc, &st);
    }
    let src = |tag: &str| -> Vec<String> { dm.all(tag).iter().filter_map(|e| e.attr("src").map(str::to_string)).collect() };
    let master_files = src("idPkg:MasterSpread");
    let mut master_els = Vec::new();
    for f in &master_files {
        if let Some(x) = c.file(f)
            && let Some(ms) = x.find("MasterSpread").cloned()
        {
            let pid = ParentId(doc.alloc());
            c.masters.insert(ms.attr("Self").unwrap_or("").to_string(), pid);
            master_els.push((pid, ms));
        }
    }
    for (pid, ms) in &master_els {
        let (pages, off) = pages_of(ms);
        let pgs: Vec<Page> = pages.iter().map(|(w, h, _, p)| make_page(&mut doc, *w, *h, p)).collect();
        let based = ms.attr("BasedOn").and_then(|b| c.masters.get(b)).copied();
        doc.parents.push(Parent { id: *pid, prefix: ms.attr("NamePrefix").unwrap_or("A").to_string(), name: ms.attr("BaseName").unwrap_or("Parent").to_string(), based_on: based, pages: pgs, items: Vec::new(), guides: Vec::new() });
        let pi = doc.parents.len() - 1;
        for k in ms.els() {
            build_item(&mut c, &mut doc, k, off * matrix(k.attr("ItemTransform")), Home::Parent(pi));
        }
    }
    let mut page_selfs: Vec<String> = Vec::new();
    for f in src("idPkg:Spread") {
        let Some(x) = c.file(&f) else { continue };
        let Some(sp) = x.find("Spread").cloned() else { continue };
        let (pages, off) = pages_of(&sp);
        let pgs: Vec<Page> = pages
            .iter()
            .map(|(w, h, _, p)| {
                let mut pg = make_page(&mut doc, *w, *h, p);
                pg.parent = p.attr("AppliedMaster").and_then(|m| c.masters.get(m)).copied();
                page_selfs.push(p.attr("Self").unwrap_or("").to_string());
                pg
            })
            .collect();
        let id = formdes_model::ids::SpreadId(doc.alloc());
        doc.spreads.push(Spread { id, pages: pgs, items: Vec::new(), guides: Vec::new(), transition: None });
        let si = doc.spreads.len() - 1;
        for k in sp.els() {
            build_item(&mut c, &mut doc, k, off * matrix(k.attr("ItemTransform")), Home::Spread(si));
        }
    }
    if doc.spreads.is_empty() {
        let pg = doc.blank_page();
        let id = formdes_model::ids::SpreadId(doc.alloc());
        doc.spreads.push(Spread { id, pages: vec![pg], items: Vec::new(), guides: Vec::new(), transition: None });
    }
    for f in src("idPkg:Story") {
        let Some(x) = c.file(&f) else { continue };
        let Some(st) = x.find("Story").cloned() else { continue };
        let selfr = st.attr("Self").unwrap_or("").to_string();
        let sid = doc.new_story(BASIC_PARA);
        c.stories.insert(selfr, sid);
        let mut paras = Vec::new();
        story_content(&mut c, &mut doc, sid, &st, &mut paras);
        for p in paras.iter_mut() {
            if p.segs.is_empty() {
                p.segs.push(Seg::text(""));
            }
            p.normalize();
        }
        if paras.len() > 1 && paras.last().is_some_and(|p| p.is_empty()) {
            paras.pop();
        }
        if let Some(s) = doc.story_mut(sid)
            && !paras.is_empty()
        {
            s.paras = paras;
        }
    }
    let frames = std::mem::take(&mut c.frames);
    let mut by_story: HashMap<String, Vec<(ItemId, String, String, String)>> = HashMap::new();
    for (story, id, selfr, prev, next) in frames {
        by_story.entry(story).or_default().push((id, selfr, prev, next));
    }
    for (story, list) in by_story {
        let sid = match c.stories.get(&story) {
            Some(s) => *s,
            None => doc.new_story(BASIC_PARA),
        };
        let mut order = Vec::new();
        let mut curr = list.iter().find(|f| f.2 == "n" || !list.iter().any(|g| g.1 == f.2)).or(list.first());
        let mut guard = 0;
        while let Some(f) = curr {
            if order.contains(&f.0) || guard > list.len() {
                break;
            }
            order.push(f.0);
            guard += 1;
            curr = list.iter().find(|g| g.1 == f.3);
        }
        for f in &list {
            if !order.contains(&f.0) {
                order.push(f.0);
            }
        }
        for id in &order {
            if let Some(ItemKind::Text(tf)) = doc.item_mut(*id).map(|i| &mut i.kind) {
                tf.story = sid;
            }
        }
        if let Some(s) = doc.story_mut(sid) {
            s.frames = order;
        }
    }
    let orphans: Vec<StoryId> = doc.stories.values().filter(|s| s.frames.is_empty()).map(|s| s.id).collect();
    let anchored_in: Vec<StoryId> = c.pending_anchors.iter().map(|a| a.1).collect();
    for s in orphans {
        if !anchored_in.contains(&s) {
            doc.stories.remove(&s);
        }
    }
    let dangling: Vec<ItemId> = doc.items.values().filter(|i| matches!(&i.kind, ItemKind::Text(t) if t.story.0 == 0)).map(|i| i.id).collect();
    for id in dangling {
        let sid = doc.new_story(BASIC_PARA);
        if let Some(ItemKind::Text(tf)) = doc.item_mut(id).map(|i| &mut i.kind) {
            tf.story = sid;
        }
        if let Some(s) = doc.story_mut(sid) {
            s.frames = vec![id];
        }
    }
    for sec in dm.all("Section") {
        let start = sec.attr("PageStart").and_then(|p| page_selfs.iter().position(|x| x == p)).unwrap_or(0);
        let style = match sec.attr("PageNumberStyle") {
            Some("LowerRoman") => "roman",
            Some("UpperRoman") => "ROMAN",
            Some("LowerLetters") => "alpha",
            Some("UpperLetters") => "ALPHA",
            _ => "arabic",
        };
        let cont = sec.flag("ContinueNumbering") == Some(true);
        doc.sections.push(Section {
            start,
            start_at: if cont && start > 0 { None } else { Some(sec.num("PageNumberStart").unwrap_or(1.0).max(1.0) as u32) },
            style: style.into(),
            prefix: sec.attr("SectionPrefix").unwrap_or("").to_string(),
            marker: sec.attr("Marker").unwrap_or("").to_string(),
            include_prefix: sec.flag("IncludeSectionPrefix") == Some(true),
        });
    }
    if !doc.sections.iter().any(|s| s.start == 0) {
        doc.sections.insert(0, Section { start: 0, start_at: Some(1), style: "arabic".into(), prefix: String::new(), marker: String::new(), include_prefix: false });
    }
    doc.sections.sort_by_key(|s| s.start);
    doc.sections.dedup_by_key(|s| s.start);
    doc.bump_ids();
    doc.check().map_err(|e| e.0)?;
    Ok(doc)
}
