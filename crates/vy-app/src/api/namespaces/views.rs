//! `views` namespace.

/// Registers and looks up pane views.
#[vy_api_schema::vy_api_namespace("views")]
mod api {
    use serde_json::json;
    use vy_api_schema::{
        ViewCloseParams, ViewDirection, ViewOpenParams, ViewToggleParams, ViewsRenderParams,
    };

    use crate::api::{Call, CallError, Reply};
    use crate::commands::UiCommand;
    use crate::plugin_host::HostAction;

    /// Opens a view pane.
    #[vy_api_schema::vy_api_method]
    fn open(call: &mut Call<'_, '_>, params: ViewOpenParams) -> Result<Reply<()>, CallError> {
        call.outcome.action = HostAction::Ui {
            command: UiCommand::ViewOpen,
            args: Some(json!({
                "view": params.view,
                "direction": params.direction.map(ViewDirection::as_str).unwrap_or("left"),
            })),
        };
        Ok(Reply::Value(()))
    }

    /// Closes a view pane.
    #[vy_api_schema::vy_api_method]
    fn close(call: &mut Call<'_, '_>, params: ViewCloseParams) -> Result<Reply<()>, CallError> {
        call.outcome.action = HostAction::Ui {
            command: UiCommand::ViewClose,
            args: Some(json!({ "view": params.view })),
        };
        Ok(Reply::Value(()))
    }

    /// Toggles a view pane.
    #[vy_api_schema::vy_api_method]
    fn toggle(call: &mut Call<'_, '_>, params: ViewToggleParams) -> Result<Reply<()>, CallError> {
        call.outcome.action = HostAction::Ui {
            command: UiCommand::ViewToggle,
            args: Some(json!({
                "view": params.view,
                "direction": params.direction.map(ViewDirection::as_str).unwrap_or("left"),
            })),
        };
        Ok(Reply::Value(()))
    }

    /// Renders a registered view into a widget tree.
    #[vy_api_schema::vy_api_method]
    fn render(call: &mut Call<'_, '_>, params: ViewsRenderParams) -> Result<Reply<()>, CallError> {
        call.outcome.action = HostAction::RenderWidgetTree {
            view_id: params.id,
            tree: params.tree,
        };
        Ok(Reply::Value(()))
    }
}

pub use self::api::{EVENTS, METHODS, NAMESPACE, dispatch};
