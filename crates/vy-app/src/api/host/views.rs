//! Client implementation of the `views` host trait.

use serde_json::json;
use vy_client_api::namespaces::views::{
    ViewCloseParams, ViewDirection, ViewOpenParams, ViewToggleParams, ViewsApi, ViewsRenderParams,
};

use super::ClientHost;
use crate::api::{CallError, Reply};
use crate::commands::UiCommand;
use crate::plugin_host::HostAction;

impl ViewsApi for ClientHost<'_, '_, '_> {
    fn open(&mut self, params: ViewOpenParams) -> Result<Reply<()>, CallError> {
        self.call.outcome.action = HostAction::Ui {
            command: UiCommand::ViewOpen,
            args: Some(json!({
                "view": params.view,
                "direction": params.direction.map(ViewDirection::as_str).unwrap_or("left"),
            })),
        };
        Ok(Reply::Value(()))
    }

    fn close(&mut self, params: ViewCloseParams) -> Result<Reply<()>, CallError> {
        self.call.outcome.action = HostAction::Ui {
            command: UiCommand::ViewClose,
            args: Some(json!({ "view": params.view })),
        };
        Ok(Reply::Value(()))
    }

    fn toggle(&mut self, params: ViewToggleParams) -> Result<Reply<()>, CallError> {
        self.call.outcome.action = HostAction::Ui {
            command: UiCommand::ViewToggle,
            args: Some(json!({
                "view": params.view,
                "direction": params.direction.map(ViewDirection::as_str).unwrap_or("left"),
            })),
        };
        Ok(Reply::Value(()))
    }

    fn render(&mut self, params: ViewsRenderParams) -> Result<Reply<()>, CallError> {
        self.call.outcome.action = HostAction::RenderWidgetTree {
            view_id: params.id,
            tree: params.tree,
        };
        Ok(Reply::Value(()))
    }
}
