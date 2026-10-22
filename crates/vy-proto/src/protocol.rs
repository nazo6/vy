//! Strongly-typed RPC request, response, and notification payloads.

use serde::{Deserialize, Serialize};
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{decode_value, to_value};

    #[test]
    fn roundtrips_initialize_params() {
        let params = InitializeParams {
            protocol_version: 2,
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
}
