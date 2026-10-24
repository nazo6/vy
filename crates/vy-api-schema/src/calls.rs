//! Request and reply shapes for every host method.
//!
//! The methods themselves are declared in `vy-app` with
//! `#[vy_api_namespace]`; this module only holds the types that cross the
//! boundary. See `docs/adr/0008-declare-plugin-api-with-macros.md`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use vy_core::{Position, Range, Selection, TextEdit, Uri};
pub use vy_proto::{DirEntry, FileStat, ProcessExecParams, ProcessOutput};

use crate::{Mode, PickerItem, StatuslineAlignment, ThemeColor, WidgetNode};

// Configuration.

/// `config.get` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConfigGetParams {
    pub key: String,
}

/// `config.set` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConfigSetParams {
    #[ts(type = "Record<string, unknown>")]
    pub values: serde_json::Map<String, Value>,
}

/// `config.set` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConfigSetReply {
    pub changed: Vec<String>,
}

/// `config.inspect` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConfigInspectParams {
    pub key: String,
}

/// `config.inspect` reply: every layer's value plus the effective one.
///
/// Layers are listed from lowest to highest precedence: `default`, `user`,
/// `project`, `memory`. A layer that does not define the key is `null`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConfigInspectReply {
    #[ts(type = "unknown")]
    pub default: Option<Value>,
    #[ts(type = "unknown")]
    pub user: Option<Value>,
    #[ts(type = "unknown")]
    pub project: Option<Value>,
    #[ts(type = "unknown")]
    pub memory: Option<Value>,
    #[ts(type = "unknown")]
    pub effective: Option<Value>,
}

// Commands.

/// `commands.registerCommand` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CommandsRegisterParams {
    pub id: String,
}

/// `commands.execute` parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CommandsExecuteParams {
    pub id: String,
    #[serde(default)]
    #[ts(type = "unknown")]
    pub args: Value,
}

// Keymap.

/// `keymap.bind` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct KeymapBindParams {
    pub keys: String,
    pub command: String,
    #[ts(optional = nullable)]
    pub when: Option<String>,
}

/// `keymap.unbind` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct KeymapUnbindParams {
    pub keys: String,
}

/// A value visible to a `when` expression, set by `keymap.setContext`.
///
/// A context value is a boolean, a string, or a number.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, TS)]
#[serde(untagged)]
#[ts(export)]
pub enum KeyContextValue {
    Bool(bool),
    Text(String),
    Number(f64),
}

/// `keymap.setContext` parameters.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct KeymapSetContextParams {
    pub key: String,
    pub value: KeyContextValue,
}

// Editor.

/// `editor.setMode` parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EditorSetModeParams {
    pub mode: Mode,
}

/// `editor.setSelections` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EditorSetSelectionsParams {
    pub selections: Vec<Selection>,
}

/// `editor.edit` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EditorEditParams {
    pub edits: Vec<TextEdit>,
}

/// `editor.edit` reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EditorEditReply {
    #[ts(type = "number")]
    pub version: u64,
}

/// `editor.open` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EditorOpenParams {
    pub uri: Uri,
    #[serde(default)]
    #[ts(type = "number")]
    #[ts(optional = nullable)]
    pub pane_id: Option<u64>,
}

// Documents.

/// A document as the script sees it.
///
/// `end` is the position just after the last character. `version` increases
/// after every applied edit and can be used to detect a stale snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DocumentInfoReply {
    #[ts(type = "number")]
    pub id: u64,
    pub uri: Uri,
    pub language_id: String,
    #[ts(type = "number")]
    pub version: u64,
    pub line_count: usize,
    pub end: Position,
}

/// `document.getText` parameters.
///
/// `documentId` defaults to the active document. `range` defaults to the whole
/// document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DocumentGetTextParams {
    #[ts(type = "number")]
    #[ts(optional = nullable)]
    pub document_id: Option<u64>,
    #[ts(optional = nullable)]
    pub range: Option<Range>,
}

/// `document.lineAt` parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DocumentLineAtParams {
    #[ts(type = "number")]
    #[ts(optional = nullable)]
    pub document_id: Option<u64>,
    pub line: usize,
}

/// `document.positionAt` parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DocumentPositionAtParams {
    #[ts(type = "number")]
    #[ts(optional = nullable)]
    pub document_id: Option<u64>,
    pub offset: usize,
}

/// `document.offsetAt` parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DocumentOffsetAtParams {
    #[ts(type = "number")]
    #[ts(optional = nullable)]
    pub document_id: Option<u64>,
    pub position: Position,
}

// Notifications and logs.

/// Severity of a log line or notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

/// `notifications.show` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export)]
pub struct NotificationsShowParams {
    pub message: String,
}

/// `log.write` parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LogWriteParams {
    pub channel: String,
    pub level: LogLevel,
    pub message: String,
    #[serde(default)]
    #[ts(type = "unknown")]
    pub data: Value,
}

// Slots: statusline, gutter, hover, picker, views.

/// `ui.statusline.createItem` parameters.
///
/// `alignment` places the item on the left or right. `priority` orders items
/// within the same side; higher values are shown first. `command` runs when the
/// item is clicked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StatuslineCreateParams {
    pub id: String,
    pub alignment: StatuslineAlignment,
    #[serde(default)]
    #[ts(optional)]
    pub priority: Option<i32>,
    pub text: String,
    #[ts(optional = nullable)]
    pub color: Option<ThemeColor>,
    #[ts(optional = nullable)]
    pub command: Option<String>,
}

/// `ui.statusline.updateItem` parameters.
///
/// `color` and `command` distinguish an absent field (leave as is) from `null`
/// (clear it). The host validates `color` against the theme tokens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StatuslineUpdateParams {
    pub id: String,
    #[ts(optional = nullable)]
    pub text: Option<String>,
    #[ts(type = "string | null")]
    pub color: Option<Value>,
    #[ts(type = "string | null")]
    pub command: Option<Value>,
    #[ts(optional = nullable)]
    pub visible: Option<bool>,
}

/// `ui.statusline.disposeItem` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StatuslineDisposeParams {
    pub id: String,
}

/// `ui.gutter.createItem` parameters.
///
/// `line` is zero-based. The item is drawn in the gutter of that line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GutterCreateParams {
    pub id: String,
    pub line: usize,
    pub text: String,
    #[ts(optional = nullable)]
    pub color: Option<ThemeColor>,
}

/// `ui.gutter.disposeItem` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct GutterDisposeParams {
    pub id: String,
}

/// `ui.hover.show` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HoverShowParams {
    pub line: usize,
    pub character: usize,
    pub text: String,
}

/// `ui.picker.show` parameters.
///
/// When `items` is omitted the picker lists commands. `placeholder` is shown
/// in the filter input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PickerShowParams {
    #[serde(default)]
    #[ts(optional)]
    pub items: Option<Vec<PickerItem>>,
    #[ts(optional = nullable)]
    pub placeholder: Option<String>,
}

/// `ui.picker.show` reply.
///
/// The reply is `null` when the picker is dismissed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PickerShowReply {
    pub selected_id: String,
}

/// `views.render` parameters.
///
/// `tree` is the widget tree returned by the view's render callback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ViewsRenderParams {
    pub id: String,
    pub tree: WidgetNode,
}

/// Direction a view pane opens toward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum ViewDirection {
    Left,
    Right,
    Above,
    Below,
}

impl ViewDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Above => "above",
            Self::Below => "below",
        }
    }
}

/// `views.open` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ViewOpenParams {
    pub view: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub direction: Option<ViewDirection>,
}

/// `views.close` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ViewCloseParams {
    pub view: String,
}

/// `views.toggle` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ViewToggleParams {
    pub view: String,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub direction: Option<ViewDirection>,
}

/// `fs.readFile` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FsReadFileParams {
    pub uri: Uri,
}

/// `fs.readFile` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FsReadFileReply {
    pub bytes: Vec<u8>,
}

/// `fs.readTextFile` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FsReadTextFileParams {
    pub uri: Uri,
}

/// `fs.readTextFile` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FsReadTextFileReply {
    pub text: String,
}

/// `fs.writeFile` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FsWriteFileParams {
    pub uri: Uri,
    #[serde(default)]
    #[ts(optional)]
    pub bytes: Option<Vec<u8>>,
    #[serde(default)]
    #[ts(optional)]
    pub text: Option<String>,
}

/// `fs.writeFile` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FsWriteFileReply {
    pub bytes_written: u64,
}

/// `fs.readDir` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FsReadDirParams {
    pub uri: Uri,
}

/// `fs.readDir` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FsReadDirReply {
    pub entries: Vec<DirEntry>,
}

/// `fs.stat` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FsStatParams {
    pub uri: Uri,
}

/// `fs.stat` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FsStatReply {
    pub stat: FileStat,
}

/// `fs.exists` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FsExistsParams {
    pub uri: Uri,
}

/// `fs.exists` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FsExistsReply {
    pub exists: bool,
}

/// `fs.createDir` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FsCreateDirParams {
    pub uri: Uri,
    #[serde(default)]
    #[ts(optional)]
    pub recursive: Option<bool>,
}

/// `fs.createDir` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FsCreateDirReply {
    pub success: bool,
}

/// `fs.remove` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FsRemoveParams {
    pub uri: Uri,
    #[serde(default)]
    #[ts(optional)]
    pub recursive: Option<bool>,
}

/// `fs.remove` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FsRemoveReply {
    pub success: bool,
}

/// `fs.rename` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FsRenameParams {
    pub from: Uri,
    pub to: Uri,
}

/// `fs.rename` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FsRenameReply {
    pub success: bool,
}

/// `fs.copy` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FsCopyParams {
    pub from: Uri,
    pub to: Uri,
}

/// `fs.copy` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FsCopyReply {
    pub success: bool,
}

/// `process.spawn` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProcessSpawnParams {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    #[ts(optional)]
    pub cwd: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub env: Option<std::collections::BTreeMap<String, String>>,
}

/// `process.spawn` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProcessSpawnReply {
    pub pid: u32,
}

/// `process.kill` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProcessKillParams {
    pub pid: u32,
    #[serde(default)]
    #[ts(optional)]
    pub signal: Option<String>,
}

/// `process.kill` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProcessKillReply {
    pub success: bool,
}

/// `process.writeStdin` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProcessStdinParams {
    pub pid: u32,
    pub data: String,
}

/// `process.writeStdin` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProcessStdinReply {
    pub success: bool,
}

// Workspace and plugin context.

/// `workspace.info` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WorkspaceInfoReply {
    pub root_uri: Option<Uri>,
    pub name: Option<String>,
    pub folders: Vec<Uri>,
    pub trusted: bool,
}

/// `context.info` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PluginContextReply {
    pub plugin_id: String,
    pub version: String,
    pub remote: RemoteInfo,
}

/// Information about the remote connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RemoteInfo {
    pub is_remote: bool,
    pub name: String,
    pub os: String,
    pub path_separator: String,
}

// Events. The topic of an event is `<namespace>.<subscription name>`, and the
// subscription name starts with `on`; the shim relies on that to tell events
// from methods.

/// Payload of `config.onDidChange`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConfigChange {
    pub key: String,
}

/// Payload of `editor.onDidChangeActiveDocument`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ActiveDocumentChange {
    pub document: Option<DocumentInfoReply>,
}

/// Payload of `editor.onDidChangeSelection`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SelectionChange {
    pub selections: Vec<Selection>,
}

/// Payload of `workspace.onDidChangeWorkspaceFolders`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WorkspaceFoldersChange {
    pub folders: Vec<String>,
}

/// Payload of `process.onStdout` and `process.onStderr`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProcessData {
    pub pid: u32,
    pub data: String,
}

/// Payload of `process.onExit`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProcessExit {
    pub pid: u32,
    pub exit_code: i32,
}
