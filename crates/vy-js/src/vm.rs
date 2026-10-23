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

use crate::api::{VyLoader, VyResolver};
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
    pub root_uri: Option<String>,
    pub name: Option<String>,
    pub folders: Vec<String>,
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
    /// Tells the script that some settings changed.
    EmitConfigChange {
        keys: Vec<String>,
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
}

impl ScriptHost {
    /// Starts the thread and installs the shim. The runtime is ready before this
    /// returns; evaluation waits for [`ScriptHost::evaluate`].
    pub fn spawn(env: ScriptEnv) -> Self {
        let (task_sender, task_receiver) = async_channel::unbounded();
        let (event_sender, event_receiver) = async_channel::unbounded();
        let (request_sender, request_receiver) = async_channel::unbounded();
        let env_json = serde_json::to_string(&env).unwrap_or_else(|_| "{}".to_string());

        let thread = std::thread::Builder::new()
            .name("vy-js".to_string())
            .spawn(move || run(env_json, task_receiver, event_sender, request_sender))
            .ok();

        Self {
            tasks: task_sender,
            events: event_receiver,
            requests: request_receiver,
            thread,
        }
    }

    pub fn events(&self) -> async_channel::Receiver<ScriptEvent> {
        self.events.clone()
    }

    pub fn requests(&self) -> async_channel::Receiver<HostRequest> {
        self.requests.clone()
    }

    /// Queues an evaluation. Errors arrive as [`ScriptEvent::Error`].
    pub fn evaluate(&self, path: impl Into<PathBuf>, source: impl Into<String>) {
        let _ = self.tasks.send_blocking(ScriptTask::Evaluate {
            path: path.into(),
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

    /// Tells the script that these keys changed.
    pub fn emit_config_change(&self, keys: Vec<String>) {
        let _ = self
            .tasks
            .send_blocking(ScriptTask::EmitConfigChange { keys });
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

    let deadline = Arc::new(AtomicU64::new(0));
    let interrupt_deadline = deadline.clone();
    runtime.set_interrupt_handler(Some(Box::new(move || {
        let until = interrupt_deadline.load(Ordering::Relaxed);
        until != 0 && now_millis() > until
    })));

    runtime.set_loader(VyResolver, VyLoader);

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

        // Expose `vy` on globalThis so interactive evaluations can use the API directly.
        let promise = Module::evaluate(
            ctx.clone(),
            "<vy-global>",
            "import * as vy from 'vy-api'; globalThis.vy = vy;\n",
        )?;
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

            let name = path.to_string_lossy().to_string();
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
        ScriptTask::EmitConfigChange { keys } => {
            let json = serde_json::to_string(&keys).unwrap_or_else(|_| "[]".to_string());
            let result = with_deadline(deadline, EVALUATION_TIMEOUT, || {
                let function: Function = ctx.globals().get("__vy_emit_config_change")?;
                let value: JsValue = function.call((json,))?;
                if let Some(promise) = value.as_promise() {
                    promise.finish::<()>()?;
                }
                drain_jobs(&ctx);
                Ok(())
            });
            if let Err(error) = result {
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
            root_uri: Some("file:///tmp/vy-test".to_string()),
            name: Some("vy-test".to_string()),
            folders: vec!["file:///tmp/vy-test".to_string()],
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
    use serde_json::json;

    #[test]
    fn runs_a_module_and_answers_config_calls() {
        let host = ScriptHost::spawn(test_env());
        host.evaluate(
            PathBuf::from("init.ts"),
            r#"
import { config, commands } from "vy-api";

config.set({ "ui.fontSize": 18 });
commands.registerCommand("user.hello", async () => {});
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
        let host = ScriptHost::spawn(test_env());
        host.evaluate(PathBuf::from("init.ts"), "const value: = 1;\n");
        let events = host.wait_for_evaluation();
        assert!(
            matches!(&events[0], ScriptEvent::Error { message } if message.contains("init.ts"))
        );
    }

    #[test]
    fn stops_an_infinite_loop() {
        let host = ScriptHost::spawn(test_env());
        host.evaluate(PathBuf::from("init.ts"), "while (true) {}\n");
        let events = host.wait_for_evaluation();
        assert!(matches!(events[0], ScriptEvent::Error { .. }));
    }

    #[test]
    fn runs_a_registered_command_handler() {
        let host = ScriptHost::spawn(test_env());
        host.evaluate(
            PathBuf::from("init.ts"),
            r#"
import { commands, notifications } from "vy-api";
commands.registerCommand("user.hello", async () => {
  await notifications.show("hi");
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
        let host = ScriptHost::spawn(test_env());
        host.evaluate(
            PathBuf::from("init.ts"),
            r#"
import { commands } from "vy-api";
commands.registerCommand("user.add", async (args) => {
  return (args.a || 0) + (args.b || 0);
});

commands.registerCommand("user.compute", async () => {
  const result = await commands.execute("user.add", { a: 10, b: 20 });
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
        assert_eq!(command_result, json!(30));
    }

    #[test]
    fn ui_picker_show_and_register_source() {
        let host = ScriptHost::spawn(test_env());
        host.evaluate(
            PathBuf::from("init.ts"),
            r#"
import { ui, commands } from "vy-api";

const disposable = ui.picker.registerSource({
  id: "test-source",
  provideItems: () => [{ id: "x", title: "Item X" }]
});

commands.registerCommand("user.pick", async () => {
  const result = await ui.picker.show({
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
        assert_eq!(command_result, json!({ "id": "foo", "title": "Foo Title" }));
    }

    #[test]
    fn ui_gutter_and_hover_apis() {
        let host = ScriptHost::spawn(test_env());
        host.evaluate(
            PathBuf::from("init.ts"),
            r#"
import { ui } from "vy-api";

const item = ui.gutter.createItem({ line: 15, text: "+", color: "success" });
ui.hover.show({ line: 15, character: 3, text: "type Info = string" });
item.dispose();
ui.hover.hide();
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
        let host = ScriptHost::spawn(test_env());
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
        let host = ScriptHost::spawn(test_env());
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
    fn evals_import_vy_api() {
        let host = ScriptHost::spawn(test_env());
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
}
