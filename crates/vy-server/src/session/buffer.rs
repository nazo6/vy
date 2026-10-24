use rmpv::Value;
use vy_core::Uri;
use vy_proto::{
    DidChangeParams, DocumentSavedResult, ErrorCode, ResyncNotification, RpcMessage,
    SnapshotResult, SyncRequestParams, SyncRequestResult, SyncedDocument,
};

use super::{Outcome, Session, decode_params, document_id, to_value};

impl Session {
    /// Writes a document back to its file.
    pub(super) fn save_document(&mut self, id: u64, params: Value) -> Outcome {
        let Some(document_id) = document_id(&params) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidRequest,
                "a document_id is required",
            ));
        };

        let Some(buffer) = self.documents.get(&document_id) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::NotFound,
                format!("document {document_id} is not open"),
            ));
        };
        let Some(path) = buffer.uri().file_path() else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidRequest,
                format!("cannot save a non-file document: {}", buffer.uri()),
            ));
        };
        let text = buffer.text();

        if let Err(error) = std::fs::write(&path, text) {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::Internal,
                format!("{}: {error}", path.display()),
            ));
        }

        let Some(buffer) = self.documents.get_mut(&document_id) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::NotFound,
                format!("document {document_id} is not open"),
            ));
        };
        buffer.mark_saved();

        Outcome::one(RpcMessage::ok(
            id,
            to_value(&DocumentSavedResult {
                document_id,
                version: buffer.version(),
                path: path.display().to_string(),
            }),
        ))
    }

    pub(super) fn snapshot(&self, id: u64, params: Value) -> Outcome {
        let Some(document_id) = document_id(&params) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidRequest,
                "a document_id is required",
            ));
        };

        let Some(buffer) = self.documents.get(&document_id) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::NotFound,
                format!("document {document_id} is not open"),
            ));
        };

        Outcome::one(RpcMessage::ok(
            id,
            to_value(&SnapshotResult {
                document_id,
                version: buffer.version(),
                line_count: Some(buffer.line_count() as u64),
                text: buffer.text(),
            }),
        ))
    }

    /// Rebuilds the documents a reconnecting client sends.
    ///
    /// Unsaved text is authoritative and replaces the file contents. Documents
    /// without text are read from disk so a changed file is picked up.
    pub(super) fn sync_request(&mut self, id: u64, params: Value) -> Outcome {
        let Some(params) = decode_params::<SyncRequestParams>(&params) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidParams,
                "could not decode sync/request",
            ));
        };

        let mut documents = Vec::with_capacity(params.documents.len());
        for document in params.documents {
            let uri = match Uri::parse(document.uri.clone()) {
                Ok(uri) => uri,
                Err(error) => {
                    return Outcome::one(RpcMessage::err(
                        id,
                        ErrorCode::InvalidParams,
                        error.to_string(),
                    ));
                }
            };

            let (document_id, _, _, _) = match self
                .document_for_uri_with_text(uri, document.text.as_deref())
            {
                Ok(document) => document,
                Err((code, message)) => {
                    // A saved file that disappeared should not block the rest
                    // of the reconnect. Leave an empty buffer in its place.
                    tracing::warn!(uri = %document.uri, %code, %message, "could not reopen a document");
                    continue;
                }
            };

            if let Some(text) = document.text.as_deref()
                && let Some(buffer) = self.documents.get_mut(&document_id)
            {
                buffer.restore(text, document.version, document.saved_version);
            }

            if let Some(buffer) = self.documents.get(&document_id) {
                documents.push(SyncedDocument {
                    uri: buffer.uri().as_str().to_string(),
                    document_id,
                    version: buffer.version(),
                    saved_version: buffer.saved_version(),
                    text: buffer.text(),
                });
            }
        }

        Outcome::one(RpcMessage::ok(
            id,
            to_value(&SyncRequestResult { documents }),
        ))
    }

    pub(super) fn did_change(&mut self, params: Value) -> Outcome {
        let Some(change) = decode_params::<DidChangeParams>(&params) else {
            tracing::warn!("buffer/did_change could not be decoded");
            return Outcome::empty();
        };

        let result = {
            let Some(buffer) = self.documents.get_mut(&change.document_id) else {
                tracing::warn!(change.document_id, "change for a document that is not open");
                return Outcome::empty();
            };
            buffer.apply_edits(
                change.base_version,
                change.transaction_id,
                change.edits,
                change.selection_before,
                change.selection_after,
            )
        };

        match result {
            Ok(applied) => {
                tracing::trace!(
                    change.document_id,
                    version = applied.version,
                    "applied the change"
                );
                Outcome::empty()
            }
            Err(error) => {
                tracing::warn!(
                    change.document_id,
                    %error,
                    "rejected the change; requesting a resync"
                );
                let (version, text) = self
                    .documents
                    .get(&change.document_id)
                    .map(|buffer| (buffer.version(), buffer.text()))
                    .unwrap_or((0, String::new()));
                Outcome::one(RpcMessage::notification(
                    "buffer/resync",
                    to_value(&ResyncNotification {
                        document_id: change.document_id,
                        version,
                        text,
                    }),
                ))
            }
        }
    }
}
