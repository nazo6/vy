//! `ui.statusline` namespace.

/// Creates and updates status line items.
#[vy_api_schema::vy_api_namespace("ui.statusline", sugar = UiStatuslineSugar)]
mod api {
    use serde_json::Value;
    use vy_api_schema::{
        StatuslineCreateParams, StatuslineDisposeParams, StatuslineUpdateParams, ThemeColor,
    };

    use crate::api::{Call, CallError, Reply};
    use crate::statusline::StatuslineItem;

    /// Creates a status line item and returns its id.
    #[vy_api_schema::vy_api_method(manual)]
    fn create_item(
        call: &mut Call<'_, '_>,
        params: StatuslineCreateParams,
    ) -> Result<Reply<()>, CallError> {
        if let Some(statusline) = call.context.statusline.as_deref_mut() {
            let mut item = StatuslineItem::new(
                params.id,
                params.alignment,
                params.priority.unwrap_or(0),
                params.text,
            );
            item.color = params.color;
            item.command = params.command;
            statusline.set_item(item);
        }
        Ok(Reply::Value(()))
    }

    /// Updates the fields of a status line item.
    #[vy_api_schema::vy_api_method(manual)]
    fn update_item(
        call: &mut Call<'_, '_>,
        params: StatuslineUpdateParams,
    ) -> Result<Reply<()>, CallError> {
        let color = params.color.map(optional_theme_color).transpose()?;
        let command = params.command.map(optional_text).transpose()?;
        if let Some(statusline) = call.context.statusline.as_deref_mut() {
            statusline.update_item(&params.id, params.text, color, command, params.visible);
        }
        Ok(Reply::Value(()))
    }

    /// Removes a status line item.
    #[vy_api_schema::vy_api_method(manual)]
    fn dispose_item(
        call: &mut Call<'_, '_>,
        params: StatuslineDisposeParams,
    ) -> Result<Reply<()>, CallError> {
        if let Some(statusline) = call.context.statusline.as_deref_mut() {
            statusline.dispose_item(&params.id);
        }
        Ok(Reply::Value(()))
    }

    /// Reads a field that distinguishes an absent value from `null`.
    fn optional_text(value: Value) -> Result<Option<String>, CallError> {
        match value {
            Value::Null => Ok(None),
            Value::String(text) => Ok(Some(text)),
            other => Err(CallError::invalid_params(format!(
                "expected a string or null, found {other}"
            ))),
        }
    }

    /// Reads a color field that distinguishes an absent value from `null`.
    fn optional_theme_color(value: Value) -> Result<Option<ThemeColor>, CallError> {
        match value {
            Value::Null => Ok(None),
            Value::String(name) => ThemeColor::parse(&name)
                .map(Some)
                .ok_or_else(|| CallError::invalid_params(format!("unknown color token: {name}"))),
            other => Err(CallError::invalid_params(format!(
                "expected a color token or null, found {other}"
            ))),
        }
    }
}

pub use self::api::{METHODS, NAMESPACE, dispatch};
