//! The bridge between `init.ts` and the client's state.
//!
//! [`PluginHost`] owns the script thread and the `init.ts` file stamp. Requests
//! from the script are answered by [`handle_request`], which only touches
//! GPUI-free state (`Editor`, `Config`, `Keymap`, `CommandRegistry`); the view
//! sends the resulting transaction to the core.
//!
//! Phase 2 evaluates one trusted script. External plugins, manifests, and the
//! approval UI come in Phase 3.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::Value;
use vy_capabilities::TrustedStore;
use vy_core::Uri;
use vy_js::{HostRequest, RemoteEnv, ScriptEnv, ScriptEvent, ScriptHost, WorkspaceEnv};
use vy_proto::ErrorCode;

use crate::commands::{CommandRegistry, DocumentCommand, PaneCommand, UiCommand};
use crate::config::Config;
use crate::editor::{Action, Editor, Transaction};
use crate::keymap::Keymap;
use crate::palette::PickerItem;
use crate::paths::Paths;
use crate::slots::{GutterRegistry, HoverState};
use crate::statusline::Statusline;
use crate::widget::WidgetNode;

/// Everything the shim knows about the open document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentInfo {
    pub id: u64,
    pub uri: Uri,
    pub language_id: String,
}

/// The workspace root and its name, as `workspace.*` reports them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceInfo {
    pub root_uri: Option<String>,
    pub name: Option<String>,
    pub folders: Vec<String>,
}

/// What the script thread owns plus the file stamp used for reloads.
pub struct PluginHost {
    script: ScriptHost,
    init_path: PathBuf,
    stamp: Option<SystemTime>,
    env: ScriptEnv,
}

/// The result of trying to load `init.ts`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitLoad {
    /// No `init.ts`; nothing runs.
    Missing,
    /// The source was handed to the script thread.
    Loaded,
    /// The file exists but could not be read.
    Failed(String),
}

impl PluginHost {
    /// Starts the script thread. The runtime is ready before this returns.
    pub fn spawn(env: ScriptEnv, init_path: PathBuf) -> Self {
        let stamp = file_stamp(&init_path);
        let stored = env.clone();
        Self {
            script: ScriptHost::spawn(env),
            init_path,
            stamp,
            env: stored,
        }
    }

    /// The environment the script was started with.
    pub fn env(&self) -> &ScriptEnv {
        &self.env
    }

    pub fn events(&self) -> async_channel::Receiver<ScriptEvent> {
        self.script.events()
    }

    pub fn requests(&self) -> async_channel::Receiver<HostRequest> {
        self.script.requests()
    }

    /// The script thread, for `commands.execute` to run a handler.
    pub fn script(&self) -> &ScriptHost {
        &self.script
    }

    pub fn init_path(&self) -> &Path {
        &self.init_path
    }

    /// Reads and evaluates `init.ts`. Missing files are not an error.
    pub fn evaluate_init(&mut self) -> InitLoad {
        self.stamp = file_stamp(&self.init_path);
        match std::fs::read_to_string(&self.init_path) {
            Ok(source) => {
                self.script.evaluate(self.init_path.clone(), source);
                InitLoad::Loaded
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => InitLoad::Missing,
            Err(error) => InitLoad::Failed(format!(
                "could not read {}: {error}",
                self.init_path.display()
            )),
        }
    }

    /// Reads and evaluates an external script file (e.g. plugin main entry).
    pub fn evaluate_file(&mut self, path: PathBuf) -> InitLoad {
        match std::fs::read_to_string(&path) {
            Ok(source) => {
                self.script.evaluate(path, source);
                InitLoad::Loaded
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => InitLoad::Missing,
            Err(error) => InitLoad::Failed(format!("could not read {}: {error}", path.display())),
        }
    }

    /// Whether `init.ts` was added, removed, or edited since the last load.
    pub fn changed(&self) -> bool {
        file_stamp(&self.init_path) != self.stamp
    }

    /// Runs a command handler registered by the script.
    pub fn execute(&self, id: String, args: Value) {
        self.script.execute(id, args);
    }

    /// Evaluates a JavaScript/TypeScript code snippet in the script context.
    pub fn eval(&self, source: impl Into<String>) {
        self.script.eval(source);
    }

    /// Tells the script which settings keys changed.
    pub fn emit_config_change(&self, keys: Vec<String>) {
        self.script.emit_config_change(keys);
    }

    /// Tells the script that the active editor's selections changed.
    pub fn emit_selection_change(&self, selections: Vec<vy_core::Selection>) {
        self.script.emit_selection_change(selections);
    }

    /// Tells the script that the active editor/document changed.
    pub fn emit_active_editor_change(&self, document_info: Option<vy_js::DocumentInfoReply>) {
        self.script.emit_active_editor_change(document_info);
    }

    /// Tells the script that workspace folders changed.
    pub fn emit_workspace_folders_change(&self, folders: Vec<String>) {
        self.script.emit_workspace_folders_change(folders);
    }

    pub fn is_running(&self) -> bool {
        self.script.is_running()
    }
}

/// Builds the constants the shim exposes. `init.ts` is always trusted.
pub fn script_env(workspace: &WorkspaceInfo, paths: &Paths) -> (ScriptEnv, Vec<String>) {
    script_env_with_remote(
        workspace,
        paths,
        RemoteEnv {
            is_remote: false,
            name: "local".to_string(),
            os: std::env::consts::OS.to_string(),
            path_separator: std::path::MAIN_SEPARATOR.to_string(),
        },
    )
}

/// Builds the script environment for a local or remote workspace.
pub fn script_env_with_remote(
    workspace: &WorkspaceInfo,
    paths: &Paths,
    remote: RemoteEnv,
) -> (ScriptEnv, Vec<String>) {
    let (store, warning) = TrustedStore::load(&paths.trusted_json());
    // Phase 2 has no manifests; reading the store here keeps a broken file from
    // going unnoticed and leaves the approval path in place for Phase 3.
    let _ = store;

    let env = ScriptEnv {
        plugin_id: "init".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        workspace: WorkspaceEnv {
            root_uri: workspace
                .root_uri
                .as_deref()
                .and_then(|uri| Uri::parse(uri).ok()),
            name: workspace.name.clone(),
            folders: workspace
                .folders
                .iter()
                .filter_map(|uri| Uri::parse(uri.clone()).ok())
                .collect(),
            trusted: true,
        },
        capabilities: Vec::new(),
        remote,
    };

    (env, warning.into_iter().collect())
}

/// What the view must do after a request was answered.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum HostAction {
    #[default]
    None,
    Edit(Transaction),
    Undo,
    Redo,
    Pane(PaneCommand),
    Document {
        command: DocumentCommand,
        args: Option<Value>,
    },
    Ui {
        command: UiCommand,
        args: Option<Value>,
    },
    RenderWidgetTree {
        view_id: String,
        tree: WidgetNode,
    },
    ShowPicker {
        id: String,
        items: Vec<PickerItem>,
        placeholder: Option<String>,
        request: vy_js::HostRequest,
    },
    ServerRpc {
        method: String,
        params: rmpv::Value,
        request: vy_js::HostRequest,
    },
}

impl From<Action> for HostAction {
    fn from(action: Action) -> Self {
        match action {
            Action::None => Self::None,
            Action::Edit(transaction) => Self::Edit(transaction),
            Action::Undo => Self::Undo,
            Action::Redo => Self::Redo,
        }
    }
}

/// Mutable client state a request may read or change.
pub struct RequestContext<'a> {
    pub editor: &'a mut Editor,
    pub config: &'a mut Config,
    pub keymap: &'a mut Keymap,
    pub commands: &'a mut CommandRegistry,
    pub document: Option<DocumentInfo>,
    pub workspace: &'a WorkspaceInfo,
    pub paths: &'a Paths,
    pub script: Option<&'a ScriptHost>,
    pub env: &'a ScriptEnv,
    pub statusline: Option<&'a mut Statusline>,
    pub gutter: Option<&'a mut GutterRegistry>,
    pub hover: Option<&'a mut Option<HoverState>>,
}

/// The effect of one request on the client.
#[derive(Debug, Default)]
pub struct HostOutcome {
    pub action: HostAction,
    pub status: Option<String>,
    pub config_changed: Vec<String>,
}

/// Answers one call from the shim. The request is always answered so the script
/// thread never waits for the full reply timeout.
pub fn handle_request(context: RequestContext<'_>, request: HostRequest) -> HostOutcome {
    let mut outcome = HostOutcome::default();
    let mut context = context;

    let result = {
        let mut call = crate::api::Call {
            context: &mut context,
            request: &request,
            outcome: &mut outcome,
        };
        crate::api::dispatch(&mut call, &request.method, &request.params)
    };

    match result {
        Some(Ok(crate::api::Reply::Value(value))) => request.respond_value(value),
        Some(Ok(crate::api::Reply::Deferred)) => {}
        Some(Err(error)) => request.respond_error(&error.code.to_string(), error.message),
        None => {
            let code = ErrorCode::InvalidRequest.to_string();
            let message = format!("unknown host method: {}", request.method);
            request.respond_error(&code, message);
        }
    }

    outcome
}

fn file_stamp(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::ThemeColor;
    use crate::commands::CommandArgs;
    use crate::keymap::KeyContext;
    use crate::statusline::StatuslineAlignment;
    use serde_json::json;

    fn test_env() -> &'static ScriptEnv {
        static ENV: std::sync::OnceLock<ScriptEnv> = std::sync::OnceLock::new();
        ENV.get_or_init(ScriptEnv::default)
    }

    fn context<'a>(
        editor: &'a mut Editor,
        config: &'a mut Config,
        keymap: &'a mut Keymap,
        commands: &'a mut CommandRegistry,
        workspace: &'a WorkspaceInfo,
        paths: &'a Paths,
    ) -> RequestContext<'a> {
        RequestContext {
            editor,
            config,
            keymap,
            commands,
            document: Some(DocumentInfo {
                id: 1,
                uri: Uri::parse("file:///tmp/a.txt").expect("uri"),
                language_id: "text".to_string(),
            }),
            workspace,
            paths,
            script: None,
            env: test_env(),
            statusline: None,
            gutter: None,
            hover: None,
        }
    }

    /// Builds a request without a live script thread.
    fn request(method: &str, params: Value) -> HostRequest {
        HostRequest::new(method.to_string(), params).0
    }

    #[test]
    fn applies_script_edits_as_one_transaction() {
        let mut editor = Editor::with_text("hello\n", 3);
        let mut config = Config::with_defaults();
        let mut keymap = Keymap::new(std::time::Duration::from_millis(1000));
        let mut commands = CommandRegistry::new();
        let workspace = WorkspaceInfo::default();
        let paths = Paths::under("/tmp/vy-host");

        let outcome = handle_request(
            context(
                &mut editor,
                &mut config,
                &mut keymap,
                &mut commands,
                &workspace,
                &paths,
            ),
            request(
                "editor.edit",
                json!({
                    "edits": [
                        { "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 0 } }, "text": "> " }
                    ]
                }),
            ),
        );

        assert!(matches!(outcome.action, HostAction::Edit(_)));
        assert_eq!(editor.text(), "> hello\n");
        assert_eq!(editor.version(), 4);
    }

    #[test]
    fn registers_commands_and_binds_keys() {
        let mut editor = Editor::new();
        let mut config = Config::with_defaults();
        let mut keymap = Keymap::new(std::time::Duration::from_millis(1000));
        let mut commands = CommandRegistry::new();
        let workspace = WorkspaceInfo::default();
        let paths = Paths::under("/tmp/vy-host");

        handle_request(
            context(
                &mut editor,
                &mut config,
                &mut keymap,
                &mut commands,
                &workspace,
                &paths,
            ),
            request("commands.registerCommand", json!({ "id": "user.hello" })),
        );
        handle_request(
            context(
                &mut editor,
                &mut config,
                &mut keymap,
                &mut commands,
                &workspace,
                &paths,
            ),
            request(
                "keymap.bind",
                json!({ "keys": "g z", "command": "user.hello", "when": "normalMode" }),
            ),
        );

        assert!(commands.is_registered("user.hello"));
        let outcome = keymap.feed(crate::keymap::Key::Char('g'), &KeyContext::default());
        assert_eq!(outcome, crate::keymap::KeymapOutcome::Pending);
        let outcome = keymap.feed(crate::keymap::Key::Char('z'), &KeyContext::default());
        assert_eq!(
            outcome,
            crate::keymap::KeymapOutcome::Command {
                id: "user.hello".to_string(),
                args: CommandArgs::default()
            }
        );
    }

    #[test]
    fn executes_pane_commands_from_scripts() {
        let mut editor = Editor::new();
        let mut config = Config::with_defaults();
        let mut keymap = Keymap::new(std::time::Duration::from_millis(1000));
        let mut commands = CommandRegistry::new();
        let workspace = WorkspaceInfo::default();
        let paths = Paths::under("/tmp/vy-host");

        let outcome = handle_request(
            context(
                &mut editor,
                &mut config,
                &mut keymap,
                &mut commands,
                &workspace,
                &paths,
            ),
            request("commands.execute", json!({ "id": "vy.pane.split_right" })),
        );

        assert_eq!(outcome.action, HostAction::Pane(PaneCommand::SplitRight));
    }

    #[test]
    fn sets_configuration_in_memory_only() {
        let mut editor = Editor::new();
        let mut config = Config::with_defaults();
        let mut keymap = Keymap::new(std::time::Duration::from_millis(1000));
        let mut commands = CommandRegistry::new();
        let workspace = WorkspaceInfo::default();
        let paths = Paths::under("/tmp/vy-host");

        let outcome = handle_request(
            context(
                &mut editor,
                &mut config,
                &mut keymap,
                &mut commands,
                &workspace,
                &paths,
            ),
            request("config.set", json!({ "values": { "ui.fontSize": 18 } })),
        );

        assert_eq!(outcome.config_changed, vec!["ui.fontSize".to_string()]);
        assert_eq!(config.font_size(), 18.0);
        assert_eq!(config.inspect("ui.fontSize").memory, Some(json!(18)));
    }

    #[test]
    fn evaluates_init_and_answers_its_requests() {
        let directory = std::env::temp_dir().join("vy-plugin-host-evaluate");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("create");

        let workspace = WorkspaceInfo::default();
        let paths = Paths::under(&directory);
        let (env, _) = script_env(&workspace, &paths);
        let mut host = PluginHost::spawn(env, paths.init_ts());
        std::fs::create_dir_all(paths.init_ts().parent().expect("config dir")).expect("config");
        std::fs::write(
            paths.init_ts(),
            r#"
import { config, commands, notifications } from "vy-api";

config.set({ values: { "ui.fontSize": 15 } });
commands.registerCommand("user.hello", async () => {
  await notifications.show({ message: "hello from init.ts" });
});
"#,
        )
        .expect("write");

        assert_eq!(host.evaluate_init(), InitLoad::Loaded);

        let mut editor = Editor::with_text("hello\n", 1);
        let mut config = Config::with_defaults();
        let mut keymap = Keymap::new(std::time::Duration::from_millis(1000));
        let mut commands = CommandRegistry::new();
        let requests = host.requests();
        let events = host.events();
        let mut evaluated = false;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);

        while std::time::Instant::now() < deadline {
            if let Ok(request) = requests.try_recv() {
                let context = RequestContext {
                    editor: &mut editor,
                    config: &mut config,
                    keymap: &mut keymap,
                    commands: &mut commands,
                    document: None,
                    workspace: &workspace,
                    paths: &paths,
                    script: Some(host.script()),
                    env: test_env(),
                    statusline: None,
                    gutter: None,
                    hover: None,
                };
                let outcome = handle_request(context, request);
                assert!(outcome.action == HostAction::None);
                continue;
            }

            match events.try_recv() {
                Ok(ScriptEvent::Evaluated) => evaluated = true,
                Ok(ScriptEvent::Error { message }) => panic!("{message}"),
                Ok(_) => {}
                Err(_) => std::thread::yield_now(),
            }
            if evaluated {
                break;
            }
        }

        assert!(evaluated, "init.ts did not finish");
        assert_eq!(config.font_size(), 15.0);
        assert!(commands.is_registered("user.hello"));

        drop(host);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn sees_the_init_script_stamp() {
        let directory = std::env::temp_dir().join("vy-plugin-host-stamp");
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("create");
        let path = directory.join("init.ts");

        let workspace = WorkspaceInfo::default();
        let (env, warnings) = script_env(&workspace, &Paths::under(&directory));
        assert!(warnings.is_empty());
        let mut host = PluginHost::spawn(env, path.clone());

        assert_eq!(host.evaluate_init(), InitLoad::Missing);
        std::fs::write(&path, "export const answer = 42;\n").expect("write");
        assert!(host.changed());
        assert_eq!(host.evaluate_init(), InitLoad::Loaded);
        assert!(!host.changed());

        drop(host);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn reports_unknown_methods() {
        let (request, receiver) = HostRequest::new("nope.nope".to_string(), Value::Null);
        let mut editor = Editor::new();
        let mut config = Config::with_defaults();
        let mut keymap = Keymap::new(std::time::Duration::from_millis(1000));
        let mut commands = CommandRegistry::new();
        let workspace = WorkspaceInfo::default();
        let paths = Paths::under("/tmp/vy-host");

        handle_request(
            context(
                &mut editor,
                &mut config,
                &mut keymap,
                &mut commands,
                &workspace,
                &paths,
            ),
            request,
        );

        let reply = receiver.recv().expect("reply");
        assert!(matches!(reply, vy_js::HostReply::Err { code, .. } if code == "InvalidRequest"));
    }

    #[test]
    fn answers_editor_mode_from_the_declared_namespace() {
        let mut editor = Editor::new();
        let mut config = Config::with_defaults();
        let mut keymap = Keymap::new(std::time::Duration::from_millis(1000));
        let mut commands = CommandRegistry::new();
        let workspace = WorkspaceInfo::default();
        let paths = Paths::under("/tmp/vy-host");

        let (request, receiver) = HostRequest::new("editor.mode".to_string(), Value::Null);
        handle_request(
            context(
                &mut editor,
                &mut config,
                &mut keymap,
                &mut commands,
                &workspace,
                &paths,
            ),
            request,
        );

        let reply = receiver.recv().expect("reply");
        assert!(matches!(reply, vy_js::HostReply::Ok(value) if value == json!("normal")));
    }

    #[test]
    fn rejects_parameters_of_the_wrong_type() {
        let mut editor = Editor::new();
        let mut config = Config::with_defaults();
        let mut keymap = Keymap::new(std::time::Duration::from_millis(1000));
        let mut commands = CommandRegistry::new();
        let workspace = WorkspaceInfo::default();
        let paths = Paths::under("/tmp/vy-host");

        // An unknown mode is rejected instead of falling back to a default.
        let (request, receiver) = HostRequest::new("editor.setMode", json!({ "mode": "bogus" }));
        handle_request(
            context(
                &mut editor,
                &mut config,
                &mut keymap,
                &mut commands,
                &workspace,
                &paths,
            ),
            request,
        );
        let reply = receiver.recv().expect("reply");
        assert!(matches!(reply, vy_js::HostReply::Err { code, .. } if code == "InvalidParams"));

        // A missing required field is an error, not an empty string.
        let (request, receiver) = HostRequest::new("config.get", json!({}));
        handle_request(
            context(
                &mut editor,
                &mut config,
                &mut keymap,
                &mut commands,
                &workspace,
                &paths,
            ),
            request,
        );
        let reply = receiver.recv().expect("reply");
        assert!(matches!(reply, vy_js::HostReply::Err { code, .. } if code == "InvalidParams"));

        // An unknown alignment is an error, not `Left`.
        let (request, receiver) = HostRequest::new(
            "ui.statusline.createItem",
            json!({ "id": "x", "alignment": "top", "text": "x" }),
        );
        handle_request(
            context(
                &mut editor,
                &mut config,
                &mut keymap,
                &mut commands,
                &workspace,
                &paths,
            ),
            request,
        );
        let reply = receiver.recv().expect("reply");
        assert!(matches!(reply, vy_js::HostReply::Err { code, .. } if code == "InvalidParams"));
    }

    #[test]
    fn rejects_unknown_color_tokens() {
        let mut editor = Editor::new();
        let mut config = Config::with_defaults();
        let mut keymap = Keymap::new(std::time::Duration::from_millis(1000));
        let mut commands = CommandRegistry::new();
        let workspace = WorkspaceInfo::default();
        let paths = Paths::under("/tmp/vy-host");

        let (request, receiver) = HostRequest::new(
            "ui.statusline.createItem",
            json!({ "id": "x", "alignment": "left", "text": "x", "color": "chartreuse" }),
        );
        handle_request(
            context(
                &mut editor,
                &mut config,
                &mut keymap,
                &mut commands,
                &workspace,
                &paths,
            ),
            request,
        );
        let reply = receiver.recv().expect("reply");
        assert!(matches!(reply, vy_js::HostReply::Err { code, .. } if code == "InvalidParams"));

        let (request, receiver) = HostRequest::new(
            "ui.statusline.updateItem",
            json!({ "id": "x", "color": "chartreuse" }),
        );
        handle_request(
            context(
                &mut editor,
                &mut config,
                &mut keymap,
                &mut commands,
                &workspace,
                &paths,
            ),
            request,
        );
        let reply = receiver.recv().expect("reply");
        assert!(matches!(reply, vy_js::HostReply::Err { code, .. } if code == "InvalidParams"));
    }

    #[test]
    fn keeps_document_offsets_in_utf16() {
        let mut editor = Editor::with_text("a😀b\n", 1);
        let mut config = Config::with_defaults();
        let mut keymap = Keymap::new(std::time::Duration::from_millis(1000));
        let mut commands = CommandRegistry::new();
        let workspace = WorkspaceInfo::default();
        let paths = Paths::under("/tmp/vy-host");

        let (request, receiver) = HostRequest::new("document.positionAt", json!({ "offset": 3 }));
        handle_request(
            context(
                &mut editor,
                &mut config,
                &mut keymap,
                &mut commands,
                &workspace,
                &paths,
            ),
            request,
        );
        let reply = receiver.recv().expect("reply");
        let vy_js::HostReply::Ok(value) = reply else {
            panic!("expected a value");
        };
        assert_eq!(value, json!({ "line": 0, "character": 3 }));
    }

    #[test]
    fn ui_statusline_creates_updates_and_disposes() {
        let mut editor = Editor::new();
        let mut config = Config::with_defaults();
        let mut keymap = Keymap::new(std::time::Duration::from_millis(1000));
        let mut commands = CommandRegistry::new();
        let workspace = WorkspaceInfo::default();
        let paths = Paths::under("/tmp/vy-host");
        let mut statusline = Statusline::new();

        let (req1, rec1) = HostRequest::new(
            "ui.statusline.createItem",
            json!({
                "id": "my_mode",
                "alignment": "left",
                "priority": 50,
                "text": "NORMAL",
                "color": "primary"
            }),
        );
        let mut ctx1 = context(
            &mut editor,
            &mut config,
            &mut keymap,
            &mut commands,
            &workspace,
            &paths,
        );
        ctx1.statusline = Some(&mut statusline);
        handle_request(ctx1, req1);
        let reply1 = rec1.recv().expect("reply");
        assert!(matches!(reply1, vy_js::HostReply::Ok(_)));

        let left = statusline.items_for(StatuslineAlignment::Left);
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].text, "NORMAL");
        assert_eq!(left[0].color, Some(ThemeColor::Primary));

        let (req2, rec2) = HostRequest::new(
            "ui.statusline.updateItem",
            json!({
                "id": "my_mode",
                "text": "INSERT"
            }),
        );
        let mut ctx2 = context(
            &mut editor,
            &mut config,
            &mut keymap,
            &mut commands,
            &workspace,
            &paths,
        );
        ctx2.statusline = Some(&mut statusline);
        handle_request(ctx2, req2);
        let reply2 = rec2.recv().expect("reply");
        assert!(matches!(reply2, vy_js::HostReply::Ok(_)));
        assert_eq!(
            statusline.items_for(StatuslineAlignment::Left)[0].text,
            "INSERT"
        );

        let (req3, rec3) = HostRequest::new(
            "ui.statusline.disposeItem",
            json!({
                "id": "my_mode"
            }),
        );
        let mut ctx3 = context(
            &mut editor,
            &mut config,
            &mut keymap,
            &mut commands,
            &workspace,
            &paths,
        );
        ctx3.statusline = Some(&mut statusline);
        handle_request(ctx3, req3);
        let reply3 = rec3.recv().expect("reply");
        assert!(matches!(reply3, vy_js::HostReply::Ok(_)));
        assert!(statusline.is_empty());
    }

    #[test]
    fn ui_picker_show_triggers_action() {
        let mut editor = Editor::new();
        let mut config = Config::with_defaults();
        let mut keymap = Keymap::new(std::time::Duration::from_millis(1000));
        let mut commands = CommandRegistry::new();
        let workspace = WorkspaceInfo::default();
        let paths = Paths::under("/tmp/vy-host");

        let (req, rec) = HostRequest::new(
            "ui.picker.show",
            json!({
                "id": "test_picker",
                "placeholder": "Choose an item",
                "items": [
                    { "id": "1", "title": "First" },
                    { "id": "2", "title": "Second", "description": "desc" }
                ]
            }),
        );
        let ctx = context(
            &mut editor,
            &mut config,
            &mut keymap,
            &mut commands,
            &workspace,
            &paths,
        );
        let outcome = handle_request(ctx, req);
        match outcome.action {
            HostAction::ShowPicker {
                id,
                items,
                placeholder,
                request,
            } => {
                assert!(id.starts_with("picker_"), "{id}");
                assert_eq!(items.len(), 2);
                assert_eq!(placeholder.as_deref(), Some("Choose an item"));
                request.respond_value(json!({ "selectedId": "1" }));
            }
            _ => panic!("expected ShowPicker"),
        }
        let reply = rec.recv().expect("reply");
        assert!(matches!(reply, vy_js::HostReply::Ok(_)));
    }

    #[test]
    fn ui_gutter_and_hover_requests() {
        let mut editor = Editor::new();
        let mut config = Config::with_defaults();
        let mut keymap = Keymap::new(std::time::Duration::from_millis(1000));
        let mut commands = CommandRegistry::new();
        let workspace = WorkspaceInfo::default();
        let paths = Paths::under("/tmp/vy-host");
        let mut gutter = GutterRegistry::new();
        let mut hover = None;

        let (req1, rec1) = HostRequest::new(
            "ui.gutter.createItem",
            json!({
                "id": "git_add",
                "line": 42,
                "text": "+"
            }),
        );
        let mut ctx1 = context(
            &mut editor,
            &mut config,
            &mut keymap,
            &mut commands,
            &workspace,
            &paths,
        );
        ctx1.gutter = Some(&mut gutter);
        handle_request(ctx1, req1);
        let reply1 = rec1.recv().expect("reply");
        assert!(matches!(reply1, vy_js::HostReply::Ok(_)));
        assert_eq!(gutter.items_for_line(42).len(), 1);

        let (req2, rec2) = HostRequest::new(
            "ui.hover.show",
            json!({
                "line": 42,
                "character": 5,
                "text": "type Foo = number"
            }),
        );
        let mut ctx2 = context(
            &mut editor,
            &mut config,
            &mut keymap,
            &mut commands,
            &workspace,
            &paths,
        );
        ctx2.hover = Some(&mut hover);
        handle_request(ctx2, req2);
        let reply2 = rec2.recv().expect("reply");
        assert!(matches!(reply2, vy_js::HostReply::Ok(_)));
        assert_eq!(hover, Some(HoverState::new(42, 5, "type Foo = number")));
    }

    #[test]
    fn fs_and_process_dispatch_to_server_rpc() {
        let mut editor = Editor::new();
        let mut config = Config::with_defaults();
        let mut keymap = Keymap::new(std::time::Duration::from_millis(1000));
        let mut commands = CommandRegistry::new();
        let workspace = WorkspaceInfo::default();
        let paths = Paths::under("/tmp/vy-host");

        // 1. fs.readDir
        let (req, _) = HostRequest::new(
            "fs.readDir",
            json!({
                "uri": "file:///tmp/vy-test"
            }),
        );
        let ctx = context(
            &mut editor,
            &mut config,
            &mut keymap,
            &mut commands,
            &workspace,
            &paths,
        );
        let outcome = handle_request(ctx, req);
        match outcome.action {
            HostAction::ServerRpc { method, params, .. } => {
                assert_eq!(method, "fs/read_dir");
                let decoded: vy_proto::FsReadDirParams = vy_proto::decode_value(&params).unwrap();
                assert_eq!(decoded.uri, "file:///tmp/vy-test");
            }
            other => panic!("expected ServerRpc action, got {other:?}"),
        }

        // 2. process.exec
        let (req, _) = HostRequest::new(
            "process.exec",
            json!({
                "command": "git",
                "args": ["status"]
            }),
        );
        let ctx = context(
            &mut editor,
            &mut config,
            &mut keymap,
            &mut commands,
            &workspace,
            &paths,
        );
        let outcome = handle_request(ctx, req);
        match outcome.action {
            HostAction::ServerRpc { method, params, .. } => {
                assert_eq!(method, "process/exec");
                let decoded: vy_proto::ProcessExecParams = vy_proto::decode_value(&params).unwrap();
                assert_eq!(decoded.command, "git");
                assert_eq!(decoded.args, vec!["status"]);
            }
            other => panic!("expected ServerRpc action, got {other:?}"),
        }
    }
}
