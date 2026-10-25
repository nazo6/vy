//! `ui.picker` declarations.

use serde::{Deserialize, Serialize};
use vy_host_api::{CallError, Reply};

use crate::PickerItem;

/// `ui.picker.show` parameters.
///
/// When `items` is omitted the picker lists commands. `placeholder` is shown
/// in the filter input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct PickerShowParams {
    #[serde(default)]
    pub items: Option<Vec<PickerItem>>,
    #[serde(default)]
    pub placeholder: Option<String>,
}

/// `ui.picker.show` reply.
///
/// The reply is `null` when the picker is dismissed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PickerShowReply {
    pub selected_id: String,
}

/// Shows a picker and returns the chosen item.
#[vy_api_schema::vy_api_trait("ui.picker")]
pub trait UiPickerApi {
    /// Shows a picker and resolves with the chosen item.
    #[vy_api_schema::vy_api_method]
    fn show(
        &mut self,
        params: PickerShowParams,
    ) -> Result<Reply<Option<PickerShowReply>>, CallError>;
}
