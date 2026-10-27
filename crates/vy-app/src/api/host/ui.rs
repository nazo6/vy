//! Client implementation of the `ui` host trait.

use vy_client_api::namespaces::ui::{UiApi, UiFocused};
use vy_host_api::{CallError, Reply};

use super::ClientHost;

impl UiApi for ClientHost<'_, '_, '_> {
    fn focused(&mut self) -> Result<Reply<Option<UiFocused>>, CallError> {
        Ok(Reply::Value(self.call.context.ui_focus.clone()))
    }
}
