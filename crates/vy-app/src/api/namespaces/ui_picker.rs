//! `ui.picker` namespace.

/// Shows a picker and returns the chosen item.
#[vy_api_schema::vy_api_namespace("ui.picker")]
mod api {
    use std::sync::atomic::{AtomicU64, Ordering};

    use serde::{Deserialize, Serialize};
    use ts_rs::TS;
    use crate::palette::PickerItem;

    use crate::api::{Call, CallError, Reply};
    use crate::plugin_host::HostAction;

    static NEXT_ID: AtomicU64 = AtomicU64::new(1);

    /// `ui.picker.show` parameters.
    ///
    /// When `items` is omitted the picker lists commands. `placeholder` is shown
    /// in the filter input.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
    pub struct PickerShowParams {
        #[serde(default)]
        #[ts(optional)]
        pub items: Option<Vec<PickerItem>>,
        #[ts(optional = nullable)]
        pub placeholder: Option<String>,
    }

    /// `ui.picker.show` reply.
    ///
    /// The reply is `null` when the picker is dismissed.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
    #[serde(rename_all = "camelCase")]
    pub struct PickerShowReply {
        pub selected_id: String,
    }

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
