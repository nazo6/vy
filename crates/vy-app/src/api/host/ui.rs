//! Client implementation of the `ui` host trait.

use vy_client_api::namespaces::ui::{UiApi, UiFocus};
use vy_host_api::{CallError, Reply};

use super::ClientHost;

impl UiApi for ClientHost<'_, '_, '_> {
    fn focused(&mut self) -> Result<Reply<Option<UiFocus>>, CallError> {
        Ok(Reply::Value(self.call.context.ui_focus.as_ref().map(
            |focus| UiFocus {
                focus_id: focus.focus_id.clone(),
                role: focus.role,
                pane_id: focus.pane_id,
                view_id: focus.view_id.clone(),
            },
        )))
    }
}
