use vy_core::{Buffer, DocumentId, SplitDirection, Uri};
use vy_proto::{
    EditorOpen, EditorOpenParams, ErrorCode, PaneClose, PaneCloseResult, PaneFocus,
    PaneFocusResult, PaneOpen, PaneOpenResult, RpcMessage,
};

use super::{Outcome, Pane, Session, ok_message, ok_response};

impl Session {
    /// Returns the document for a URI, opening it when needed.
    pub(super) fn document_for_uri(
        &mut self,
        uri: Uri,
    ) -> Result<(DocumentId, u64, usize, bool), (ErrorCode, String)> {
        self.document_for_uri_with_text(uri, None)
    }

    /// Returns the document for a URI, using `provided` instead of the file when
    /// the client is restoring unsaved text.
    pub(super) fn document_for_uri_with_text(
        &mut self,
        uri: Uri,
        provided: Option<&str>,
    ) -> Result<(DocumentId, u64, usize, bool), (ErrorCode, String)> {
        let key = uri.as_str().to_string();
        if let Some(document_id) = self.documents_by_uri.get(&key).copied() {
            let buffer = self
                .documents
                .get(&document_id)
                .expect("the URI index points at an open document");
            return Ok((document_id, buffer.version(), buffer.line_count(), false));
        }

        let text = match provided {
            Some(text) => text.to_string(),
            None if uri.scheme() == "file" => {
                let Some(path) = uri.file_path() else {
                    return Err((
                        ErrorCode::InvalidRequest,
                        format!("cannot convert to a path: {uri}"),
                    ));
                };
                // Phase 1 reads synchronously. Move this to spawn_blocking when large
                // files arrive.
                match std::fs::read_to_string(&path) {
                    Ok(text) => text,
                    Err(error) => {
                        return Err((ErrorCode::NotFound, format!("{}: {error}", path.display())));
                    }
                }
            }
            None => String::new(),
        };

        let document_id = self.next_document_id;
        self.next_document_id += 1;
        let buffer = Buffer::new(document_id, uri, text);
        let version = buffer.version();
        let line_count = buffer.line_count();
        self.documents.insert(document_id, buffer);
        self.documents_by_uri.insert(key, document_id);
        Ok((document_id, version, line_count, true))
    }

    /// Opens a document in a new pane. The placement decides where the pane goes.
    pub(super) fn open_pane(&mut self, id: u64, params: vy_proto::PaneOpenParams) -> Outcome {
        let view = params.view.unwrap_or_else(|| "editor".to_string());

        let uri = match Uri::parse(params.uri.clone()) {
            Ok(uri) => uri,
            Err(error) => {
                return Outcome::one(RpcMessage::err(
                    id,
                    ErrorCode::InvalidRequest,
                    error.to_string(),
                ));
            }
        };

        let placement = params.placement.unwrap_or_default();
        let target = placement.target;
        let position = placement.direction.as_deref().unwrap_or("right");
        let Some(direction) = SplitDirection::parse(position) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidRequest,
                format!("unknown placement direction: {position}"),
            ));
        };
        if let Some(ratio) = placement.ratio
            && !(0.0 < ratio && ratio < 1.0)
        {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidRequest,
                "placement.ratio must be between 0 and 1",
            ));
        }

        let (document_id, version, line_count, created) = match self.document_for_uri(uri) {
            Ok(document) => document,
            Err((code, message)) => {
                return Outcome::one(RpcMessage::err(id, code, message));
            }
        };

        let pane_id = self.next_pane_id;
        if let Err(error) = self.layout.open(
            pane_id,
            target,
            direction,
            SplitDirection::is_before(position),
        ) {
            if created {
                self.documents.remove(&document_id);
                self.documents_by_uri.retain(|_, id| *id != document_id);
            }
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidRequest,
                error.to_string(),
            ));
        }

        self.next_pane_id += 1;
        self.panes.insert(
            pane_id,
            Pane {
                view,
                document_id: Some(document_id),
            },
        );
        self.persist_session();

        let response = ok_message::<PaneOpen>(
            id,
            &PaneOpenResult {
                pane_id,
                document_id,
                version,
                line_count: line_count as u64,
                uri: params.uri,
                replaced: false,
            },
        );

        Outcome {
            messages: vec![response, self.layout_notification("open")],
            close: false,
        }
    }

    /// Focuses a pane. The client only moves the caret after the confirmation.
    pub(super) fn focus_pane(&mut self, id: u64, params: vy_proto::PaneFocusParams) -> Outcome {
        let pane_id = params.pane_id;

        if let Err(error) = self.layout.focus(pane_id) {
            return Outcome::one(RpcMessage::err(id, ErrorCode::NotFound, error.to_string()));
        }
        self.persist_session();

        Outcome {
            messages: vec![
                ok_message::<PaneFocus>(id, &PaneFocusResult { pane_id }),
                self.layout_notification("focus"),
            ],
            close: false,
        }
    }

    /// Closes a pane. The layout collapses the split that becomes empty.
    pub(super) fn close_pane(&mut self, id: u64, params: vy_proto::PaneCloseParams) -> Outcome {
        let pane_id = params.pane_id;

        if let Err(error) = self.layout.close(pane_id) {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidRequest,
                error.to_string(),
            ));
        }

        let document_id = self
            .panes
            .remove(&pane_id)
            .and_then(|pane| pane.document_id);
        if let Some(document_id) = document_id {
            // Drop the buffer when no pane refers to it anymore, so a later open
            // rereads the file instead of showing the stale copy.
            let still_used = self
                .panes
                .values()
                .any(|pane| pane.document_id == Some(document_id));
            if !still_used {
                self.documents.remove(&document_id);
                self.documents_by_uri.retain(|_, id| *id != document_id);
            }
        }
        self.persist_session();

        Outcome {
            messages: vec![
                ok_message::<PaneClose>(id, &PaneCloseResult { pane_id }),
                self.layout_notification("close"),
            ],
            close: false,
        }
    }

    /// Opens a document in an existing pane. Used by `:e` and `editor.open`.
    pub(super) fn editor_open(&mut self, id: u64, params: EditorOpenParams) -> Outcome {
        let uri = match Uri::parse(params.uri.clone()) {
            Ok(uri) => uri,
            Err(error) => {
                return Outcome::one(RpcMessage::err(
                    id,
                    ErrorCode::InvalidRequest,
                    error.to_string(),
                ));
            }
        };

        let pane_id = params.pane_id.or_else(|| self.layout.active_pane());
        let Some(pane_id) = pane_id else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidState,
                "there is no active pane",
            ));
        };
        let Some(previous) = self.panes.get(&pane_id).and_then(|pane| pane.document_id) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::NotFound,
                format!("pane {pane_id} is not open"),
            ));
        };

        let (document_id, version, line_count, _created) = match self.document_for_uri(uri) {
            Ok(document) => document,
            Err((code, message)) => {
                return Outcome::one(RpcMessage::err(id, code, message));
            }
        };

        if let Some(pane) = self.panes.get_mut(&pane_id) {
            pane.document_id = Some(document_id);
        }
        self.persist_session();

        ok_response::<EditorOpen>(
            id,
            &PaneOpenResult {
                pane_id,
                document_id,
                version,
                line_count: line_count as u64,
                uri: params.uri,
                replaced: previous != document_id,
            },
        )
    }
}
