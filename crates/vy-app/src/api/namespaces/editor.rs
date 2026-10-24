//! `editor` namespace.

/// Reads and edits the active document.
#[vy_api_schema::vy_api_namespace("editor")]
mod api {
    use vy_api_schema::{EditorSetModeParams, Mode};

    use crate::api::{Call, CallError, Reply};

    /// Returns the editor mode of the focused pane.
    #[vy_api_schema::vy_api_method]
    fn mode(call: &mut Call<'_, '_>) -> Result<Reply<Mode>, CallError> {
        Ok(Reply::Value(call.context.editor.mode()))
    }

    /// Switches the focused pane to a mode.
    #[vy_api_schema::vy_api_method]
    fn set_mode(
        call: &mut Call<'_, '_>,
        params: EditorSetModeParams,
    ) -> Result<Reply<()>, CallError> {
        call.context.editor.set_mode(params.mode);
        Ok(Reply::Value(()))
    }
}

pub use self::api::{METHODS, NAMESPACE, dispatch};
