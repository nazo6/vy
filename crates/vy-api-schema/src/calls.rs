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

/// TypeScript type the table uses for methods without parameters.
pub const EMPTY_PARAMS: &str = "Record<string, never>";

/// Every host method. `HostCall` and this table must agree (see the tests).
pub const HOST_METHODS: &[MethodSpec] = &[
    MethodSpec {
        method: "config.get",
        params: "ConfigGetParams",
        result: "unknown",
        manual: true,
    },
    MethodSpec {
        method: "config.set",
        params: "ConfigSetParams",
        result: "ConfigSetReply",
        manual: true,
    },
    MethodSpec {
        method: "config.inspect",
        params: "ConfigInspectParams",
        result: "ConfigInspectReply",
        manual: false,
    },
    MethodSpec {
        method: "commands.registerCommand",
        params: "CommandsRegisterParams",
        result: "null",
        manual: true,
    },
    MethodSpec {
        method: "commands.execute",
        params: "CommandsExecuteParams",
        result: "null",
        manual: true,
    },
    MethodSpec {
        method: "commands.getCommands",
        params: "Record<string, never>",
        result: "string[]",
        manual: false,
    },
    MethodSpec {
        method: "keymap.bind",
        params: "KeymapBindParams",
        result: "null",
        manual: true,
    },
    MethodSpec {
        method: "keymap.unbind",
        params: "KeymapUnbindParams",
        result: "number",
        manual: false,
    },
    MethodSpec {
        method: "keymap.setContext",
        params: "KeymapSetContextParams",
        result: "null",
        manual: false,
    },
    MethodSpec {
        method: "editor.activeDocument",
        params: "Record<string, never>",
        result: "DocumentInfoReply | null",
        manual: true,
    },
    MethodSpec {
        method: "editor.documents",
        params: "Record<string, never>",
        result: "DocumentInfoReply[]",
        manual: true,
    },
    MethodSpec {
        method: "editor.mode",
        params: "Record<string, never>",
        result: "Mode",
        manual: false,
    },
    MethodSpec {
        method: "editor.setMode",
        params: "EditorSetModeParams",
        result: "null",
        manual: false,
    },
    MethodSpec {
        method: "editor.selections",
        params: "Record<string, never>",
        result: "Selection[]",
        manual: false,
    },
    MethodSpec {
        method: "editor.setSelections",
        params: "EditorSetSelectionsParams",
        result: "null",
        manual: false,
    },
    MethodSpec {
        method: "editor.edit",
        params: "EditorEditParams",
        result: "EditorEditReply",
        manual: true,
    },
    MethodSpec {
        method: "editor.undo",
        params: "Record<string, never>",
        result: "null",
        manual: false,
    },
    MethodSpec {
        method: "editor.redo",
        params: "Record<string, never>",
        result: "null",
        manual: false,
    },
    MethodSpec {
        method: "workspace.textDocuments",
        params: "Record<string, never>",
        result: "DocumentInfoReply[]",
        manual: true,
    },
    MethodSpec {
        method: "document.getText",
        params: "DocumentGetTextParams",
        result: "string",
        manual: true,
    },
    MethodSpec {
        method: "document.lineAt",
        params: "DocumentLineAtParams",
        result: "string | null",
        manual: true,
    },
    MethodSpec {
        method: "document.positionAt",
        params: "DocumentPositionAtParams",
        result: "Position",
        manual: true,
    },
    MethodSpec {
        method: "document.offsetAt",
        params: "DocumentOffsetAtParams",
        result: "number",
        manual: true,
    },
    MethodSpec {
        method: "notifications.show",
        params: "NotificationsShowParams",
        result: "null",
        manual: true,
    },
    MethodSpec {
        method: "log.write",
        params: "LogWriteParams",
        result: "null",
        manual: false,
    },
    MethodSpec {
        method: "ui.statusline.createItem",
        params: "StatuslineCreateParams",
        result: "null",
        manual: true,
    },
    MethodSpec {
        method: "ui.statusline.updateItem",
        params: "StatuslineUpdateParams",
        result: "null",
        manual: true,
    },
    MethodSpec {
        method: "ui.statusline.disposeItem",
        params: "StatuslineDisposeParams",
        result: "null",
        manual: true,
    },
    MethodSpec {
        method: "views.render",
        params: "ViewsRenderParams",
        result: "null",
        manual: true,
    },
    MethodSpec {
        method: "ui.picker.show",
        params: "PickerShowParams",
        result: "null",
        manual: true,
    },
    MethodSpec {
        method: "ui.gutter.createItem",
        params: "GutterCreateParams",
        result: "null",
        manual: true,
    },
    MethodSpec {
        method: "ui.gutter.disposeItem",
        params: "GutterDisposeParams",
        result: "null",
        manual: false,
    },
    MethodSpec {
        method: "ui.gutter.clear",
        params: "Record<string, never>",
        result: "null",
        manual: false,
    },
    MethodSpec {
        method: "ui.hover.show",
        params: "HoverShowParams",
        result: "null",
        manual: false,
    },
    MethodSpec {
        method: "ui.hover.hide",
        params: "Record<string, never>",
        result: "null",
        manual: false,
    },
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StatuslineCreateParams {
    pub id: String,
    pub alignment: StatuslineAlignment,
    #[serde(default)]
    pub priority: i32,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PickerShowParams {
    pub id: String,
    #[serde(default)]
    pub items: Vec<PickerItem>,
    #[ts(optional = nullable)]
    pub placeholder: Option<String>,
}

/// `views.render` parameters.
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
