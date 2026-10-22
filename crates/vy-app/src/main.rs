//! `vy-app` の雛形です。
//!
//! 起動すると `vy-server` を子プロセスで立ち上げ、`initialize`、`pane/open`、
//! `buffer/snapshot` の順に要求を出して、受け取った本文を 1 つのペインに描きます。
//! キー入力は `editor` のモーダル入力が処理し、`buffer/did_change` でコアへ送ります。
//! レイアウト木は Phase 3 で足します。

mod editor;
mod rpc;

use std::collections::HashMap;
use std::time::Duration;

use gpui::{
    App, Application, Bounds, Context, Div, FocusHandle, Focusable, KeyDownEvent, Keystroke,
    Render, SharedString, Window, WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};
use rmpv::Value;
use serde::Serialize;
use serde::de::DeserializeOwned;
use vy_core::{Range, Selection, TextEdit};
use vy_proto::{PROTOCOL_VERSION, RpcMessage, RpcResult, get, object};

use crate::editor::{Action, Editor, Key, Mode, Transaction};
use crate::rpc::{Client, Poll};

/// 応答を待っている要求の種類です。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    Initialize,
    Open,
    Snapshot,
    Undo,
    Redo,
}

struct EditorView {
    client: Option<Client>,
    pending: HashMap<u64, Pending>,
    document_id: Option<u64>,
    pane_id: Option<u64>,
    editor: Editor,
    focus_handle: FocusHandle,
    status: SharedString,
}

impl EditorView {
    fn new(cx: &mut Context<Self>) -> Self {
        let mut view = Self {
            client: None,
            pending: HashMap::new(),
            document_id: None,
            pane_id: None,
            editor: Editor::new(),
            focus_handle: cx.focus_handle(),
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
                Pending::Undo | Pending::Redo => self.received_history(&value),
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

        self.request(
            "buffer/snapshot",
            object([("document_id", Value::from(document_id))]),
            Pending::Snapshot,
        );
    }

    fn received_snapshot(&mut self, value: &Value) {
        let text = get(value, "text")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let version = get(value, "version")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        self.editor.reset(text, version);

        self.status = format!(
            "{} 行 / version {} / pane {}",
            self.editor.line_count(),
            self.editor.version(),
            self.pane_id
                .map(|pane| pane.to_string())
                .unwrap_or_else(|| "-".into())
        )
        .into();
    }

    fn received_history(&mut self, value: &Value) {
        let version = get(value, "version")
            .and_then(Value::as_u64)
            .unwrap_or(self.editor.version());
        let edits = get(value, "edits")
            .and_then(decode_value::<Vec<TextEdit>>)
            .unwrap_or_default();
        let selection = get(value, "selection")
            .and_then(decode_value::<Vec<Selection>>)
            .unwrap_or_default();

        self.editor.apply_history(&edits, version, &selection);
    }

    fn request(&mut self, method: &str, params: Value, pending: Pending) {
        let Some(client) = self.client.as_mut() else {
            return;
        };
        let id = client.request(method, params);
        self.pending.insert(id, pending);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = convert_key(&event.keystroke) else {
            return;
        };

        match self.editor.handle_key(key) {
            Action::None => {}
            Action::Edit(transaction) => self.send_transaction(transaction),
            Action::Undo => self.send_history("buffer/undo", Pending::Undo),
            Action::Redo => self.send_history("buffer/redo", Pending::Redo),
        }

        cx.notify();
    }

    fn send_transaction(&mut self, transaction: Transaction) {
        let Some(document_id) = self.document_id else {
            return;
        };
        let Some(client) = self.client.as_mut() else {
            return;
        };

        client.notify(
            "buffer/did_change",
            object([
                ("document_id", Value::from(document_id)),
                ("base_version", Value::from(transaction.base_version)),
                ("version", Value::from(transaction.base_version + 1)),
                ("transaction_id", Value::from(transaction.transaction_id)),
                ("edits", value_of(&transaction.edits)),
                ("selection_before", value_of(&transaction.selection_before)),
                ("selection_after", value_of(&transaction.selection_after)),
            ]),
        );
    }

    fn send_history(&mut self, method: &str, pending: Pending) {
        let Some(document_id) = self.document_id else {
            return;
        };
        let Some(client) = self.client.as_mut() else {
            return;
        };

        let id = client.request(method, object([("document_id", Value::from(document_id))]));
        self.pending.insert(id, pending);
    }

    /// カーソルの前後を描くための行を集めます。スクロールは Phase 1 では固定です。
    fn visible_lines(&self) -> Vec<Div> {
        const AROUND: usize = 20;
        const COUNT: usize = 40;

        let cursor = self.editor.cursor();
        let start = (cursor.line as usize).saturating_sub(AROUND);
        let end = (start + COUNT).min(self.editor.line_count());

        (start..end).map(|line| self.render_line(line)).collect()
    }

    fn render_line(&self, line_number: usize) -> Div {
        let row = div().flex().h(px(20.)).items_center().child(
            div()
                .w(px(52.))
                .flex_shrink_0()
                .text_color(rgb(0x5a5a5a))
                .child(format!("{:>4} ", line_number + 1)),
        );

        let text = self.editor.line(line_number).unwrap_or_default();
        let cursor = self.editor.cursor();
        let length: u32 = text
            .chars()
            .map(|character| character.len_utf16() as u32)
            .sum();

        if line_number as u32 != cursor.line {
            return row.child(text);
        }

        if self.editor.mode() == Mode::Insert {
            let (before, after) = split_at_units(&text, cursor.character);
            return row
                .child(before.to_string())
                .child(div().text_color(rgb(0x4a9eff)).child("▏"))
                .child(after.to_string());
        }

        let Some((from, to)) =
            selection_on_line(self.editor.selection(), line_number as u32, length)
        else {
            return row.child(text);
        };

        if from >= to {
            // 改行だけを選んでいるときは、行末に印を出す。
            return row.child(text).child(div().bg(rgb(0x264f78)).child(" "));
        }

        let (before, rest) = split_at_units(&text, from);
        let (selected, after) = split_at_units(rest, to - from);

        row.child(before.to_string())
            .child(div().bg(rgb(0x264f78)).child(selected.to_string()))
            .child(after.to_string())
    }
}

impl Focusable for EditorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for EditorView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let header = div()
            .flex()
            .justify_between()
            .text_sm()
            .text_color(rgb(0x9a9a9a))
            .child(format!("vy {}", env!("CARGO_PKG_VERSION")))
            .child(self.status.clone());

        let cursor = self.editor.cursor();
        let footer = div()
            .flex()
            .justify_between()
            .text_sm()
            .text_color(rgb(0x9a9a9a))
            .child(format!(
                "{}  {}:{}",
                self.editor.mode().name(),
                cursor.line + 1,
                cursor.character + 1
            ))
            .child(format!("version {}", self.editor.version()));

        div()
            .id("vy-editor")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x1e1e1e))
            .text_color(rgb(0xd4d4d4))
            .font_family("monospace")
            .p_4()
            .gap_2()
            .child(header)
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .children(self.visible_lines()),
            )
            .child(footer)
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1000.0), px(700.0)), cx);

        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                |_window, cx| cx.new(EditorView::new),
            )
            .expect("ウィンドウを開けません");

        window
            .update(cx, |view, window, _cx| {
                view.focus_handle.focus(window);
            })
            .ok();

        cx.activate(true);
    });
}

/// GPUI のキーを、エディタのキーに直します。
fn convert_key(keystroke: &Keystroke) -> Option<Key> {
    if keystroke.modifiers.control {
        return match keystroke.key.as_str() {
            "r" => Some(Key::Ctrl('r')),
            "u" => Some(Key::Ctrl('u')),
            _ => None,
        };
    }

    if keystroke.modifiers.alt || keystroke.modifiers.platform {
        return None;
    }

    match keystroke.key.as_str() {
        "escape" => return Some(Key::Esc),
        "enter" => return Some(Key::Enter),
        "backspace" => return Some(Key::Backspace),
        "tab" => return Some(Key::Tab),
        "left" => return Some(Key::Left),
        "right" => return Some(Key::Right),
        "up" => return Some(Key::Up),
        "down" => return Some(Key::Down),
        "home" => return Some(Key::Home),
        "end" => return Some(Key::End),
        "space" => return Some(Key::Char(' ')),
        _ => {}
    }

    // IME の前編集はまだ受け取らない。確定した文字だけを `key_char` から拾う。
    let character = keystroke.key_char.as_ref()?.chars().next()?;
    Some(Key::Char(character))
}

/// UTF-16 の位置で文字列を 2 つに分けます。サロゲートペアの途中は文字の先頭で切ります。
fn split_at_units(text: &str, units: u32) -> (&str, &str) {
    let mut count = 0u32;

    for (index, character) in text.char_indices() {
        if count >= units || units < count + character.len_utf16() as u32 {
            return (&text[..index], &text[index..]);
        }
        count += character.len_utf16() as u32;
    }

    (text, "")
}

/// 1 行のうち選択に含まれる範囲です。行が選択の外なら [`None`] です。
fn selection_on_line(selection: Range, line: u32, length: u32) -> Option<(u32, u32)> {
    if selection.start == selection.end || line < selection.start.line || line > selection.end.line
    {
        return None;
    }

    let from = if line == selection.start.line {
        selection.start.character
    } else {
        0
    };
    let to = if line == selection.end.line {
        selection.end.character.min(length)
    } else {
        length
    };

    if from >= to {
        // 行末の改行だけを選んでいる場合は、印を出してもらう。
        return (to >= length).then_some((length, length));
    }

    Some((from.min(length), to))
}

fn value_of<T: Serialize>(value: &T) -> Value {
    // rmpv の serde 連携は struct をタプルにするため、rmp-serde を通す。
    rmp_serde::to_vec_named(value)
        .ok()
        .and_then(|bytes| rmpv::decode::read_value(&mut bytes.as_slice()).ok())
        .unwrap_or(Value::Nil)
}

fn decode_value<T: DeserializeOwned>(value: &Value) -> Option<T> {
    let mut bytes = Vec::new();
    rmpv::encode::write_value(&mut bytes, value).ok()?;
    rmp_serde::from_slice(&bytes).ok()
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

#[cfg(test)]
mod tests {
    use super::*;
    use vy_core::Position;

    #[test]
    fn splits_at_utf16_units() {
        assert_eq!(split_at_units("a😀b", 0), ("", "a😀b"));
        assert_eq!(split_at_units("a😀b", 1), ("a", "😀b"));
        // サロゲートペアの途中は、その文字の先頭で切る。
        assert_eq!(split_at_units("a😀b", 2), ("a", "😀b"));
        assert_eq!(split_at_units("a😀b", 3), ("a😀", "b"));
        assert_eq!(split_at_units("a😀b", 4), ("a😀b", ""));
    }

    #[test]
    fn clips_a_selection_to_each_line() {
        let selection = Range::new(Position::new(0, 1), Position::new(2, 1));

        assert_eq!(selection_on_line(selection, 0, 3), Some((1, 3)));
        assert_eq!(selection_on_line(selection, 1, 5), Some((0, 5)));
        assert_eq!(selection_on_line(selection, 2, 4), Some((0, 1)));
        assert_eq!(selection_on_line(selection, 3, 4), None);
    }

    #[test]
    fn marks_a_selected_line_break() {
        let selection = Range::new(Position::new(0, 3), Position::new(1, 0));
        assert_eq!(selection_on_line(selection, 0, 3), Some((3, 3)));
    }

    #[test]
    fn round_trips_edits_through_message_pack() {
        let edits = vec![
            TextEdit::new(Range::new(Position::new(0, 1), Position::new(0, 3)), "x"),
            TextEdit::new(
                Range::new(Position::new(2, 0), Position::new(2, 0)),
                "こんにちは",
            ),
        ];

        let value = value_of(&edits);
        let decoded = decode_value::<Vec<TextEdit>>(&value).expect("decode");
        assert_eq!(decoded, edits);

        let selection = vec![Selection::caret(Position::new(1, 2))];
        let value = value_of(&selection);
        assert_eq!(
            decode_value::<Vec<Selection>>(&value).expect("decode"),
            selection
        );
    }
}
