//! `notifications` declarations.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use vy_host_api::{CallError, Reply};

/// `notifications.show` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct NotificationsShowParams {
    pub message: String,
}

/// Shows notifications to the user.
#[vy_api_schema::vy_api_trait("notifications")]
pub trait NotificationsApi {
    /// Shows a notification to the user.
    #[vy_api_schema::vy_api_method]
    fn show(&mut self, params: NotificationsShowParams) -> Result<Reply<()>, CallError>;
}
