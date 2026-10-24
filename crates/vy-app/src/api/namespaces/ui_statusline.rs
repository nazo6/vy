//! `ui.statusline` namespace.

/// Creates and updates status line items.
#[vy_api_schema::vy_api_namespace("ui.statusline")]
mod api {
    use crate::color::ThemeColor;
    use crate::statusline::StatuslineAlignment;
    use serde::{Deserialize, Serialize};
    use serde_json::Value;
    use ts_rs::TS;

    use crate::api::{Call, CallError, Reply};
    use crate::statusline::StatuslineItem;

    /// `ui.statusline.createItem` parameters.
    ///
    /// `alignment` places the item on the left or right. `priority` orders items
    /// within the same side; higher values are shown first. `command` runs when the
    /// item is clicked.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
    pub struct StatuslineCreateParams {
        pub id: String,
        pub alignment: StatuslineAlignment,
        #[serde(default)]
        #[ts(optional)]
        pub priority: Option<i32>,
        pub text: String,
        #[ts(optional = nullable)]
        pub color: Option<ThemeColor>,
        #[ts(optional = nullable)]
        pub command: Option<String>,
    }

    /// `ui.statusline.updateItem` parameters.
    ///
    /// `color` and `command` distinguish an absent field (leave as is) from `null`
    /// (clear it). The host validates `color` against the theme tokens.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
    pub struct StatuslineUpdateParams {
        pub id: String,
        #[ts(optional = nullable)]
        pub text: Option<String>,
        #[ts(type = "string | null")]
        pub color: Option<Value>,
        #[ts(type = "string | null")]
        pub command: Option<Value>,
        #[ts(optional = nullable)]
        pub visible: Option<bool>,
    }

    /// `ui.statusline.disposeItem` parameters.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
    pub struct StatuslineDisposeParams {
        pub id: String,
    }

    /// Creates a status line item and returns its id.
    #[vy_api_schema::vy_api_method]
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
    #[vy_api_schema::vy_api_method]
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
    #[vy_api_schema::vy_api_method]
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

pub use self::api::{EVENTS, METHODS, NAMESPACE, dispatch};
