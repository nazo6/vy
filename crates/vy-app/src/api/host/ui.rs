//! Client implementation of the empty `ui` host trait.

use vy_client_api::namespaces::ui::UiApi;

use super::ClientHost;

impl UiApi for ClientHost<'_, '_, '_> {}
