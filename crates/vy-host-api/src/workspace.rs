//! Shared `workspace` method and event declarations.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use vy_api_schema::{EventDecl, TypeRef};
use vy_core::Uri;

use crate::{CallError, DocumentInfoReply, Reply};

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

/// Describes the workspace and its open documents.
#[vy_api_schema::vy_api_trait("workspace")]
pub trait WorkspaceApi {
    /// Returns the workspace root, name, folders, and trust state.
    #[vy_api_schema::vy_api_method]
    fn info(&mut self) -> Result<Reply<WorkspaceInfoReply>, CallError>;

    /// Returns the text documents open in the workspace.
    #[vy_api_schema::vy_api_method]
    fn text_documents(&mut self) -> Result<Reply<Vec<DocumentInfoReply>>, CallError>;
}

/// Fires when the workspace folders change.
pub static ON_DID_CHANGE_WORKSPACE_FOLDERS_TOPIC: &str = "workspace.onDidChangeWorkspaceFolders";

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
