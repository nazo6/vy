//! `workspace` namespace.

/// Describes the workspace and its open documents.
#[vy_api_schema::vy_api_namespace("workspace", sugar = WorkspaceSugar)]
mod api {
    use vy_api_schema::DocumentInfoReply;

    use crate::api::{Call, CallError, Reply, document_info};

    /// Returns the text documents open in the workspace.
    #[vy_api_schema::vy_api_method(manual)]
    fn text_documents(call: &mut Call<'_, '_>) -> Result<Reply<Vec<DocumentInfoReply>>, CallError> {
        let documents = call
            .context
            .document
            .iter()
            .map(|document| document_info(call.context.editor, document))
            .collect();
        Ok(Reply::Value(documents))
    }
}

pub use self::api::{METHODS, NAMESPACE, dispatch};
