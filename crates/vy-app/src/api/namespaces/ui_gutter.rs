//! `ui.gutter` namespace.

/// Creates and removes gutter items.
#[vy_api_schema::vy_api_namespace("ui.gutter")]
mod api {
    use serde::{Deserialize, Serialize};
    use ts_rs::TS;
    use vy_api_schema::ThemeColor;

    use crate::api::{Call, CallError, Reply};
    use crate::slots::GutterItem;

    /// `ui.gutter.createItem` parameters.
    ///
    /// `line` is zero-based. The item is drawn in the gutter of that line.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
    pub struct GutterCreateParams {
        pub id: String,
        pub line: usize,
        pub text: String,
        #[ts(optional = nullable)]
        pub color: Option<ThemeColor>,
    }

    /// `ui.gutter.disposeItem` parameters.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
    pub struct GutterDisposeParams {
        pub id: String,
    }

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
