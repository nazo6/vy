//! `views` declarations.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use vy_api_schema::{EventDecl, MethodDecl, NamespaceDecl};

use crate::{WidgetNode, method_decl};

/// `views.render` parameters.
///
/// `tree` is the widget tree returned by the view's render callback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ViewsRenderParams {
    pub id: String,
    pub tree: WidgetNode,
}

/// Direction a view pane opens toward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ViewOpenParams {
    pub view: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub direction: Option<ViewDirection>,
}

/// `views.close` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ViewCloseParams {
    pub view: String,
}

/// `views.toggle` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ViewToggleParams {
    pub view: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub direction: Option<ViewDirection>,
}

method_decl!(OPEN, "views.open", ViewOpenParams, (), "Opens a view pane.");
method_decl!(
    CLOSE,
    "views.close",
    ViewCloseParams,
    (),
    "Closes a view pane."
);
method_decl!(
    TOGGLE,
    "views.toggle",
    ViewToggleParams,
    (),
    "Toggles a view pane."
);
method_decl!(
    RENDER,
    "views.render",
    ViewsRenderParams,
    (),
    "Renders a registered view into a widget tree."
);

/// Every method in this namespace.
pub static METHODS: &[MethodDecl] = &[OPEN, CLOSE, TOGGLE, RENDER];

/// Every event in this namespace.
pub static EVENTS: &[EventDecl] = &[];

/// JSDoc and sugar metadata for the namespace.
pub static NAMESPACE: NamespaceDecl = NamespaceDecl {
    path: "views",
    doc: "Registers and looks up pane views.",
    sugar: None,
};
