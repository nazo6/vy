//! Client implementation of the `ui.picker` host trait.

use std::sync::atomic::{AtomicU64, Ordering};

use vy_client_api::namespaces::ui_picker::{PickerShowParams, PickerShowReply, UiPickerApi};
use vy_host_api::{CallError, Reply};

use super::ClientHost;
use crate::plugin_host::HostAction;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

impl UiPickerApi for ClientHost<'_, '_, '_> {
    fn show(
        &mut self,
        params: PickerShowParams,
    ) -> Result<Reply<Option<PickerShowReply>>, CallError> {
        let id = format!("picker_{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
        self.call.outcome.action = HostAction::ShowPicker {
            id,
            items: params.items.unwrap_or_default(),
            placeholder: params.placeholder,
            request: self.call.request.clone(),
        };
        Ok(Reply::Deferred)
    }
}
