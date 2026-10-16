pub mod attrs;
pub mod captionmeta;
pub mod color;
pub mod corners;
pub mod doc;
pub mod format;
pub mod geom;
pub mod ids;
pub mod item;
pub mod page;
pub mod story;
pub mod style;

pub use attrs::{Attrs, CharProps, ParaProps};
pub use color::{Color, Paint, Swatch};
pub use doc::{Doc, DocSettings, Intent};
pub use geom::{Edges, Rect};
pub use ids::{AssetId, ItemId, LayerId, PageId, ParentId, SpreadId, StoryId};
pub use item::{Item, ItemKind, Shape, Stroke};
pub use page::{Page, Parent, Section, Spread};
pub use story::{Para, Run, Story, TextPos};
pub use style::Styles;

pub type Result<T> = std::result::Result<T, ModelError>;

#[derive(Debug, Clone, PartialEq)]
pub struct ModelError(pub String);

impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ModelError {}

pub fn err<T>(msg: impl Into<String>) -> Result<T> {
    Err(ModelError(msg.into()))
}
