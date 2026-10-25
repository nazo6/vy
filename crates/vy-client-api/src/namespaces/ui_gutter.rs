//! `ui.gutter` declarations.

use serde::{Deserialize, Serialize};
use vy_host_api::{CallError, Reply};

use crate::ThemeColor;

/// `ui.gutter.createItem` parameters.
///
/// `line` is zero-based. The item is drawn in the gutter of that line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct GutterCreateParams {
    pub id: String,
    pub line: usize,
    pub text: String,
    #[serde(default)]
    pub color: Option<ThemeColor>,
}

/// `ui.gutter.disposeItem` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct GutterDisposeParams {
    pub id: String,
}

/// Creates and removes gutter items.
#[vy_api_schema::vy_api_trait("ui.gutter")]
pub trait UiGutterApi {
    /// Creates a gutter item and returns its id.
    #[vy_api_schema::vy_api_method]
    fn create_item(&mut self, params: GutterCreateParams) -> Result<Reply<()>, CallError>;

    /// Removes one gutter item.
    #[vy_api_schema::vy_api_method]
    fn dispose_item(&mut self, params: GutterDisposeParams) -> Result<Reply<()>, CallError>;

    /// Removes every gutter item.
    #[vy_api_schema::vy_api_method]
    fn clear(&mut self) -> Result<Reply<()>, CallError>;
}
