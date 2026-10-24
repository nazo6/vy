//! Client implementation of the `workspace` host trait.

use vy_core::Uri;
use vy_host_api::DocumentInfoReply;
use vy_host_api::workspace::{WorkspaceApi, WorkspaceInfoReply};

use super::ClientHost;
use crate::api::{CallError, Reply, document_info};

impl WorkspaceApi for ClientHost<'_, '_, '_> {
    fn info(&mut self) -> Result<Reply<WorkspaceInfoReply>, CallError> {
        let workspace = self.call.context.workspace;
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

    fn text_documents(&mut self) -> Result<Reply<Vec<DocumentInfoReply>>, CallError> {
        let documents = self
            .call
            .context
            .document
            .iter()
            .map(|document| document_info(self.call.context.editor, document))
            .collect();
        Ok(Reply::Value(documents))
    }
}
