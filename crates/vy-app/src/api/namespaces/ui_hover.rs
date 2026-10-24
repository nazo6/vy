//! `ui.hover` namespace.

/// Shows and hides hover popups.
#[vy_api_schema::vy_api_namespace("ui.hover")]
mod api {
    use serde::{Deserialize, Serialize};
    use ts_rs::TS;

    use crate::api::{Call, CallError, Reply};
    use crate::slots::HoverState;

    /// `ui.hover.show` parameters.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
    pub struct HoverShowParams {
        pub line: usize,
        pub character: usize,
        pub text: String,
    }

    /// Shows a hover popup at a position.
    #[vy_api_schema::vy_api_method]
    fn show(call: &mut Call<'_, '_>, params: HoverShowParams) -> Result<Reply<()>, CallError> {
        if let Some(hover) = call.context.hover.as_deref_mut() {
            *hover = Some(HoverState::new(params.line, params.character, params.text));
        }
        Ok(Reply::Value(()))
    }

    /// Hides the hover popup.
    #[vy_api_schema::vy_api_method]
    fn hide(call: &mut Call<'_, '_>) -> Result<Reply<()>, CallError> {
        if let Some(hover) = call.context.hover.as_deref_mut() {
            *hover = None;
        }
        Ok(Reply::Value(()))
    }
}

pub use self::api::{EVENTS, METHODS, NAMESPACE, dispatch};
