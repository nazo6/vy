//! `vy-app` の雛形です。
//!
//! 起動すると `vy-server` を子プロセスで立ち上げ、`initialize`、`pane/open`、
//! `buffer/snapshot` の順に要求を出して、受け取った本文を 1 つのペインに描きます。
//! レイアウト木、キーマップ、編集の送信は次の段階で足します。

mod rpc;

use std::collections::HashMap;
use std::time::Duration;

use gpui::{
    App, Application, Bounds, Context, Render, SharedString, Window, WindowBounds, WindowOptions,
    div, prelude::*, px, rgb, size,
};
use rmpv::Value;
use vy_proto::{PROTOCOL_VERSION, RpcMessage, RpcResult, get, object};

use crate::rpc::{Client, Poll};

/// 応答を待っている要求の種類です。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    Initialize,
    Open,
    Snapshot,
}

struct EditorView {
    client: Option<Client>,
    pending: HashMap<u64, Pending>,
    document_id: Option<u64>,
    pane_id: Option<u64>,
    version: u64,
    text: String,
    status: SharedString,
}

impl EditorView {
    fn new(cx: &mut Context<Self>) -> Self {
        let mut view = Self {
            client: None,
            pending: HashMap::new(),
            document_id: None,
            pane_id: None,
            version: 0,
            text: String::new(),
            status: "vy-server を起動しています".into(),
        };

        match Client::spawn() {
            Ok(mut client) => {
                let id = client.request(
                    "initialize",
                    object([
                        ("protocol_version", Value::from(PROTOCOL_VERSION)),
                        ("client_version", Value::from(env!("CARGO_PKG_VERSION"))),
                        ("client_capabilities", Value::Array(Vec::new())),
                        ("workspace", Value::from(workspace_uri())),
                        ("resume_session", Value::Nil),
                    ]),
                );
                view.pending.insert(id, Pending::Initialize);
                view.client = Some(client);
                view.status = "initialize への応答を待っています".into();
            }
            Err(error) => {
                view.status = format!("vy-server を起動できません: {error}").into();
            }
        }

        view.start_polling(cx);
        view
    }

    /// 16ms ごとに受信キューを見ます。UI のスレッドを止めないため、読み取りは
    /// 別スレッドが行い、ここでは溜まった分を取り出すだけにします。
    fn start_polling(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;

                let keep_polling = this.update(cx, |this, cx| {
                    let keep_polling = this.drain();
                    cx.notify();
                    keep_polling
                });

                if !keep_polling.unwrap_or(false) {
                    break;
                }
            }
        })
        .detach();
    }

    fn drain(&mut self) -> bool {
        let mut received = Vec::new();

        let closed = {
            let Some(client) = self.client.as_ref() else {
                return false;
            };

            loop {
                match client.poll() {
                    Poll::Message(message) => received.push(message),
                    Poll::Empty => break false,
                    Poll::Closed => break true,
                }
            }
        };

        for message in received {
            self.handle(message);
        }

        if closed {
            self.status = "vy-server との接続が切れました。再起動は手動で行います".into();
            self.client = None;
            return false;
        }

        true
    }

    fn handle(&mut self, message: RpcMessage) {
        match message {
            RpcMessage::Response { id, result } => self.handle_response(id, result),
            RpcMessage::Notification { method, params } => {
                self.handle_notification(&method, params)
            }
            other => {
                eprintln!("vy: 未対応のメッセージ: {other:?}");
            }
        }
    }

    fn handle_response(&mut self, id: u64, result: RpcResult) {
        let Some(pending) = self.pending.remove(&id) else {
            return;
        };

        match result {
            RpcResult::Err { code, message, .. } => {
                self.status = format!("{code}: {message}").into();
            }
            RpcResult::Ok { value } => match pending {
                Pending::Initialize => {
                    if let Some(session_id) = get(&value, "session_id").and_then(Value::as_u64)
                        && let Some(client) = self.client.as_mut()
                    {
                        client.set_session_id(session_id);
                    }
                }
                Pending::Open => self.opened(&value),
                Pending::Snapshot => self.received_snapshot(&value),
            },
        }
    }

    fn handle_notification(&mut self, method: &str, params: Value) {
        match method {
            "initialized" => {
                self.status = "接続しました。文書を開いています".into();
                self.open_document();
            }
            "buffer/resync" => self.received_snapshot(&params),
            other => {
                eprintln!("vy: 未対応の通知: {other}");
            }
        }
    }

    fn open_document(&mut self) {
        let uri = document_uri();
        self.request(
            "pane/open",
            object([("uri", Value::from(uri))]),
            Pending::Open,
        );
    }

    fn opened(&mut self, value: &Value) {
        let Some(document_id) = get(value, "document_id").and_then(Value::as_u64) else {
            self.status = "pane/open の応答に document_id がありません".into();
            return;
        };

        self.document_id = Some(document_id);
        self.pane_id = get(value, "pane_id").and_then(Value::as_u64);
        self.version = get(value, "version").and_then(Value::as_u64).unwrap_or(0);

        self.request(
            "buffer/snapshot",
            object([("document_id", Value::from(document_id))]),
            Pending::Snapshot,
        );
    }

    fn received_snapshot(&mut self, value: &Value) {
        if let Some(text) = get(value, "text").and_then(Value::as_str) {
            self.text = text.to_string();
        }
        if let Some(version) = get(value, "version").and_then(Value::as_u64) {
            self.version = version;
        }

        self.status = format!(
            "{} 行 / version {} / pane {}",
            self.text.lines().count(),
            self.version,
            self.pane_id
                .map(|pane| pane.to_string())
                .unwrap_or_else(|| "-".into())
        )
        .into();
    }

    fn request(&mut self, method: &str, params: Value, pending: Pending) {
        let Some(client) = self.client.as_mut() else {
            return;
        };
        let id = client.request(method, params);
        self.pending.insert(id, pending);
    }
}

impl Render for EditorView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let header = div()
            .flex()
            .justify_between()
            .text_sm()
            .text_color(rgb(0x9a9a9a))
            .child(format!("vy {}", env!("CARGO_PKG_VERSION")))
            .child(self.status.clone());

        let body = if self.text.is_empty() {
            div()
                .child("(空の文書)")
                .text_color(rgb(0x6a6a6a))
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .children(
                    self.text
                        .lines()
                        .take(400)
                        .map(|line| div().child(line.to_string())),
                )
                .into_any_element()
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x1e1e1e))
            .text_color(rgb(0xd4d4d4))
            .font_family("monospace")
            .p_4()
            .gap_2()
            .child(header)
            .child(div().flex_1().overflow_hidden().child(body))
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1000.0), px(700.0)), cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_window, cx| cx.new(EditorView::new),
        )
        .expect("ウィンドウを開けません");

        cx.activate(true);
    });
}

/// 開く文書の URI を決めます。
///
/// 引数が `file:` か `vy:` で始まればそのまま、パスなら絶対パスにして `file:` を
/// 付けます。引数がなければ空の `vy:untitled` を開きます。
fn document_uri() -> String {
    match std::env::args().nth(1) {
        Some(argument) if argument.starts_with("file:") || argument.starts_with("vy:") => argument,
        Some(path) => match std::fs::canonicalize(&path) {
            Ok(absolute) => format!("file://{}", absolute.display()),
            // まだ存在しないファイルは、渡されたパスのまま開かせる。
            Err(_) => format!("file://{path}"),
        },
        None => "vy:untitled".to_string(),
    }
}

fn workspace_uri() -> String {
    match std::env::current_dir() {
        Ok(directory) => format!("file://{}", directory.display()),
        Err(_) => "file:///".to_string(),
    }
}
