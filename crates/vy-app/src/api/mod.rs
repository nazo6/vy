//! Declarations for the host methods the plugin API exposes.
//!
//! Each namespace lives in `namespaces/<name>.rs` inside an inline module
//! annotated with `#[vy_api_namespace]`. The registry macro joins the modules,
//! so adding a method needs no central table. See
//! `docs/adr/0008-declare-plugin-api-with-macros.md`.

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use vy_api_schema::{DocumentInfoReply, MethodDecl};
use vy_capabilities::TrustedStore;
use vy_core::Uri;
use vy_js::HostRequest;
use vy_proto::ErrorCode;

use crate::editor::Editor;
use crate::plugin_host::{DocumentInfo, HostOutcome, RequestContext};

/// Mutable client state one host call may touch.
pub struct Call<'a, 'b> {
    pub context: &'a mut RequestContext<'b>,
    pub request: &'a HostRequest,
    pub outcome: &'a mut HostOutcome,
}

/// The value a handler returns now, or a reply the view sends later.
pub enum Reply<T> {
    Value(T),
    Deferred,
}

impl<T> Reply<T> {
    pub fn value(value: T) -> Self {
        Self::Value(value)
    }
}

/// A failure the shim receives as an exception with a stable code.
pub struct CallError {
    pub(crate) code: ErrorCode,
    pub(crate) message: String,
}

impl CallError {
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::InvalidParams,
            message: message.into(),
        }
    }

    pub fn invalid_state(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::InvalidState,
            message: message.into(),
        }
    }
}

impl From<serde_json::Error> for CallError {
    fn from(error: serde_json::Error) -> Self {
        Self::invalid_params(error.to_string())
    }
}

/// Deserializes the parameters of one method.
///
/// Missing parameters arrive as `null`; treating that as an empty object keeps
/// methods without parameters from needing a special case.
pub fn parse_params<T: DeserializeOwned>(params: &Value) -> Result<T, CallError> {
    let empty = Value::Object(serde_json::Map::new());
    let value = if params.is_null() { &empty } else { params };
    Ok(serde_json::from_value(value.clone())?)
}

/// Serializes a handler's reply for the script boundary.
pub fn erase_reply<T: Serialize>(reply: Reply<T>) -> Result<Reply<Value>, CallError> {
    match reply {
        Reply::Value(value) => Ok(Reply::Value(serde_json::to_value(value)?)),
        Reply::Deferred => Ok(Reply::Deferred),
    }
}

/// The erased signature of a handler stored in a namespace table.
pub type HandlerFn =
    for<'x, 'y, 'a, 'b> fn(&'x mut Call<'a, 'b>, &'y Value) -> Result<Reply<Value>, CallError>;

/// One method and the function that answers it.
pub struct HandlerDecl {
    pub decl: &'static MethodDecl,
    pub handler: HandlerFn,
}

/// Builds the document snapshot the script sees.
pub(crate) fn document_info(editor: &Editor, document: &DocumentInfo) -> DocumentInfoReply {
    DocumentInfoReply {
        id: document.id,
        uri: document.uri.clone(),
        language_id: document.language_id.clone(),
        version: editor.version(),
        line_count: editor.line_count(),
        end: editor.position_at_utf16(editor.len_utf16()),
    }
}

/// Validates the capability a file or process operation asks for.
///
/// Phase 2 has no manifests, so this only rejects malformed capability names.
pub(crate) fn check_capability(
    context: &RequestContext<'_>,
    kind: &str,
    target: Option<&str>,
) -> Result<(), CallError> {
    let request = match target {
        Some(target) => format!("{kind}:{target}"),
        None => kind.to_string(),
    };
    let Ok(requested) = vy_capabilities::Capability::parse(&request) else {
        return Err(CallError::invalid_params(format!(
            "invalid capability: {request}"
        )));
    };

    let roots = vy_capabilities::Roots::new(
        context
            .workspace
            .root_uri
            .as_deref()
            .and_then(|uri| Uri::parse(uri).ok().and_then(|uri| uri.file_path())),
        dirs::home_dir(),
    );

    let (store, _) = TrustedStore::load(&context.paths.trusted_json());
    let _ = (store, roots, requested);
    Ok(())
}

#[vy_api_schema::vy_api_registry]
mod namespaces {
    mod commands;
    mod config;
    mod document;
    mod editor;
    mod fs;
    mod keymap;
    mod log;
    mod notifications;
    mod process;
    mod ui;
    mod ui_gutter;
    mod ui_hover;
    mod ui_picker;
    mod ui_statusline;
    mod views;
    mod workspace;
}

pub use namespaces::{dispatch, methods, namespace_decls};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declares_every_method_and_namespace() {
        let methods = methods();
        let names: Vec<&str> = methods.iter().map(|entry| entry.decl.method).collect();
        assert_eq!(names.len(), 49);

        let mut unique = names.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), names.len(), "a method is declared twice");

        for expected in [
            "commands.execute",
            "commands.getCommands",
            "commands.registerCommand",
            "config.get",
            "config.inspect",
            "config.set",
            "document.getText",
            "document.lineAt",
            "document.offsetAt",
            "document.positionAt",
            "editor.activeDocument",
            "editor.documents",
            "editor.edit",
            "editor.mode",
            "editor.redo",
            "editor.selections",
            "editor.setMode",
            "editor.setSelections",
            "editor.undo",
            "fs.copy",
            "fs.createDir",
            "fs.exists",
            "fs.readDir",
            "fs.readFile",
            "fs.readTextFile",
            "fs.remove",
            "fs.rename",
            "fs.stat",
            "fs.writeFile",
            "keymap.bind",
            "keymap.setContext",
            "keymap.unbind",
            "log.write",
            "notifications.show",
            "process.exec",
            "process.kill",
            "process.spawn",
            "process.writeStdin",
            "ui.gutter.clear",
            "ui.gutter.createItem",
            "ui.gutter.disposeItem",
            "ui.hover.hide",
            "ui.hover.show",
            "ui.picker.show",
            "ui.statusline.createItem",
            "ui.statusline.disposeItem",
            "ui.statusline.updateItem",
            "views.render",
            "workspace.textDocuments",
        ] {
            assert!(names.contains(&expected), "{expected} is missing");
        }

        for entry in &methods {
            assert!(
                !entry.decl.doc.trim().is_empty(),
                "{} has no description",
                entry.decl.method
            );
        }

        let find = |name: &str| -> &HandlerDecl {
            methods
                .iter()
                .copied()
                .find(|entry| entry.decl.method == name)
                .unwrap_or_else(|| panic!("{name} is missing"))
        };

        assert!(find("editor.mode").decl.params.is_none());
        assert_eq!((find("editor.mode").decl.result)(), "Mode");
        assert_eq!(
            (find("editor.activeDocument").decl.result)(),
            "DocumentInfoReply | null"
        );
        assert_eq!(
            (find("editor.documents").decl.result)(),
            "Array<DocumentInfoReply>"
        );
        assert_eq!(
            find("editor.setMode")
                .decl
                .params
                .map(|params| params())
                .as_deref(),
            Some("EditorSetModeParams")
        );
        assert_eq!((find("editor.setMode").decl.result)(), "null");
        assert_eq!((find("config.get").decl.result)(), "unknown");
        assert_eq!(
            find("fs.readFile")
                .decl
                .params
                .map(|params| params())
                .as_deref(),
            Some("FsReadFileParams")
        );
        assert_eq!((find("fs.readFile").decl.result)(), "FsReadFileReply");

        assert!(find("commands.registerCommand").decl.manual);
        assert!(!find("commands.getCommands").decl.manual);
        assert!(find("config.get").decl.manual);
        assert!(!find("config.inspect").decl.manual);
        assert!(!find("editor.mode").decl.manual);
        assert!(find("editor.activeDocument").decl.manual);
        assert!(find("document.getText").decl.manual);
        assert!(find("fs.readFile").decl.manual);
        assert!(!find("log.write").decl.manual);
        assert!(!find("ui.hover.show").decl.manual);

        let namespaces = namespace_decls();
        assert_eq!(namespaces.len(), 16);

        let paths: Vec<&str> = namespaces.iter().map(|entry| entry.path).collect();
        for expected in [
            "commands",
            "config",
            "document",
            "editor",
            "fs",
            "keymap",
            "log",
            "notifications",
            "process",
            "ui",
            "ui.gutter",
            "ui.hover",
            "ui.picker",
            "ui.statusline",
            "views",
            "workspace",
        ] {
            assert!(paths.contains(&expected), "{expected} is missing");
        }

        for namespace in &namespaces {
            assert!(
                !namespace.doc.trim().is_empty(),
                "{} has no description",
                namespace.path
            );
        }

        let sugar = |path: &str| {
            namespaces
                .iter()
                .find(|entry| entry.path == path)
                .unwrap_or_else(|| panic!("{path} is missing"))
                .sugar
        };
        assert_eq!(sugar("commands"), Some("CommandsSugar"));
        assert_eq!(sugar("config"), Some("ConfigSugar"));
        assert_eq!(sugar("document"), None);
        assert_eq!(sugar("editor"), Some("EditorSugar"));
        assert_eq!(sugar("fs"), Some("FsSugar"));
        assert_eq!(sugar("ui"), Some("UiSugar"));
        assert_eq!(sugar("ui.gutter"), Some("UiGutterSugar"));
        assert_eq!(sugar("ui.hover"), None);
        assert_eq!(sugar("ui.picker"), Some("UiPickerSugar"));
        assert_eq!(sugar("ui.statusline"), Some("UiStatuslineSugar"));
        assert_eq!(sugar("views"), Some("ViewsSugar"));
        assert_eq!(sugar("workspace"), Some("WorkspaceSugar"));
    }

    /// Regenerates the TypeScript declarations. The filter `export_bindings`
    /// runs this together with the ts-rs tests.
    #[test]
    fn export_bindings_plugin_api() {
        let entries = methods();
        let methods: Vec<&MethodDecl> = entries.iter().map(|entry| entry.decl).collect();
        let namespaces = namespace_decls();
        vy_api_schema::codegen::export(&methods, &namespaces);
    }
}
