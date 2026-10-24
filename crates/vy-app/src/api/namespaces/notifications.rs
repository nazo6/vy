//! `notifications` namespace.

/// Shows notifications to the user.
#[vy_api_schema::vy_api_namespace("notifications")]
mod api {
    use vy_api_schema::NotificationsShowParams;

    use crate::api::{Call, CallError, Reply};

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
