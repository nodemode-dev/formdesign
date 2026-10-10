use serde::{Deserialize, Serialize};

use crate::attrs::{alpha, roman};
use crate::geom::Edges;
use crate::ids::{ItemId, LayerId, PageId, ParentId, SpreadId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub id: PageId,
    pub width: f64,
    pub height: f64,
    pub margins: Edges,
    pub columns: u32,
    pub gutter: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<ParentId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub detached: Vec<ItemId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_rule: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub joined: bool,
}

impl Page {
    pub fn new(id: PageId, width: f64, height: f64) -> Page {
        Page { id, width, height, margins: Edges::all(36.0), columns: 1, gutter: 12.0, parent: None, detached: Vec::new(), layout_rule: None, joined: false }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Orientation {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Guide {
    pub orientation: Orientation,
    pub pos: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<usize>,
    #[serde(default = "guide_color")]
    pub color: String,
    #[serde(default)]
    pub locked: bool,
}

fn guide_color() -> String {
    "#4ae3ff".into()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Spread {
    pub id: SpreadId,
    pub pages: Vec<Page>,
    pub items: Vec<ItemId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guides: Vec<Guide>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<serde_json::Value>,
}

impl Spread {
    pub fn page_x(&self, index: usize) -> f64 {
        self.pages.iter().take(index).map(|p| p.width).sum()
    }

    pub fn width(&self) -> f64 {
        self.pages.iter().map(|p| p.width).sum()
    }

    pub fn height(&self) -> f64 {
        self.pages.iter().map(|p| p.height).fold(0.0, f64::max)
    }

    pub fn page_at_x(&self, x: f64) -> Option<usize> {
        let mut acc = 0.0;
        for (i, p) in self.pages.iter().enumerate() {
            if x >= acc && x < acc + p.width {
                return Some(i);
            }
            acc += p.width;
        }
        None
    }

    pub fn nearest_page(&self, x: f64) -> usize {
        self.page_at_x(x).unwrap_or(if x < 0.0 { 0 } else { self.pages.len().saturating_sub(1) })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parent {
    pub id: ParentId,
    pub prefix: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub based_on: Option<ParentId>,
    pub pages: Vec<Page>,
    pub items: Vec<ItemId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guides: Vec<Guide>,
}

impl Parent {
    pub fn label(&self) -> String {
        format!("{}-{}", self.prefix, self.name)
    }

    pub fn page_x(&self, index: usize) -> f64 {
        self.pages.iter().take(index).map(|p| p.width).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Section {
    pub start: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_at: Option<u32>,
    #[serde(default = "arabic")]
    pub style: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub prefix: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub marker: String,
    #[serde(default)]
    pub include_prefix: bool,
}

fn arabic() -> String {
    "arabic".into()
}

pub fn format_number(n: u32, style: &str) -> String {
    match style {
        "roman" => roman(n as i64).to_lowercase(),
        "ROMAN" => roman(n as i64),
        "alpha" => alpha(n as i64).to_lowercase(),
        "ALPHA" => alpha(n as i64),
        "01" => format!("{n:02}"),
        "001" => format!("{n:03}"),
        _ => n.to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub id: LayerId,
    pub name: String,
    pub color: String,
    pub visible: bool,
    pub locked: bool,
    pub printable: bool,
}

impl Layer {
    pub fn new(id: LayerId, name: &str, color: &str) -> Layer {
        Layer { id, name: name.into(), color: color.into(), visible: true, locked: false, printable: true }
    }
}

pub const LAYER_COLORS: &[&str] = &["#4f8bff", "#e5484d", "#30a46c", "#f5a524", "#8e4ec6", "#12a594", "#e93d82", "#6e56cf"];
