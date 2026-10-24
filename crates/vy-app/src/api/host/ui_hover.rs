//! Client implementation of the `ui.hover` host trait.

use vy_client_api::namespaces::ui_hover::{HoverShowParams, UiHoverApi};
use vy_host_api::{CallError, Reply};

use super::ClientHost;
use crate::slots::HoverState;

impl UiHoverApi for ClientHost<'_, '_, '_> {
    fn show(&mut self, params: HoverShowParams) -> Result<Reply<()>, CallError> {
        if let Some(hover) = self.call.context.hover.as_deref_mut() {
            *hover = Some(HoverState::new(params.line, params.character, params.text));
        }
        Ok(Reply::Value(()))
    }

    fn hide(&mut self) -> Result<Reply<()>, CallError> {
        if let Some(hover) = self.call.context.hover.as_deref_mut() {
            *hover = None;
        }
        Ok(Reply::Value(()))
    }
}
