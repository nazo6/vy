//! `ui` declarations.
//!
//! Widget builders live in the shim. This namespace also answers queries about
//! the widget that currently holds focus, so a plugin can act on the focused
//! item without asking the host to round-trip focus state first
//! ([ADR 0024](../../docs/adr/0024-keyboard-first-plugin-ui.md) §5).

use serde::{Deserialize, Serialize};
use vy_host_api::{CallError, Reply};

use crate::widget::UiRole;

/// The focused widget, as `ui.focused` reports it.
///
/// Selection and expansion state arrive with the list and tree widgets, so
/// they are not reported yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UiFocus {
    /// `id` of the focused widget.
    pub focus_id: String,
    pub role: UiRole,
    /// `id` of the pane the widget lives in.
    pub pane_id: u64,
    /// View id the pane shows.
    pub view_id: String,
}

/// Builds native widget trees and updates UI slots.
#[vy_api_schema::vy_api_trait("ui")]
pub trait UiApi {
    /// Returns the focused widget, or `null` in an editor pane.
    #[vy_api_schema::vy_api_method]
    fn focused(&mut self) -> Result<Reply<Option<UiFocus>>, CallError>;
}
