use vy_core::Uri;
use vy_proto::{
    BufferResync, BufferSnapshot, DidChangeParams, DocumentIdParams, DocumentSave,
    DocumentSaveParams, DocumentSavedResult, ErrorCode, ResyncNotification, RpcMessage,
    SnapshotResult, SyncRequest, SyncRequestParams, SyncRequestResult, SyncedDocument,
};

use super::{Outcome, Session, atomic, notification, ok_response};

impl Session {
    /// Writes a document back to its file atomically.
    pub(super) fn save_document(&mut self, id: u64, params: DocumentSaveParams) -> Outcome {
        let document_id = params.document_id;
        let force = params.force;

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

        // Check for external modification if not forcing.
        if !force
            && let Some(last_known) = self.file_metadata.get(&document_id)
            && let Some(current_meta) = atomic::FileMetadata::from_path(&path)
            && current_meta != *last_known
        {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidRequest,
                "File modified externally (use ! to override)",
            ));
        }

        let text = buffer.text();

        if let Err(error) = atomic::atomic_write(&path, text.as_bytes()) {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::Internal,
                format!("{}: {error}", path.display()),
            ));
        }

        // Update last known metadata after successful save.
        if let Some(new_meta) = atomic::FileMetadata::from_path(&path) {
            self.file_metadata.insert(document_id, new_meta);
        }

        let Some(buffer) = self.documents.get_mut(&document_id) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::NotFound,
                format!("document {document_id} is not open"),
            ));
        };
        buffer.mark_saved();

        ok_response::<DocumentSave>(
            id,
            &DocumentSavedResult {
                document_id,
                version: buffer.version(),
                path: path.display().to_string(),
            },
        )
    }

    pub(super) fn snapshot(&self, id: u64, params: DocumentIdParams) -> Outcome {
        let document_id = params.document_id;

        let Some(buffer) = self.documents.get(&document_id) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::NotFound,
                format!("document {document_id} is not open"),
            ));
        };

        ok_response::<BufferSnapshot>(
            id,
            &SnapshotResult {
                document_id,
                version: buffer.version(),
                line_count: Some(buffer.line_count() as u64),
                text: buffer.text(),
            },
        )
    }

    /// Rebuilds the documents a reconnecting client sends.
    ///
    /// Unsaved text is authoritative and replaces the file contents. Documents
    /// without text are read from disk so a changed file is picked up.
    pub(super) fn sync_request(&mut self, id: u64, params: SyncRequestParams) -> Outcome {
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

        ok_response::<SyncRequest>(id, &SyncRequestResult { documents })
    }

    pub(super) fn did_change(&mut self, change: DidChangeParams) -> Outcome {
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
                Outcome::one(notification::<BufferResync>(&ResyncNotification {
                    document_id: change.document_id,
                    version,
                    text,
                }))
            }
        }
    }
}
