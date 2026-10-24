//! Client implementation of the `editor` host trait.

use serde_json::json;
use vy_client_api::keymap::Mode;
use vy_client_api::namespaces::editor::{
    EditorApi, EditorEditParams, EditorEditReply, EditorOpenParams, EditorSetModeParams,
    EditorSetSelectionsParams,
};
use vy_core::Selection;
use vy_host_api::{CallError, DocumentInfoReply, Reply};

use super::ClientHost;
use crate::api::document_info;
use crate::commands::DocumentCommand;
use crate::plugin_host::HostAction;

impl EditorApi for ClientHost<'_, '_, '_> {
    fn active_document(&mut self) -> Result<Reply<Option<DocumentInfoReply>>, CallError> {
        let document = self
            .call
            .context
            .document
            .as_ref()
            .map(|document| document_info(self.call.context.editor, document));
        Ok(Reply::Value(document))
    }

    fn open(&mut self, params: EditorOpenParams) -> Result<Reply<()>, CallError> {
        self.call.outcome.action = HostAction::Document {
            command: DocumentCommand::Open,
            args: Some(json!({
                "uri": params.uri.to_string(),
                "paneId": params.pane_id,
            })),
        };
        Ok(Reply::Value(()))
    }

    fn mode(&mut self) -> Result<Reply<Mode>, CallError> {
        Ok(Reply::Value(self.call.context.editor.mode()))
    }

    fn set_mode(&mut self, params: EditorSetModeParams) -> Result<Reply<()>, CallError> {
        self.call.context.editor.set_mode(params.mode);
        Ok(Reply::Value(()))
    }

    fn selections(&mut self) -> Result<Reply<Vec<Selection>>, CallError> {
        Ok(Reply::Value(self.call.context.editor.selections()))
    }

    fn set_selections(
        &mut self,
        params: EditorSetSelectionsParams,
    ) -> Result<Reply<()>, CallError> {
        self.call.context.editor.set_selections(&params.selections);
        Ok(Reply::Value(()))
    }

    fn edit(&mut self, params: EditorEditParams) -> Result<Reply<EditorEditReply>, CallError> {
        if params.edits.is_empty() {
            return Ok(Reply::Value(EditorEditReply {
                version: self.call.context.editor.version(),
            }));
        }

        match self.call.context.editor.apply_edits(params.edits) {
            Ok(transaction) => {
                let version = self.call.context.editor.version();
                self.call.outcome.action = HostAction::Edit(transaction);
                Ok(Reply::Value(EditorEditReply { version }))
            }
            Err(error) => Err(CallError::invalid_state(error.to_string())),
        }
    }

    fn undo(&mut self) -> Result<Reply<()>, CallError> {
        self.call.outcome.action = HostAction::Undo;
        Ok(Reply::Value(()))
    }

    fn redo(&mut self) -> Result<Reply<()>, CallError> {
        self.call.outcome.action = HostAction::Redo;
        Ok(Reply::Value(()))
    }

    fn documents(&mut self) -> Result<Reply<Vec<DocumentInfoReply>>, CallError> {
        let documents = self
            .call
            .context
            .document
            .iter()
            .map(|document| document_info(self.call.context.editor, document))
            .collect();
        Ok(Reply::Value(documents))
    }
}
