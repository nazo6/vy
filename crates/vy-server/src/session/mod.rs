mod buffer;
mod history;
mod init;
mod layout;
mod pane;

use std::collections::HashMap;

use rmpv::Value;
use serde::Serialize;
use serde::de::DeserializeOwned;
use vy_core::{Applied, Buffer, DocumentId, Layout, PaneId, Uri};
use vy_proto::ErrorCode;
use vy_proto::RpcMessage;
use vy_proto::value::{get, object};

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
        }
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

fn decode_params<T: DeserializeOwned>(value: &Value) -> Option<T> {
    // rmpv's serde support treats structs as tuples, so always go through rmp-serde.
    let mut bytes = Vec::new();
    rmpv::encode::write_value(&mut bytes, value).ok()?;
    rmp_serde::from_slice(&bytes).ok()
}

/// Returns the value in the shape described by `docs/protocol.md`.
fn applied_value(document_id: u64, applied: &Applied) -> Value {
    object([
        ("document_id", Value::from(document_id)),
        ("version", Value::from(applied.version)),
        ("edits", to_value(&applied.edits)),
        ("selection", to_value(&applied.selection)),
    ])
}

fn to_value<T: Serialize>(value: &T) -> Value {
    // rmpv's serde support turns structs into tuples; rmp-serde keeps field names.
    rmp_serde::to_vec_named(value)
        .ok()
        .and_then(|bytes| rmpv::decode::read_value(&mut bytes.as_slice()).ok())
        .unwrap_or(Value::Nil)
}

#[cfg(test)]
mod tests;
