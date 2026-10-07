pub mod args;
pub mod assets;
pub mod canvas;
pub mod cmd;
pub mod input;
pub mod ops;
pub mod overlays;
pub mod registry;
pub mod session;
pub mod target;
pub mod tools;
pub mod view;
pub mod walk;
pub mod wire;

pub use registry::{Needs, Registry, Spec};
pub use session::{Ctx, DocState, Selection, Session, TextSel};

#[derive(Debug, Clone, PartialEq)]
pub struct EngineError(pub String);

impl EngineError {
    pub fn new(m: impl Into<String>) -> EngineError {
        EngineError(m.into())
    }
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for EngineError {}

impl From<formdes_model::ModelError> for EngineError {
    fn from(e: formdes_model::ModelError) -> Self {
        EngineError(e.0)
    }
}

pub type Result<T> = std::result::Result<T, EngineError>;

pub fn fail<T>(m: impl Into<String>) -> Result<T> {
    Err(EngineError::new(m))
}
