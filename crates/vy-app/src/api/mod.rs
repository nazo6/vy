//! Declarations for the host methods the plugin API exposes.
//!
//! Each namespace lives in `namespaces/<name>.rs` inside an inline module
//! annotated with `#[vy_api_namespace]`. The registry macro joins the modules,
//! so adding a method needs no central table. See
//! `docs/adr/0008-declare-plugin-api-with-macros.md`.

use serde_json::Value;
use vy_api_schema::{EventDecl, MethodDecl, NamespaceDecl};
use vy_capabilities::TrustedStore;
use vy_core::Uri;
use vy_js::DocumentInfoReply;
use vy_js::HostRequest;

pub use vy_host_api::{CallError, Reply, erase_reply, parse_params};

use crate::editor::Editor;
use crate::plugin_host::{DocumentInfo, HostOutcome, RequestContext};

/// Mutable client state one host call may touch.
pub struct Call<'a, 'b> {
    pub context: &'a mut RequestContext<'b>,
    pub request: &'a HostRequest,
    pub outcome: &'a mut HostOutcome,
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

mod host;

use host::ClientHost;

#[vy_api_schema::vy_api_registry]
mod namespaces {
    mod commands;
    mod config;
    mod context;
    mod document;
    mod editor;
    mod fs;
    mod keymap;
    mod log;
    mod process;
    mod ui;
    mod ui_gutter;
    mod ui_hover;
    mod ui_picker;
    mod ui_statusline;
    mod views;
}

pub use namespaces::{events, methods, namespace_decls};

/// Answers a call from the shim.
pub fn dispatch(
    call: &mut Call<'_, '_>,
    method: &str,
    params: &Value,
) -> Option<Result<Reply<Value>, CallError>> {
    let trait_result = {
        let mut host = ClientHost { call };
        vy_client_api::ClientHostApi::dispatch(&mut host, method, params)
    };
    if let Some(result) = trait_result {
        return Some(result);
    }
    namespaces::dispatch(call, method, params)
}

/// Every declared method, including the trait-based namespaces.
pub fn declarations() -> Vec<&'static MethodDecl> {
    let mut declarations = Vec::new();
    declarations.extend(vy_host_api::workspace::METHODS.iter());
    declarations.extend(vy_client_api::namespaces::notifications::METHODS.iter());
    declarations.extend(methods().iter().map(|entry| entry.decl));
    declarations
}

/// Every declared namespace, including the trait-based namespaces.
pub fn all_namespace_decls() -> Vec<&'static NamespaceDecl> {
    let mut decls = vec![
        &vy_host_api::workspace::NAMESPACE,
        &vy_client_api::namespaces::notifications::NAMESPACE,
    ];
    decls.extend(namespace_decls());
    decls
}

/// Every declared event, including the trait-based namespaces.
pub fn all_events() -> Vec<&'static EventDecl> {
    let mut events: Vec<&'static EventDecl> = Vec::new();
    events.extend(vy_host_api::workspace::EVENTS.iter());
    events.extend(namespaces::events());
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declares_every_method_and_namespace() {
        let methods = declarations();
        let names: Vec<&str> = methods.iter().map(|entry| entry.method).collect();
        assert_eq!(names.len(), 55);

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
            "context.info",
            "document.getText",
            "document.lineAt",
            "document.offsetAt",
            "document.positionAt",
            "editor.activeDocument",
            "editor.documents",
            "editor.edit",
            "editor.mode",
            "editor.open",
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
            "views.close",
            "views.open",
            "views.render",
            "views.toggle",
            "workspace.info",
            "workspace.textDocuments",
        ] {
            assert!(names.contains(&expected), "{expected} is missing");
        }

        for entry in &methods {
            assert!(
                !entry.doc.trim().is_empty(),
                "{} has no description",
                entry.method
            );
            let name = entry.method.rsplit('.').next().unwrap_or(entry.method);
            assert!(
                !name.starts_with("on"),
                "{} would be treated as an event",
                entry.method
            );
            assert_eq!(
                entry.manual,
                entry.method == "commands.registerCommand",
                "{} has the wrong manual flag",
                entry.method
            );
        }

        let find = |name: &str| -> &MethodDecl {
            methods
                .iter()
                .copied()
                .find(|entry| entry.method == name)
                .unwrap_or_else(|| panic!("{name} is missing"))
        };

        assert!(find("editor.mode").params.is_none());
        assert_eq!((find("editor.mode").result.name)(), "Mode");
        assert_eq!(
            (find("editor.activeDocument").result.name)(),
            "DocumentInfoReply | null"
        );
        assert_eq!(
            (find("editor.documents").result.name)(),
            "Array<DocumentInfoReply>"
        );
        assert_eq!(
            find("editor.setMode")
                .params
                .map(|params| (params.name)())
                .as_deref(),
            Some("EditorSetModeParams")
        );
        assert_eq!((find("editor.setMode").result.name)(), "null");
        assert_eq!((find("config.get").result.name)(), "unknown");
        assert_eq!(find("config.get").typed_key, Some("SettingsMap"));
        assert_eq!(find("commands.execute").typed_args, Some("CommandMap"));
        assert_eq!(
            find("fs.readFile")
                .params
                .map(|params| (params.name)())
                .as_deref(),
            Some("FsReadFileParams")
        );
        assert_eq!((find("fs.readFile").result.name)(), "FsReadFileReply");

        let namespaces = all_namespace_decls();
        assert_eq!(namespaces.len(), 17);

        let paths: Vec<&str> = namespaces.iter().map(|entry| entry.path).collect();
        for expected in [
            "commands",
            "config",
            "context",
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
            let expected = (namespace.path == "commands").then_some("CommandsSugar");
            assert_eq!(namespace.sugar, expected, "{}", namespace.path);
        }
    }

    #[test]
    fn declares_every_event() {
        let events = all_events();
        assert_eq!(events.len(), 7);

        let topics: Vec<&str> = events.iter().map(|event| event.topic).collect();
        for expected in [
            "config.onDidChange",
            "editor.onDidChangeActiveDocument",
            "editor.onDidChangeSelection",
            "process.onExit",
            "process.onStderr",
            "process.onStdout",
            "workspace.onDidChangeWorkspaceFolders",
        ] {
            assert!(topics.contains(&expected), "{expected} is missing");
        }

        for event in &events {
            assert!(!event.doc.trim().is_empty(), "{} has no doc", event.topic);
            assert!(event.name.starts_with("on"), "{}", event.topic);
            assert!(!event.namespace.is_empty(), "{}", event.topic);
            assert_eq!(
                event.topic,
                format!("{}.{}", event.namespace, event.name),
                "the shim derives the topic from namespace and name"
            );
        }

        let find = |topic: &str| {
            events
                .iter()
                .copied()
                .find(|event| event.topic == topic)
                .unwrap_or_else(|| panic!("{topic} is missing"))
        };
        assert_eq!((find("config.onDidChange").payload.name)(), "ConfigChange");
        assert_eq!(
            (find("editor.onDidChangeActiveDocument").payload.name)(),
            "ActiveDocumentChange"
        );
        assert_eq!(
            (find("editor.onDidChangeSelection").payload.name)(),
            "SelectionChange"
        );
        assert_eq!(
            (find("workspace.onDidChangeWorkspaceFolders").payload.name)(),
            "WorkspaceFoldersChange"
        );
        assert_eq!((find("process.onStdout").payload.name)(), "ProcessData");
        assert_eq!((find("process.onStderr").payload.name)(), "ProcessData");
        assert_eq!((find("process.onExit").payload.name)(), "ProcessExit");
    }

    /// Regenerates the TypeScript declarations. The filter `export_bindings`
    /// runs this together with the ts-rs tests.
    #[test]
    fn export_bindings_plugin_api() {
        let methods = declarations();
        let namespaces = all_namespace_decls();
        let events = all_events();
        vy_api_schema::codegen::export(&methods, &namespaces, &events);
    }
}
