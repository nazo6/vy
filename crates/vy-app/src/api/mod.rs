//! The client host for the plugin API.
//!
//! The API surface lives in `vy-host-api` and `vy-client-api` as namespace
//! traits. This module turns one `Call` into a `ClientHost` and dispatches
//! through the generated facade. See ADR 0013.

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
        Some(target) => {
            // Capability patterns use paths (`${workspace}/**`), while the API
            // passes file URIs. Normalize before matching.
            let target = if target.starts_with("file:") {
                Uri::parse(target)
                    .ok()
                    .and_then(|uri| uri.file_path())
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| target.to_string())
            } else {
                target.to_string()
            };
            format!("{kind}:{target}")
        }
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

/// Answers a call from the shim.
pub fn dispatch(
    call: &mut Call<'_, '_>,
    method: &str,
    params: &Value,
) -> Option<Result<Reply<Value>, CallError>> {
    let mut host = ClientHost { call };
    vy_client_api::ClientHostApi::dispatch(&mut host, method, params)
}

/// Every declared method.
pub fn declarations() -> Vec<&'static MethodDecl> {
    vy_client_api::client_host_api_methods()
}

/// Every declared namespace.
pub fn all_namespace_decls() -> Vec<&'static NamespaceDecl> {
    vy_client_api::client_host_api_namespaces()
}

/// Every declared event.
pub fn all_events() -> Vec<&'static EventDecl> {
    vy_client_api::client_host_api_events()
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
            let expected_manual = matches!(
                entry.method,
                "commands.registerCommand" | "commands.execute" | "keymap.bind"
            );
            assert_eq!(
                entry.manual, expected_manual,
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
}
