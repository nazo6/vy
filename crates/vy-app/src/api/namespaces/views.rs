//! `views` namespace.

/// Registers and looks up pane views.
#[vy_api_schema::vy_api_namespace("views", sugar = ViewsSugar)]
mod api {
    use vy_api_schema::ViewsRenderParams;

    use crate::api::{Call, CallError, Reply};
    use crate::plugin_host::HostAction;

    /// Renders a registered view into a widget tree.
    #[vy_api_schema::vy_api_method(manual)]
    fn render(call: &mut Call<'_, '_>, params: ViewsRenderParams) -> Result<Reply<()>, CallError> {
        call.outcome.action = HostAction::RenderWidgetTree {
            view_id: params.id,
            tree: params.tree,
        };
        Ok(Reply::Value(()))
    }
}

pub use self::api::{METHODS, NAMESPACE, dispatch};
