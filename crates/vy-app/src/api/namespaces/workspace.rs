//! `workspace` namespace.

/// Describes the workspace and its open documents.
#[vy_api_schema::vy_api_namespace("workspace")]
mod api {
    use vy_api_schema::{DocumentInfoReply, WorkspaceFoldersChange, WorkspaceInfoReply};
    use vy_core::Uri;

    use crate::api::{Call, CallError, Reply, document_info};

    /// Returns the workspace root, name, folders, and trust state.
    #[vy_api_schema::vy_api_method]
    fn info(call: &mut Call<'_, '_>) -> Result<Reply<WorkspaceInfoReply>, CallError> {
        let workspace = call.context.workspace;
        Ok(Reply::Value(WorkspaceInfoReply {
            root_uri: workspace
                .root_uri
                .as_deref()
                .and_then(|uri| Uri::parse(uri).ok()),
            name: workspace.name.clone(),
            folders: workspace
                .folders
                .iter()
                .filter_map(|uri| Uri::parse(uri.clone()).ok())
                .collect(),
            trusted: true,
        }))
    }

    /// Returns the text documents open in the workspace.
    #[vy_api_schema::vy_api_method]
    fn text_documents(call: &mut Call<'_, '_>) -> Result<Reply<Vec<DocumentInfoReply>>, CallError> {
        let documents = call
            .context
            .document
            .iter()
            .map(|document| document_info(call.context.editor, document))
            .collect();
        Ok(Reply::Value(documents))
    }

    /// Fires when the workspace folders change.
    #[vy_api_schema::vy_api_event(
        "workspace.onDidChangeWorkspaceFolders",
        payload = WorkspaceFoldersChange
    )]
    const ON_DID_CHANGE_WORKSPACE_FOLDERS: () = ();
}

pub use self::api::{EVENTS, METHODS, NAMESPACE, dispatch};
