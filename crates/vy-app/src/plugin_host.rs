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

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use vy_api_schema::{
    CommandsExecuteParams, CommandsRegisterParams, ConfigGetParams, ConfigInspectParams,
    ConfigInspectReply, ConfigSetParams, ConfigSetReply, DocumentGetTextParams, DocumentInfoReply,
    DocumentLineAtParams, DocumentOffsetAtParams, DocumentPositionAtParams, EditorEditParams,
    EditorEditReply, EditorSetModeParams, EditorSetSelectionsParams, GutterCreateParams,
    GutterDisposeParams, HostCall, HoverShowParams, KeyContextValue, KeymapBindParams,
    KeymapSetContextParams, KeymapUnbindParams, LogWriteParams, NotificationsShowParams,
    PickerShowParams, StatuslineCreateParams, StatuslineDisposeParams, StatuslineUpdateParams,
    ThemeColor, ViewsRenderParams,
};
use vy_capabilities::TrustedStore;
use vy_core::Uri;
use vy_js::{HostRequest, RemoteEnv, ScriptEnv, ScriptEvent, ScriptHost, WorkspaceEnv};
use vy_proto::ErrorCode;

use crate::commands::{
    BuiltinCommand, CommandArgs, CommandRegistry, DocumentCommand, Owner, PaneCommand, UiCommand,
};
use crate::config::Config;
use crate::editor::{Action, Editor, Transaction};
use crate::keymap::Keymap;
use crate::palette::PickerItem;
use crate::paths::Paths;
use crate::slots::{GutterItem, GutterRegistry, HoverState};
use crate::statusline::{Statusline, StatuslineItem};
use crate::when;
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

    /// Evaluates a JavaScript/TypeScript code snippet in the script context.
    pub fn eval(&self, source: impl Into<String>) {
        self.script.eval(source);
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
    Document(DocumentCommand),
    Ui(UiCommand),
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

/// A reply the shim receives now, or a request the view answers later.
enum Reply {
    Value(Value),
    Deferred,
}

impl Reply {
    fn value(value: impl Serialize) -> Result<Self, CallError> {
        Ok(Self::Value(serde_json::to_value(value)?))
    }

    fn null() -> Self {
        Self::Value(Value::Null)
    }
}

/// A failure that the shim receives as an exception with a stable code.
struct CallError {
    code: ErrorCode,
    message: String,
}

impl CallError {
    fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::InvalidParams,
            message: message.into(),
        }
    }

    fn invalid_state(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::InvalidState,
            message: message.into(),
        }
    }
}

impl From<serde_json::Error> for CallError {
    fn from(error: serde_json::Error) -> Self {
        Self::invalid_params(error.to_string())
    }
}

/// Deserializes the parameters of one method.
///
/// Missing parameters arrive as `null`; treating that as an empty object keeps
/// methods without parameters from needing a special case.
fn parse_params<T: DeserializeOwned>(params: &Value) -> Result<T, CallError> {
    let empty = Value::Object(serde_json::Map::new());
    let value = if params.is_null() { &empty } else { params };
    Ok(serde_json::from_value(value.clone())?)
}

/// Reads a field that distinguishes an absent value from `null`.
fn optional_text(value: Value) -> Result<Option<String>, CallError> {
    match value {
        Value::Null => Ok(None),
        Value::String(text) => Ok(Some(text)),
        other => Err(CallError::invalid_params(format!(
            "expected a string or null, found {other}"
        ))),
    }
}

/// Reads a color field that distinguishes an absent value from `null`.
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

/// Answers one call from the shim. The request is always answered so the script
/// thread never waits for the full reply timeout.
pub fn handle_request(context: RequestContext<'_>, request: HostRequest) -> HostOutcome {
    let mut outcome = HostOutcome::default();

    let Some(call) = HostCall::parse(&request.method) else {
        let code = ErrorCode::InvalidRequest.to_string();
        let message = format!("unknown host method: {}", request.method);
        request.respond_error(&code, message);
        return outcome;
    };

    let mut context = context;
    let result = dispatch(&mut context, call, &request.params, &mut outcome, &request);
    match result {
        Ok(Reply::Value(value)) => request.respond_value(value),
        Ok(Reply::Deferred) => {}
        Err(error) => request.respond_error(&error.code.to_string(), error.message),
    }

    outcome
}

fn dispatch(
    context: &mut RequestContext<'_>,
    call: HostCall,
    params: &Value,
    outcome: &mut HostOutcome,
    request: &HostRequest,
) -> Result<Reply, CallError> {
    match call {
        HostCall::ConfigGet => {
            let params: ConfigGetParams = parse_params(params)?;
            let value = context
                .config
                .get(&params.key)
                .cloned()
                .unwrap_or(Value::Null);
            Reply::value(value)
        }
        HostCall::ConfigSet => {
            let params: ConfigSetParams = parse_params(params)?;
            let changed = context.config.apply(&params.values);
            outcome.config_changed = changed.clone();
            Reply::value(ConfigSetReply { changed })
        }
        HostCall::ConfigInspect => {
            let params: ConfigInspectParams = parse_params(params)?;
            let inspect = context.config.inspect(&params.key);
            Reply::value(ConfigInspectReply {
                default: inspect.default,
                user: inspect.user,
                project: inspect.project,
                memory: inspect.memory,
                effective: inspect.effective,
            })
        }
        HostCall::CommandsRegisterCommand => {
            let params: CommandsRegisterParams = parse_params(params)?;
            context
                .commands
                .register(params.id, Owner::Init)
                .map_err(|error| CallError::invalid_state(error.to_string()))?;
            Ok(Reply::null())
        }
        HostCall::CommandsExecute => {
            let params: CommandsExecuteParams = parse_params(params)?;
            let id = params.id;
            if let Some(command) = UiCommand::parse(&id) {
                outcome.action = HostAction::Ui(command);
            } else if let Some(command) = PaneCommand::parse(&id) {
                outcome.action = HostAction::Pane(command);
            } else if let Some(command) = DocumentCommand::parse(&id) {
                outcome.action = HostAction::Document(command);
            } else if let Some(command) = BuiltinCommand::parse(&id) {
                let args = command_args(&params.args);
                outcome.action = context.editor.execute(command, args).into();
            } else if context.commands.is_registered(&id) {
                if let Some(script) = context.script {
                    script.execute(id.clone(), params.args);
                }
            } else {
                return Err(CallError::invalid_state(format!("unknown command: {id}")));
            }
            Ok(Reply::null())
        }
        HostCall::CommandsGetCommands => {
            let mut ids: Vec<String> = BuiltinCommand::all()
                .iter()
                .map(|command| command.id().to_string())
                .chain(
                    PaneCommand::all()
                        .iter()
                        .map(|command| command.id().to_string()),
                )
                .chain(
                    DocumentCommand::all()
                        .iter()
                        .map(|command| command.id().to_string()),
                )
                .chain(
                    UiCommand::all()
                        .iter()
                        .map(|command| command.id().to_string()),
                )
                .chain(context.commands.script_commands())
                .collect();
            ids.sort();
            Reply::value(ids)
        }
        HostCall::KeymapBind => {
            let params: KeymapBindParams = parse_params(params)?;
            context
                .keymap
                .bind(
                    &params.keys,
                    params.command,
                    params.when.as_deref(),
                    Owner::Init,
                )
                .map_err(|error| CallError::invalid_state(error.to_string()))?;
            Ok(Reply::null())
        }
        HostCall::KeymapUnbind => {
            let params: KeymapUnbindParams = parse_params(params)?;
            let count = context
                .keymap
                .unbind(&params.keys)
                .map_err(|error| CallError::invalid_state(error.to_string()))?;
            Reply::value(count)
        }
        HostCall::KeymapSetContext => {
            let params: KeymapSetContextParams = parse_params(params)?;
            context
                .keymap
                .set_context(params.key, context_value(params.value));
            Ok(Reply::null())
        }
        HostCall::EditorActiveDocument => match context.document.as_ref() {
            Some(document) => Reply::value(document_info(context.editor, document)),
            None => Ok(Reply::null()),
        },
        HostCall::EditorDocuments | HostCall::WorkspaceTextDocuments => {
            let documents: Vec<DocumentInfoReply> = context
                .document
                .iter()
                .map(|document| document_info(context.editor, document))
                .collect();
            Reply::value(documents)
        }
        HostCall::EditorMode => Reply::value(context.editor.mode()),
        HostCall::EditorSetMode => {
            let params: EditorSetModeParams = parse_params(params)?;
            context.editor.set_mode(params.mode);
            Ok(Reply::null())
        }
        HostCall::EditorSelections => Reply::value(context.editor.selections()),
        HostCall::EditorSetSelections => {
            let params: EditorSetSelectionsParams = parse_params(params)?;
            context.editor.set_selections(&params.selections);
            Ok(Reply::null())
        }
        HostCall::EditorEdit => {
            let params: EditorEditParams = parse_params(params)?;
            if params.edits.is_empty() {
                // Nothing to apply, but the caller still expects the version.
                return Reply::value(EditorEditReply {
                    version: context.editor.version(),
                });
            }

            match context.editor.apply_edits(params.edits) {
                Ok(transaction) => {
                    let version = context.editor.version();
                    outcome.action = HostAction::Edit(transaction);
                    Reply::value(EditorEditReply { version })
                }
                Err(error) => Err(CallError::invalid_state(error.to_string())),
            }
        }
        HostCall::EditorUndo => {
            outcome.action = HostAction::Undo;
            Ok(Reply::null())
        }
        HostCall::EditorRedo => {
            outcome.action = HostAction::Redo;
            Ok(Reply::null())
        }
        HostCall::DocumentGetText => {
            let params: DocumentGetTextParams = parse_params(params)?;
            let text = match params.range {
                Some(range) => {
                    let start = context.editor.utf16_offset(range.start);
                    let end = context.editor.utf16_offset(range.end).max(start);
                    context.editor.text_utf16(start..end)
                }
                None => context.editor.text(),
            };
            Reply::value(text)
        }
        HostCall::DocumentLineAt => {
            let params: DocumentLineAtParams = parse_params(params)?;
            match context.editor.line(params.line) {
                Some(line) => Reply::value(line),
                None => Ok(Reply::null()),
            }
        }
        HostCall::DocumentPositionAt => {
            let params: DocumentPositionAtParams = parse_params(params)?;
            Reply::value(context.editor.position_at_utf16(params.offset))
        }
        HostCall::DocumentOffsetAt => {
            let params: DocumentOffsetAtParams = parse_params(params)?;
            Reply::value(context.editor.utf16_offset(params.position))
        }
        HostCall::NotificationsShow => {
            let params: NotificationsShowParams = parse_params(params)?;
            outcome.status = Some(params.message);
            Ok(Reply::null())
        }
        HostCall::LogWrite => {
            let params: LogWriteParams = parse_params(params)?;
            append_log(
                &context.paths.logs_dir().join("init.log"),
                params.level.as_str(),
                &params.channel,
                &params.message,
                &params.data,
            );
            Ok(Reply::null())
        }
        HostCall::UiStatuslineCreateItem => {
            let params: StatuslineCreateParams = parse_params(params)?;
            if let Some(statusline) = context.statusline.as_deref_mut() {
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
            Ok(Reply::null())
        }
        HostCall::UiStatuslineUpdateItem => {
            let params: StatuslineUpdateParams = parse_params(params)?;
            let color = params.color.map(optional_theme_color).transpose()?;
            let command = params.command.map(optional_text).transpose()?;
            if let Some(statusline) = context.statusline.as_deref_mut() {
                statusline.update_item(&params.id, params.text, color, command, params.visible);
            }
            Ok(Reply::null())
        }
        HostCall::UiStatuslineDisposeItem => {
            let params: StatuslineDisposeParams = parse_params(params)?;
            if let Some(statusline) = context.statusline.as_deref_mut() {
                statusline.dispose_item(&params.id);
            }
            Ok(Reply::null())
        }
        HostCall::ViewsRender => {
            let params: ViewsRenderParams = parse_params(params)?;
            outcome.action = HostAction::RenderWidgetTree {
                view_id: params.id,
                tree: params.tree,
            };
            Ok(Reply::null())
        }
        HostCall::UiPickerShow => {
            let params: PickerShowParams = parse_params(params)?;
            outcome.action = HostAction::ShowPicker {
                id: params.id,
                items: params.items.unwrap_or_default(),
                placeholder: params.placeholder,
                request: request.clone(),
            };
            Ok(Reply::Deferred)
        }
        HostCall::UiGutterCreateItem => {
            let params: GutterCreateParams = parse_params(params)?;
            if let Some(gutter) = context.gutter.as_deref_mut() {
                let mut item = GutterItem::new(params.id, params.line, params.text);
                item.color = params.color;
                gutter.set_item(item);
            }
            Ok(Reply::null())
        }
        HostCall::UiGutterDisposeItem => {
            let params: GutterDisposeParams = parse_params(params)?;
            if let Some(gutter) = context.gutter.as_deref_mut() {
                gutter.dispose_item(&params.id);
            }
            Ok(Reply::null())
        }
        HostCall::UiGutterClear => {
            if let Some(gutter) = context.gutter.as_deref_mut() {
                gutter.clear();
            }
            Ok(Reply::null())
        }
        HostCall::UiHoverShow => {
            let params: HoverShowParams = parse_params(params)?;
            if let Some(hover) = context.hover.as_deref_mut() {
                *hover = Some(HoverState::new(params.line, params.character, params.text));
            }
            Ok(Reply::null())
        }
        HostCall::UiHoverHide => {
            if let Some(hover) = context.hover.as_deref_mut() {
                *hover = None;
            }
            Ok(Reply::null())
        }
    }
}

fn command_args(args: &Value) -> CommandArgs {
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

fn context_value(value: KeyContextValue) -> when::Value {
    match value {
        KeyContextValue::Bool(value) => when::Value::Bool(value),
        KeyContextValue::Text(value) => when::Value::Text(value),
        KeyContextValue::Number(value) => when::Value::Number(value),
    }
}

fn document_info(editor: &Editor, document: &DocumentInfo) -> DocumentInfoReply {
    DocumentInfoReply {
        id: document.id,
        uri: document.uri.clone(),
        language_id: document.language_id.clone(),
        version: editor.version(),
        line_count: editor.line_count(),
        end: editor.position_at_utf16(editor.len_utf16()),
    }
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
    use crate::statusline::StatuslineAlignment;
    use serde_json::json;

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
                assert_eq!(id, "test_picker");
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
}
