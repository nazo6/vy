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
use vy_proto::{ErrorCode, RpcCall};

use crate::commands::{CommandRegistry, DocumentCommand, Owner, PaneCommand, UiCommand};
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
    entry_path: PathBuf,
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
    pub fn spawn(env: ScriptEnv, entry_path: PathBuf) -> Self {
        let root = entry_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        Self::spawn_in(env, entry_path, root)
    }

    /// Starts the script thread with an explicit module root.
    pub fn spawn_in(env: ScriptEnv, entry_path: PathBuf, root: PathBuf) -> Self {
        let stamp = file_stamp(&entry_path);
        let stored = env.clone();
        Self {
            script: ScriptHost::spawn(env, root),
            entry_path,
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

    pub fn entry_path(&self) -> &Path {
        &self.entry_path
    }

    /// Files loaded by the script runtime, including the entry point.
    pub fn loaded_files(&self) -> Vec<PathBuf> {
        self.script.loaded_files()
    }

    /// Reads and evaluates `init.ts`. Missing files are not an error.
    pub fn evaluate_init(&mut self) -> InitLoad {
        self.stamp = file_stamp(&self.entry_path);
        match std::fs::read_to_string(&self.entry_path) {
            Ok(source) => {
                self.script.evaluate(self.entry_path.clone(), source);
                InitLoad::Loaded
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => InitLoad::Missing,
            Err(error) => InitLoad::Failed(format!(
                "could not read {}: {error}",
                self.entry_path.display()
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

    /// Whether the entry or any imported module changed since it was loaded.
    pub fn changed(&self) -> bool {
        file_stamp(&self.entry_path) != self.stamp || !self.script.changed_files().is_empty()
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
        for key in keys {
            self.script.emit_event::<vy_host_api::config::OnDidChange>(
                &vy_host_api::config::ConfigChange { key },
            );
        }
    }

    /// Tells the script that the active editor's selections changed.
    pub fn emit_selection_change(&self, selections: Vec<vy_core::Selection>) {
        self.script
            .emit_event::<vy_client_api::editor::OnDidChangeSelection>(
                &vy_client_api::editor::SelectionChange { selections },
            );
    }

    /// Tells the script that the active editor/document changed.
    pub fn emit_active_editor_change(&self, document_info: Option<vy_js::DocumentInfoReply>) {
        self.script
            .emit_event::<vy_client_api::editor::OnDidChangeActiveDocument>(
                &vy_client_api::editor::ActiveDocumentChange {
                    document: document_info,
                },
            );
    }

    /// Tells the script that workspace folders changed.
    pub fn emit_workspace_folders_change(&self, folders: Vec<String>) {
        self.script
            .emit_event::<vy_host_api::workspace::OnDidChangeWorkspaceFolders>(
                &vy_host_api::workspace::WorkspaceFoldersChange { folders },
            );
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
    // Reading the store here keeps a broken file from going unnoticed.
    let _ = store;

    let env = build_env(
        "init",
        env!("CARGO_PKG_VERSION"),
        workspace,
        remote,
        Vec::new(),
    );

    (env, warning.into_iter().collect())
}

/// Builds the script environment for an activated plugin.
pub fn plugin_script_env(
    workspace: &WorkspaceInfo,
    remote: RemoteEnv,
    plugin_id: &str,
    version: &str,
    capabilities: Vec<String>,
) -> ScriptEnv {
    build_env(plugin_id, version, workspace, remote, capabilities)
}

fn build_env(
    plugin_id: &str,
    version: &str,
    workspace: &WorkspaceInfo,
    remote: RemoteEnv,
    capabilities: Vec<String>,
) -> ScriptEnv {
    ScriptEnv {
        plugin_id: plugin_id.to_string(),
        version: version.to_string(),
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
        capabilities,
        remote,
    }
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
        call: RpcCall,
        request: vy_js::HostRequest,
    },
    Execute {
        id: String,
        args: Value,
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
    pub owner: Owner,
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
    use crate::commands::{CommandArgs, Owner};
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
            owner: Owner::Init,
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
vy.config.set({ values: { "ui.fontSize": 15 } });
vy.commands.registerCommand("user.hello", async () => {
  await vy.notifications.show({ message: "hello from init.ts" });
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
                    owner: Owner::Init,
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
    fn plugin_runtimes_keep_their_own_source_and_owner() {
        let base = std::env::temp_dir().join(format!("vy-plugin-runtimes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let plugin_a = base.join("plugin-a");
        let plugin_b = base.join("plugin-b");
        std::fs::create_dir_all(plugin_a.join("lib")).expect("create a");
        std::fs::create_dir_all(&plugin_b).expect("create b");
        std::fs::write(
            plugin_a.join("lib/value.ts"),
            "export const value: string = \"a\";\n",
        )
        .expect("write helper");
        std::fs::write(
            plugin_a.join("frontend.ts"),
            "import { value } from \"./lib/value\";\nvy.commands.registerCommand(\"a.one\", async () => value);\n",
        )
        .expect("write a");
        std::fs::write(
            plugin_b.join("frontend.ts"),
            "vy.commands.registerCommand(\"b.one\", async () => \"b\");\n",
        )
        .expect("write b");

        let workspace = WorkspaceInfo::default();
        let remote = RemoteEnv::default();
        let env_a = plugin_script_env(&workspace, remote.clone(), "a.plugin", "0.1.0", Vec::new());
        let env_b = plugin_script_env(&workspace, remote, "b.plugin", "0.1.0", Vec::new());
        let a_main = plugin_a.join("frontend.ts");
        let b_main = plugin_b.join("frontend.ts");
        let mut host_a = PluginHost::spawn_in(env_a, a_main.clone(), plugin_a.clone());
        let mut host_b = PluginHost::spawn_in(env_b, b_main.clone(), plugin_b);

        assert_eq!(host_a.evaluate_file(a_main), InitLoad::Loaded);
        assert_eq!(host_b.evaluate_file(b_main), InitLoad::Loaded);

        let mut state = PluginTestState::new(&base);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let (mut a_done, mut b_done) = (false, false);

        while std::time::Instant::now() < deadline && (!a_done || !b_done) {
            state.drain(&host_a, Owner::Plugin("a.plugin".into()));
            state.drain(&host_b, Owner::Plugin("b.plugin".into()));
            for event in drain_plugin_events(&host_a) {
                match event {
                    ScriptEvent::Evaluated => a_done = true,
                    ScriptEvent::Error { message } => panic!("{message}"),
                    _ => {}
                }
            }
            for event in drain_plugin_events(&host_b) {
                match event {
                    ScriptEvent::Evaluated => b_done = true,
                    ScriptEvent::Error { message } => panic!("{message}"),
                    _ => {}
                }
            }
            std::thread::yield_now();
        }

        assert!(a_done && b_done, "both plugins should evaluate");
        assert_eq!(
            state.commands.owner_of("a.one"),
            Some(&Owner::Plugin("a.plugin".into()))
        );
        assert_eq!(
            state.commands.owner_of("b.one"),
            Some(&Owner::Plugin("b.plugin".into()))
        );

        host_a.execute("a.one".to_string(), json!(null));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut result = None;
        while std::time::Instant::now() < deadline {
            state.drain(&host_a, Owner::Plugin("a.plugin".into()));
            if let Some(event) = drain_plugin_events(&host_a)
                .into_iter()
                .find(|event| matches!(event, ScriptEvent::CommandFinished { .. }))
            {
                result = match event {
                    ScriptEvent::CommandFinished { result, .. } => Some(result),
                    _ => None,
                };
                break;
            }
            std::thread::yield_now();
        }
        assert_eq!(result, Some(json!("a")));

        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(
            base.join("plugin-a/lib/value.ts"),
            "export const value: string = \"changed\";\n",
        )
        .expect("rewrite helper");
        assert!(host_a.changed());

        drop(host_a);
        drop(host_b);
        let _ = std::fs::remove_dir_all(&base);
    }

    struct PluginTestState {
        editor: Editor,
        config: Config,
        keymap: Keymap,
        commands: CommandRegistry,
        workspace: WorkspaceInfo,
        paths: Paths,
    }

    impl PluginTestState {
        fn new(base: &Path) -> Self {
            Self {
                editor: Editor::with_text("hello\n", 1),
                config: Config::with_defaults(),
                keymap: Keymap::new(std::time::Duration::from_millis(1000)),
                commands: CommandRegistry::new(),
                workspace: WorkspaceInfo::default(),
                paths: Paths::under(base),
            }
        }

        fn drain(&mut self, host: &PluginHost, owner: Owner) {
            while let Ok(request) = host.requests().try_recv() {
                let context = RequestContext {
                    editor: &mut self.editor,
                    config: &mut self.config,
                    keymap: &mut self.keymap,
                    commands: &mut self.commands,
                    document: None,
                    workspace: &self.workspace,
                    paths: &self.paths,
                    script: Some(host.script()),
                    env: host.env(),
                    owner: owner.clone(),
                    statusline: None,
                    gutter: None,
                    hover: None,
                };
                let _ = handle_request(context, request);
            }
        }
    }

    fn drain_plugin_events(host: &PluginHost) -> Vec<ScriptEvent> {
        let events = host.events();
        let mut drained = Vec::new();
        while let Ok(event) = events.try_recv() {
            drained.push(event);
        }
        drained
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
    fn views_open_triggers_ui_action() {
        let mut editor = Editor::new();
        let mut config = Config::with_defaults();
        let mut keymap = Keymap::new(std::time::Duration::from_millis(1000));
        let mut commands = CommandRegistry::new();
        let workspace = WorkspaceInfo::default();
        let paths = Paths::under("/tmp/vy-host");

        let (req, rec) = HostRequest::new(
            "views.open",
            json!({ "view": "example.tree", "direction": "right" }),
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
            HostAction::Ui { command, args } => {
                assert_eq!(command, UiCommand::ViewOpen);
                let args = args.expect("args");
                assert_eq!(
                    args.get("view").and_then(|value| value.as_str()),
                    Some("example.tree")
                );
                assert_eq!(
                    args.get("direction").and_then(|value| value.as_str()),
                    Some("right")
                );
            }
            _ => panic!("expected a UI action"),
        }
        assert!(matches!(
            rec.recv().expect("reply"),
            vy_js::HostReply::Ok(_)
        ));
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
            HostAction::ServerRpc { call, .. } => {
                assert_eq!(call.method, "fs/read_dir");
                let decoded: vy_proto::FsReadDirParams =
                    vy_proto::decode_value(&call.params).unwrap();
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
            HostAction::ServerRpc { call, .. } => {
                assert_eq!(call.method, "process/exec");
                let decoded: vy_proto::ProcessExecParams =
                    vy_proto::decode_value(&call.params).unwrap();
                assert_eq!(decoded.command, "git");
                assert_eq!(decoded.args, vec!["status"]);
            }
            other => panic!("expected ServerRpc action, got {other:?}"),
        }
    }
}
