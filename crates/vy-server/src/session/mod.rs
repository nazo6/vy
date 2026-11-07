mod atomic;
mod buffer;
mod fs;
mod history;
#[cfg(test)]
mod host;
mod init;
mod layout;
mod pane;
mod process;
mod state;

use std::collections::HashMap;
use std::path::PathBuf;

use rmpv::Value;
use vy_core::{Applied, Buffer, DocumentId, Layout, PaneId, Uri};
#[cfg(test)]
pub(super) use vy_proto::value::get;
use vy_proto::{
    BufferDidChange, BufferRedo, BufferSnapshot, BufferUndo, DocumentSave, EditorOpen, ErrorCode,
    FsCopy, FsCreateDir, FsExists, FsReadDir, FsReadFile, FsReadTextFile, FsRemove, FsRename,
    FsStat, FsWriteFile, Initialize, LayoutGet, PaneClose, PaneFocus, PaneOpen, PaneResize,
    ProcessExec, ProcessKill, ProcessSpawn, ProcessStdin, RpcMessage, RpcMethod, RpcNotification,
    SyncRequest,
};
#[cfg(test)]
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
    /// Last known on-disk metadata for open documents to detect external modifications.
    file_metadata: HashMap<DocumentId, atomic::FileMetadata>,
    panes: HashMap<PaneId, Pane>,
    layout: Layout,
    processes: process::ProcessTable,
    outgoing: Option<tokio::sync::mpsc::UnboundedSender<RpcMessage>>,
    data_dir: Option<PathBuf>,
    state_path: Option<PathBuf>,
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

/// Decodes a request's typed params, turning a mismatch into an `InvalidParams` reply.
fn decode_request<M: RpcMethod>(id: u64, params: &Value) -> Result<M::Params, Outcome> {
    vy_proto::decode_params::<M>(params).ok_or_else(|| {
        Outcome::one(RpcMessage::err(
            id,
            ErrorCode::InvalidParams,
            format!("invalid {} params", M::METHOD),
        ))
    })
}

/// Builds an `Ok` reply message for a typed method.
fn ok_message<M: RpcMethod>(id: u64, reply: &M::Reply) -> RpcMessage {
    vy_proto::ok::<M>(id, reply)
}

/// Builds an `Ok` outcome for a typed method.
fn ok_response<M: RpcMethod>(id: u64, reply: &M::Reply) -> Outcome {
    Outcome::one(ok_message::<M>(id, reply))
}

/// Builds a typed notification.
fn notification<N: RpcNotification>(params: &N::Params) -> RpcMessage {
    vy_proto::notification::<N>(params)
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
            file_metadata: HashMap::new(),
            panes: HashMap::new(),
            layout: Layout::new(1),
            processes: HashMap::new(),
            outgoing: None,
            data_dir: None,
            state_path: None,
        }
    }

    /// Enables the saved layout under `data_dir`.
    pub fn set_data_dir(&mut self, data_dir: PathBuf) {
        self.data_dir = Some(data_dir);
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
        if method == Initialize::METHOD {
            if matches!(self.state, State::Ready { .. }) {
                return Outcome::one(RpcMessage::err(
                    id,
                    ErrorCode::InvalidState,
                    "initialize can only be sent once per connection",
                ));
            }
            let params = match vy_proto::decode_params::<Initialize>(&params) {
                Some(params) => params,
                None => {
                    return Outcome::closing(RpcMessage::err(
                        id,
                        ErrorCode::UnsupportedProtocolVersion,
                        "missing protocol_version".to_string(),
                    ));
                }
            };
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

        macro_rules! call {
            ($method:ty => $handler:ident) => {
                match decode_request::<$method>(id, &params) {
                    Ok(params) => self.$handler(id, params),
                    Err(outcome) => outcome,
                }
            };
        }

        match method {
            m if m == PaneOpen::METHOD => call!(PaneOpen => open_pane),
            m if m == PaneFocus::METHOD => call!(PaneFocus => focus_pane),
            m if m == PaneClose::METHOD => call!(PaneClose => close_pane),
            m if m == PaneResize::METHOD => call!(PaneResize => resize_pane),
            m if m == LayoutGet::METHOD => self.layout_get(id),
            m if m == EditorOpen::METHOD => call!(EditorOpen => editor_open),
            m if m == DocumentSave::METHOD => call!(DocumentSave => save_document),
            m if m == BufferSnapshot::METHOD => call!(BufferSnapshot => snapshot),
            m if m == SyncRequest::METHOD => call!(SyncRequest => sync_request),
            m if m == BufferUndo::METHOD => call!(BufferUndo => undo),
            m if m == BufferRedo::METHOD => call!(BufferRedo => redo),
            m if m == FsReadFile::METHOD => call!(FsReadFile => fs_read_file),
            m if m == FsReadTextFile::METHOD => call!(FsReadTextFile => fs_read_text_file),
            m if m == FsWriteFile::METHOD => call!(FsWriteFile => fs_write_file),
            m if m == FsReadDir::METHOD => call!(FsReadDir => fs_read_dir),
            m if m == FsStat::METHOD => call!(FsStat => fs_stat),
            m if m == FsExists::METHOD => call!(FsExists => fs_exists),
            m if m == FsCreateDir::METHOD => call!(FsCreateDir => fs_create_dir),
            m if m == FsRemove::METHOD => call!(FsRemove => fs_remove),
            m if m == FsRename::METHOD => call!(FsRename => fs_rename),
            m if m == FsCopy::METHOD => call!(FsCopy => fs_copy),
            m if m == ProcessExec::METHOD => call!(ProcessExec => process_exec),
            m if m == ProcessSpawn::METHOD => call!(ProcessSpawn => process_spawn),
            m if m == ProcessKill::METHOD => call!(ProcessKill => process_kill),
            m if m == ProcessStdin::METHOD => call!(ProcessStdin => process_stdin),
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
        if method == BufferDidChange::METHOD {
            match vy_proto::decode_notification::<BufferDidChange>(&params) {
                Some(change) => self.did_change(change),
                None => {
                    tracing::warn!("buffer/did_change could not be decoded");
                    Outcome::empty()
                }
            }
        } else {
            tracing::debug!(method, "unknown notification");
            Outcome::empty()
        }
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns the history result in the shape described by `docs/specs/protocol.md`.
fn applied_result(document_id: u64, applied: &Applied) -> vy_proto::HistoryResult {
    vy_proto::HistoryResult {
        document_id,
        version: applied.version,
        edits: applied.edits.clone(),
        selection: applied.selection.clone(),
    }
}

#[cfg(test)]
mod tests;
