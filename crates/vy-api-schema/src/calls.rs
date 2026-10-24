//! Request and reply shapes for every host method.
//!
//! The methods themselves are declared in `vy-app` with
//! `#[vy_api_namespace]`; this module only holds the types that cross the
//! boundary. See `docs/adr/0008-declare-plugin-api-with-macros.md`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use vy_core::Uri;

use crate::{PickerItem, StatuslineAlignment, ThemeColor, WidgetNode};

// Slots: statusline, gutter, hover, picker, views.

/// `ui.statusline.createItem` parameters.
///
/// `alignment` places the item on the left or right. `priority` orders items
/// within the same side; higher values are shown first. `command` runs when the
/// item is clicked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StatuslineCreateParams {
    pub id: String,
    pub alignment: StatuslineAlignment,
    #[serde(default)]
    #[ts(optional)]
    pub priority: Option<i32>,
    pub text: String,
    #[ts(optional = nullable)]
    pub color: Option<ThemeColor>,
    #[ts(optional = nullable)]
    pub command: Option<String>,
}

/// `ui.statusline.updateItem` parameters.
///
/// `color` and `command` distinguish an absent field (leave as is) from `null`
/// (clear it). The host validates `color` against the theme tokens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StatuslineUpdateParams {
    pub id: String,
    #[ts(optional = nullable)]
    pub text: Option<String>,
    #[ts(type = "string | null")]
    pub color: Option<Value>,
    #[ts(type = "string | null")]
    pub command: Option<Value>,
    #[ts(optional = nullable)]
    pub visible: Option<bool>,
}

/// `ui.statusline.disposeItem` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StatuslineDisposeParams {
    pub id: String,
}

/// `ui.gutter.createItem` parameters.
///
/// `line` is zero-based. The item is drawn in the gutter of that line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GutterCreateParams {
    pub id: String,
    pub line: usize,
    pub text: String,
    #[ts(optional = nullable)]
    pub color: Option<ThemeColor>,
}

/// `ui.gutter.disposeItem` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GutterDisposeParams {
    pub id: String,
}

/// `ui.hover.show` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HoverShowParams {
    pub line: usize,
    pub character: usize,
    pub text: String,
}

/// `ui.picker.show` parameters.
///
/// When `items` is omitted the picker lists commands. `placeholder` is shown
/// in the filter input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PickerShowParams {
    #[serde(default)]
    #[ts(optional)]
    pub items: Option<Vec<PickerItem>>,
    #[ts(optional = nullable)]
    pub placeholder: Option<String>,
}

/// `ui.picker.show` reply.
///
/// The reply is `null` when the picker is dismissed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PickerShowReply {
    pub selected_id: String,
}

/// `views.render` parameters.
///
/// `tree` is the widget tree returned by the view's render callback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ViewsRenderParams {
    pub id: String,
    pub tree: WidgetNode,
}

/// Direction a view pane opens toward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
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
#[ts(export)]
pub struct ViewOpenParams {
    pub view: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub direction: Option<ViewDirection>,
}

/// `views.close` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ViewCloseParams {
    pub view: String,
}

/// `views.toggle` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ViewToggleParams {
    pub view: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub direction: Option<ViewDirection>,
}

// Workspace and plugin context.

/// `workspace.info` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WorkspaceInfoReply {
    pub root_uri: Option<Uri>,
    pub name: Option<String>,
    pub folders: Vec<Uri>,
    pub trusted: bool,
}

// Events. The topic of an event is `<namespace>.<subscription name>`, and the
// subscription name starts with `on`; the shim relies on that to tell events
// from methods.

/// Payload of `workspace.onDidChangeWorkspaceFolders`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkspaceFoldersChange {
    pub folders: Vec<String>,
}
