//! `editor` namespace.

/// Reads and edits the active document.
#[vy_api_schema::vy_api_namespace("editor", shared = vy_client_api::namespaces::editor)]
mod api {
    use serde_json::json;
    use vy_client_api::keymap::Mode;
    use vy_client_api::namespaces::editor::{
        EditorEditParams, EditorEditReply, EditorOpenParams, EditorSetModeParams,
        EditorSetSelectionsParams,
    };
    use vy_core::Selection;
    use vy_host_api::DocumentInfoReply;

    use crate::api::{Call, CallError, Reply, document_info};
    use crate::commands::DocumentCommand;
    use crate::plugin_host::HostAction;

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

    /// Opens a document in an editor pane.
    #[vy_api_schema::vy_api_method]
    fn open(call: &mut Call<'_, '_>, params: EditorOpenParams) -> Result<Reply<()>, CallError> {
        call.outcome.action = HostAction::Document {
            command: DocumentCommand::Open,
            args: Some(json!({
                "uri": params.uri.to_string(),
                "paneId": params.pane_id,
            })),
        };
        Ok(Reply::Value(()))
    }

    /// Returns the document in the focused pane, or null when none is open.
    #[vy_api_schema::vy_api_method]
    fn active_document(
        call: &mut Call<'_, '_>,
    ) -> Result<Reply<Option<DocumentInfoReply>>, CallError> {
        let document = call
            .context
            .document
            .as_ref()
            .map(|document| document_info(call.context.editor, document));
        Ok(Reply::Value(document))
    }

    /// Returns every open document.
    #[vy_api_schema::vy_api_method]
    fn documents(call: &mut Call<'_, '_>) -> Result<Reply<Vec<DocumentInfoReply>>, CallError> {
        let documents = call
            .context
            .document
            .iter()
            .map(|document| document_info(call.context.editor, document))
            .collect();
        Ok(Reply::Value(documents))
    }

    /// Returns the selections of the focused pane.
    #[vy_api_schema::vy_api_method]
    fn selections(call: &mut Call<'_, '_>) -> Result<Reply<Vec<Selection>>, CallError> {
        Ok(Reply::Value(call.context.editor.selections()))
    }

    /// Replaces the selections of the focused pane.
    #[vy_api_schema::vy_api_method]
    fn set_selections(
        call: &mut Call<'_, '_>,
        params: EditorSetSelectionsParams,
    ) -> Result<Reply<()>, CallError> {
        call.context.editor.set_selections(&params.selections);
        Ok(Reply::Value(()))
    }

    /// Applies edits to the active document in one transaction.
    #[vy_api_schema::vy_api_method]
    fn edit(
        call: &mut Call<'_, '_>,
        params: EditorEditParams,
    ) -> Result<Reply<EditorEditReply>, CallError> {
        if params.edits.is_empty() {
            // Nothing to apply, but the caller still expects the version.
            return Ok(Reply::Value(EditorEditReply {
                version: call.context.editor.version(),
            }));
        }

        match call.context.editor.apply_edits(params.edits) {
            Ok(transaction) => {
                let version = call.context.editor.version();
                call.outcome.action = HostAction::Edit(transaction);
                Ok(Reply::Value(EditorEditReply { version }))
            }
            Err(error) => Err(CallError::invalid_state(error.to_string())),
        }
    }

    /// Undoes the last transaction.
    #[vy_api_schema::vy_api_method]
    fn undo(call: &mut Call<'_, '_>) -> Result<Reply<()>, CallError> {
        call.outcome.action = HostAction::Undo;
        Ok(Reply::Value(()))
    }

    /// Redoes the last undone transaction.
    #[vy_api_schema::vy_api_method]
    fn redo(call: &mut Call<'_, '_>) -> Result<Reply<()>, CallError> {
        call.outcome.action = HostAction::Redo;
        Ok(Reply::Value(()))
    }
}

pub use self::api::{EVENTS, METHODS, NAMESPACE, dispatch};
