use std::sync::Arc;

use kurbo::{Affine, BezPath, Ellipse, Line, Point, Rect, RoundedRect, Shape as _};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::attrs::Attrs;
use crate::color::Paint;
use crate::geom::Edges;
use crate::ids::{AssetId, ItemId, LayerId, StoryId};

pub mod affine_serde {
    use super::*;
    pub fn serialize<S: Serializer>(a: &Affine, s: S) -> Result<S::Ok, S::Error> {
        a.as_coeffs().serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Affine, D::Error> {
        Ok(Affine::new(<[f64; 6]>::deserialize(d)?))
    }
}

pub mod path_serde {
    use super::*;
    pub fn serialize<S: Serializer>(p: &BezPath, s: S) -> Result<S::Ok, S::Error> {
        p.to_svg().serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<BezPath, D::Error> {
        let s = String::deserialize(d)?;
        BezPath::from_svg(&s).map_err(serde::de::Error::custom)
    }
}

pub mod rect_serde {
    use super::*;
    pub fn serialize<S: Serializer>(r: &Rect, s: S) -> Result<S::Ok, S::Error> {
        [r.x0, r.y0, r.x1, r.y1].serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Rect, D::Error> {
        let [a, b, c, e] = <[f64; 4]>::deserialize(d)?;
        Ok(Rect::new(a, b, c, e))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Shape {
    Rect {
        #[serde(with = "rect_serde")]
        rect: Rect,
    },
    Ellipse {
        #[serde(with = "rect_serde")]
        rect: Rect,
    },
    Polygon {
        #[serde(with = "rect_serde")]
        rect: Rect,
        sides: u32,
        inset: f64,
    },
    Line {
        from: (f64, f64),
        to: (f64, f64),
    },
    Path {
        #[serde(with = "path_serde")]
        path: BezPath,
        closed: bool,
    },
}

impl Shape {
    pub fn rect(r: Rect) -> Shape {
        Shape::Rect { rect: r }
    }

    pub fn bbox(&self) -> Rect {
        match self {
            Shape::Rect { rect } | Shape::Ellipse { rect } | Shape::Polygon { rect, .. } => *rect,
            Shape::Line { from, to } => Rect::from_points(Point::new(from.0, from.1), Point::new(to.0, to.1)),
            Shape::Path { path, .. } => path.bounding_box(),
        }
    }

    pub fn closed(&self) -> bool {
        match self {
            Shape::Line { .. } => false,
            Shape::Path { closed, .. } => *closed,
            _ => true,
        }
    }

    pub fn to_path(&self, corner: &Corner) -> BezPath {
        match self {
            Shape::Rect { rect } if corner.radius > 0.0 && corner.kind == "rounded" => RoundedRect::from_rect(*rect, corner.radius.min(rect.width().abs() / 2.0).min(rect.height().abs() / 2.0)).to_path(0.1),
            Shape::Rect { rect } if corner.radius > 0.0 => corner_path(*rect, corner),
            Shape::Rect { rect } => rect.to_path(0.1),
            Shape::Ellipse { rect } => Ellipse::from_rect(*rect).to_path(0.1),
            Shape::Polygon { rect, sides, inset } => polygon_path(*rect, *sides, *inset),
            Shape::Line { from, to } => Line::new(*from, *to).to_path(0.1),
            Shape::Path { path, .. } => path.clone(),
        }
    }

    pub fn with_rect(&self, r: Rect) -> Shape {
        match self {
            Shape::Rect { .. } => Shape::Rect { rect: r },
            Shape::Ellipse { .. } => Shape::Ellipse { rect: r },
            Shape::Polygon { sides, inset, .. } => Shape::Polygon { rect: r, sides: *sides, inset: *inset },
            Shape::Line { from, to } => {
                let old = Rect::from_points(Point::new(from.0, from.1), Point::new(to.0, to.1));
                let map = map_rect(old, r);
                let a = map * Point::new(from.0, from.1);
                let b = map * Point::new(to.0, to.1);
                Shape::Line { from: (a.x, a.y), to: (b.x, b.y) }
            }
            Shape::Path { path, closed } => {
                let map = map_rect(path.bounding_box(), r);
                Shape::Path { path: map * path.clone(), closed: *closed }
            }
        }
    }
}

pub fn map_rect(from: Rect, to: Rect) -> Affine {
    let sx = if from.width().abs() < 1e-9 { 1.0 } else { to.width() / from.width() };
    let sy = if from.height().abs() < 1e-9 { 1.0 } else { to.height() / from.height() };
    Affine::translate((to.x0, to.y0)) * Affine::scale_non_uniform(sx, sy) * Affine::translate((-from.x0, -from.y0))
}

fn polygon_path(r: Rect, sides: u32, inset: f64) -> BezPath {
    let n = sides.max(3) as usize;
    let c = r.center();
    let (rx, ry) = (r.width() / 2.0, r.height() / 2.0);
    let star = inset > 0.0;
    let count = if star { n * 2 } else { n };
    let mut p = BezPath::new();
    for i in 0..count {
        let a = -std::f64::consts::FRAC_PI_2 + i as f64 * std::f64::consts::TAU / count as f64;
        let k = if star && i % 2 == 1 { 1.0 - inset.clamp(0.0, 1.0) } else { 1.0 };
        let pt = Point::new(c.x + rx * k * a.cos(), c.y + ry * k * a.sin());
        if i == 0 { p.move_to(pt) } else { p.line_to(pt) }
    }
    p.close_path();
    p
}

fn corner_path(r: Rect, corner: &Corner) -> BezPath {
    let rad = corner.radius.min(r.width().abs() / 2.0).min(r.height().abs() / 2.0);
    let mut p = BezPath::new();
    let pts = [(r.x0, r.y0, 1.0, 1.0), (r.x1, r.y0, -1.0, 1.0), (r.x1, r.y1, -1.0, -1.0), (r.x0, r.y1, 1.0, -1.0)];
    for (i, (x, y, dx, dy)) in pts.iter().enumerate() {
        let (into, out) = if i % 2 == 0 { (Point::new(*x, y + dy * rad), Point::new(x + dx * rad, *y)) } else { (Point::new(x + dx * rad, *y), Point::new(*x, y + dy * rad)) };
        if i == 0 { p.move_to(into) } else { p.line_to(into) }
        match corner.kind.as_str() {
            "bevel" => p.line_to(out),
            "inset" => {
                p.line_to(Point::new(into.x + if i % 2 == 0 { dx * rad } else { 0.0 }, into.y + if i % 2 == 0 { 0.0 } else { dy * rad }));
                p.line_to(out);
            }
            "inverse" => p.quad_to(Point::new(x + dx * rad, y + dy * rad), out),
            _ => p.quad_to(Point::new(*x, *y), out),
        }
    }
    p.close_path();
    p
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Corner {
    pub radius: f64,
    pub kind: String,
}

impl Default for Corner {
    fn default() -> Self {
        Corner { radius: 0.0, kind: "rounded".into() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StrokeAlign {
    Center,
    Inside,
    Outside,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub paint: Paint,
    pub weight: f64,
    pub align: StrokeAlign,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dash: Vec<f64>,
    #[serde(default = "butt")]
    pub cap: String,
    #[serde(default = "miter")]
    pub join: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub start: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub end: String,
}

fn butt() -> String {
    "butt".into()
}

fn miter() -> String {
    "miter".into()
}

impl Default for Stroke {
    fn default() -> Self {
        Stroke { paint: Paint::None, weight: 1.0, align: StrokeAlign::Center, dash: Vec::new(), cap: butt(), join: miter(), start: String::new(), end: String::new() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VJust {
    Top,
    Center,
    Bottom,
    Justify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FirstBaseline {
    Ascent,
    CapHeight,
    XHeight,
    Leading,
    Fixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AutoSize {
    Off,
    Height,
    Width,
    Both,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathText {
    pub start: f64,
    pub flip: bool,
    pub align: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextFrame {
    pub story: StoryId,
    pub columns: u32,
    pub gutter: f64,
    #[serde(default)]
    pub insets: Edges,
    pub vjust: VJust,
    pub first_baseline: FirstBaseline,
    #[serde(default)]
    pub first_baseline_min: f64,
    pub autosize: AutoSize,
    #[serde(default)]
    pub balance: bool,
    #[serde(default)]
    pub ignore_wrap: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_path: Option<PathText>,
    #[serde(default)]
    pub fixed_column_width: Option<f64>,
    #[serde(default)]
    pub column_rule: ColumnRule,
    #[serde(default)]
    pub max_column_width: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ColumnRule {
    #[default]
    Number,
    Fixed,
    Flexible,
}

impl TextFrame {
    pub fn new(story: StoryId) -> TextFrame {
        TextFrame { story, columns: 1, gutter: 12.0, insets: Edges::default(), vjust: VJust::Top, first_baseline: FirstBaseline::Ascent, first_baseline_min: 0.0, autosize: AutoSize::Off, balance: false, ignore_wrap: false, on_path: None, fixed_column_width: None, column_rule: ColumnRule::Number, max_column_width: 0.0 }
    }

    pub fn column_plan(&self, inner_width: f64) -> (u32, Option<f64>) {
        let g = self.gutter.max(0.0);
        let w = inner_width.max(1.0);
        match (self.column_rule, self.fixed_column_width.filter(|f| *f > 0.0)) {
            (ColumnRule::Fixed, Some(cw)) => (((w + g) / (cw + g) + 1e-6).floor().max(1.0).min(216.0) as u32, Some(cw)),
            (ColumnRule::Flexible, _) if self.max_column_width > 0.0 => ((((w + g) / (self.max_column_width + g) - 1e-6).ceil()).max(1.0).min(216.0) as u32, None),
            (_, fixed) => (self.columns.max(1), fixed),
        }
    }

    pub fn fitted_width(&self, columns: u32) -> Option<f64> {
        match (self.column_rule, self.fixed_column_width.filter(|f| *f > 0.0)) {
            (ColumnRule::Fixed, Some(cw)) => Some(cw * columns as f64 + self.gutter.max(0.0) * (columns as f64 - 1.0) + self.insets.inside + self.insets.outside),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fitting {
    pub kind: String,
    pub auto: bool,
    #[serde(default)]
    pub reference: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Graphic {
    pub asset: Option<AssetId>,
    #[serde(with = "affine_serde")]
    pub content: Affine,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fitting: Option<Fitting>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub name: String,
    pub items: Vec<ItemId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ItemKind {
    Plain,
    Text(TextFrame),
    Graphic(Graphic),
    Group { children: Vec<ItemId> },
    States { states: Vec<State>, active: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WrapMode {
    None,
    BoundingBox,
    Contour,
    JumpObject,
    NextColumn,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextWrap {
    pub mode: WrapMode,
    pub offsets: Edges,
    #[serde(default)]
    pub invert: bool,
    #[serde(default = "both")]
    pub side: String,
}

fn both() -> String {
    "both".into()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: ItemId,
    pub kind: ItemKind,
    pub shape: Shape,
    #[serde(with = "affine_serde")]
    pub xf: Affine,
    pub layer: LayerId,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    pub fill: Paint,
    pub stroke: Stroke,
    #[serde(default = "full")]
    pub opacity: f64,
    #[serde(default = "normal", skip_serializing_if = "is_normal")]
    pub blend: String,
    #[serde(default)]
    pub corner: Corner,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub nonprinting: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrap: Option<TextWrap>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub alt_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<ItemId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchored_in: Option<StoryId>,
    #[serde(default, skip_serializing_if = "Attrs::is_empty")]
    pub effects: Attrs,
    #[serde(default, skip_serializing_if = "Attrs::is_empty")]
    pub extra: Attrs,
}

fn full() -> f64 {
    1.0
}

fn normal() -> String {
    "normal".into()
}

fn is_normal(s: &String) -> bool {
    s == "normal"
}

impl Item {
    pub fn new(id: ItemId, kind: ItemKind, shape: Shape, layer: LayerId) -> Item {
        Item {
            id,
            kind,
            shape,
            xf: Affine::IDENTITY,
            layer,
            name: String::new(),
            label: String::new(),
            fill: Paint::None,
            stroke: Stroke::default(),
            opacity: 1.0,
            blend: normal(),
            corner: Corner::default(),
            locked: false,
            hidden: false,
            nonprinting: false,
            wrap: None,
            style: None,
            alt_text: String::new(),
            group: None,
            anchored_in: None,
            effects: Attrs::new(),
            extra: Attrs::new(),
        }
    }

    pub fn local_path(&self) -> BezPath {
        self.shape.to_path(&self.corner)
    }

    pub fn path(&self) -> BezPath {
        self.xf * self.local_path()
    }

    pub fn own_bounds(&self) -> Rect {
        self.xf.transform_rect_bbox(self.shape.bbox())
    }

    pub fn story(&self) -> Option<StoryId> {
        match &self.kind {
            ItemKind::Text(t) => Some(t.story),
            _ => None,
        }
    }

    pub fn text(&self) -> Option<&TextFrame> {
        match &self.kind {
            ItemKind::Text(t) => Some(t),
            _ => None,
        }
    }

    pub fn text_mut(&mut self) -> Option<&mut TextFrame> {
        match &mut self.kind {
            ItemKind::Text(t) => Some(t),
            _ => None,
        }
    }

    pub fn graphic(&self) -> Option<&Graphic> {
        match &self.kind {
            ItemKind::Graphic(g) => Some(g),
            _ => None,
        }
    }

    pub fn children(&self) -> Vec<ItemId> {
        match &self.kind {
            ItemKind::Group { children } => children.clone(),
            ItemKind::States { states, active } => states.get(*active).map(|s| s.items.clone()).unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    pub fn all_children(&self) -> Vec<ItemId> {
        match &self.kind {
            ItemKind::Group { children } => children.clone(),
            ItemKind::States { states, .. } => states.iter().flat_map(|s| s.items.clone()).collect(),
            _ => Vec::new(),
        }
    }

    pub fn kind_name(&self) -> &'static str {
        match &self.kind {
            ItemKind::Text(t) if t.on_path.is_some() => "Type on a path",
            ItemKind::Text(_) => "Text frame",
            ItemKind::Graphic(g) if g.asset.is_some() => "Graphic frame",
            ItemKind::Graphic(_) => "Empty graphic frame",
            ItemKind::Group { .. } => "Group",
            ItemKind::States { .. } => "Multi-state object",
            ItemKind::Plain => match self.shape {
                Shape::Rect { .. } => "Rectangle",
                Shape::Ellipse { .. } => "Ellipse",
                Shape::Polygon { .. } => "Polygon",
                Shape::Line { .. } => "Line",
                Shape::Path { .. } => "Path",
            },
        }
    }

    pub fn display_name(&self) -> String {
        if !self.name.is_empty() { self.name.clone() } else { format!("<{}>", self.kind_name().to_lowercase()) }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    pub id: AssetId,
    pub name: String,
    pub mime: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    #[serde(skip)]
    pub data: Arc<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pixels: Option<(u32, u32)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ppi: Option<(f64, f64)>,
    #[serde(default)]
    pub page: u32,
    #[serde(default)]
    pub embedded: bool,
    #[serde(default)]
    pub modified: u64,
}

impl Asset {
    pub fn natural_size(&self) -> (f64, f64) {
        match (self.pixels, self.ppi) {
            (Some((w, h)), Some((px, py))) if px > 0.0 && py > 0.0 => (w as f64 * 72.0 / px, h as f64 * 72.0 / py),
            (Some((w, h)), _) => (w as f64, h as f64),
            _ => (100.0, 100.0),
        }
    }

    pub fn is_media(&self) -> bool {
        self.mime.starts_with("video/") || self.mime.starts_with("audio/")
    }
}
