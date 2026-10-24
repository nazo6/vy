//! `ui.gutter` namespace.

/// Creates and removes gutter items.
#[vy_api_schema::vy_api_namespace("ui.gutter")]
mod api {
    use vy_api_schema::{GutterCreateParams, GutterDisposeParams};

    use crate::api::{Call, CallError, Reply};
    use crate::slots::GutterItem;

    /// Creates a gutter item and returns its id.
    #[vy_api_schema::vy_api_method]
    fn create_item(
        call: &mut Call<'_, '_>,
        params: GutterCreateParams,
    ) -> Result<Reply<()>, CallError> {
        if let Some(gutter) = call.context.gutter.as_deref_mut() {
            let mut item = GutterItem::new(params.id, params.line, params.text);
            item.color = params.color;
            gutter.set_item(item);
        }
        Ok(Reply::Value(()))
    }

    /// Removes one gutter item.
    #[vy_api_schema::vy_api_method]
    fn dispose_item(
        call: &mut Call<'_, '_>,
        params: GutterDisposeParams,
    ) -> Result<Reply<()>, CallError> {
        if let Some(gutter) = call.context.gutter.as_deref_mut() {
            gutter.dispose_item(&params.id);
        }
        Ok(Reply::Value(()))
    }

    /// Removes every gutter item.
    #[vy_api_schema::vy_api_method]
    fn clear(call: &mut Call<'_, '_>) -> Result<Reply<()>, CallError> {
        if let Some(gutter) = call.context.gutter.as_deref_mut() {
            gutter.clear();
        }
        Ok(Reply::Value(()))
    }
}

pub use self::api::{EVENTS, METHODS, NAMESPACE, dispatch};
