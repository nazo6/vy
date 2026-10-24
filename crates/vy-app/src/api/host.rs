//! The client implementation of the shared and client-only host traits.

use vy_client_api::ClientHostApi;
use vy_client_api::namespaces::notifications::{NotificationsApi, NotificationsShowParams};
use vy_core::Uri;
use vy_host_api::DocumentInfoReply;
use vy_host_api::workspace::{WorkspaceApi, WorkspaceInfoReply};

use crate::api::{Call, CallError, Reply, document_info};

/// Mutable client state exposed to one host call.
pub struct ClientHost<'a, 'b, 'c> {
    pub call: &'a mut Call<'b, 'c>,
}

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

impl NotificationsApi for ClientHost<'_, '_, '_> {
    fn show(&mut self, params: NotificationsShowParams) -> Result<Reply<()>, CallError> {
        self.call.outcome.status = Some(params.message);
        Ok(Reply::Value(()))
    }
}

impl vy_host_api::HostApi for ClientHost<'_, '_, '_> {}

impl ClientHostApi for ClientHost<'_, '_, '_> {}
