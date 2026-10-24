//! `views` namespace.

/// Registers and looks up pane views.
#[vy_api_schema::vy_api_namespace("views")]
mod api {
    use crate::widget::WidgetNode;
    use serde::{Deserialize, Serialize};
    use serde_json::json;
    use ts_rs::TS;

    use crate::api::{Call, CallError, Reply};
    use crate::commands::UiCommand;
    use crate::plugin_host::HostAction;

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
