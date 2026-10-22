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

use serde_json::{json, Value};
use vy_capabilities::TrustedStore;
use vy_core::{Position, Range, Selection, TextEdit};
use vy_js::{HostRequest, RemoteEnv, ScriptEnv, ScriptEvent, ScriptHost, WorkspaceEnv};

use crate::commands::{BuiltinCommand, CommandArgs, CommandRegistry, Owner, PaneCommand};
use crate::config::Config;
use crate::editor::{Action, Editor, Mode, Transaction};
use crate::keymap::Keymap;
use crate::paths::Paths;
use crate::when;

/// Everything the shim knows about the open document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentInfo {
    pub id: u64,
    pub uri: String,
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
        Self {
            script: ScriptHost::spawn(env),
            init_path,
            stamp,
        }
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

    /// Whether `init.ts` was added, removed, or edited since the last load.
    pub fn changed(&self) -> bool {
        file_stamp(&self.init_path) != self.stamp
    }

    /// Runs a command handler registered by the script.
    pub fn execute(&self, id: String, args: Value) {
        self.script.execute(id, args);
    }

    /// Tells the script which settings keys changed.
    pub fn emit_config_change(&self, keys: Vec<String>) {
        self.script.emit_config_change(keys);
    }

    pub fn is_running(&self) -> bool {
        self.script.is_running()
    }
}

/// Builds the constants the shim exposes. `init.ts` is always trusted.
pub fn script_env(workspace: &WorkspaceInfo, paths: &Paths) -> (ScriptEnv, Vec<String>) {
    let (store, warning) = TrustedStore::load(&paths.trusted_json());
    // Phase 2 has no manifests; reading the store here keeps a broken file from
    // going unnoticed and leaves the approval path in place for Phase 3.
    let _ = store;

    let env = ScriptEnv {
        plugin_id: "init".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        workspace: WorkspaceEnv {
            root_uri: workspace.root_uri.clone(),
            name: workspace.name.clone(),
            folders: workspace.folders.clone(),
            trusted: true,
        },
        capabilities: Vec::new(),
        remote: RemoteEnv {
            is_remote: false,
            name: "local".to_string(),
            os: std::env::consts::OS.to_string(),
            path_separator: std::path::MAIN_SEPARATOR.to_string(),
        },
    };

    (env, warning.into_iter().collect())
}

/// What the view must do after a request was answered.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum HostAction {
    #[default]
    None,
    Edit(Transaction),
    Undo,
    Redo,
    Pane(PaneCommand),
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
    let method = request.method.clone();
    let params = request.params.clone();

    match method.as_str() {
        "config.get" => {
            let key = text_param(&params, "key").unwrap_or_default();
            let value = context.config.get(&key).cloned().unwrap_or(Value::Null);
            request.respond_value(value);
        }
        "config.set" => {
            let values = params
                .get("values")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            outcome.config_changed = context.config.apply(&values);
            request.respond_value(json!({ "changed": outcome.config_changed }));
        }
        "config.inspect" => {
            let key = text_param(&params, "key").unwrap_or_default();
            let inspect = serde_json::to_value(context.config.inspect(&key)).unwrap_or(Value::Null);
            request.respond_value(inspect);
        }
        "commands.registerCommand" => {
            let id = text_param(&params, "id").unwrap_or_default();
            match context.commands.register(id, Owner::Init) {
                Ok(()) => request.respond_value(Value::Null),
                Err(error) => request.respond_error("InvalidState", error.to_string()),
            }
        }
        "commands.execute" => {
            let id = text_param(&params, "id").unwrap_or_default();
            let args = command_args(&params);
            if let Some(command) = PaneCommand::parse(&id) {
                outcome.action = HostAction::Pane(command);
                request.respond_value(Value::Null);
            } else if let Some(command) = BuiltinCommand::parse(&id) {
                outcome.action = context.editor.execute(command, args).into();
                request.respond_value(Value::Null);
            } else if context.commands.is_registered(&id) {
                if let Some(script) = context.script {
                    script.execute(
                        id.clone(),
                        params.get("args").cloned().unwrap_or(Value::Null),
                    );
                }
                request.respond_value(Value::Null);
            } else {
                request.respond_error("InvalidState", format!("unknown command: {id}"));
            }
        }
        "commands.getCommands" => {
            let mut ids: Vec<String> = BuiltinCommand::all()
                .iter()
                .map(|command| command.id().to_string())
                .chain(
                    PaneCommand::all()
                        .iter()
                        .map(|command| command.id().to_string()),
                )
                .chain(context.commands.script_commands())
                .collect();
            ids.sort();
            request.respond_value(json!(ids));
        }
        "keymap.bind" => {
            let keys = text_param(&params, "keys").unwrap_or_default();
            let command = text_param(&params, "command").unwrap_or_default();
            let when = text_param(&params, "when");
            match context
                .keymap
                .bind(&keys, command, when.as_deref(), Owner::Init)
            {
                Ok(()) => request.respond_value(Value::Null),
                Err(error) => request.respond_error("InvalidState", error.to_string()),
            }
        }
        "keymap.unbind" => {
            let keys = text_param(&params, "keys").unwrap_or_default();
            match context.keymap.unbind(&keys) {
                Ok(count) => request.respond_value(json!(count)),
                Err(error) => request.respond_error("InvalidState", error.to_string()),
            }
        }
        "keymap.setContext" => {
            let key = text_param(&params, "key").unwrap_or_default();
            match context_value(params.get("value")) {
                Some(value) => {
                    context.keymap.set_context(key, value);
                    request.respond_value(Value::Null);
                }
                None => request.respond_error(
                    "InvalidState",
                    "keymap.setContext accepts a boolean, string, or number",
                ),
            }
        }
        "editor.activeDocument" => {
            let value = context
                .document
                .as_ref()
                .map(|document| document_json(context.editor, document))
                .unwrap_or(Value::Null);
            request.respond_value(value);
        }
        "editor.documents" => {
            let value = context
                .document
                .as_ref()
                .map(|document| json!([document_json(context.editor, document)]))
                .unwrap_or_else(|| json!([]));
            request.respond_value(value);
        }
        "editor.mode" => {
            request.respond_value(json!(context.editor.mode().name()));
        }
        "editor.setMode" => {
            let name = text_param(&params, "mode").unwrap_or_default();
            match Mode::parse(&name) {
                Some(mode) => {
                    context.editor.set_mode(mode);
                    request.respond_value(Value::Null);
                }
                None => request.respond_error("InvalidState", format!("unknown mode: {name}")),
            }
        }
        "editor.selections" => {
            request.respond_value(json!(context.editor.selections()));
        }
        "editor.setSelections" => {
            let selections: Vec<Selection> = params
                .get("selections")
                .and_then(|value| serde_json::from_value(value.clone()).ok())
                .unwrap_or_default();
            context.editor.set_selections(&selections);
            request.respond_value(Value::Null);
        }
        "editor.edit" => {
            let edits: Vec<TextEdit> = match params
                .get("edits")
                .cloned()
                .map(serde_json::from_value)
                .transpose()
            {
                Ok(edits) => edits.unwrap_or_default(),
                Err(error) => {
                    request.respond_error("InvalidState", format!("invalid edits: {error}"));
                    return outcome;
                }
            };

            if edits.is_empty() {
                // Nothing to apply, but the caller still expects the version.
                request.respond_value(json!({ "version": context.editor.version() }));
                return outcome;
            }

            match context.editor.apply_edits(edits) {
                Ok(transaction) => {
                    let version = context.editor.version();
                    outcome.action = HostAction::Edit(transaction);
                    request.respond_value(json!({ "version": version }));
                }
                Err(error) => request.respond_error("InvalidState", error.to_string()),
            }
        }
        "editor.undo" => {
            outcome.action = HostAction::Undo;
            request.respond_value(Value::Null);
        }
        "editor.redo" => {
            outcome.action = HostAction::Redo;
            request.respond_value(Value::Null);
        }
        "workspace.textDocuments" => {
            let value = context
                .document
                .as_ref()
                .map(|document| json!([document_json(context.editor, document)]))
                .unwrap_or_else(|| json!([]));
            request.respond_value(value);
        }
        "document.getText" => {
            let range: Option<Range> = params
                .get("range")
                .and_then(|value| serde_json::from_value(value.clone()).ok());
            let text = match range {
                Some(range) => {
                    let start = context.editor.utf16_offset(range.start);
                    let end = context.editor.utf16_offset(range.end).max(start);
                    context.editor.text_utf16(start..end)
                }
                None => context.editor.text(),
            };
            request.respond_value(json!(text));
        }
        "document.lineAt" => {
            let line = params.get("line").and_then(Value::as_u64).unwrap_or(0);
            let value = context
                .editor
                .line(line as usize)
                .map(|line| json!(line))
                .unwrap_or(Value::Null);
            request.respond_value(value);
        }
        "document.positionAt" => {
            let offset = params.get("offset").and_then(Value::as_u64).unwrap_or(0);
            let position = context.editor.position_at_utf16(offset as usize);
            request.respond_value(json!(position));
        }
        "document.offsetAt" => {
            let position: Position = params
                .get("position")
                .and_then(|value| serde_json::from_value(value.clone()).ok())
                .unwrap_or(Position::new(0, 0));
            request.respond_value(json!(context.editor.utf16_offset(position)));
        }
        "notifications.show" => {
            let message = text_param(&params, "message").unwrap_or_default();
            outcome.status = Some(message);
            request.respond_value(Value::Null);
        }
        "log.write" => {
            let channel = text_param(&params, "channel").unwrap_or_else(|| "init".to_string());
            let level = text_param(&params, "level").unwrap_or_else(|| "info".to_string());
            let message = text_param(&params, "message").unwrap_or_default();
            let data = params.get("data").cloned().unwrap_or(Value::Null);
            append_log(
                &context.paths.logs_dir().join("init.log"),
                &level,
                &channel,
                &message,
                &data,
            );
            request.respond_value(Value::Null);
        }
        other => {
            request.respond_error("InvalidState", format!("unknown host method: {other}"));
        }
    }

    outcome
}

fn text_param(params: &Value, key: &str) -> Option<String> {
    params.get(key).and_then(Value::as_str).map(str::to_string)
}

fn command_args(params: &Value) -> CommandArgs {
    let args = params.get("args").unwrap_or(&Value::Null);
    CommandArgs {
        count: args
            .get("count")
            .and_then(Value::as_u64)
            .map(|count| count as u32),
        register: args
            .get("register")
            .and_then(Value::as_str)
            .and_then(|value| value.chars().next()),
    }
}

fn context_value(value: Option<&Value>) -> Option<when::Value> {
    match value? {
        Value::Bool(value) => Some(when::Value::Bool(*value)),
        Value::String(value) => Some(when::Value::Text(value.clone())),
        Value::Number(value) => value.as_f64().map(when::Value::Number),
        _ => None,
    }
}

fn document_json(editor: &Editor, document: &DocumentInfo) -> Value {
    let end = editor.position_at_utf16(editor.len_utf16());
    json!({
        "id": document.id,
        "uri": document.uri,
        "languageId": document.language_id,
        "version": editor.version(),
        "lineCount": editor.line_count(),
        "end": end,
    })
}

fn append_log(path: &Path, level: &str, channel: &str, message: &str, data: &Value) {
    let line = json!({
        "level": level,
        "channel": channel,
        "message": message,
        "data": data,
    })
    .to_string();

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        use std::io::Write;
        let _ = writeln!(file, "{line}");
    }
    eprintln!("vy: [{channel}] {message}");
}

fn file_stamp(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::KeyContext;

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
                uri: "file:///tmp/a.txt".to_string(),
                language_id: "text".to_string(),
            }),
            workspace,
            paths,
            script: None,
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

config.set({ "ui.fontSize": 15 });
commands.registerCommand("user.hello", async () => {
  await notifications.show("hello from init.ts");
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
        assert!(matches!(reply, vy_js::HostReply::Err { code, .. } if code == "InvalidState"));
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
}
