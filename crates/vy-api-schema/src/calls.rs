//! Request and reply shapes for every host method.
//!
//! [`HOST_METHODS`] is the single source of truth for the API surface. The shim
//! and the generated TypeScript declarations are derived from it, and the host
//! parses each call with the parameter type named in the table.
//!
//! A method that has a hand-written wrapper in the shim is marked `manual`: such
//! a method needs JavaScript state (a command handler map, a builder, a deferred
//! reply) and cannot be generated as a plain forwarder.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum::VariantArray;
use ts_rs::TS;

use vy_core::{Position, Range, Selection, TextEdit, Uri};

use crate::{Mode, PickerItem, StatuslineAlignment, ThemeColor, WidgetNode};

/// Parameters of a call that takes none.
///
/// Missing parameters arrive as `null` and as `{}`, so both are accepted. The
/// table names its TypeScript shape directly because an empty struct exports as
/// an unusable type.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, TS)]
pub struct Empty {}

impl<'de> Deserialize<'de> for Empty {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let _ = Value::deserialize(deserializer)?;
        Ok(Self {})
    }
}

/// One host method. Variants are named after the wire method.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, strum::EnumString, strum::IntoStaticStr, VariantArray,
)]
pub enum HostCall {
    #[strum(serialize = "config.get")]
    ConfigGet,
    #[strum(serialize = "config.set")]
    ConfigSet,
    #[strum(serialize = "config.inspect")]
    ConfigInspect,
    #[strum(serialize = "commands.registerCommand")]
    CommandsRegisterCommand,
    #[strum(serialize = "commands.execute")]
    CommandsExecute,
    #[strum(serialize = "commands.getCommands")]
    CommandsGetCommands,
    #[strum(serialize = "keymap.bind")]
    KeymapBind,
    #[strum(serialize = "keymap.unbind")]
    KeymapUnbind,
    #[strum(serialize = "keymap.setContext")]
    KeymapSetContext,
    #[strum(serialize = "editor.activeDocument")]
    EditorActiveDocument,
    #[strum(serialize = "editor.documents")]
    EditorDocuments,
    #[strum(serialize = "editor.mode")]
    EditorMode,
    #[strum(serialize = "editor.setMode")]
    EditorSetMode,
    #[strum(serialize = "editor.selections")]
    EditorSelections,
    #[strum(serialize = "editor.setSelections")]
    EditorSetSelections,
    #[strum(serialize = "editor.edit")]
    EditorEdit,
    #[strum(serialize = "editor.undo")]
    EditorUndo,
    #[strum(serialize = "editor.redo")]
    EditorRedo,
    #[strum(serialize = "workspace.textDocuments")]
    WorkspaceTextDocuments,
    #[strum(serialize = "document.getText")]
    DocumentGetText,
    #[strum(serialize = "document.lineAt")]
    DocumentLineAt,
    #[strum(serialize = "document.positionAt")]
    DocumentPositionAt,
    #[strum(serialize = "document.offsetAt")]
    DocumentOffsetAt,
    #[strum(serialize = "notifications.show")]
    NotificationsShow,
    #[strum(serialize = "log.write")]
    LogWrite,
    #[strum(serialize = "ui.statusline.createItem")]
    UiStatuslineCreateItem,
    #[strum(serialize = "ui.statusline.updateItem")]
    UiStatuslineUpdateItem,
    #[strum(serialize = "ui.statusline.disposeItem")]
    UiStatuslineDisposeItem,
    #[strum(serialize = "views.render")]
    ViewsRender,
    #[strum(serialize = "ui.picker.show")]
    UiPickerShow,
    #[strum(serialize = "ui.gutter.createItem")]
    UiGutterCreateItem,
    #[strum(serialize = "ui.gutter.disposeItem")]
    UiGutterDisposeItem,
    #[strum(serialize = "ui.gutter.clear")]
    UiGutterClear,
    #[strum(serialize = "ui.hover.show")]
    UiHoverShow,
    #[strum(serialize = "ui.hover.hide")]
    UiHoverHide,
}

impl HostCall {
    /// Parses a wire method name.
    pub fn parse(method: &str) -> Option<Self> {
        method.parse().ok()
    }

    /// The wire method name.
    pub fn name(self) -> &'static str {
        self.into()
    }
}

/// How one method appears in the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MethodSpec {
    /// Wire name, as the shim sends it.
    pub method: &'static str,
    /// Name of the TypeScript type of the parameters.
    pub params: &'static str,
    /// Name of the TypeScript type of the reply.
    pub result: &'static str,
    /// Whether the shim needs a hand-written wrapper for this method.
    pub manual: bool,
    /// One-line description emitted as JSDoc in the generated declarations.
    pub doc: &'static str,
}

impl MethodSpec {
    /// Whether the method takes parameters.
    ///
    /// The shim generates a zero-argument function for methods without any, so
    /// scripts write `editor.mode()` rather than `editor.mode({})`.
    pub fn has_params(&self) -> bool {
        self.params != EMPTY_PARAMS
    }
}

/// Builds a table entry without repeating the field names.
const fn spec(
    method: &'static str,
    params: &'static str,
    result: &'static str,
    manual: bool,
    doc: &'static str,
) -> MethodSpec {
    MethodSpec {
        method,
        params,
        result,
        manual,
        doc,
    }
}

/// TypeScript type the table uses for methods without parameters.
pub const EMPTY_PARAMS: &str = "Record<string, never>";

/// Every host method. `HostCall` and this table must agree (see the tests).
pub const HOST_METHODS: &[MethodSpec] = &[
    spec(
        "config.get",
        "ConfigGetParams",
        "unknown",
        true,
        "Returns the effective value of a setting key.",
    ),
    spec(
        "config.set",
        "ConfigSetParams",
        "ConfigSetReply",
        true,
        "Sets one or more setting values in memory and returns the keys that changed.",
    ),
    spec(
        "config.inspect",
        "ConfigInspectParams",
        "ConfigInspectReply",
        false,
        "Returns every layer's value for a setting key, plus the effective value.",
    ),
    spec(
        "commands.registerCommand",
        "CommandsRegisterParams",
        "null",
        true,
        "Registers a command handler and returns a disposable registration.",
    ),
    spec(
        "commands.execute",
        "CommandsExecuteParams",
        "null",
        true,
        "Runs a command by id with optional arguments.",
    ),
    spec(
        "commands.getCommands",
        EMPTY_PARAMS,
        "string[]",
        false,
        "Lists the ids of every registered command.",
    ),
    spec(
        "keymap.bind",
        "KeymapBindParams",
        "null",
        true,
        "Binds a key sequence to a command.",
    ),
    spec(
        "keymap.unbind",
        "KeymapUnbindParams",
        "number",
        false,
        "Removes a key binding and returns the number removed.",
    ),
    spec(
        "keymap.setContext",
        "KeymapSetContextParams",
        "null",
        false,
        "Sets a keymap context value used by `when` clauses.",
    ),
    spec(
        "editor.activeDocument",
        EMPTY_PARAMS,
        "DocumentInfoReply | null",
        true,
        "Returns the document in the focused pane, or null when none is open.",
    ),
    spec(
        "editor.documents",
        EMPTY_PARAMS,
        "DocumentInfoReply[]",
        true,
        "Returns every open document.",
    ),
    spec(
        "editor.mode",
        EMPTY_PARAMS,
        "Mode",
        false,
        "Returns the editor mode of the focused pane.",
    ),
    spec(
        "editor.setMode",
        "EditorSetModeParams",
        "null",
        false,
        "Switches the focused pane to a mode.",
    ),
    spec(
        "editor.selections",
        EMPTY_PARAMS,
        "Selection[]",
        false,
        "Returns the selections of the focused pane.",
    ),
    spec(
        "editor.setSelections",
        "EditorSetSelectionsParams",
        "null",
        false,
        "Replaces the selections of the focused pane.",
    ),
    spec(
        "editor.edit",
        "EditorEditParams",
        "EditorEditReply",
        true,
        "Applies edits to the active document in one transaction.",
    ),
    spec(
        "editor.undo",
        EMPTY_PARAMS,
        "null",
        false,
        "Undoes the last transaction.",
    ),
    spec(
        "editor.redo",
        EMPTY_PARAMS,
        "null",
        false,
        "Redoes the last undone transaction.",
    ),
    spec(
        "workspace.textDocuments",
        EMPTY_PARAMS,
        "DocumentInfoReply[]",
        true,
        "Returns the text documents open in the workspace.",
    ),
    spec(
        "document.getText",
        "DocumentGetTextParams",
        "string",
        true,
        "Returns the document text, optionally limited to a range.",
    ),
    spec(
        "document.lineAt",
        "DocumentLineAtParams",
        "string | null",
        true,
        "Returns the text of a line, or null when the line is out of range.",
    ),
    spec(
        "document.positionAt",
        "DocumentPositionAtParams",
        "Position",
        true,
        "Converts a UTF-16 offset to a position.",
    ),
    spec(
        "document.offsetAt",
        "DocumentOffsetAtParams",
        "number",
        true,
        "Converts a position to a UTF-16 offset.",
    ),
    spec(
        "notifications.show",
        "NotificationsShowParams",
        "null",
        true,
        "Shows a notification to the user.",
    ),
    spec(
        "log.write",
        "LogWriteParams",
        "null",
        false,
        "Writes one entry to a log channel.",
    ),
    spec(
        "ui.statusline.createItem",
        "StatuslineCreateParams",
        "null",
        true,
        "Creates a status line item and returns its id.",
    ),
    spec(
        "ui.statusline.updateItem",
        "StatuslineUpdateParams",
        "null",
        true,
        "Updates the fields of a status line item.",
    ),
    spec(
        "ui.statusline.disposeItem",
        "StatuslineDisposeParams",
        "null",
        true,
        "Removes a status line item.",
    ),
    spec(
        "views.render",
        "ViewsRenderParams",
        "null",
        true,
        "Renders a registered view into a widget tree.",
    ),
    spec(
        "ui.picker.show",
        "PickerShowParams",
        "null",
        true,
        "Shows a picker and resolves with the chosen item.",
    ),
    spec(
        "ui.gutter.createItem",
        "GutterCreateParams",
        "null",
        true,
        "Creates a gutter item and returns its id.",
    ),
    spec(
        "ui.gutter.disposeItem",
        "GutterDisposeParams",
        "null",
        false,
        "Removes one gutter item.",
    ),
    spec(
        "ui.gutter.clear",
        EMPTY_PARAMS,
        "null",
        false,
        "Removes every gutter item.",
    ),
    spec(
        "ui.hover.show",
        "HoverShowParams",
        "null",
        false,
        "Shows a hover popup at a position.",
    ),
    spec(
        "ui.hover.hide",
        EMPTY_PARAMS,
        "null",
        false,
        "Hides the hover popup.",
    ),
];

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
    pub id: String,
    #[serde(default)]
    #[ts(optional)]
    pub items: Option<Vec<PickerItem>>,
    #[ts(optional = nullable)]
    pub placeholder: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_methods_cover_every_call() {
        let names: Vec<&str> = HostCall::VARIANTS.iter().map(|call| call.name()).collect();
        assert_eq!(names.len(), HOST_METHODS.len());
        for spec in HOST_METHODS {
            assert!(
                HostCall::parse(spec.method).is_some(),
                "{} is missing from HostCall",
                spec.method
            );
        }
        for name in names {
            assert!(
                HOST_METHODS.iter().any(|spec| spec.method == name),
                "{name} is missing from HOST_METHODS"
            );
        }
    }

    #[test]
    fn host_methods_have_no_duplicates() {
        let mut methods: Vec<&str> = HOST_METHODS.iter().map(|spec| spec.method).collect();
        methods.sort_unstable();
        methods.dedup();
        assert_eq!(methods.len(), HOST_METHODS.len());
    }

    #[test]
    fn every_host_method_has_a_description() {
        for spec in HOST_METHODS {
            assert!(
                !spec.doc.trim().is_empty(),
                "{} has no description",
                spec.method
            );
        }
    }

    /// Writes `HostMethods.ts` and `packages/vy-api/index.d.ts`.
    ///
    /// The filter `export_bindings` runs this together with the ts-rs tests.
    #[test]
    fn export_bindings_plugin_api() {
        crate::codegen::export();
    }

    #[test]
    fn empty_accepts_null_and_an_empty_object() {
        let parse = |value: Value| serde_json::from_value::<Empty>(value).is_ok();
        assert!(parse(Value::Null));
        assert!(parse(serde_json::json!({})));
    }
}
