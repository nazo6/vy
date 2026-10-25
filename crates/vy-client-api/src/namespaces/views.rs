//! `views` declarations.

use serde::{Deserialize, Serialize};
use vy_host_api::{CallError, Reply};

use crate::WidgetNode;

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
