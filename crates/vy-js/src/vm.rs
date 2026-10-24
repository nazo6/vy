//! The dedicated JavaScript thread.
//!
//! `init.ts` runs on its own thread with one QuickJS runtime and one context.
//! The UI never blocks on it: tasks go in, events and host requests come out on
//! separate channels. A host request blocks the JavaScript thread until the UI
//! answers, which is safe because the UI always answers through its task.
//!
//! See `docs/plugin-system.md` §2.2 and `docs/architecture.md`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rquickjs::{CaughtError, Context, Ctx, Exception, Function, Module, Runtime, Value as JsValue};
use serde::Serialize;
use vy_core::Uri;

use crate::api::SHIM_SOURCE;
use crate::modules::{ModuleFiles, ModuleLoader};
use crate::transpile::strip_types;

/// Time given to one evaluation of `init.ts`.
pub const EVALUATION_TIMEOUT: Duration = Duration::from_secs(1);
/// Time given to one command handler.
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
/// Time the UI has to answer a host request.
pub const REPLY_TIMEOUT: Duration = Duration::from_secs(5);
/// Upper bound on memory held by the QuickJS runtime.
pub const MEMORY_LIMIT: usize = 64 * 1024 * 1024;

/// Information the shim exposes as constants (`workspace.rootUri` and friends).
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptEnv {
    pub plugin_id: String,
    pub version: String,
    pub workspace: WorkspaceEnv,
    pub capabilities: Vec<String>,
    pub remote: RemoteEnv,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEnv {
    pub root_uri: Option<Uri>,
    pub name: Option<String>,
    pub folders: Vec<Uri>,
    pub trusted: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEnv {
    pub is_remote: bool,
    pub name: String,
    pub os: String,
    pub path_separator: String,
}

/// A task for the JavaScript thread.
#[derive(Debug)]
pub enum ScriptTask {
    /// Transpiles and runs a module. A missing file is the caller's concern.
    Evaluate {
        path: PathBuf,
        source: String,
    },
    /// Runs a command handler registered by the module.
    Execute {
        id: String,
        args: serde_json::Value,
    },
    /// Tells the script that an event happened.
    Emit {
        topic: String,
        payload: serde_json::Value,
    },
    /// Evaluates a JavaScript/TypeScript expression or script in the global context.
    Eval {
        source: String,
    },
    Shutdown,
}

/// Something the JavaScript thread reports to the UI.
#[derive(Debug, Clone, PartialEq)]
pub enum ScriptEvent {
    /// The module finished evaluating.
    Evaluated,
    /// A command handler returned with an optional result.
    CommandFinished {
        id: String,
        result: serde_json::Value,
    },
    /// The module or a handler failed. The message is in English.
    Error { message: String },
    /// An evaluated expression produced a formatted string result.
    EvalFinished { result: String },
    /// The thread exited.
    Stopped,
}

/// A call from the shim to the UI.
#[derive(Debug, Clone)]
pub struct HostRequest {
    pub method: String,
    pub params: serde_json::Value,
    reply: Arc<Mutex<Option<mpsc::Sender<HostReply>>>>,
}

impl PartialEq for HostRequest {
    fn eq(&self, other: &Self) -> bool {
        self.method == other.method && self.params == other.params
    }
}

impl Eq for HostRequest {}

impl HostRequest {
    /// Creates a request and the channel its reply arrives on. The UI owns the
    /// returned request; tests use this to answer without a script thread.
    pub fn new(
        method: impl Into<String>,
        params: serde_json::Value,
    ) -> (Self, mpsc::Receiver<HostReply>) {
        let (reply, receiver) = mpsc::channel();
        (
            Self {
                method: method.into(),
                params,
                reply: Arc::new(Mutex::new(Some(reply))),
            },
            receiver,
        )
    }

    /// Sends the result and unblocks the JavaScript thread.
    pub fn respond(self, reply: HostReply) {
        if let Ok(mut lock) = self.reply.lock()
            && let Some(sender) = lock.take()
        {
            let _ = sender.send(reply);
        }
    }

    pub fn respond_value(self, value: serde_json::Value) {
        self.respond(HostReply::Ok(value));
    }

    pub fn respond_error(self, code: &str, message: impl Into<String>) {
        self.respond(HostReply::Err {
            code: code.to_string(),
            message: message.into(),
        });
    }
}

/// The UI's answer to a [`HostRequest`].
#[derive(Debug, Clone)]
pub enum HostReply {
    Ok(serde_json::Value),
    Err { code: String, message: String },
}

/// Handle for the JavaScript thread.
pub struct ScriptHost {
    tasks: async_channel::Sender<ScriptTask>,
    events: async_channel::Receiver<ScriptEvent>,
    requests: async_channel::Receiver<HostRequest>,
    thread: Option<std::thread::JoinHandle<()>>,
    files: ModuleFiles,
}

impl ScriptHost {
    /// Starts the thread and installs the shim. The runtime is ready before this
    /// returns; evaluation waits for [`ScriptHost::evaluate`]. `root` bounds the
    /// files that relative imports may load.
    pub fn spawn(env: ScriptEnv, root: impl Into<PathBuf>) -> Self {
        let (task_sender, task_receiver) = async_channel::unbounded();
        let (event_sender, event_receiver) = async_channel::unbounded();
        let (request_sender, request_receiver) = async_channel::unbounded();
        let env_json = serde_json::to_string(&env).unwrap_or_else(|_| "{}".to_string());
        let root = root.into();
        let files = ModuleFiles::default();
        let thread_files = files.clone();

        let thread = std::thread::Builder::new()
            .name("vy-js".to_string())
            .spawn(move || {
                run(
                    env_json,
                    root,
                    thread_files,
                    task_receiver,
                    event_sender,
                    request_sender,
                )
            })
            .ok();

        Self {
            tasks: task_sender,
            events: event_receiver,
            requests: request_receiver,
            thread,
            files,
        }
    }

    pub fn events(&self) -> async_channel::Receiver<ScriptEvent> {
        self.events.clone()
    }

    pub fn requests(&self) -> async_channel::Receiver<HostRequest> {
        self.requests.clone()
    }

    /// Files loaded by this runtime, including the entry module.
    pub fn loaded_files(&self) -> Vec<PathBuf> {
        self.files.snapshot()
    }

    /// Queues an evaluation. Errors arrive as [`ScriptEvent::Error`].
    pub fn evaluate(&self, path: impl Into<PathBuf>, source: impl Into<String>) {
        let path = path.into();
        let tracked = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        self.files.insert(tracked);
        let _ = self.tasks.send_blocking(ScriptTask::Evaluate {
            path,
            source: source.into(),
        });
    }

    /// Queues a command handler registered by the script.
    pub fn execute(&self, id: impl Into<String>, args: serde_json::Value) {
        let _ = self.tasks.send_blocking(ScriptTask::Execute {
            id: id.into(),
            args,
        });
    }

    /// Emits an event to the script.
    fn emit(&self, topic: impl Into<String>, payload: serde_json::Value) {
        let _ = self.tasks.send_blocking(ScriptTask::Emit {
            topic: topic.into(),
            payload,
        });
    }

    /// Emits one typed event. The marker fixes both the topic and payload type.
    pub fn emit_event<E: vy_host_api::HostEvent>(&self, payload: &E::Payload) {
        match serde_json::to_value(payload) {
            Ok(payload) => self.emit(E::TOPIC, payload),
            Err(error) => {
                eprintln!("vy: could not serialize event {}: {error}", E::TOPIC);
            }
        }
    }

    /// Evaluates a JavaScript/TypeScript code snippet in the script context.
    pub fn eval(&self, source: impl Into<String>) {
        let _ = self.tasks.send_blocking(ScriptTask::Eval {
            source: source.into(),
        });
    }

    /// Whether the thread is still running.
    pub fn is_running(&self) -> bool {
        self.thread
            .as_ref()
            .is_some_and(|thread| !thread.is_finished())
    }
}

impl vy_host_api::EventTransport for ScriptHost {
    fn emit_event<E: vy_host_api::HostEvent>(&self, payload: &E::Payload) {
        ScriptHost::emit_event::<E>(self, payload);
    }
}

impl Drop for ScriptHost {
    fn drop(&mut self) {
        let _ = self.tasks.send_blocking(ScriptTask::Shutdown);
        self.tasks.close();
        // The thread exits on its own. Joining here would block the UI while a
        // script is still inside a host call.
    }
}

/// Runs the thread until the task channel closes.
fn run(
    env_json: String,
    root: PathBuf,
    files: ModuleFiles,
    tasks: async_channel::Receiver<ScriptTask>,
    events: async_channel::Sender<ScriptEvent>,
    requests: async_channel::Sender<HostRequest>,
) {
    let runtime = match Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = events.send_blocking(ScriptEvent::Error {
                message: format!("could not create the QuickJS runtime: {error}"),
            });
            return;
        }
    };

    runtime.set_memory_limit(MEMORY_LIMIT);

    let loader = ModuleLoader::new(root, files);
    runtime.set_loader(loader.clone(), loader);

    let deadline = Arc::new(AtomicU64::new(0));
    let interrupt_deadline = deadline.clone();
    runtime.set_interrupt_handler(Some(Box::new(move || {
        let until = interrupt_deadline.load(Ordering::Relaxed);
        until != 0 && now_millis() > until
    })));

    let context = match Context::full(&runtime) {
        Ok(context) => context,
        Err(error) => {
            let _ = events.send_blocking(ScriptEvent::Error {
                message: format!("could not create the QuickJS context: {error}"),
            });
            return;
        }
    };

    let setup = context.with(|ctx| -> rquickjs::Result<()> {
        let env = ctx.json_parse(env_json)?;
        ctx.globals().set("__vy_env", env)?;

        let bridge = Bridge { requests };
        let call = Function::new(
            ctx.clone(),
            move |ctx: Ctx<'_>,
                  method: String,
                  params: Option<String>|
                  -> rquickjs::Result<String> { bridge.call(&ctx, method, params) },
        )?;
        ctx.globals().set("__vy_host_call", call)?;

        // Expose `vy` on globalThis. The shim has no imports and assigns the
        // global itself.
        let promise = Module::evaluate(ctx.clone(), "<vy-global>", SHIM_SOURCE)?;
        promise.finish::<()>()?;
        drain_jobs(&ctx);

        Ok(())
    });

    if let Err(error) = setup {
        let _ = events.send_blocking(ScriptEvent::Error {
            message: format!("could not start the JavaScript host: {error}"),
        });
        return;
    }

    while let Ok(task) = tasks.recv_blocking() {
        let shutdown = matches!(task, ScriptTask::Shutdown);
        if shutdown {
            break;
        }
        run_task(&context, &deadline, &events, task);
    }

    let _ = events.send_blocking(ScriptEvent::Stopped);
}

fn run_task(
    context: &Context,
    deadline: &Arc<AtomicU64>,
    events: &async_channel::Sender<ScriptEvent>,
    task: ScriptTask,
) {
    context.with(|ctx| match task {
        ScriptTask::Shutdown => {}
        ScriptTask::Evaluate { path, source } => {
            let code = match strip_types(&path, &source) {
                Ok(code) => code,
                Err(error) => {
                    let _ = events.send_blocking(ScriptEvent::Error {
                        message: format!("{}: {error}", path.display()),
                    });
                    return;
                }
            };

            let module_path = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            let name = module_path.to_string_lossy().into_owned();
            let result = with_deadline(deadline, EVALUATION_TIMEOUT, || {
                let promise = Module::evaluate(ctx.clone(), name.as_str(), code)?;
                promise.finish::<()>()?;
                drain_jobs(&ctx);
                Ok(())
            });

            match result {
                Ok(()) => {
                    let _ = events.send_blocking(ScriptEvent::Evaluated);
                }
                Err(error) => {
                    let _ = events.send_blocking(ScriptEvent::Error {
                        message: describe_error(&ctx, error),
                    });
                }
            }
        }
        ScriptTask::Execute { id, args } => {
            let args_json = serde_json::to_string(&args).unwrap_or_else(|_| "null".to_string());
            let result: rquickjs::Result<serde_json::Value> =
                with_deadline(deadline, COMMAND_TIMEOUT, || {
                    let function: Function = ctx.globals().get("__vy_run_command")?;
                    let value: JsValue = function.call((id.clone(), args_json))?;
                    let resolved = if let Some(promise) = value.as_promise() {
                        let inner: JsValue = promise.finish()?;
                        inner
                    } else {
                        value
                    };
                    drain_jobs(&ctx);
                    let json_str: String = ctx
                        .json_stringify(resolved)?
                        .and_then(|val| val.to_string().ok())
                        .unwrap_or_else(|| "null".to_string());
                    let json_val =
                        serde_json::from_str(&json_str).unwrap_or(serde_json::Value::Null);
                    Ok(json_val)
                });

            match result {
                Ok(result) => {
                    let _ = events.send_blocking(ScriptEvent::CommandFinished { id, result });
                }
                Err(error) => {
                    let _ = events.send_blocking(ScriptEvent::Error {
                        message: format!("command {id} failed: {}", describe_error(&ctx, error)),
                    });
                }
            }
        }
        ScriptTask::Emit { topic, payload } => {
            let json = serde_json::to_string(&payload).unwrap_or_else(|_| "null".to_string());
            if let Err(error) = call_global_emit(&ctx, deadline, topic, json) {
                let _ = events.send_blocking(ScriptEvent::Error {
                    message: describe_error(&ctx, error),
                });
            }
        }
        ScriptTask::Eval { source } => {
            let code = match strip_types(&PathBuf::from("<eval>.ts"), &source) {
                Ok(code) => code,
                Err(error) => {
                    let _ = events.send_blocking(ScriptEvent::Error {
                        message: format!("{error}"),
                    });
                    return;
                }
            };

            let result: rquickjs::Result<String> = with_deadline(deadline, COMMAND_TIMEOUT, || {
                let promise = ctx.eval_promise(code)?;
                let resolved: JsValue = promise.finish()?;
                drain_jobs(&ctx);
                let final_val = if let Some(inner_promise) = resolved.as_promise() {
                    inner_promise.finish()?
                } else if let Some(obj) = resolved.as_object() {
                    if let Ok(val) = obj.get::<_, JsValue>("value") {
                        val
                    } else {
                        resolved
                    }
                } else {
                    resolved
                };
                format_js_value(&ctx, final_val)
            });

            match result {
                Ok(result) => {
                    let _ = events.send_blocking(ScriptEvent::EvalFinished { result });
                }
                Err(error) => {
                    let _ = events.send_blocking(ScriptEvent::Error {
                        message: describe_error(&ctx, error),
                    });
                }
            }
        }
    });
}

/// Resets the interrupt deadline around `body` so a runaway script stops.
fn with_deadline<T>(
    deadline: &Arc<AtomicU64>,
    timeout: Duration,
    body: impl FnOnce() -> rquickjs::Result<T>,
) -> rquickjs::Result<T> {
    deadline.store(
        now_millis().saturating_add(timeout.as_millis() as u64),
        Ordering::Relaxed,
    );
    let result = body();
    deadline.store(0, Ordering::Relaxed);
    result
}

fn call_global_emit(
    ctx: &Ctx<'_>,
    deadline: &Arc<AtomicU64>,
    topic: String,
    json: String,
) -> rquickjs::Result<()> {
    with_deadline(deadline, EVALUATION_TIMEOUT, || {
        let function: Function = ctx.globals().get("__vy_emit")?;
        let value: JsValue = function.call((topic, json))?;
        if let Some(promise) = value.as_promise() {
            promise.finish::<()>()?;
        }
        drain_jobs(ctx);
        Ok(())
    })
}

fn drain_jobs(ctx: &Ctx<'_>) {
    // `Ctx::execute_pending_job` runs one job and returns false when the queue is
    // empty. A throwing job has already surfaced its exception on the context.
    while ctx.execute_pending_job() {}
}

fn describe_error(ctx: &Ctx<'_>, error: rquickjs::Error) -> String {
    match CaughtError::from_error(ctx, error) {
        CaughtError::Error(error) => error.to_string(),
        CaughtError::Exception(exception) => exception
            .message()
            .or_else(|| exception.stack())
            .unwrap_or_else(|| "JavaScript exception".to_string()),
        CaughtError::Value(_) => "a script threw a value that is not an Error".to_string(),
    }
}

fn format_js_value<'js>(ctx: &Ctx<'js>, value: JsValue<'js>) -> rquickjs::Result<String> {
    if value.is_undefined() {
        return Ok("undefined".to_string());
    }
    if value.is_null() {
        return Ok("null".to_string());
    }
    if let Some(s) = value.as_string() {
        return s.to_string();
    }
    if let Ok(Some(json_str)) = ctx.json_stringify(value.clone())
        && let Ok(s) = json_str.to_string()
    {
        return Ok(s);
    }
    if let Some(s) = value.into_string()
        && let Ok(res) = s.to_string()
    {
        return Ok(res);
    }
    Ok("[object]".to_string())
}

/// The bridge used by the native `__vy_host_call` function.
struct Bridge {
    requests: async_channel::Sender<HostRequest>,
}

impl Bridge {
    fn call(
        &self,
        ctx: &Ctx<'_>,
        method: String,
        params: Option<String>,
    ) -> rquickjs::Result<String> {
        let params: serde_json::Value = match params {
            Some(text) if !text.is_empty() => serde_json::from_str(&text).map_err(|error| {
                Exception::throw_message(
                    ctx,
                    &format!("host call {method} has invalid JSON: {error}"),
                )
            })?,
            _ => serde_json::Value::Null,
        };

        let (reply_sender, reply_receiver) = mpsc::channel();
        let request = HostRequest {
            method: method.clone(),
            params,
            reply: Arc::new(Mutex::new(Some(reply_sender))),
        };

        if self.requests.send_blocking(request).is_err() {
            return Ok(error_json(
                "RemoteDisconnected",
                "the editor is not accepting calls",
            ));
        }

        // The interrupt deadline cannot fire while a native function blocks, so
        // the wait has its own limit.
        match reply_receiver.recv_timeout(REPLY_TIMEOUT) {
            Ok(HostReply::Ok(value)) => Ok(value_json(value)),
            Ok(HostReply::Err { code, message }) => Ok(error_json(&code, &message)),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(error_json(
                "Timeout",
                &format!("the editor did not answer {method} within 5s"),
            )),
            Err(mpsc::RecvTimeoutError::Disconnected) => Ok(error_json(
                "RemoteDisconnected",
                &format!("the editor dropped the reply to {method}"),
            )),
        }
    }
}

fn value_json(value: serde_json::Value) -> String {
    serde_json::json!({ "value": value }).to_string()
}

fn error_json(code: &str, message: &str) -> String {
    serde_json::json!({ "error": { "code": code, "message": message } }).to_string()
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

/// Helper used by tests to describe what happens when a script loops.
#[cfg(test)]
pub fn test_env() -> ScriptEnv {
    ScriptEnv {
        plugin_id: "init".to_string(),
        version: "0.1.0".to_string(),
        workspace: WorkspaceEnv {
            root_uri: Some(Uri::parse("file:///tmp/vy-test").expect("uri")),
            name: Some("vy-test".to_string()),
            folders: vec![Uri::parse("file:///tmp/vy-test").expect("uri")],
            trusted: true,
        },
        capabilities: Vec::new(),
        remote: RemoteEnv {
            is_remote: false,
            name: "local".to_string(),
            os: "linux".to_string(),
            path_separator: "/".to_string(),
        },
    }
}

/// Implemented by [`ScriptHost`] so tests can read events without the UI.
impl ScriptHost {
    #[cfg(test)]
    fn wait_for_evaluation(&self) -> Vec<ScriptEvent> {
        let mut events = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            match self.events.recv_blocking() {
                Ok(event) => {
                    let done = matches!(
                        event,
                        ScriptEvent::Evaluated | ScriptEvent::Error { .. } | ScriptEvent::Stopped
                    );
                    events.push(event);
                    if done {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::DocumentInfoReply;
    use serde::Serialize;
    use serde_json::json;

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("vy-js-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create temp root");
        root
    }

    fn eval_string(host: &ScriptHost, source: &str) -> String {
        host.eval(source);
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            match host.events.recv_blocking() {
                Ok(ScriptEvent::EvalFinished { result }) => return result,
                Ok(ScriptEvent::Error { message }) => panic!("{message}"),
                Ok(_) => {}
                Err(_) => break,
            }
        }
        panic!("eval timed out");
    }

    fn evaluation_error(host: &ScriptHost, path: PathBuf, source: &str) -> String {
        host.evaluate(path, source);
        host.wait_for_evaluation()
            .into_iter()
            .find_map(|event| match event {
                ScriptEvent::Error { message } => Some(message),
                _ => None,
            })
            .unwrap_or_else(|| panic!("expected an evaluation error"))
    }

    #[derive(Serialize)]
    struct SelectionPayload {
        selections: Vec<vy_core::Selection>,
    }

    struct SelectionEvent;

    impl vy_host_api::HostEvent for SelectionEvent {
        const TOPIC: &'static str = "editor.onDidChangeSelection";
        type Payload = SelectionPayload;
    }

    #[derive(Serialize)]
    struct ActiveDocumentPayload {
        document: Option<DocumentInfoReply>,
    }

    struct ActiveDocumentEvent;

    impl vy_host_api::HostEvent for ActiveDocumentEvent {
        const TOPIC: &'static str = "editor.onDidChangeActiveDocument";
        type Payload = ActiveDocumentPayload;
    }

    #[test]
    fn runs_a_module_and_answers_config_calls() {
        let host = ScriptHost::spawn(test_env(), std::env::temp_dir());
        host.evaluate(
            PathBuf::from("init.ts"),
            r#"
vy.config.set({ values: { "ui.fontSize": 18 } });
vy.commands.registerCommand("user.hello", async () => {});
export const answer: number = 42;
"#,
        );

        let mut evaluated = false;
        let mut registered = false;
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            if let Ok(request) = host.requests.try_recv() {
                match request.method.as_str() {
                    "config.set" => {
                        assert_eq!(request.params["values"]["ui.fontSize"], json!(18));
                        request.respond_value(json!(true));
                    }
                    "commands.registerCommand" => {
                        registered = true;
                        request.respond_value(json!(null));
                    }
                    _ => request.respond_value(json!(null)),
                }
                continue;
            }
            match host.events.try_recv() {
                Ok(event) => {
                    if event == ScriptEvent::Evaluated {
                        evaluated = true;
                        break;
                    }
                }
                Err(_) => std::thread::yield_now(),
            }
        }

        assert!(evaluated, "the module did not finish");
        assert!(registered, "commands.registerCommand was not called");
    }

    #[test]
    fn reports_a_syntax_error_with_a_position() {
        let host = ScriptHost::spawn(test_env(), std::env::temp_dir());
        host.evaluate(PathBuf::from("init.ts"), "const value: = 1;\n");
        let events = host.wait_for_evaluation();
        assert!(
            matches!(&events[0], ScriptEvent::Error { message } if message.contains("init.ts"))
        );
    }

    #[test]
    fn generated_forwarders_keep_the_object_shape() {
        let host = ScriptHost::spawn(test_env(), std::env::temp_dir());
        host.evaluate(
            PathBuf::from("init.ts"),
            r#"
vy.commands.registerCommand("user.check", async () => {
  await vy.editor.setMode({ mode: "insert" });
  const mode = await vy.editor.mode();
  const size = await vy.config.get({ key: "ui.fontSize" });
  return { mode, size };
});
"#,
        );

        let mut evaluated = false;
        let mut running = false;
        let mut result = serde_json::Value::Null;
        let mut calls: Vec<(String, serde_json::Value)> = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);

        while std::time::Instant::now() < deadline {
            if let Ok(request) = host.requests.try_recv() {
                calls.push((request.method.clone(), request.params.clone()));
                match request.method.as_str() {
                    "editor.mode" => request.respond_value(json!("normal")),
                    "config.get" => request.respond_value(json!(18)),
                    _ => request.respond_value(json!(null)),
                }
                continue;
            }
            match host.events.try_recv() {
                Ok(ScriptEvent::Evaluated) if !running => {
                    evaluated = true;
                    running = true;
                    host.execute("user.check", json!(null));
                }
                Ok(ScriptEvent::CommandFinished { result: value, .. }) => {
                    result = value;
                    break;
                }
                Ok(ScriptEvent::Error { message }) => panic!("{message}"),
                Ok(_) => {}
                Err(_) => std::thread::yield_now(),
            }
        }

        assert!(evaluated, "the module did not finish");
        assert_eq!(result, json!({ "mode": "normal", "size": 18 }));

        let set_mode = calls
            .iter()
            .find(|(method, _)| method == "editor.setMode")
            .expect("setMode");
        assert_eq!(set_mode.1, json!({ "mode": "insert" }));
        // A method without parameters is called with no arguments.
        let mode = calls
            .iter()
            .find(|(method, _)| method == "editor.mode")
            .expect("mode");
        assert_eq!(mode.1, json!({}));
        let get = calls
            .iter()
            .find(|(method, _)| method == "config.get")
            .expect("get");
        assert_eq!(get.1, json!({ "key": "ui.fontSize" }));
    }

    #[test]
    fn stops_an_infinite_loop() {
        let host = ScriptHost::spawn(test_env(), std::env::temp_dir());
        host.evaluate(PathBuf::from("init.ts"), "while (true) {}\n");
        let events = host.wait_for_evaluation();
        assert!(matches!(events[0], ScriptEvent::Error { .. }));
    }

    #[test]
    fn runs_a_registered_command_handler() {
        let host = ScriptHost::spawn(test_env(), std::env::temp_dir());
        host.evaluate(
            PathBuf::from("init.ts"),
            r#"
vy.commands.registerCommand("user.hello", async () => {
  await vy.notifications.show("hi");
  return 42;
});
"#,
        );

        let mut running = true;
        let mut shown = false;
        let mut finished = false;
        let mut command_result = serde_json::Value::Null;
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline && !finished {
            if let Ok(request) = host.requests.try_recv() {
                match request.method.as_str() {
                    "notifications.show" => {
                        shown = true;
                        request.respond_value(json!(null));
                    }
                    _ => request.respond_value(json!(null)),
                }
                continue;
            }
            match host.events.try_recv() {
                Ok(ScriptEvent::Evaluated) if running => {
                    running = false;
                    host.execute("user.hello", json!(null));
                }
                Ok(ScriptEvent::CommandFinished { result, .. }) => {
                    command_result = result;
                    finished = true;
                }
                Ok(ScriptEvent::Error { message }) => panic!("{message}"),
                Ok(_) => {}
                Err(_) => std::thread::yield_now(),
            }
        }

        assert!(shown, "the handler did not call notifications.show");
        assert!(finished, "the handler did not finish");
        assert_eq!(command_result, json!(42));
    }

    #[test]
    fn executes_registered_command_from_script_and_returns_value() {
        let host = ScriptHost::spawn(test_env(), std::env::temp_dir());
        host.evaluate(
            PathBuf::from("init.ts"),
            r#"
vy.commands.registerCommand("user.add", async (args) => {
  return (args.a || 0) + (args.b || 0);
});

vy.commands.registerCommand("user.compute", async () => {
  const result = await vy.commands.execute({ id: "user.add", args: { a: 10, b: 20 } });
  return result;
});
"#,
        );

        let mut running = true;
        let mut finished = false;
        let mut command_result = serde_json::Value::Null;
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline && !finished {
            if let Ok(request) = host.requests.try_recv() {
                request.respond_value(json!(null));
                continue;
            }
            match host.events.try_recv() {
                Ok(ScriptEvent::Evaluated) if running => {
                    running = false;
                    host.execute("user.compute", json!(null));
                }
                Ok(ScriptEvent::CommandFinished { result, .. }) => {
                    command_result = result;
                    finished = true;
                }
                Ok(ScriptEvent::Error { message }) => panic!("{message}"),
                Ok(_) => {}
                Err(_) => std::thread::yield_now(),
            }
        }

        assert!(finished, "user.compute did not finish");
        assert_eq!(command_result, json!(null));
    }

    #[test]
    fn ui_picker_returns_the_selected_id() {
        let host = ScriptHost::spawn(test_env(), std::env::temp_dir());
        host.evaluate(
            PathBuf::from("init.ts"),
            r#"
vy.commands.registerCommand("user.pick", async () => {
  const result = await vy.ui.picker.show({
    placeholder: "Select one",
    items: [{ id: "foo", title: "Foo Title" }, { id: "bar", title: "Bar Title" }]
  });
  return result;
});
"#,
        );

        let mut running = true;
        let mut finished = false;
        let mut command_result = serde_json::Value::Null;
        let deadline = std::time::Instant::now() + Duration::from_secs(10);

        while std::time::Instant::now() < deadline && !finished {
            if let Ok(request) = host.requests.try_recv() {
                if request.method == "ui.picker.show" {
                    assert_eq!(
                        request.params.get("placeholder").and_then(|v| v.as_str()),
                        Some("Select one")
                    );
                    request.respond_value(json!({ "selectedId": "foo" }));
                } else {
                    request.respond_value(json!(null));
                }
                continue;
            }
            match host.events.try_recv() {
                Ok(ScriptEvent::Evaluated) if running => {
                    running = false;
                    host.execute("user.pick", json!(null));
                }
                Ok(ScriptEvent::CommandFinished { id, result }) if id == "user.pick" => {
                    command_result = result;
                    finished = true;
                }
                Ok(ScriptEvent::Error { message }) => panic!("{message}"),
                Ok(_) => {}
                Err(_) => std::thread::yield_now(),
            }
        }

        assert!(finished, "user.pick did not finish");
        assert_eq!(command_result, json!({ "selectedId": "foo" }));
    }

    #[test]
    fn ui_gutter_and_hover_apis() {
        let host = ScriptHost::spawn(test_env(), std::env::temp_dir());
        host.evaluate(
            PathBuf::from("init.ts"),
            r#"
vy.ui.gutter.createItem({ id: "g1", line: 15, text: "+", color: "success" });
vy.ui.hover.show({ line: 15, character: 3, text: "type Info = string" });
vy.ui.gutter.disposeItem({ id: "g1" });
vy.ui.hover.hide();
"#,
        );

        let mut gutter_created = false;
        let mut gutter_disposed = false;
        let mut hover_shown = false;
        let mut hover_hidden = false;
        let deadline = std::time::Instant::now() + Duration::from_secs(10);

        while std::time::Instant::now() < deadline {
            if let Ok(request) = host.requests.try_recv() {
                match request.method.as_str() {
                    "ui.gutter.createItem" => gutter_created = true,
                    "ui.gutter.disposeItem" => gutter_disposed = true,
                    "ui.hover.show" => hover_shown = true,
                    "ui.hover.hide" => hover_hidden = true,
                    _ => {}
                }
                request.respond_value(json!(null));
                continue;
            }
            if let Ok(ScriptEvent::Evaluated) = host.events.try_recv() {
                break;
            }
            std::thread::yield_now();
        }

        assert!(gutter_created);
        assert!(gutter_disposed);
        assert!(hover_shown);
        assert!(hover_hidden);
    }

    #[test]
    fn evals_typescript_expression_and_returns_result() {
        let host = ScriptHost::spawn(test_env(), std::env::temp_dir());
        host.eval("const x: number = 20; x * 2");

        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut eval_result = None;

        while std::time::Instant::now() < deadline {
            match host.events.try_recv() {
                Ok(ScriptEvent::EvalFinished { result }) => {
                    eval_result = Some(result);
                    break;
                }
                Ok(ScriptEvent::Error { message }) => panic!("{message}"),
                _ => std::thread::yield_now(),
            }
        }

        assert_eq!(eval_result, Some("40".to_string()));
    }

    #[test]
    fn evals_top_level_await_expression() {
        let host = ScriptHost::spawn(test_env(), std::env::temp_dir());
        host.eval("await Promise.resolve('hello world')");

        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut eval_result = None;

        while std::time::Instant::now() < deadline {
            match host.events.try_recv() {
                Ok(ScriptEvent::EvalFinished { result }) => {
                    eval_result = Some(result);
                    break;
                }
                Ok(ScriptEvent::Error { message }) => panic!("{message}"),
                _ => std::thread::yield_now(),
            }
        }

        assert_eq!(eval_result, Some("hello world".to_string()));
    }

    #[test]
    fn evals_global_vy() {
        let host = ScriptHost::spawn(test_env(), std::env::temp_dir());
        host.eval("vy.commands.registerCommand ? 'has_register' : 'no'");

        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut eval_result = None;

        while std::time::Instant::now() < deadline {
            match host.events.try_recv() {
                Ok(ScriptEvent::EvalFinished { result }) => {
                    eval_result = Some(result);
                    break;
                }
                Ok(ScriptEvent::Error { message }) => panic!("{message}"),
                _ => std::thread::yield_now(),
            }
        }

        assert_eq!(eval_result, Some("has_register".to_string()));
    }

    #[test]
    fn editor_and_workspace_event_listeners() {
        let host = ScriptHost::spawn(test_env(), std::env::temp_dir());

        // Set up listeners in the script environment
        host.eval(
            r#"
            globalThis.__test_selection_count = 0;
            globalThis.__test_active_doc_id = 0;
            globalThis.__test_folders_count = 0;

            globalThis.selSub = vy.editor.onDidChangeSelection((e) => {
                globalThis.__test_selection_count = e.selections.length;
            });

            globalThis.docSub = vy.editor.onDidChangeActiveDocument((event) => {
                globalThis.__test_active_doc_id = event.document ? event.document.id : -1;
            });

            globalThis.wsSub = vy.workspace.onDidChangeWorkspaceFolders((e) => {
                globalThis.__test_folders_count = e.folders.length;
            });
            "#,
        );

        // Wait for setup
        let setup_event = host.events.recv_blocking();
        assert!(
            matches!(setup_event, Ok(ScriptEvent::EvalFinished { .. })),
            "setup eval failed: {setup_event:?}"
        );

        // 1. Selection change
        host.emit_event::<SelectionEvent>(&SelectionPayload {
            selections: vec![
                vy_core::Selection::caret(vy_core::Position::new(0, 5)),
                vy_core::Selection::caret(vy_core::Position::new(1, 5)),
            ],
        });

        host.eval("globalThis.__test_selection_count");
        if let Ok(ScriptEvent::EvalFinished { result }) = host.events.recv_blocking() {
            assert_eq!(result, "2");
        } else {
            panic!("failed to get selection count");
        }

        // 2. Active editor change
        host.emit_event::<ActiveDocumentEvent>(&ActiveDocumentPayload {
            document: Some(DocumentInfoReply {
                id: 42,
                uri: vy_core::Uri::parse("file:///test.rs").unwrap(),
                language_id: "rust".into(),
                version: 1,
                line_count: 10,
                end: vy_core::Position::new(10, 0),
            }),
        });

        host.eval("globalThis.__test_active_doc_id");
        if let Ok(ScriptEvent::EvalFinished { result }) = host.events.recv_blocking() {
            assert_eq!(result, "42");
        } else {
            panic!("failed to get active doc id");
        }

        // 3. Workspace folders change
        host.emit_event::<vy_host_api::workspace::OnDidChangeWorkspaceFolders>(
            &vy_host_api::workspace::WorkspaceFoldersChange {
                folders: vec![
                    "/workspace/one".into(),
                    "/workspace/two".into(),
                    "/workspace/three".into(),
                ],
            },
        );

        host.eval("globalThis.__test_folders_count");
        if let Ok(ScriptEvent::EvalFinished { result }) = host.events.recv_blocking() {
            assert_eq!(result, "3");
        } else {
            panic!("failed to get folders count");
        }

        // 4. Test dispose
        host.eval(
            r#"
            globalThis.selSub.dispose();
            globalThis.__test_selection_count = 0;
            "#,
        );
        let dispose_event = host.events.recv_blocking();
        assert!(
            matches!(dispose_event, Ok(ScriptEvent::EvalFinished { .. })),
            "dispose eval failed: {dispose_event:?}"
        );

        host.emit_event::<SelectionEvent>(&SelectionPayload {
            selections: vec![vy_core::Selection::caret(vy_core::Position::new(0, 1))],
        });

        host.eval("globalThis.__test_selection_count");
        if let Ok(ScriptEvent::EvalFinished { result }) = host.events.recv_blocking() {
            assert_eq!(result, "0", "listener should not have fired after dispose");
        } else {
            panic!("failed to verify dispose");
        }
    }

    #[test]
    fn fs_and_process_calls() {
        let host = ScriptHost::spawn(test_env(), std::env::temp_dir());

        // Test fs.readTextFile
        host.eval(
            r#"
            (async () => {
                const text = await vy.fs.readTextFile({ uri: "file:///test.txt" });
                globalThis.__test_fs_result = text.text;
            })();
            "#,
        );

        // Host receives fs.readTextFile request
        if let Ok(request) = host.requests.recv_blocking() {
            assert_eq!(request.method, "fs.readTextFile");
            assert_eq!(request.params["uri"], "file:///test.txt");
            request.respond_value(serde_json::json!({ "text": "file content here" }));
        } else {
            panic!("failed to receive fs.readTextFile request");
        }

        // Wait for eval task to finish
        let _ = host.events.recv_blocking();

        host.eval("globalThis.__test_fs_result");
        if let Ok(ScriptEvent::EvalFinished { result }) = host.events.recv_blocking() {
            assert_eq!(result, "file content here");
        } else {
            panic!("failed to verify fs.readTextFile result");
        }

        // Test process.exec
        host.eval(
            r#"
            (async () => {
                const out = await vy.process.exec({ command: "echo", args: ["hello"] });
                globalThis.__test_proc_code = out.exit_code;
                globalThis.__test_proc_stdout = out.stdout;
            })();
            "#,
        );

        if let Ok(request) = host.requests.recv_blocking() {
            assert_eq!(request.method, "process.exec");
            assert_eq!(request.params["command"], "echo");
            request.respond_value(serde_json::json!({
                "exit_code": 0,
                "stdout": "hello\n",
                "stderr": ""
            }));
        } else {
            panic!("failed to receive process.exec request");
        }

        let _ = host.events.recv_blocking();

        host.eval("globalThis.__test_proc_stdout");
        if let Ok(ScriptEvent::EvalFinished { result }) = host.events.recv_blocking() {
            assert_eq!(result, "hello\n");
        } else {
            panic!("failed to verify process.exec stdout");
        }
    }

    #[test]
    fn loads_relative_typescript_modules() {
        let root = temp_root("relative");
        std::fs::create_dir_all(root.join("lib")).expect("create lib");
        std::fs::write(
            root.join("lib/util.ts"),
            "export const answer: number = 40;\n",
        )
        .expect("write util");
        std::fs::write(
            root.join("lib/index.ts"),
            "export const extra: number = 2;\n",
        )
        .expect("write index");

        let source = r#"
import type { Missing } from "zod";
import { answer } from "./lib/util";
import { extra } from "./lib";
globalThis.__answer = answer + extra;
"#;
        let init = root.join("init.ts");
        std::fs::write(&init, source).expect("write init");

        let host = ScriptHost::spawn(test_env(), &root);
        host.evaluate(&init, source);
        let events = host.wait_for_evaluation();
        assert!(
            matches!(events.last(), Some(ScriptEvent::Evaluated)),
            "{events:?}"
        );
        assert_eq!(eval_string(&host, "globalThis.__answer"), "42");

        let loaded = host.loaded_files();
        assert!(loaded.contains(&std::fs::canonicalize(&init).expect("init")));
        assert!(loaded.contains(&std::fs::canonicalize(root.join("lib/util.ts")).expect("util")));
        assert!(loaded.contains(&std::fs::canonicalize(root.join("lib/index.ts")).expect("index")));

        drop(host);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn loads_circular_relative_modules() {
        let root = temp_root("circular");
        std::fs::write(
            root.join("a.ts"),
            "import { b } from \"./b\";\nexport const a = b + 1;\n",
        )
        .expect("write a");
        std::fs::write(
            root.join("b.ts"),
            "import { a } from \"./a\";\nexport const b = 1;\n",
        )
        .expect("write b");

        let source = "import { a } from \"./a\";\nglobalThis.__a = a;\n";
        let init = root.join("init.ts");
        std::fs::write(&init, source).expect("write init");

        let host = ScriptHost::spawn(test_env(), &root);
        host.evaluate(&init, source);
        let events = host.wait_for_evaluation();
        assert!(
            matches!(events.last(), Some(ScriptEvent::Evaluated)),
            "{events:?}"
        );
        assert_eq!(eval_string(&host, "globalThis.__a"), "2");

        drop(host);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_bare_imports_without_special_cases() {
        for specifier in ["zod", "vy-api"] {
            let root = temp_root(&format!("bare-{}", specifier.replace('-', "_")));
            let source = format!("import \"{specifier}\";\n");
            let init = root.join("init.ts");
            std::fs::write(&init, &source).expect("write init");

            let host = ScriptHost::spawn(test_env(), &root);
            let message = evaluation_error(&host, init, &source);
            assert!(
                message.contains("only relative imports are supported"),
                "{specifier}: {message}"
            );

            drop(host);
            let _ = std::fs::remove_dir_all(&root);
        }
    }

    #[test]
    fn rejects_modules_outside_the_root() {
        let base = temp_root("escape");
        let root = base.join("plugin");
        std::fs::create_dir_all(&root).expect("create plugin");
        std::fs::write(base.join("outside.ts"), "export const value = 1;\n")
            .expect("write outside");

        let source = "import { value } from \"../outside\";\nglobalThis.__value = value;\n";
        let init = root.join("init.ts");
        std::fs::write(&init, source).expect("write init");

        let host = ScriptHost::spawn(test_env(), &root);
        let message = evaluation_error(&host, init, source);
        assert!(message.contains("escapes the plugin root"), "{message}");

        drop(host);
        let _ = std::fs::remove_dir_all(&base);
    }
}
