mod buffer;
mod fs;
mod history;
mod init;
mod layout;
mod pane;
mod process;

use std::collections::HashMap;

use rmpv::Value;
use vy_core::{Applied, Buffer, DocumentId, Layout, PaneId, Uri};
use vy_proto::ErrorCode;
use vy_proto::RpcMessage;
pub(super) use vy_proto::value::get;
pub(super) use vy_proto::{decode_value as decode_params, to_value};

/// One pane in the layout tree.
#[derive(Debug, Clone)]
struct Pane {
    view: String,
    document_id: Option<DocumentId>,
}

/// State owned by one connection.
///
/// The layout tree lives here. Floating panes, saved sessions, and views other than
/// `editor` are not implemented yet.
pub struct Session {
    state: State,
    next_session_id: u64,
    next_document_id: u64,
    next_pane_id: u64,
    workspace: Option<Uri>,
    documents: HashMap<u64, Buffer>,
    /// Documents already open, keyed by URI. A second pane reuses the buffer.
    documents_by_uri: HashMap<String, DocumentId>,
    panes: HashMap<PaneId, Pane>,
    layout: Layout,
    processes: process::ProcessTable,
    outgoing: Option<tokio::sync::mpsc::UnboundedSender<RpcMessage>>,
}

#[derive(Debug, Clone, Copy)]
enum State {
    Uninitialized,
    Ready { session_id: u64 },
}

/// The result of handling one message.
pub struct Outcome {
    /// Messages to send to the client, in order.
    pub messages: Vec<RpcMessage>,
    /// Whether to close the connection after sending them.
    pub close: bool,
}

impl Outcome {
    fn empty() -> Self {
        Self {
            messages: Vec::new(),
            close: false,
        }
    }

    fn one(message: RpcMessage) -> Self {
        Self {
            messages: vec![message],
            close: false,
        }
    }

    fn closing(message: RpcMessage) -> Self {
        Self {
            messages: vec![message],
            close: true,
        }
    }
}

impl Session {
    pub fn new() -> Self {
        Self {
            state: State::Uninitialized,
            next_session_id: 1,
            next_document_id: 1,
            next_pane_id: 1,
            workspace: None,
            documents: HashMap::new(),
            documents_by_uri: HashMap::new(),
            panes: HashMap::new(),
            layout: Layout::new(1),
            processes: HashMap::new(),
            outgoing: None,
        }
    }

    pub fn set_outgoing(&mut self, sender: tokio::sync::mpsc::UnboundedSender<RpcMessage>) {
        self.outgoing = Some(sender);
    }

    /// Handles one message. The caller does the I/O.
    pub fn handle(&mut self, message: RpcMessage) -> Outcome {
        match message {
            RpcMessage::Request {
                id,
                session_id,
                method,
                params,
                ..
            } => self.request(id, session_id, &method, params),
            RpcMessage::Notification { method, params } => self.notification(&method, params),
            RpcMessage::Cancel { id, reason } => {
                tracing::debug!(id, ?reason, "cancellation is not handled in Phase 1");
                Outcome::empty()
            }
            RpcMessage::Response { .. } | RpcMessage::Stream { .. } => Outcome::empty(),
        }
    }

    fn request(&mut self, id: u64, session_id: u64, method: &str, params: Value) -> Outcome {
        if method == "initialize" {
            if matches!(self.state, State::Ready { .. }) {
                return Outcome::one(RpcMessage::err(
                    id,
                    ErrorCode::InvalidState,
                    "initialize can only be sent once per connection",
                ));
            }
            return self.initialize(id, params);
        }

        let State::Ready {
            session_id: current,
        } = self.state
        else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidRequest,
                "send initialize first",
            ));
        };

        if session_id != 0 && session_id != current {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidRequest,
                format!("session_id {session_id} does not belong to this connection"),
            ));
        }

        match method {
            "pane/open" => self.open_pane(id, params),
            "pane/focus" => self.focus_pane(id, params),
            "pane/close" => self.close_pane(id, params),
            "pane/resize" => Outcome::one(RpcMessage::err(
                id,
                ErrorCode::UnsupportedFeature,
                "pane/resize is not implemented yet",
            )),
            "layout/get" => self.layout_get(id),
            "editor/open" => self.editor_open(id, params),
            "document/save" => self.save_document(id, params),
            "buffer/snapshot" => self.snapshot(id, params),
            "buffer/undo" => self.undo(id, params),
            "buffer/redo" => self.redo(id, params),
            "fs/read_file" => self.fs_read_file(id, params),
            "fs/read_text_file" => self.fs_read_text_file(id, params),
            "fs/write_file" => self.fs_write_file(id, params),
            "fs/read_dir" => self.fs_read_dir(id, params),
            "fs/stat" => self.fs_stat(id, params),
            "fs/exists" => self.fs_exists(id, params),
            "fs/create_dir" => self.fs_create_dir(id, params),
            "fs/remove" => self.fs_remove(id, params),
            "fs/rename" => self.fs_rename(id, params),
            "fs/copy" => self.fs_copy(id, params),
            "process/exec" => self.process_exec(id, params),
            "process/spawn" => self.process_spawn(id, params),
            "process/kill" => self.process_kill(id, params),
            "process/stdin" => self.process_stdin(id, params),
            "shutdown" => Outcome {
                messages: vec![RpcMessage::ok(id, Value::Map(Vec::new()))],
                close: true,
            },
            other => Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidRequest,
                format!("unknown method: {other}"),
            )),
        }
    }

    fn notification(&mut self, method: &str, params: Value) -> Outcome {
        match method {
            "buffer/did_change" => self.did_change(params),
            other => {
                tracing::debug!(method = other, "unknown notification");
                Outcome::empty()
            }
        }
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

fn document_id(params: &Value) -> Option<u64> {
    get(params, "document_id")
        .or_else(|| get(params, "buffer_id"))
        .and_then(Value::as_u64)
}

/// Returns the value in the shape described by `docs/protocol.md`.
fn applied_value(document_id: u64, applied: &Applied) -> Value {
    to_value(&vy_proto::HistoryResult {
        document_id,
        version: applied.version,
        edits: applied.edits.clone(),
        selection: applied.selection.clone(),
    })
}

#[cfg(test)]
mod tests;
