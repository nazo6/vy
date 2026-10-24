//! The plugin API surface that only the client implements.
//!
//! Shared wire types and declarations live in `vy-host-api`. This crate owns
//! the UI, view, keymap, command, and editor-client declarations, plus the
//! value types that cross those APIs. Handlers stay in `vy-app`.

pub use vy_host_api::{event_decl, method_decl};

use vy_host_api::{CallError, Reply};

/// The client host surface: shared APIs plus the client-only namespaces.
pub trait ClientHostApi:
    vy_host_api::HostApi + namespaces::notifications::NotificationsApi
{
    fn dispatch(
        &mut self,
        method: &str,
        params: &serde_json::Value,
    ) -> Option<Result<Reply<serde_json::Value>, CallError>> {
        if let Some(result) = <Self as vy_host_api::HostApi>::dispatch(self, method, params) {
            return Some(result);
        }
        if let Some(result) = namespaces::notifications::dispatch(self, method, params) {
            return Some(result);
        }
        None
    }
}

pub mod color;
pub mod keymap;
pub mod namespaces;
pub mod palette;
pub mod statusline;
pub mod widget;

pub use color::ThemeColor;
pub use palette::PickerItem;
pub use statusline::StatuslineAlignment;
pub use widget::WidgetNode;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_declares_client_state_methods_and_events() {
        assert_eq!(namespaces::editor::METHODS.len(), 10);
        assert_eq!(namespaces::editor::EVENTS.len(), 2);
        assert_eq!(namespaces::editor::NAMESPACE.path, "editor");
    }

    #[test]
    fn commands_and_config_split_from_the_core() {
        assert_eq!(namespaces::commands::METHODS.len(), 3);
        assert_eq!(namespaces::config::METHODS.len(), 1);
        assert_eq!(namespaces::config::METHODS[0].method, "config.set");
        assert_eq!(
            vy_host_api::config::GET.method,
            namespaces::config::GET.method
        );
        assert_eq!(
            namespaces::config::NAMESPACE.path,
            vy_host_api::config::NAMESPACE.path
        );
    }

    #[test]
    fn ui_namespaces_are_grouped() {
        assert!(namespaces::ui::METHODS.is_empty());
        assert_eq!(namespaces::views::METHODS.len(), 4);
        assert_eq!(namespaces::ui_statusline::METHODS.len(), 3);
        assert_eq!(namespaces::ui_picker::METHODS.len(), 1);
    }

    struct TestHost;

    impl vy_host_api::workspace::WorkspaceApi for TestHost {
        fn info(&mut self) -> Result<Reply<vy_host_api::workspace::WorkspaceInfoReply>, CallError> {
            Ok(Reply::Value(vy_host_api::workspace::WorkspaceInfoReply {
                root_uri: None,
                name: Some("test".into()),
                folders: Vec::new(),
                trusted: true,
            }))
        }

        fn text_documents(
            &mut self,
        ) -> Result<Reply<Vec<vy_host_api::DocumentInfoReply>>, CallError> {
            Ok(Reply::Value(Vec::new()))
        }
    }

    impl namespaces::notifications::NotificationsApi for TestHost {
        fn show(
            &mut self,
            _params: namespaces::notifications::NotificationsShowParams,
        ) -> Result<Reply<()>, CallError> {
            Ok(Reply::Value(()))
        }
    }

    impl vy_host_api::HostApi for TestHost {}
    impl ClientHostApi for TestHost {}

    #[test]
    fn dispatches_shared_and_client_methods_through_the_facade() {
        let mut host = TestHost;

        let result = ClientHostApi::dispatch(&mut host, "workspace.info", &serde_json::Value::Null)
            .expect("workspace.info");
        let Ok(Reply::Value(value)) = result else {
            panic!("workspace.info should return a value");
        };
        assert_eq!(
            value.get("name").and_then(|value| value.as_str()),
            Some("test")
        );

        let params = serde_json::json!({ "message": "hello" });
        let result = ClientHostApi::dispatch(&mut host, "notifications.show", &params)
            .expect("notifications.show");
        assert!(matches!(result, Ok(Reply::Value(value)) if value.is_null()));

        assert!(
            ClientHostApi::dispatch(&mut host, "missing.method", &serde_json::Value::Null)
                .is_none()
        );
    }
}
