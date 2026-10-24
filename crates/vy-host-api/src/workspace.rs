//! Shared `workspace` method and event declarations.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use vy_api_schema::{EventDecl, MethodDecl, NamespaceDecl, TypeRef};
use vy_core::Uri;

use crate::{DocumentInfoReply, method_decl};

/// `workspace.info` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInfoReply {
    pub root_uri: Option<Uri>,
    pub name: Option<String>,
    pub folders: Vec<Uri>,
    pub trusted: bool,
}

/// Payload of `workspace.onDidChangeWorkspaceFolders`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WorkspaceFoldersChange {
    pub folders: Vec<String>,
}

method_decl!(
    INFO,
    "workspace.info",
    WorkspaceInfoReply,
    "Returns the workspace root, name, folders, and trust state."
);
method_decl!(
    TEXT_DOCUMENTS,
    "workspace.textDocuments",
    Vec<DocumentInfoReply>,
    "Returns the text documents open in the workspace."
);

/// Fires when the workspace folders change.
pub static ON_DID_CHANGE_WORKSPACE_FOLDERS_TOPIC: &str = "workspace.onDidChangeWorkspaceFolders";

/// Every method in this namespace.
pub static METHODS: &[MethodDecl] = &[INFO, TEXT_DOCUMENTS];

/// Every event in this namespace.
pub static EVENTS: &[EventDecl] = &[EventDecl {
    topic: ON_DID_CHANGE_WORKSPACE_FOLDERS_TOPIC,
    name: "onDidChangeWorkspaceFolders",
    namespace: "workspace",
    payload: TypeRef {
        name: ::vy_api_schema::ts_name::<WorkspaceFoldersChange> as fn() -> String,
        collect: ::vy_api_schema::collect::<WorkspaceFoldersChange>
            as fn(&mut ::vy_api_schema::TypeCollector),
    },
    doc: "Fires when the workspace folders change.",
}];

/// JSDoc and sugar metadata for the namespace.
pub static NAMESPACE: NamespaceDecl = NamespaceDecl {
    path: "workspace",
    doc: "Describes the workspace and its open documents.",
    sugar: None,
};
