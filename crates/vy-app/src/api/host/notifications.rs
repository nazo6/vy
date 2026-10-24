//! Client implementation of the `notifications` host trait.

use vy_client_api::namespaces::notifications::{NotificationsApi, NotificationsShowParams};

use super::ClientHost;
use crate::api::{CallError, Reply};

impl NotificationsApi for ClientHost<'_, '_, '_> {
    fn show(&mut self, params: NotificationsShowParams) -> Result<Reply<()>, CallError> {
        self.call.outcome.status = Some(params.message);
        Ok(Reply::Value(()))
    }
}
