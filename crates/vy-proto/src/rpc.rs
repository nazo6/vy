//! Typed method and notification markers for the client/server RPC.
//!
//! The envelope in [`crate::message`] carries the method name as a string and
//! the payload as an [`rmpv::Value`]. That is the right shape on the wire, but
//! it lets call sites pair a method with the wrong params or reply type.
//!
//! [`RpcMethod`] and [`RpcNotification`] tie the wire name to its payload
//! types. The marker types below are the only place where the pair is written,
//! so `Client::request::<PaneOpen>` and the server dispatcher cannot drift.

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::message::{RpcMessage, RpcResult};
use crate::protocol::*;
use crate::value::{decode_value, to_value};

/// One client-to-server request: its wire name, params, and reply.
pub trait RpcMethod {
    /// Wire method name, for example `pane/open`.
    const METHOD: &'static str;
    /// Parameters sent with the request.
    type Params: Serialize + DeserializeOwned;
    /// Reply returned by the server.
    type Reply: Serialize + DeserializeOwned;
}

/// One one-way notification in either direction.
pub trait RpcNotification {
    /// Wire method name, for example `buffer/did_change`.
    const METHOD: &'static str;
    /// Payload sent with the notification.
    type Params: Serialize + DeserializeOwned;
}

macro_rules! define_methods {
    ($(
        $(#[$meta:meta])*
        $name:ident => ($method:literal, $params:ty, $reply:ty),
    )*) => {
        $(
            $(#[$meta])*
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub struct $name;

            impl RpcMethod for $name {
                const METHOD: &'static str = $method;
                type Params = $params;
                type Reply = $reply;
            }
        )*
    };
}

macro_rules! define_notifications {
    ($(
        $(#[$meta:meta])*
        $name:ident => ($method:literal, $params:ty),
    )*) => {
        $(
            $(#[$meta])*
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub struct $name;

            impl RpcNotification for $name {
                const METHOD: &'static str = $method;
                type Params = $params;
            }
        )*
    };
}

define_methods! {
    /// Starts a connection and attaches a session.
    Initialize => ("initialize", InitializeParams, InitializeResult),
    /// Opens a document in a new pane.
    PaneOpen => ("pane/open", PaneOpenParams, PaneOpenResult),
    /// Focuses a pane.
    PaneFocus => ("pane/focus", PaneFocusParams, PaneFocusResult),
    /// Closes a pane.
    PaneClose => ("pane/close", PaneCloseParams, PaneCloseResult),
    /// Returns the whole layout.
    LayoutGet => ("layout/get", (), LayoutGetResult),
    /// Opens a document in an existing pane.
    EditorOpen => ("editor/open", EditorOpenParams, PaneOpenResult),
    /// Writes a document back to its file.
    DocumentSave => ("document/save", DocumentIdParams, DocumentSavedResult),
    /// Returns the full text of a document.
    BufferSnapshot => ("buffer/snapshot", DocumentIdParams, SnapshotResult),
    /// Rebuilds documents after a reconnect.
    SyncRequest => ("sync/request", SyncRequestParams, SyncRequestResult),
    /// Undoes one transaction.
    BufferUndo => ("buffer/undo", DocumentIdParams, HistoryResult),
    /// Redoes one transaction.
    BufferRedo => ("buffer/redo", DocumentIdParams, HistoryResult),
    /// Reads a binary file.
    FsReadFile => ("fs/read_file", FsReadFileParams, FsReadFileResult),
    /// Reads a UTF-8 text file.
    FsReadTextFile => ("fs/read_text_file", FsReadTextFileParams, FsReadTextFileResult),
    /// Writes a file.
    FsWriteFile => ("fs/write_file", FsWriteFileParams, FsWriteFileResult),
    /// Lists a directory.
    FsReadDir => ("fs/read_dir", FsReadDirParams, FsReadDirResult),
    /// Returns file metadata.
    FsStat => ("fs/stat", FsStatParams, FsStatResult),
    /// Checks whether a path exists.
    FsExists => ("fs/exists", FsExistsParams, FsExistsResult),
    /// Creates a directory.
    FsCreateDir => ("fs/create_dir", FsCreateDirParams, FsCreateDirResult),
    /// Removes a file or directory.
    FsRemove => ("fs/remove", FsRemoveParams, FsRemoveResult),
    /// Renames or moves a file.
    FsRename => ("fs/rename", FsRenameParams, FsRenameResult),
    /// Copies a file or directory.
    FsCopy => ("fs/copy", FsCopyParams, FsCopyResult),
    /// Runs a process and waits for it.
    ProcessExec => ("process/exec", ProcessExecParams, ProcessOutput),
    /// Spawns an asynchronous process.
    ProcessSpawn => ("process/spawn", ProcessSpawnParams, ProcessSpawnResult),
    /// Kills a process.
    ProcessKill => ("process/kill", ProcessKillParams, ProcessKillResult),
    /// Writes to a process's standard input.
    ProcessStdin => ("process/stdin", ProcessStdinParams, ProcessStdinResult),
}

define_notifications! {
    /// The server finished the handshake.
    Initialized => ("initialized", InitializedNotification),
    /// The client applied edits to a document.
    BufferDidChange => ("buffer/did_change", DidChangeParams),
    /// The server asks the client to reread a document.
    BufferResync => ("buffer/resync", ResyncNotification),
    /// The server committed a new layout.
    LayoutDidChange => ("layout/did_change", LayoutDidChangeNotification),
}

/// Serializes a request's params for the envelope.
pub fn encode_params<M: RpcMethod>(params: &M::Params) -> rmpv::Value {
    to_value(params)
}

/// Deserializes a request's params from the envelope.
pub fn decode_params<M: RpcMethod>(params: &rmpv::Value) -> Option<M::Params> {
    decode_value(params)
}

/// Serializes a request's reply for the envelope.
pub fn encode_reply<M: RpcMethod>(reply: &M::Reply) -> rmpv::Value {
    to_value(reply)
}

/// Deserializes a request's reply from the envelope.
pub fn decode_reply<M: RpcMethod>(value: &rmpv::Value) -> Option<M::Reply> {
    decode_value(value)
}

/// Serializes a notification payload.
pub fn encode_notification<N: RpcNotification>(params: &N::Params) -> rmpv::Value {
    to_value(params)
}

/// Deserializes a notification payload.
pub fn decode_notification<N: RpcNotification>(params: &rmpv::Value) -> Option<N::Params> {
    decode_value(params)
}

/// Builds a typed request message.
pub fn request<M: RpcMethod>(id: u64, session_id: u64, params: &M::Params) -> RpcMessage {
    RpcMessage::request(id, session_id, M::METHOD, encode_params::<M>(params))
}

/// Builds an `Ok` response for a typed method.
pub fn ok<M: RpcMethod>(id: u64, reply: &M::Reply) -> RpcMessage {
    RpcMessage::Response {
        id,
        result: RpcResult::Ok {
            value: encode_reply::<M>(reply),
        },
    }
}

/// Builds a typed notification message.
pub fn notification<N: RpcNotification>(params: &N::Params) -> RpcMessage {
    RpcMessage::notification(N::METHOD, encode_notification::<N>(params))
}

/// A request ready to be sent over the envelope.
///
/// Dynamic dispatch points (the plugin host) cannot name the generic method
/// type at the call site. Building this value where the params are known keeps
/// the method name and payload together until the client sends it.
#[derive(Debug, Clone, PartialEq)]
pub struct RpcCall {
    pub method: &'static str,
    pub params: rmpv::Value,
}

impl RpcCall {
    /// Builds a call from a typed request.
    pub fn new<M: RpcMethod>(params: M::Params) -> Self {
        Self {
            method: M::METHOD,
            params: encode_params::<M>(&params),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_markers_keep_the_wire_names() {
        assert_eq!(PaneOpen::METHOD, "pane/open");
        assert_eq!(FsReadFile::METHOD, "fs/read_file");
        assert_eq!(ProcessStdin::METHOD, "process/stdin");
        assert_eq!(BufferDidChange::METHOD, "buffer/did_change");
    }

    #[test]
    fn a_typed_request_roundtrips_its_params() {
        let params = PaneFocusParams { pane_id: 7 };
        let message = request::<PaneFocus>(1, 2, &params);
        let RpcMessage::Request {
            method,
            params: value,
            ..
        } = message
        else {
            panic!("expected a request");
        };
        assert_eq!(method, PaneFocus::METHOD);
        assert_eq!(decode_params::<PaneFocus>(&value), Some(params));
    }

    #[test]
    fn a_typed_reply_roundtrips_its_value() {
        let reply = PaneFocusResult { pane_id: 7 };
        let RpcMessage::Response {
            result: RpcResult::Ok { value },
            ..
        } = ok::<PaneFocus>(1, &reply)
        else {
            panic!("expected an ok response");
        };
        assert_eq!(decode_reply::<PaneFocus>(&value), Some(reply));
    }

    #[test]
    fn a_typed_notification_roundtrips_its_payload() {
        let payload = LayoutDidChangeNotification {
            window_id: Some(1),
            revision: 3,
            layout: None,
            floating: Vec::new(),
            active_pane: None,
            reason: Some("open".into()),
        };
        let RpcMessage::Notification { method, params } = notification::<LayoutDidChange>(&payload)
        else {
            panic!("expected a notification");
        };
        assert_eq!(method, LayoutDidChange::METHOD);
        assert_eq!(
            decode_notification::<LayoutDidChange>(&params),
            Some(payload)
        );
    }

    #[test]
    fn unit_params_roundtrip_from_nil() {
        assert_eq!(decode_params::<LayoutGet>(&rmpv::Value::Nil), Some(()));
    }
}
