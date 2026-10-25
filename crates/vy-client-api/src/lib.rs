//! The plugin API surface that only the client implements.
//!
//! Shared wire types and declarations live in `vy-host-api`. This crate owns
//! the UI, view, keymap, command, and editor-client declarations, plus the
//! value types that cross those APIs. Handlers stay in `vy-app`.

vy_host_api::host_api! {
    trait ClientHostApi: vy_host_api::HostApi {
        base_methods_fn = vy_host_api::host_api_methods;
        base_namespaces_fn = vy_host_api::host_api_namespaces;
        base_events_fn = vy_host_api::host_api_events;
        traits = [
            commands => CommandsApi,
            config => ConfigApi,
            editor => EditorApi,
            keymap_api => KeymapApi,
            notifications => NotificationsApi,
            ui => UiApi,
            ui_gutter => UiGutterApi,
            ui_hover => UiHoverApi,
            ui_picker => UiPickerApi,
            ui_statusline => UiStatuslineApi,
            views => ViewsApi,
        ];
        methods_fn = client_host_api_methods;
        namespaces_fn = client_host_api_namespaces;
        events_fn = client_host_api_events;
    }
}

pub mod color;
pub mod command;
pub mod keymap;
pub mod namespaces;
pub mod palette;
pub mod settings;
pub mod statusline;
pub mod widget;

pub use namespaces::{
    commands, config, editor, keymap as keymap_api, notifications, ui, ui_gutter, ui_hover,
    ui_picker, ui_statusline, views,
};

pub use color::ThemeColor;
pub use palette::PickerItem;
pub use statusline::StatuslineAlignment;
pub use widget::WidgetNode;

#[cfg(test)]
mod tests {
    use super::*;
    use vy_host_api::{CallError, Reply};

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

    impl namespaces::commands::CommandsApi for TestHost {
        fn register_command(
            &mut self,
            _params: namespaces::commands::CommandsRegisterParams,
        ) -> Result<Reply<()>, CallError> {
            Ok(Reply::Value(()))
        }

        fn execute(
            &mut self,
            _params: namespaces::commands::CommandsExecuteParams,
        ) -> Result<Reply<()>, CallError> {
            Ok(Reply::Value(()))
        }

        fn get_commands(
            &mut self,
        ) -> Result<Reply<Vec<namespaces::commands::CommandInfo>>, CallError> {
            Ok(Reply::Value(Vec::new()))
        }
    }

    impl namespaces::editor::EditorApi for TestHost {
        fn active_document(
            &mut self,
        ) -> Result<Reply<Option<vy_host_api::DocumentInfoReply>>, CallError> {
            Ok(Reply::Value(None))
        }

        fn open(
            &mut self,
            _params: namespaces::editor::EditorOpenParams,
        ) -> Result<Reply<()>, CallError> {
            Ok(Reply::Value(()))
        }

        fn mode(&mut self) -> Result<Reply<keymap::Mode>, CallError> {
            Ok(Reply::Value(keymap::Mode::Normal))
        }

        fn set_mode(
            &mut self,
            _params: namespaces::editor::EditorSetModeParams,
        ) -> Result<Reply<()>, CallError> {
            Ok(Reply::Value(()))
        }

        fn selections(&mut self) -> Result<Reply<Vec<vy_core::Selection>>, CallError> {
            Ok(Reply::Value(Vec::new()))
        }

        fn set_selections(
            &mut self,
            _params: namespaces::editor::EditorSetSelectionsParams,
        ) -> Result<Reply<()>, CallError> {
            Ok(Reply::Value(()))
        }

        fn edit(
            &mut self,
            _params: namespaces::editor::EditorEditParams,
        ) -> Result<Reply<namespaces::editor::EditorEditReply>, CallError> {
            Ok(Reply::Value(namespaces::editor::EditorEditReply {
                version: 0,
            }))
        }

        fn undo(&mut self) -> Result<Reply<()>, CallError> {
            Ok(Reply::Value(()))
        }

        fn redo(&mut self) -> Result<Reply<()>, CallError> {
            Ok(Reply::Value(()))
        }

        fn documents(&mut self) -> Result<Reply<Vec<vy_host_api::DocumentInfoReply>>, CallError> {
            Ok(Reply::Value(Vec::new()))
        }
    }

    impl namespaces::ui::UiApi for TestHost {}

    impl namespaces::ui_statusline::UiStatuslineApi for TestHost {
        fn create_item(
            &mut self,
            _params: namespaces::ui_statusline::StatuslineCreateParams,
        ) -> Result<Reply<()>, CallError> {
            Ok(Reply::Value(()))
        }

        fn update_item(
            &mut self,
            _params: namespaces::ui_statusline::StatuslineUpdateParams,
        ) -> Result<Reply<()>, CallError> {
            Ok(Reply::Value(()))
        }

        fn dispose_item(
            &mut self,
            _params: namespaces::ui_statusline::StatuslineDisposeParams,
        ) -> Result<Reply<()>, CallError> {
            Ok(Reply::Value(()))
        }
    }

    impl namespaces::ui_gutter::UiGutterApi for TestHost {
        fn create_item(
            &mut self,
            _params: namespaces::ui_gutter::GutterCreateParams,
        ) -> Result<Reply<()>, CallError> {
            Ok(Reply::Value(()))
        }

        fn dispose_item(
            &mut self,
            _params: namespaces::ui_gutter::GutterDisposeParams,
        ) -> Result<Reply<()>, CallError> {
            Ok(Reply::Value(()))
        }

        fn clear(&mut self) -> Result<Reply<()>, CallError> {
            Ok(Reply::Value(()))
        }
    }

    impl namespaces::ui_hover::UiHoverApi for TestHost {
        fn show(
            &mut self,
            _params: namespaces::ui_hover::HoverShowParams,
        ) -> Result<Reply<()>, CallError> {
            Ok(Reply::Value(()))
        }

        fn hide(&mut self) -> Result<Reply<()>, CallError> {
            Ok(Reply::Value(()))
        }
    }

    impl namespaces::ui_picker::UiPickerApi for TestHost {
        fn show(
            &mut self,
            _params: namespaces::ui_picker::PickerShowParams,
        ) -> Result<Reply<Option<namespaces::ui_picker::PickerShowReply>>, CallError> {
            Ok(Reply::Value(None))
        }
    }

    impl namespaces::keymap::KeymapApi for TestHost {
        fn bind(
            &mut self,
            _params: namespaces::keymap::KeymapBindParams,
        ) -> Result<Reply<()>, CallError> {
            Ok(Reply::Value(()))
        }

        fn unbind(
            &mut self,
            _params: namespaces::keymap::KeymapUnbindParams,
        ) -> Result<Reply<usize>, CallError> {
            Ok(Reply::Value(0))
        }

        fn set_context(
            &mut self,
            _params: namespaces::keymap::KeymapSetContextParams,
        ) -> Result<Reply<()>, CallError> {
            Ok(Reply::Value(()))
        }
    }

    impl vy_host_api::context::ContextApi for TestHost {
        fn info(&mut self) -> Result<Reply<vy_host_api::context::PluginContextReply>, CallError> {
            Ok(Reply::Value(vy_host_api::context::PluginContextReply {
                plugin_id: "test".into(),
                version: "0.0.0".into(),
                remote: vy_host_api::context::RemoteInfo {
                    is_remote: false,
                    name: "local".into(),
                    os: "linux".into(),
                    path_separator: "/".into(),
                },
            }))
        }
    }

    impl vy_host_api::document::DocumentApi for TestHost {
        fn get_text(
            &mut self,
            _params: vy_host_api::document::DocumentGetTextParams,
        ) -> Result<Reply<String>, CallError> {
            Ok(Reply::Value(String::new()))
        }

        fn line_at(
            &mut self,
            _params: vy_host_api::document::DocumentLineAtParams,
        ) -> Result<Reply<Option<String>>, CallError> {
            Ok(Reply::Value(None))
        }

        fn position_at(
            &mut self,
            _params: vy_host_api::document::DocumentPositionAtParams,
        ) -> Result<Reply<vy_core::Position>, CallError> {
            Ok(Reply::Value(vy_core::Position::new(0, 0)))
        }

        fn offset_at(
            &mut self,
            _params: vy_host_api::document::DocumentOffsetAtParams,
        ) -> Result<Reply<usize>, CallError> {
            Ok(Reply::Value(0))
        }
    }

    impl vy_host_api::log::LogApi for TestHost {
        fn write(
            &mut self,
            _params: vy_host_api::log::LogWriteParams,
        ) -> Result<Reply<()>, CallError> {
            Ok(Reply::Value(()))
        }
    }

    impl vy_host_api::process::ProcessApi for TestHost {
        fn exec(
            &mut self,
            _params: vy_proto::ProcessExecParams,
        ) -> Result<Reply<vy_proto::ProcessOutput>, CallError> {
            Err(CallError::invalid_state("not implemented"))
        }

        fn spawn(
            &mut self,
            _params: vy_host_api::process::ProcessSpawnParams,
        ) -> Result<Reply<vy_host_api::process::ProcessSpawnReply>, CallError> {
            Err(CallError::invalid_state("not implemented"))
        }

        fn kill(
            &mut self,
            _params: vy_host_api::process::ProcessKillParams,
        ) -> Result<Reply<vy_host_api::process::ProcessKillReply>, CallError> {
            Err(CallError::invalid_state("not implemented"))
        }

        fn write_stdin(
            &mut self,
            _params: vy_host_api::process::ProcessStdinParams,
        ) -> Result<Reply<vy_host_api::process::ProcessStdinReply>, CallError> {
            Err(CallError::invalid_state("not implemented"))
        }
    }

    impl vy_host_api::config::ConfigReadApi for TestHost {
        fn get(
            &mut self,
            _params: vy_host_api::config::ConfigGetParams,
        ) -> Result<Reply<vy_api_schema::Json>, CallError> {
            Ok(Reply::Value(vy_api_schema::Json(serde_json::Value::Null)))
        }

        fn inspect(
            &mut self,
            _params: vy_host_api::config::ConfigInspectParams,
        ) -> Result<Reply<vy_host_api::config::ConfigInspectReply>, CallError> {
            Ok(Reply::Value(vy_host_api::config::ConfigInspectReply {
                default: None,
                user: None,
                project: None,
                memory: None,
                effective: None,
            }))
        }
    }

    impl namespaces::config::ConfigApi for TestHost {
        fn set(
            &mut self,
            _params: namespaces::config::ConfigSetParams,
        ) -> Result<Reply<namespaces::config::ConfigSetReply>, CallError> {
            Ok(Reply::Value(namespaces::config::ConfigSetReply {
                changed: Vec::new(),
            }))
        }
    }

    impl vy_host_api::fs::FsApi for TestHost {
        fn read_file(
            &mut self,
            _params: vy_host_api::fs::FsReadFileParams,
        ) -> Result<Reply<vy_host_api::fs::FsReadFileReply>, CallError> {
            Err(CallError::invalid_state("not implemented"))
        }

        fn read_text_file(
            &mut self,
            _params: vy_host_api::fs::FsReadTextFileParams,
        ) -> Result<Reply<vy_host_api::fs::FsReadTextFileReply>, CallError> {
            Err(CallError::invalid_state("not implemented"))
        }

        fn write_file(
            &mut self,
            _params: vy_host_api::fs::FsWriteFileParams,
        ) -> Result<Reply<vy_host_api::fs::FsWriteFileReply>, CallError> {
            Err(CallError::invalid_state("not implemented"))
        }

        fn read_dir(
            &mut self,
            _params: vy_host_api::fs::FsReadDirParams,
        ) -> Result<Reply<vy_host_api::fs::FsReadDirReply>, CallError> {
            Err(CallError::invalid_state("not implemented"))
        }

        fn stat(
            &mut self,
            _params: vy_host_api::fs::FsStatParams,
        ) -> Result<Reply<vy_host_api::fs::FsStatReply>, CallError> {
            Err(CallError::invalid_state("not implemented"))
        }

        fn exists(
            &mut self,
            _params: vy_host_api::fs::FsExistsParams,
        ) -> Result<Reply<vy_host_api::fs::FsExistsReply>, CallError> {
            Err(CallError::invalid_state("not implemented"))
        }

        fn create_dir(
            &mut self,
            _params: vy_host_api::fs::FsCreateDirParams,
        ) -> Result<Reply<vy_host_api::fs::FsCreateDirReply>, CallError> {
            Err(CallError::invalid_state("not implemented"))
        }

        fn remove(
            &mut self,
            _params: vy_host_api::fs::FsRemoveParams,
        ) -> Result<Reply<vy_host_api::fs::FsRemoveReply>, CallError> {
            Err(CallError::invalid_state("not implemented"))
        }

        fn rename(
            &mut self,
            _params: vy_host_api::fs::FsRenameParams,
        ) -> Result<Reply<vy_host_api::fs::FsRenameReply>, CallError> {
            Err(CallError::invalid_state("not implemented"))
        }

        fn copy(
            &mut self,
            _params: vy_host_api::fs::FsCopyParams,
        ) -> Result<Reply<vy_host_api::fs::FsCopyReply>, CallError> {
            Err(CallError::invalid_state("not implemented"))
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

    impl namespaces::views::ViewsApi for TestHost {
        fn open(
            &mut self,
            _params: namespaces::views::ViewOpenParams,
        ) -> Result<Reply<()>, CallError> {
            Ok(Reply::Value(()))
        }

        fn close(
            &mut self,
            _params: namespaces::views::ViewCloseParams,
        ) -> Result<Reply<()>, CallError> {
            Ok(Reply::Value(()))
        }

        fn toggle(
            &mut self,
            _params: namespaces::views::ViewToggleParams,
        ) -> Result<Reply<()>, CallError> {
            Ok(Reply::Value(()))
        }

        fn render(
            &mut self,
            _params: namespaces::views::ViewsRenderParams,
        ) -> Result<Reply<()>, CallError> {
            Ok(Reply::Value(()))
        }
    }

    impl vy_host_api::HostRoot for TestHost {}
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

        let params = serde_json::json!({ "key": "ui.fontSize" });
        let result = ClientHostApi::dispatch(&mut host, "config.get", &params).expect("config.get");
        assert!(matches!(result, Ok(Reply::Value(_))));

        let params = serde_json::json!({ "values": { "ui.fontSize": 14 } });
        let result = ClientHostApi::dispatch(&mut host, "config.set", &params).expect("config.set");
        assert!(matches!(result, Ok(Reply::Value(_))));

        let params = serde_json::json!({ "view": "example.tree" });
        let result = ClientHostApi::dispatch(&mut host, "views.open", &params).expect("views.open");
        assert!(matches!(result, Ok(Reply::Value(value)) if value.is_null()));

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
