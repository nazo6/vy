//! `ui.statusline` declarations.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use vy_host_api::{CallError, Reply};

use crate::{ThemeColor, statusline::StatuslineAlignment};

/// `ui.statusline.createItem` parameters.
///
/// `alignment` places the item on the left or right. `priority` orders items
/// within the same side; higher values are shown first. `command` runs when the
/// item is clicked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct StatuslineCreateParams {
    pub id: String,
    pub alignment: StatuslineAlignment,
    #[serde(default)]
    pub priority: Option<i32>,
    pub text: String,
    #[serde(default)]
    pub color: Option<ThemeColor>,
    #[serde(default)]
    pub command: Option<String>,
}

/// `ui.statusline.updateItem` parameters.
///
/// `color` and `command` distinguish an absent field (leave as is) from `null`
/// (clear it). The host validates `color` against the theme tokens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct StatuslineUpdateParams {
    pub id: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    #[specta(type = Option<String>)]
    pub color: Option<Value>,
    #[serde(default)]
    #[specta(type = Option<String>)]
    pub command: Option<Value>,
    #[serde(default)]
    pub visible: Option<bool>,
}

/// `ui.statusline.disposeItem` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct StatuslineDisposeParams {
    pub id: String,
}

/// Creates and updates status line items.
#[vy_api_schema::vy_api_trait("ui.statusline")]
pub trait UiStatuslineApi {
    /// Creates a status line item and returns its id.
    #[vy_api_schema::vy_api_method]
    fn create_item(&mut self, params: StatuslineCreateParams) -> Result<Reply<()>, CallError>;

    /// Updates the fields of a status line item.
    #[vy_api_schema::vy_api_method]
    fn update_item(&mut self, params: StatuslineUpdateParams) -> Result<Reply<()>, CallError>;

    /// Removes a status line item.
    #[vy_api_schema::vy_api_method]
    fn dispose_item(&mut self, params: StatuslineDisposeParams) -> Result<Reply<()>, CallError>;
}
