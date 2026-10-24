//! `notifications` namespace.

/// Shows notifications to the user.
#[vy_api_schema::vy_api_namespace("notifications")]
mod api {
    use serde::{Deserialize, Serialize};
    use ts_rs::TS;

    use crate::api::{Call, CallError, Reply};

    /// `notifications.show` parameters.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
    #[serde(deny_unknown_fields)]
    pub struct NotificationsShowParams {
        pub message: String,
    }

    /// Shows a notification to the user.
    #[vy_api_schema::vy_api_method]
    fn show(
        call: &mut Call<'_, '_>,
        params: NotificationsShowParams,
    ) -> Result<Reply<()>, CallError> {
        call.outcome.status = Some(params.message);
        Ok(Reply::Value(()))
    }
}

pub use self::api::{EVENTS, METHODS, NAMESPACE, dispatch};
