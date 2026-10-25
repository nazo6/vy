//! Shared `workspace` method and event declarations.

use serde::{Deserialize, Serialize};
use vy_core::Uri;

use crate::{CallError, DocumentInfoReply, Reply};

/// `workspace.info` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInfoReply {
    pub root_uri: Option<Uri>,
    pub name: Option<String>,
    pub folders: Vec<Uri>,
    pub trusted: bool,
}

/// Payload of `workspace.onDidChangeWorkspaceFolders`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
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

    /// Fires when the workspace folders change.
    #[vy_api_schema::vy_api_event(
        "workspace.onDidChangeWorkspaceFolders",
        payload = WorkspaceFoldersChange
    )]
    const ON_DID_CHANGE_WORKSPACE_FOLDERS: ();
}
