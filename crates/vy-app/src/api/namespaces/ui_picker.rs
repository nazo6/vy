//! `ui.picker` namespace.

/// Shows a picker and returns the chosen item.
#[vy_api_schema::vy_api_namespace("ui.picker")]
mod api {
    use std::sync::atomic::{AtomicU64, Ordering};

    use vy_api_schema::{PickerShowParams, PickerShowReply};

    use crate::api::{Call, CallError, Reply};
    use crate::plugin_host::HostAction;

    static NEXT_ID: AtomicU64 = AtomicU64::new(1);

    /// Shows a picker and resolves with the chosen item.
    #[vy_api_schema::vy_api_method]
    fn show(
        call: &mut Call<'_, '_>,
        params: PickerShowParams,
    ) -> Result<Reply<Option<PickerShowReply>>, CallError> {
        let id = format!("picker_{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
        call.outcome.action = HostAction::ShowPicker {
            id,
            items: params.items.unwrap_or_default(),
            placeholder: params.placeholder,
            request: call.request.clone(),
        };
        Ok(Reply::Deferred)
    }
}

pub use self::api::{EVENTS, METHODS, NAMESPACE, dispatch};
