use std::collections::HashMap;

use rmpv::Value;
use serde::Serialize;
use serde::de::DeserializeOwned;
use vy_core::{Applied, Buffer, DocumentId, Layout, PaneId, SplitDirection, TextEdit, Uri};
use vy_proto::value::{get, object};
use vy_proto::{ErrorCode, PROTOCOL_VERSION, RpcMessage};

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

    fn initialize(&mut self, id: u64, params: Value) -> Outcome {
        let protocol_version = get(&params, "protocol_version").and_then(Value::as_u64);

        if protocol_version != Some(PROTOCOL_VERSION as u64) {
            let message = match protocol_version {
                Some(version) => format!(
                    "protocol version mismatch: server {PROTOCOL_VERSION}, client {version}"
                ),
                None => "missing protocol_version".to_string(),
            };
            return Outcome::closing(RpcMessage::err(
                id,
                ErrorCode::UnsupportedProtocolVersion,
                message,
            ));
        }

        let workspace = match get(&params, "workspace").and_then(Value::as_str) {
            Some(value) => match Uri::parse(value) {
                Ok(uri) => Some(uri),
                Err(error) => {
                    return Outcome::one(RpcMessage::err(
                        id,
                        ErrorCode::InvalidRequest,
                        error.to_string(),
                    ));
                }
            },
            None => None,
        };
        self.workspace = workspace;

        let session_id = self.next_session_id;
        self.next_session_id += 1;
        self.state = State::Ready { session_id };

        let response = RpcMessage::ok(id, object([("session_id", Value::from(session_id))]));
        let initialized = RpcMessage::notification(
            "initialized",
            object([
                ("protocol_version", Value::from(PROTOCOL_VERSION)),
                ("session_id", Value::from(session_id)),
                ("server_version", Value::from(env!("CARGO_PKG_VERSION"))),
                // Streams, Blob, and multiplexing are not implemented yet, so this is empty.
                ("server_capabilities", Value::Array(Vec::new())),
                (
                    "platform",
                    object([
                        ("os", Value::from(std::env::consts::OS)),
                        (
                            "path_separator",
                            Value::from(std::path::MAIN_SEPARATOR.to_string()),
                        ),
                    ]),
                ),
            ]),
        );

        Outcome {
            messages: vec![response, initialized],
            close: false,
        }
    }

    /// Opens a document in a new pane. The placement decides where the pane goes.
    fn open_pane(&mut self, id: u64, params: Value) -> Outcome {
        let view = get(&params, "view")
            .and_then(Value::as_str)
            .unwrap_or("editor");
        if view != "editor" {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::UnsupportedFeature,
                format!("view {view} is not implemented yet"),
            ));
        }

        let Some(value) = get(&params, "uri").and_then(Value::as_str) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidRequest,
                "pane/open requires a uri",
            ));
        };

        let uri = match Uri::parse(value) {
            Ok(uri) => uri,
            Err(error) => {
                return Outcome::one(RpcMessage::err(
                    id,
                    ErrorCode::InvalidRequest,
                    error.to_string(),
                ));
            }
        };

        let placement = get(&params, "placement");
        let target = placement
            .and_then(|value| get(value, "target"))
            .and_then(Value::as_u64);
        let position = placement
            .and_then(|value| get(value, "direction"))
            .and_then(Value::as_str)
            .unwrap_or("right");
        let Some(direction) = SplitDirection::parse(position) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidRequest,
                format!("unknown placement direction: {position}"),
            ));
        };
        if let Some(ratio) = placement
            .and_then(|value| get(value, "ratio"))
            .and_then(Value::as_f64)
            && !(0.0 < ratio && ratio < 1.0)
        {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidRequest,
                "placement.ratio must be between 0 and 1",
            ));
        }

        let key = uri.as_str().to_string();
        let (document_id, version, line_count, created) =
            match self.documents_by_uri.get(&key).copied() {
                Some(document_id) => {
                    let buffer = self
                        .documents
                        .get(&document_id)
                        .expect("the URI index points at an open document");
                    (document_id, buffer.version(), buffer.line_count(), false)
                }
                None => {
                    let text = if uri.scheme() == "file" {
                        let Some(path) = uri.file_path() else {
                            return Outcome::one(RpcMessage::err(
                                id,
                                ErrorCode::InvalidRequest,
                                format!("cannot convert to a path: {uri}"),
                            ));
                        };
                        // Phase 1 reads synchronously. Move this to spawn_blocking when
                        // large files arrive.
                        match std::fs::read_to_string(&path) {
                            Ok(text) => text,
                            Err(error) => {
                                return Outcome::one(RpcMessage::err(
                                    id,
                                    ErrorCode::NotFound,
                                    format!("{}: {error}", path.display()),
                                ));
                            }
                        }
                    } else {
                        String::new()
                    };

                    let document_id = self.next_document_id;
                    self.next_document_id += 1;
                    let buffer = Buffer::new(document_id, uri, text);
                    let version = buffer.version();
                    let line_count = buffer.line_count();
                    self.documents.insert(document_id, buffer);
                    self.documents_by_uri.insert(key, document_id);
                    (document_id, version, line_count, true)
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
                view: view.to_string(),
                document_id: Some(document_id),
            },
        );

        let response = RpcMessage::ok(
            id,
            object([
                ("pane_id", Value::from(pane_id)),
                ("document_id", Value::from(document_id)),
                ("version", Value::from(version)),
                ("line_count", Value::from(line_count as u64)),
                ("uri", Value::from(value)),
            ]),
        );

        Outcome {
            messages: vec![response, self.layout_notification("open")],
            close: false,
        }
    }

    /// Focuses a pane. The client only moves the caret after the confirmation.
    fn focus_pane(&mut self, id: u64, params: Value) -> Outcome {
        let Some(pane_id) = get(&params, "pane_id").and_then(Value::as_u64) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidRequest,
                "pane/focus requires a pane_id",
            ));
        };

        if let Err(error) = self.layout.focus(pane_id) {
            return Outcome::one(RpcMessage::err(id, ErrorCode::NotFound, error.to_string()));
        }

        Outcome {
            messages: vec![
                RpcMessage::ok(id, object([("pane_id", Value::from(pane_id))])),
                self.layout_notification("focus"),
            ],
            close: false,
        }
    }

    /// Closes a pane. The layout collapses the split that becomes empty.
    fn close_pane(&mut self, id: u64, params: Value) -> Outcome {
        let Some(pane_id) = get(&params, "pane_id").and_then(Value::as_u64) else {
            return Outcome::one(RpcMessage::err(
                id,
                ErrorCode::InvalidRequest,
                "pane/close requires a pane_id",
            ));
        };

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

        Outcome {
            messages: vec![
                RpcMessage::ok(id, object([("pane_id", Value::from(pane_id))])),
                self.layout_notification("close"),
            ],
            close: false,
        }
    }

    /// Returns the whole layout. The client uses this after reconnecting.
    fn layout_get(&self, id: u64) -> Outcome {
        let mut panes = Vec::new();
        for pane_id in self.layout.leaves() {
            if let Some(pane) = self.panes.get(&pane_id) {
                panes.push(object([
                    ("pane_id", Value::from(pane_id)),
                    ("view", Value::from(pane.view.clone())),
                    (
                        "document_id",
                        pane.document_id.map(Value::from).unwrap_or(Value::Nil),
                    ),
                ]));
            }
        }

        let mut value = self.layout_value();
        if let Value::Map(entries) = &mut value {
            entries.push((Value::from("panes"), Value::Array(panes)));
        }

        Outcome::one(RpcMessage::ok(id, value))
    }

    fn layout_notification(&self, reason: &str) -> RpcMessage {
        let mut value = self.layout_value();
        if let Value::Map(entries) = &mut value {
            entries.push((Value::from("reason"), Value::from(reason)));
        }
        RpcMessage::notification("layout/did_change", value)
    }

    fn layout_value(&self) -> Value {
        object([
            ("window_id", Value::from(self.layout.window_id())),
            ("revision", Value::from(self.layout.revision())),
            (
                "layout",
                self.layout.root().map(to_value).unwrap_or(Value::Nil),
            ),
            ("floating", Value::Array(Vec::new())),
            (
                "active_pane",
                self.layout
                    .active_pane()
                    .map(Value::from)
                    .unwrap_or(Value::Nil),
            ),
        ])
    }

    fn snapshot(&self, id: u64, params: Value) -> Outcome {
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

    fn undo(&mut self, id: u64, params: Value) -> Outcome {
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

    fn redo(&mut self, id: u64, params: Value) -> Outcome {
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

    fn notification(&mut self, method: &str, params: Value) -> Outcome {
        match method {
            "buffer/did_change" => self.did_change(params),
            other => {
                tracing::debug!(method = other, "unknown notification");
                Outcome::empty()
            }
        }
    }

    fn did_change(&mut self, params: Value) -> Outcome {
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
mod tests {
    use super::*;
    use vy_core::{Position, Range, Selection};
    use vy_proto::RpcResult;

    fn request(session: &mut Session, id: u64, method: &str, params: Value) -> Outcome {
        session.handle(RpcMessage::request(id, 1, method, params))
    }

    fn initialize(session: &mut Session) {
        let outcome = session.handle(RpcMessage::request(
            1,
            0,
            "initialize",
            object([
                ("protocol_version", Value::from(PROTOCOL_VERSION)),
                ("client_version", Value::from("0.1.0")),
                ("client_capabilities", Value::Array(Vec::new())),
                ("workspace", Value::from("file:///tmp")),
                ("resume_session", Value::Nil),
            ]),
        ));
        assert!(!outcome.close);
        assert_eq!(outcome.messages.len(), 2);
    }

    fn ok_value(message: &RpcMessage) -> &Value {
        match message {
            RpcMessage::Response {
                result: RpcResult::Ok { value },
                ..
            } => value,
            other => panic!("expected Ok: {other:?}"),
        }
    }

    fn is_error(message: &RpcMessage, code: ErrorCode) -> bool {
        matches!(
            message,
            RpcMessage::Response {
                result: RpcResult::Err { code: actual, .. },
                ..
            } if *actual == code
        )
    }

    fn open_untitled(session: &mut Session) -> (u64, u64) {
        let outcome = request(
            session,
            2,
            "pane/open",
            object([("uri", Value::from("vy:untitled"))]),
        );
        let value = ok_value(&outcome.messages[0]);
        let document_id = get(value, "document_id")
            .and_then(Value::as_u64)
            .expect("document_id");
        let pane_id = get(value, "pane_id")
            .and_then(Value::as_u64)
            .expect("pane_id");
        (document_id, pane_id)
    }

    fn position(line: u64, character: u64) -> Value {
        object([
            ("line", Value::from(line)),
            ("character", Value::from(character)),
        ])
    }

    fn insert_change(document_id: u64, base_version: u64, text: &str) -> RpcMessage {
        RpcMessage::notification(
            "buffer/did_change",
            object([
                ("document_id", Value::from(document_id)),
                ("base_version", Value::from(base_version)),
                ("transaction_id", Value::from(1)),
                (
                    "edits",
                    Value::Array(vec![object([
                        (
                            "range",
                            object([("start", position(0, 0)), ("end", position(0, 0))]),
                        ),
                        ("text", Value::from(text)),
                    ])]),
                ),
                ("selection_before", Value::Array(Vec::new())),
                ("selection_after", Value::Array(Vec::new())),
            ]),
        )
    }

    #[test]
    fn answers_initialize_and_notifies() {
        let mut session = Session::new();
        let outcome = session.handle(RpcMessage::request(
            1,
            0,
            "initialize",
            object([
                ("protocol_version", Value::from(PROTOCOL_VERSION)),
                ("workspace", Value::from("file:///tmp")),
            ]),
        ));

        assert!(!outcome.close);
        assert_eq!(outcome.messages.len(), 2);
        assert_eq!(
            get(ok_value(&outcome.messages[0]), "session_id").and_then(Value::as_u64),
            Some(1)
        );

        match &outcome.messages[1] {
            RpcMessage::Notification { method, params } => {
                assert_eq!(method, "initialized");
                assert_eq!(
                    get(params, "protocol_version").and_then(Value::as_u64),
                    Some(PROTOCOL_VERSION as u64)
                );
                assert_eq!(get(params, "session_id").and_then(Value::as_u64), Some(1));
            }
            other => panic!("expected a notification: {other:?}"),
        }

        assert_eq!(
            session.workspace,
            Some(Uri::parse("file:///tmp").expect("uri"))
        );
    }

    #[test]
    fn closes_on_a_mismatched_protocol_version() {
        let mut session = Session::new();
        let outcome = session.handle(RpcMessage::request(
            1,
            0,
            "initialize",
            object([("protocol_version", Value::from(99u64))]),
        ));

        assert!(outcome.close);
        assert!(is_error(
            &outcome.messages[0],
            ErrorCode::UnsupportedProtocolVersion
        ));
    }

    #[test]
    fn requires_initialize_first() {
        let mut session = Session::new();
        let outcome = request(&mut session, 1, "pane/open", Value::Map(Vec::new()));
        assert!(is_error(&outcome.messages[0], ErrorCode::InvalidRequest));
    }

    #[test]
    fn rejects_a_foreign_session_id() {
        let mut session = Session::new();
        initialize(&mut session);
        let outcome = session.handle(RpcMessage::request(
            2,
            9,
            "buffer/snapshot",
            Value::Map(Vec::new()),
        ));
        assert!(is_error(&outcome.messages[0], ErrorCode::InvalidRequest));
    }

    #[test]
    fn opens_a_document_and_returns_a_snapshot() {
        let mut session = Session::new();
        initialize(&mut session);
        let (document_id, pane_id) = open_untitled(&mut session);

        assert_eq!(document_id, 1);
        assert_eq!(pane_id, 1);

        let outcome = request(
            &mut session,
            3,
            "buffer/snapshot",
            object([("document_id", Value::from(document_id))]),
        );
        let value = ok_value(&outcome.messages[0]);
        assert_eq!(get(value, "version").and_then(Value::as_u64), Some(0));
        assert_eq!(get(value, "text").and_then(Value::as_str), Some(""));
    }

    #[test]
    fn applies_a_change_and_undoes_it() {
        let mut session = Session::new();
        initialize(&mut session);
        let (document_id, _) = open_untitled(&mut session);

        let outcome = session.handle(insert_change(document_id, 0, "こんにちは"));
        assert!(outcome.messages.is_empty());

        let outcome = request(
            &mut session,
            3,
            "buffer/snapshot",
            object([("document_id", Value::from(document_id))]),
        );
        let value = ok_value(&outcome.messages[0]);
        assert_eq!(
            get(value, "text").and_then(Value::as_str),
            Some("こんにちは")
        );
        assert_eq!(get(value, "version").and_then(Value::as_u64), Some(1));

        let outcome = request(
            &mut session,
            4,
            "buffer/undo",
            object([("document_id", Value::from(document_id))]),
        );
        let value = ok_value(&outcome.messages[0]);
        assert_eq!(get(value, "version").and_then(Value::as_u64), Some(2));
        match get(value, "edits") {
            Some(Value::Array(batches)) => {
                assert_eq!(batches.len(), 1);
                match &batches[0] {
                    Value::Array(edits) => assert_eq!(edits.len(), 1),
                    other => panic!("a batch is not an array: {other:?}"),
                }
            }
            other => panic!("edits is not an array: {other:?}"),
        }

        let outcome = request(
            &mut session,
            5,
            "buffer/snapshot",
            object([("document_id", Value::from(document_id))]),
        );
        assert_eq!(
            get(ok_value(&outcome.messages[0]), "text").and_then(Value::as_str),
            Some("")
        );
    }

    #[test]
    fn asks_for_a_resync_when_the_base_version_is_stale() {
        let mut session = Session::new();
        initialize(&mut session);
        let (document_id, _) = open_untitled(&mut session);

        let outcome = session.handle(insert_change(document_id, 9, "x"));
        assert_eq!(outcome.messages.len(), 1);
        match &outcome.messages[0] {
            RpcMessage::Notification { method, params } => {
                assert_eq!(method, "buffer/resync");
                assert_eq!(
                    get(params, "document_id").and_then(Value::as_u64),
                    Some(document_id)
                );
            }
            other => panic!("expected a notification: {other:?}"),
        }
    }

    #[test]
    fn reports_unknown_methods() {
        let mut session = Session::new();
        initialize(&mut session);
        let outcome = request(&mut session, 2, "nope/nope", Value::Map(Vec::new()));
        assert!(is_error(&outcome.messages[0], ErrorCode::InvalidRequest));
    }

    #[test]
    fn closes_on_shutdown() {
        let mut session = Session::new();
        initialize(&mut session);
        let outcome = request(&mut session, 2, "shutdown", Value::Map(Vec::new()));
        assert!(outcome.close);
    }

    // Make sure the position types match the wire format.
    #[test]
    fn serializes_core_types_as_maps() {
        let edit = TextEdit::new(Range::new(Position::new(1, 2), Position::new(1, 4)), "abc");
        let value = to_value(&edit);
        assert_eq!(get(&value, "text").and_then(Value::as_str), Some("abc"));
        let start = get(&value, "range")
            .and_then(|range| get(range, "start"))
            .expect("start");
        assert_eq!(get(start, "line").and_then(Value::as_u64), Some(1));
        assert_eq!(get(start, "character").and_then(Value::as_u64), Some(2));

        let selection = Selection::caret(Position::new(0, 0));
        let decoded: Selection =
            decode_params(&to_value(&selection)).expect("selection round trip");
        assert_eq!(decoded, selection);
    }

    fn layout_notification(outcome: &Outcome) -> (&Value, &Value) {
        match &outcome.messages[1] {
            RpcMessage::Notification { method, params } => {
                assert_eq!(method, "layout/did_change");
                let layout = get(params, "layout").expect("layout");
                (params, layout)
            }
            other => panic!("expected a layout notification: {other:?}"),
        }
    }

    #[test]
    fn opens_a_second_pane_next_to_the_first_and_shares_the_buffer() {
        let mut session = Session::new();
        initialize(&mut session);
        let (document_id, pane_id) = open_untitled(&mut session);

        let outcome = request(
            &mut session,
            3,
            "pane/open",
            object([
                ("uri", Value::from("vy:untitled")),
                (
                    "placement",
                    object([
                        ("target", Value::from(pane_id)),
                        ("direction", Value::from("right")),
                    ]),
                ),
            ]),
        );

        let value = ok_value(&outcome.messages[0]);
        assert_eq!(
            get(value, "document_id").and_then(Value::as_u64),
            Some(document_id),
            "an open URI reuses the document"
        );
        assert_eq!(get(value, "pane_id").and_then(Value::as_u64), Some(2));

        let (params, layout) = layout_notification(&outcome);
        assert_eq!(get(params, "active_pane").and_then(Value::as_u64), Some(2));
        assert_eq!(get(params, "reason").and_then(Value::as_str), Some("open"));
        assert_eq!(
            decode_params::<vy_core::LayoutNode>(layout),
            Some(vy_core::LayoutNode::Split {
                direction: SplitDirection::Horizontal,
                ratio: 0.5,
                first: Box::new(vy_core::LayoutNode::Leaf { pane_id }),
                second: Box::new(vy_core::LayoutNode::Leaf { pane_id: 2 }),
            })
        );
    }

    #[test]
    fn focuses_and_closes_panes_through_the_layout_tree() {
        let mut session = Session::new();
        initialize(&mut session);
        let (_, first) = open_untitled(&mut session);
        let outcome = request(
            &mut session,
            3,
            "pane/open",
            object([
                ("uri", Value::from("vy:untitled")),
                (
                    "placement",
                    object([
                        ("target", Value::from(first)),
                        ("direction", Value::from("below")),
                    ]),
                ),
            ]),
        );
        let second = get(ok_value(&outcome.messages[0]), "pane_id")
            .and_then(Value::as_u64)
            .expect("pane_id");

        let outcome = request(
            &mut session,
            4,
            "pane/focus",
            object([("pane_id", Value::from(first))]),
        );
        let (params, _) = layout_notification(&outcome);
        assert_eq!(
            get(params, "active_pane").and_then(Value::as_u64),
            Some(first)
        );

        let outcome = request(
            &mut session,
            5,
            "pane/close",
            object([("pane_id", Value::from(first))]),
        );
        let (params, layout) = layout_notification(&outcome);
        assert_eq!(get(params, "reason").and_then(Value::as_str), Some("close"));
        assert_eq!(
            decode_params::<vy_core::LayoutNode>(layout),
            Some(vy_core::LayoutNode::Leaf { pane_id: second })
        );

        let outcome = request(
            &mut session,
            6,
            "pane/close",
            object([("pane_id", Value::from(second))]),
        );
        assert!(is_error(&outcome.messages[0], ErrorCode::InvalidRequest));
    }

    #[test]
    fn returns_the_layout_with_its_panes() {
        let mut session = Session::new();
        initialize(&mut session);
        let (document_id, pane_id) = open_untitled(&mut session);

        let outcome = request(&mut session, 3, "layout/get", Value::Map(Vec::new()));
        let value = ok_value(&outcome.messages[0]);
        assert_eq!(get(value, "revision").and_then(Value::as_u64), Some(1));
        assert_eq!(
            get(value, "active_pane").and_then(Value::as_u64),
            Some(pane_id)
        );

        let panes = match get(value, "panes") {
            Some(Value::Array(panes)) => panes,
            other => panic!("expected panes: {other:?}"),
        };
        assert_eq!(panes.len(), 1);
        assert_eq!(
            get(&panes[0], "document_id").and_then(Value::as_u64),
            Some(document_id)
        );
        assert_eq!(
            get(&panes[0], "view").and_then(Value::as_str),
            Some("editor")
        );
    }
}
