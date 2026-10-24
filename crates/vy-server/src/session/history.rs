use vy_core::Applied;
use vy_proto::{BufferRedo, BufferUndo, DocumentIdParams, ErrorCode, RpcMessage};

use super::{Outcome, Session, applied_result, ok_response};

impl Session {
    pub(super) fn undo(&mut self, id: u64, params: DocumentIdParams) -> Outcome {
        let document_id = params.document_id;

        let Some(buffer) = self.documents.get_mut(&document_id) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::NotFound,
                format!("document {document_id} is not open"),
            ));
        };

        match buffer.undo() {
            Ok(Some(applied)) => {
                ok_response::<BufferUndo>(id, &applied_result(document_id, &applied))
            }
            Ok(None) => ok_response::<BufferUndo>(
                id,
                &applied_result(
                    document_id,
                    &Applied {
                        version: buffer.version(),
                        edits: Vec::new(),
                        selection: Vec::new(),
                    },
                ),
            ),
            Err(error) => Outcome::one(RpcMessage::err(id, ErrorCode::Internal, error.to_string())),
        }
    }

    pub(super) fn redo(&mut self, id: u64, params: DocumentIdParams) -> Outcome {
        let document_id = params.document_id;

        let Some(buffer) = self.documents.get_mut(&document_id) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::NotFound,
                format!("document {document_id} is not open"),
            ));
        };

        match buffer.redo() {
            Ok(Some(applied)) => {
                ok_response::<BufferRedo>(id, &applied_result(document_id, &applied))
            }
            Ok(None) => ok_response::<BufferRedo>(
                id,
                &applied_result(
                    document_id,
                    &Applied {
                        version: buffer.version(),
                        edits: Vec::new(),
                        selection: Vec::new(),
                    },
                ),
            ),
            Err(error) => Outcome::one(RpcMessage::err(id, ErrorCode::Internal, error.to_string())),
        }
    }
}
