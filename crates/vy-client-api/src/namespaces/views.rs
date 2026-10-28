//! `views` declarations.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use vy_host_api::{CallError, Reply};

use crate::WidgetNode;

/// `views.register` parameters.
///
/// `actions` maps an action name (`next`, `activate`, ...) to a command id the
/// view implements, overriding the host default
/// ([ADR 0024](../../docs/adr/0024-keyboard-first-plugin-ui.md) §6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ViewsRegisterParams {
    /// View id, used by `views.open` and `viewType`.
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    /// View kind shown by `views.open`. `ui` is the widget tree kind.
    #[serde(default)]
    pub kind: Option<String>,
    /// Action name to command id.
    #[serde(default)]
    pub actions: BTreeMap<String, String>,
    /// Command id that restores the view from `params` and `state`.
    #[serde(default)]
    pub restore: Option<String>,
}

/// `views.render` parameters.
///
/// `tree` is the widget tree returned by the view's render callback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct ViewsRenderParams {
    pub id: String,
    pub tree: WidgetNode,
}

/// Direction a view pane opens toward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum ViewDirection {
    Left,
    Right,
    Above,
    Below,
}

impl ViewDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Above => "above",
            Self::Below => "below",
        }
    }
}

/// `views.open` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ViewOpenParams {
    pub view: String,
    #[serde(default)]
    pub direction: Option<ViewDirection>,
}

/// `views.close` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ViewCloseParams {
    pub view: String,
}

/// `views.toggle` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ViewToggleParams {
    pub view: String,
    #[serde(default)]
    pub direction: Option<ViewDirection>,
}

/// Registers and looks up pane views.
#[vy_api_schema::vy_api_trait("views")]
pub trait ViewsApi {
    /// Declares a view: its kind, its action overrides, and its restore command.
    #[vy_api_schema::vy_api_method]
    fn register(&mut self, params: ViewsRegisterParams) -> Result<Reply<()>, CallError>;

    /// Opens a view pane.
    #[vy_api_schema::vy_api_method]
    fn open(&mut self, params: ViewOpenParams) -> Result<Reply<()>, CallError>;

    /// Closes a view pane.
    #[vy_api_schema::vy_api_method]
    fn close(&mut self, params: ViewCloseParams) -> Result<Reply<()>, CallError>;

    /// Toggles a view pane.
    #[vy_api_schema::vy_api_method]
    fn toggle(&mut self, params: ViewToggleParams) -> Result<Reply<()>, CallError>;

    /// Renders a registered view into a widget tree.
    #[vy_api_schema::vy_api_method]
    fn render(&mut self, params: ViewsRenderParams) -> Result<Reply<()>, CallError>;
}
