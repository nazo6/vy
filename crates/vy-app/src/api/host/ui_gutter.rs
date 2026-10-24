//! Client implementation of the `ui.gutter` host trait.

use vy_client_api::namespaces::ui_gutter::{GutterCreateParams, GutterDisposeParams, UiGutterApi};
use vy_host_api::{CallError, Reply};

use super::ClientHost;
use crate::slots::GutterItem;

impl UiGutterApi for ClientHost<'_, '_, '_> {
    fn create_item(&mut self, params: GutterCreateParams) -> Result<Reply<()>, CallError> {
        if let Some(gutter) = self.call.context.gutter.as_deref_mut() {
            let mut item = GutterItem::new(params.id, params.line, params.text);
            item.color = params.color;
            gutter.set_item(item);
        }
        Ok(Reply::Value(()))
    }

    fn dispose_item(&mut self, params: GutterDisposeParams) -> Result<Reply<()>, CallError> {
        if let Some(gutter) = self.call.context.gutter.as_deref_mut() {
            gutter.dispose_item(&params.id);
        }
        Ok(Reply::Value(()))
    }

    fn clear(&mut self) -> Result<Reply<()>, CallError> {
        if let Some(gutter) = self.call.context.gutter.as_deref_mut() {
            gutter.clear();
        }
        Ok(Reply::Value(()))
    }
}
