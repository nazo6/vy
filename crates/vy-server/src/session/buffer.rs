use rmpv::Value;
use vy_core::TextEdit;
use vy_proto::value::{get, object};
use vy_proto::{ErrorCode, RpcMessage};

use super::{Outcome, Session, decode_params, document_id};

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
            object([
                ("document_id", Value::from(document_id)),
                ("version", Value::from(buffer.version())),
                ("path", Value::from(path.display().to_string())),
            ]),
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
            object([
                ("document_id", Value::from(document_id)),
                ("version", Value::from(buffer.version())),
                ("line_count", Value::from(buffer.line_count() as u64)),
                ("text", Value::from(buffer.text())),
            ]),
        ))
    }

    pub(super) fn did_change(&mut self, params: Value) -> Outcome {
        let Some(document_id) = document_id(&params) else {
            tracing::warn!("buffer/did_change has no document_id");
            return Outcome::empty();
        };

        let Some(base_version) = get(&params, "base_version").and_then(Value::as_u64) else {
            tracing::warn!(document_id, "buffer/did_change has no base_version");
            return Outcome::empty();
        };

        let Some(transaction_id) = get(&params, "transaction_id").and_then(Value::as_u64) else {
            tracing::warn!(document_id, "buffer/did_change has no transaction_id");
            return Outcome::empty();
        };

        let edits: Vec<TextEdit> = match get(&params, "edits").and_then(decode_params) {
            Some(edits) => edits,
            None => {
                tracing::warn!(document_id, "could not read edits in buffer/did_change");
                return Outcome::empty();
            }
        };

        let selection_before = get(&params, "selection_before")
            .and_then(decode_params)
            .unwrap_or_default();
        let selection_after = get(&params, "selection_after")
            .and_then(decode_params)
            .unwrap_or_default();

        let result = {
            let Some(buffer) = self.documents.get_mut(&document_id) else {
                tracing::warn!(document_id, "change for a document that is not open");
                return Outcome::empty();
            };
            buffer.apply_edits(
                base_version,
                transaction_id,
                edits,
                selection_before,
                selection_after,
            )
        };

        match result {
            Ok(applied) => {
                tracing::trace!(document_id, version = applied.version, "applied the change");
                Outcome::empty()
            }
            Err(error) => {
                tracing::warn!(document_id, %error, "rejected the change; requesting a resync");
                let (version, text) = self
                    .documents
                    .get(&document_id)
                    .map(|buffer| (buffer.version(), buffer.text()))
                    .unwrap_or((0, String::new()));
                Outcome::one(RpcMessage::notification(
                    "buffer/resync",
                    object([
                        ("document_id", Value::from(document_id)),
                        ("version", Value::from(version)),
                        ("text", Value::from(text)),
                    ]),
                ))
            }
        }
    }
}
