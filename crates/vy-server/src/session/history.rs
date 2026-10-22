use rmpv::Value;
use vy_core::Applied;
use vy_proto::{ErrorCode, RpcMessage};

use super::{Outcome, Session, applied_value, document_id};

impl Session {
    pub(super) fn undo(&mut self, id: u64, params: Value) -> Outcome {
        let Some(document_id) = document_id(&params) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidRequest,
                "a document_id is required",
            ));
        };

        let Some(buffer) = self.documents.get_mut(&document_id) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::NotFound,
                format!("document {document_id} is not open"),
            ));
        };

        match buffer.undo() {
            Ok(Some(applied)) => {
                Outcome::one(RpcMessage::ok(id, applied_value(document_id, &applied)))
            }
            Ok(None) => Outcome::one(RpcMessage::ok(
                id,
                applied_value(
                    document_id,
                    &Applied {
                        version: buffer.version(),
                        edits: Vec::new(),
                        selection: Vec::new(),
                    },
                ),
            )),
            Err(error) => Outcome::one(RpcMessage::err(id, ErrorCode::Internal, error.to_string())),
        }
    }

    pub(super) fn redo(&mut self, id: u64, params: Value) -> Outcome {
        let Some(document_id) = document_id(&params) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidRequest,
                "a document_id is required",
            ));
        };

        let Some(buffer) = self.documents.get_mut(&document_id) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::NotFound,
                format!("document {document_id} is not open"),
            ));
        };

        match buffer.redo() {
            Ok(Some(applied)) => {
                Outcome::one(RpcMessage::ok(id, applied_value(document_id, &applied)))
            }
            Ok(None) => Outcome::one(RpcMessage::ok(
                id,
                applied_value(
                    document_id,
                    &Applied {
                        version: buffer.version(),
                        edits: Vec::new(),
                        selection: Vec::new(),
                    },
                ),
            )),
            Err(error) => Outcome::one(RpcMessage::err(id, ErrorCode::Internal, error.to_string())),
        }
    }
}
