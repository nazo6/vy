//! `ui` namespace: plugin UI actions and focus info (ADR 0024 §5, ADR 0032 Phase 2).
//!
//! Widget builders live in the shim. The namespace groups the slot namespaces.

use serde::{Deserialize, Serialize};
use vy_host_api::{CallError, Reply};

/// Arguments a plugin command receives for a `vy.ui.*` action (ADR 0024 §5).
///
/// The host builds this from the active pane when a `vy.ui.*` command runs.
/// Fields a button-list view cannot provide stay empty: the host knows the
/// focused widget, but not list selection, tree expansion, or picker query
/// state, which live in the plugin until native list/tree nodes arrive.
/// `action` is the canonical action name (`next`, `activate`, …) or the
/// `action` argument a key binding attached to the command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UiActionArgs {
    pub action: String,
    pub view_id: String,
    pub pane_id: u64,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub focus_id: Option<String>,
    #[serde(default)]
    pub selected_ids: Vec<String>,
    #[serde(default)]
    pub anchor_id: Option<String>,
    #[serde(default)]
    pub expanded_ids: Vec<String>,
    #[serde(default)]
    pub depth: Option<u32>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub count: Option<u32>,
    #[serde(default)]
    pub register: Option<String>,
}

/// The focused widget of the active UI pane, as returned by `ui.focused()`.
///
/// This is `null` for editor panes and for UI panes with no focusable widget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UiFocused {
    pub view_id: String,
    pub pane_id: u64,
    pub role: String,
    pub focus_id: String,
}

/// Builds native widget trees and updates UI slots.
#[vy_api_schema::vy_api_trait("ui")]
pub trait UiApi {
    /// Returns the focused widget of the active UI pane, or null.
    #[vy_api_schema::vy_api_method]
    fn focused(&mut self) -> Result<Reply<Option<UiFocused>>, CallError>;
}
