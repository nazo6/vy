//! Client implementation of the `ui.statusline` host trait.

use serde_json::Value;
use vy_client_api::ThemeColor;
use vy_client_api::namespaces::ui_statusline::{
    StatuslineCreateParams, StatuslineDisposeParams, StatuslineUpdateParams, UiStatuslineApi,
};
use vy_host_api::{CallError, Reply};

use super::ClientHost;
use crate::statusline::StatuslineItem;

impl UiStatuslineApi for ClientHost<'_, '_, '_> {
    fn create_item(&mut self, params: StatuslineCreateParams) -> Result<Reply<()>, CallError> {
        if let Some(statusline) = self.call.context.statusline.as_deref_mut() {
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

    fn update_item(&mut self, params: StatuslineUpdateParams) -> Result<Reply<()>, CallError> {
        let color = params.color.map(optional_theme_color).transpose()?;
        let command = params.command.map(optional_text).transpose()?;
        if let Some(statusline) = self.call.context.statusline.as_deref_mut() {
            statusline.update_item(&params.id, params.text, color, command, params.visible);
        }
        Ok(Reply::Value(()))
    }

    fn dispose_item(&mut self, params: StatuslineDisposeParams) -> Result<Reply<()>, CallError> {
        if let Some(statusline) = self.call.context.statusline.as_deref_mut() {
            statusline.dispose_item(&params.id);
        }
        Ok(Reply::Value(()))
    }
}

fn optional_text(value: Value) -> Result<Option<String>, CallError> {
    match value {
        Value::Null => Ok(None),
        Value::String(text) => Ok(Some(text)),
        other => Err(CallError::invalid_params(format!(
            "expected a string or null, found {other}"
        ))),
    }
}

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
