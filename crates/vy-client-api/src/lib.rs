//! The plugin API surface that only the client implements.
//!
//! Shared wire types and declarations live in `vy-host-api`. This crate owns
//! the UI, view, keymap, command, and editor-client declarations, plus the
//! value types that cross those APIs. Handlers stay in `vy-app`.

pub use vy_host_api::{event_decl, method_decl};

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
}
