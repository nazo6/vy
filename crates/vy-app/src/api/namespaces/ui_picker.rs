//! `ui.picker` namespace.

/// Shows a picker and returns the chosen item.
#[vy_api_schema::vy_api_namespace("ui.picker", sugar = UiPickerSugar)]
mod api {
    use vy_api_schema::PickerShowParams;

    use crate::api::{Call, CallError, Reply};
    use crate::plugin_host::HostAction;

    /// Shows a picker and resolves with the chosen item.
    #[vy_api_schema::vy_api_method(manual)]
    fn show(call: &mut Call<'_, '_>, params: PickerShowParams) -> Result<Reply<()>, CallError> {
        call.outcome.action = HostAction::ShowPicker {
            id: params.id,
            items: params.items.unwrap_or_default(),
            placeholder: params.placeholder,
            request: call.request.clone(),
        };
        Ok(Reply::Deferred)
    }
}

pub use self::api::{METHODS, NAMESPACE, dispatch};
