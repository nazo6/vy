//! Strongly-typed RPC request, response, and notification payloads.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use vy_core::{LayoutNode, Selection, TextEdit};

/// Parameters sent with an `initialize` request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InitializeParams {
    pub protocol_version: u32,
    #[serde(default)]
    pub client_version: String,
    #[serde(default)]
    pub client_capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_session: Option<u64>,
}

/// Response returned from an `initialize` request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InitializeResult {
    pub session_id: u64,
}

/// Platform information returned in `initialized`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformInfo {
    pub os: String,
    pub path_separator: String,
}

/// Notification sent by the server after initialization completes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InitializedNotification {
    pub protocol_version: u32,
    pub session_id: u64,
    pub server_version: String,
    #[serde(default)]
    pub server_capabilities: Vec<String>,
    pub platform: PlatformInfo,
}

/// Placement options when opening a pane.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PanePlacement {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ratio: Option<f64>,
}

/// Parameters for `pane/open`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaneOpenParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<String>,
    pub uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<PanePlacement>,
}

/// Response payload from `pane/open` or `editor/open`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneOpenResult {
    pub pane_id: u64,
    pub document_id: u64,
    pub version: u64,
    #[serde(default)]
    pub line_count: u64,
    pub uri: String,
    #[serde(default)]
    pub replaced: bool,
}

/// Parameters for `pane/focus`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneFocusParams {
    pub pane_id: u64,
}

/// Response from `pane/focus`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneFocusResult {
    pub pane_id: u64,
}

/// Parameters for `pane/close`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneCloseParams {
    pub pane_id: u64,
    #[serde(default)]
    pub force: bool,
}

/// Response from `pane/close`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneCloseResult {
    pub pane_id: u64,
}

/// Parameters for `editor/open`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorOpenParams {
    pub uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane_id: Option<u64>,
}

/// Parameters identifying a document by its integer ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentIdParams {
    #[serde(alias = "buffer_id")]
    pub document_id: u64,
}

/// Result of `document/save` acknowledging the saved document ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentSavedResult {
    pub document_id: u64,
    pub version: u64,
    pub path: String,
}

/// Response from `buffer/snapshot` or payload of `buffer/resync`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotResult {
    pub document_id: u64,
    pub version: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_count: Option<u64>,
    pub text: String,
}

/// Notification payload for `buffer/resync`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResyncNotification {
    pub document_id: u64,
    pub version: u64,
    pub text: String,
}

/// One document in a `sync/request`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncDocument {
    pub uri: String,
    pub version: u64,
    #[serde(default)]
    pub saved_version: u64,
    /// Present only when the client has unsaved edits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// Parameters for `sync/request`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncRequestParams {
    #[serde(default)]
    pub documents: Vec<SyncDocument>,
}

/// One synchronized document in the reply to `sync/request`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncedDocument {
    pub uri: String,
    pub document_id: u64,
    pub version: u64,
    #[serde(default)]
    pub saved_version: u64,
    pub text: String,
}

/// Reply to `sync/request`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncRequestResult {
    #[serde(default)]
    pub documents: Vec<SyncedDocument>,
}

/// Notification payload for `buffer/did_change`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DidChangeParams {
    #[serde(alias = "buffer_id")]
    pub document_id: u64,
    pub base_version: u64,
    #[serde(default)]
    pub version: u64,
    pub transaction_id: u64,
    pub edits: Vec<TextEdit>,
    #[serde(default)]
    pub selection_before: Vec<Selection>,
    #[serde(default)]
    pub selection_after: Vec<Selection>,
}

/// Response from `buffer/undo` and `buffer/redo`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryResult {
    pub document_id: u64,
    pub version: u64,
    #[serde(default)]
    pub edits: Vec<Vec<TextEdit>>,
    #[serde(default)]
    pub selection: Vec<Selection>,
}

/// Information about a pane returned within layout queries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutPaneInfo {
    pub pane_id: u64,
    pub view: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_id: Option<u64>,
    /// URI of the document shown in the pane, used to remap after a reconnect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
}

/// Response from `layout/get`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutGetResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_id: Option<u64>,
    pub revision: u64,
    #[serde(rename = "layout", default)]
    pub layout: Option<LayoutNode>,
    #[serde(default)]
    pub floating: Vec<rmpv::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_pane: Option<u64>,
    #[serde(default)]
    pub panes: Vec<LayoutPaneInfo>,
}

/// Notification payload for `layout/did_change`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutDidChangeNotification {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_id: Option<u64>,
    pub revision: u64,
    #[serde(rename = "layout", default)]
    pub layout: Option<LayoutNode>,
    #[serde(default)]
    pub floating: Vec<rmpv::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_pane: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Information about a directory entry returned by `fs/read_dir`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DirEntry {
    pub name: String,
    pub uri: String,
    pub is_file: bool,
    pub is_directory: bool,
    pub is_symlink: bool,
}

/// File metadata returned by `fs/stat`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FileStat {
    pub is_file: bool,
    pub is_directory: bool,
    pub is_symlink: bool,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtime_ms: Option<u64>,
}

/// Parameters for `fs/read_file`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsReadFileParams {
    pub uri: String,
}

/// Result of `fs/read_file`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsReadFileResult {
    pub bytes: Vec<u8>,
}

/// Parameters for `fs/read_text_file`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsReadTextFileParams {
    pub uri: String,
}

/// Result of `fs/read_text_file`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsReadTextFileResult {
    pub text: String,
}

/// Parameters for `fs/write_file`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsWriteFileParams {
    pub uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// Result of `fs/write_file`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsWriteFileResult {
    pub bytes_written: u64,
}

/// Parameters for `fs/read_dir`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsReadDirParams {
    pub uri: String,
}

/// Result of `fs/read_dir`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsReadDirResult {
    pub entries: Vec<DirEntry>,
}

/// Parameters for `fs/stat`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsStatParams {
    pub uri: String,
}

/// Result of `fs/stat`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsStatResult {
    pub stat: FileStat,
}

/// Parameters for `fs/exists`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsExistsParams {
    pub uri: String,
}

/// Result of `fs/exists`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsExistsResult {
    pub exists: bool,
}

/// Parameters for `fs/create_dir`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsCreateDirParams {
    pub uri: String,
    #[serde(default)]
    pub recursive: bool,
}

/// Result of `fs/create_dir`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsCreateDirResult {
    pub success: bool,
}

/// Parameters for `fs/remove`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsRemoveParams {
    pub uri: String,
    #[serde(default)]
    pub recursive: bool,
}

/// Result of `fs/remove`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsRemoveResult {
    pub success: bool,
}

/// Parameters for `fs/rename`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsRenameParams {
    pub from: String,
    pub to: String,
}

/// Result of `fs/rename`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsRenameResult {
    pub success: bool,
}

/// Parameters for `fs/copy`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsCopyParams {
    pub from: String,
    pub to: String,
}

/// Result of `fs/copy`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsCopyResult {
    pub success: bool,
}

/// Parameters for `process/exec`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ProcessExecParams {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<BTreeMap<String, String>>,
}

/// Output of a completed process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ProcessOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Parameters for `process/spawn`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessSpawnParams {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<BTreeMap<String, String>>,
}

/// Result of `process/spawn`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessSpawnResult {
    pub pid: u32,
}

/// Parameters for `process/kill`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessKillParams {
    pub pid: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal: Option<String>,
}

/// Result of `process/kill`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessKillResult {
    pub success: bool,
}

/// Parameters for `process/stdin`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessStdinParams {
    pub pid: u32,
    pub data: String,
}

/// Result of `process/stdin`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessStdinResult {
    pub success: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{decode_value, to_value};

    #[test]
    fn roundtrips_initialize_params() {
        let params = InitializeParams {
            protocol_version: 3,
            client_version: "0.1.0".into(),
            client_capabilities: vec!["stream".into()],
            workspace: Some("file:///tmp".into()),
            resume_session: None,
        };
        let value = to_value(&params);
        let decoded: InitializeParams = decode_value(&value).expect("decode");
        assert_eq!(params, decoded);
    }

    #[test]
    fn roundtrips_pane_open_result() {
        let result = PaneOpenResult {
            pane_id: 1,
            document_id: 2,
            version: 0,
            line_count: 10,
            uri: "file:///test".into(),
            replaced: false,
        };
        let value = to_value(&result);
        let decoded: PaneOpenResult = decode_value(&value).expect("decode");
        assert_eq!(result, decoded);
    }

    #[test]
    fn roundtrips_fs_and_process_params() {
        let fs_read = FsReadFileParams {
            uri: "file:///workspace/test.txt".into(),
        };
        let val = to_value(&fs_read);
        let decoded: FsReadFileParams = decode_value(&val).expect("decode fs read");
        assert_eq!(fs_read, decoded);

        let proc_exec = ProcessExecParams {
            command: "echo".into(),
            args: vec!["hello".into()],
            cwd: Some("/workspace".into()),
            env: None,
        };
        let val = to_value(&proc_exec);
        let decoded: ProcessExecParams = decode_value(&val).expect("decode proc exec");
        assert_eq!(proc_exec, decoded);
    }
}
