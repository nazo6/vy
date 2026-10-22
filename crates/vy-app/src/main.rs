//! Skeleton of `vy-app`.
//!
//! On startup it launches `vy-server` as a child process, sends `initialize`,
//! `pane/open`, and `buffer/snapshot` in that order, and draws the returned text in
//! one pane. The modal input in `editor` handles keys and sends `buffer/did_change`
//! to the core. The layout tree lands in Phase 3.

mod editor;
mod rpc;

use std::collections::HashMap;
use std::time::{Duration, Instant};

use gpui::{
    App, Application, Bounds, Context, Div, ElementInputHandler, EntityInputHandler, FocusHandle,
    Focusable, HighlightStyle, KeyDownEvent, Keystroke, Pixels, Render, ScrollWheelEvent,
    SharedString, StyledText, TextRun, UTF16Selection, Window, WindowBounds, WindowOptions, canvas,
    div, point, prelude::*, px, rgb, size,
};
use rmpv::Value;
use serde::Serialize;
use serde::de::DeserializeOwned;
use vy_core::{Range, Selection, TextEdit};
use vy_proto::{PROTOCOL_VERSION, RpcMessage, RpcResult, get, object};

use crate::editor::{Action, Editor, Key, Mode, Transaction};
use crate::rpc::{Client, Poll};

/// Height of one text row.
const LINE_HEIGHT: f32 = 20.0;
/// Lines drawn before the window has been measured.
const VISIBLE_LINES: usize = 30;
/// Space taken by the header, the footer, and the padding around them.
const CHROME_HEIGHT: f32 = 100.0;
/// Width of one character, used to size the line number gutter.
const CHARACTER_WIDTH: f32 = 10.0;

/// The kind of request we are waiting for a response to.
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
    /// Text the input method is still converting. It never reaches the core.
    preedit: String,
    /// Range the input method selected inside the preedit, in UTF-16 code units.
    preedit_selection: Option<std::ops::Range<usize>>,
    /// First line drawn in the text area.
    scroll_top: usize,
    /// Rows that fit in the text area. Updated on every render.
    viewport_rows: usize,
    focus_handle: FocusHandle,
    status: SharedString,
    frame_log: bool,
    last_frame: Option<Instant>,
    /// Set once the input handler has been registered with the platform.
    input_registered: bool,
}

impl EditorView {
    fn new(cx: &mut Context<Self>) -> Self {
        let mut view = Self {
            client: None,
            pending: HashMap::new(),
            document_id: None,
            pane_id: None,
            editor: Editor::new(),
            preedit: String::new(),
            preedit_selection: None,
            scroll_top: 0,
            viewport_rows: VISIBLE_LINES,
            focus_handle: cx.focus_handle(),
            status: "starting vy-server".into(),
            frame_log: std::env::var_os("VY_FRAME_LOG").is_some(),
            last_frame: None,
            input_registered: false,
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
                view.status = "waiting for the initialize response".into();
            }
            Err(error) => {
                view.status = format!("could not start vy-server: {error}").into();
            }
        }

        view.start_polling(cx);
        view
    }

    /// Polls the receive queue every 16ms. Reads happen on another thread, so the UI
    /// thread stays free; this only drains what has arrived.
    fn start_polling(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;

                let keep_polling = this.update(cx, |this, cx| {
                    let (keep_polling, changed) = this.drain();
                    // Do not repaint when nothing arrived in this frame.
                    if changed {
                        cx.notify();
                    }
                    keep_polling
                });

                if !keep_polling.unwrap_or(false) {
                    break;
                }
            }
        })
        .detach();
    }

    /// Handles arrived messages. Returns (keep polling, repaint).
    fn drain(&mut self) -> (bool, bool) {
        let mut received = Vec::new();

        let closed = {
            let Some(client) = self.client.as_ref() else {
                return (false, false);
            };

            loop {
                match client.poll() {
                    Poll::Message(message) => received.push(message),
                    Poll::Empty => break false,
                    Poll::Closed => break true,
                }
            }
        };

        let changed = !received.is_empty();
        if changed && self.frame_log {
            eprintln!("vy: {} message(s)", received.len());
        }
        for message in received {
            self.handle(message);
        }

        if closed {
            self.status = "lost the connection to vy-server; restart it manually".into();
            self.client = None;
            return (false, true);
        }

        (true, changed)
    }

    fn handle(&mut self, message: RpcMessage) {
        match message {
            RpcMessage::Response { id, result } => self.handle_response(id, result),
            RpcMessage::Notification { method, params } => {
                self.handle_notification(&method, params)
            }
            other => {
                eprintln!("vy: unsupported message: {other:?}");
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
                self.status = "connected; opening the document".into();
                self.open_document();
            }
            "buffer/resync" => self.received_snapshot(&params),
            other => {
                eprintln!("vy: unsupported notification: {other}");
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
            self.status = "pane/open response has no document_id".into();
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
        let started = Instant::now();
        let text = get(value, "text")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let version = get(value, "version")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        self.editor.reset(text, version);
        self.clear_preedit();
        self.scroll_top = 0;

        if self.frame_log {
            eprintln!(
                "vy: snapshot {} lines in {:.1}ms",
                self.editor.line_count(),
                started.elapsed().as_secs_f64() * 1000.0
            );
        }

        self.status = format!(
            "{} lines / version {} / pane {}",
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

        // Printable text reaches Insert mode through `replace_text_in_range`, which every
        // platform calls for the same key press. Inserting here would duplicate it.
        if self.editor.mode() == Mode::Insert && matches!(key, Key::Char(_)) {
            return;
        }

        // A modal command that starts with a letter (`i`, `a`, `p`) must not reach the
        // input handler, or the same letter would also be inserted after the mode change.
        // `Enter`, `Tab`, and `Backspace` are editable keys too: macOS sends them to the
        // input method as text, so they are handled here only.
        if matches!(key, Key::Char(_) | Key::Enter | Key::Tab | Key::Backspace) {
            cx.stop_propagation();
        }

        if self.apply_key(key) {
            cx.notify();
        }
    }

    /// Passes the key to the editor and reports whether the display changed.
    fn apply_key(&mut self, key: Key) -> bool {
        let before = (
            self.editor.mode(),
            self.editor.selection(),
            self.editor.version(),
        );

        match self.editor.handle_key(key) {
            Action::None => {}
            Action::Edit(transaction) => self.send_transaction(transaction),
            Action::Undo => self.send_history("buffer/undo", Pending::Undo),
            Action::Redo => self.send_history("buffer/redo", Pending::Redo),
        }

        // A preedit only makes sense while Insert mode keeps the caret.
        if self.editor.mode() != Mode::Insert {
            self.clear_preedit();
        }

        // Do not repaint for keys that change nothing, such as a motion at a boundary.
        let changed = before
            != (
                self.editor.mode(),
                self.editor.selection(),
                self.editor.version(),
            );

        self.ensure_cursor_visible();
        changed
    }

    /// Scrolls by whole lines. The wheel moves the view, not the cursor.
    fn on_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = f32::from(event.delta.pixel_delta(px(LINE_HEIGHT)).y);
        if delta == 0.0 {
            return;
        }

        // Positive deltas scroll up, so they move towards earlier lines.
        let mut lines = (delta / LINE_HEIGHT).round() as isize;
        if lines == 0 {
            lines = delta.signum() as isize;
        }

        let next = scroll_lines(
            self.scroll_top,
            -lines,
            self.viewport_rows,
            self.editor.line_count(),
        );

        if next != self.scroll_top {
            self.scroll_top = next;
            cx.notify();
        }
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

    /// Collects the lines visible in the text area.
    fn visible_lines(&self) -> Vec<Div> {
        let start = self.first_visible_line();
        let end = (start + self.viewport_rows.max(1)).min(self.editor.line_count());

        (start..end).map(|line| self.render_line(line)).collect()
    }

    fn render_line(&self, line_number: usize) -> Div {
        let row = div().flex().h(px(LINE_HEIGHT)).items_center().child(
            div()
                .w(px(self.gutter_width()))
                .flex_shrink_0()
                .text_color(rgb(0x5a5a5a))
                .child(format!(
                    "{:>width$} ",
                    line_number + 1,
                    width = self.line_number_width()
                )),
        );

        let text = self.editor.line(line_number).unwrap_or_default();
        let cursor = self.editor.cursor();

        if line_number as u32 != cursor.line {
            return row.child(text);
        }

        let (display, highlight) = self.cursor_marker(&text);

        row.child(StyledText::new(display).with_highlights(vec![(
            highlight,
            HighlightStyle {
                background_color: Some(rgb(0x264f78).into()),
                ..Default::default()
            },
        )]))
    }

    /// Returns the text to draw on the cursor line and the byte range to mark.
    ///
    /// The line is shaped once as a whole. Splitting it into several elements would
    /// change the glyphs and widths under the cursor.
    fn cursor_marker(&self, text: &str) -> (String, std::ops::Range<usize>) {
        let mut display = text.to_string();

        if self.editor.mode() == Mode::Insert {
            let caret = utf16_to_byte(&display, self.editor.cursor().character);

            if !self.preedit.is_empty() {
                // The preedit is drawn on the cursor line without entering the buffer.
                display.insert_str(caret, &self.preedit);
                let end = caret + self.preedit.len();
                return (display, caret..end);
            }

            if caret >= display.len() {
                display.push(' ');
            }
            let end = display[caret..]
                .chars()
                .next()
                .map(|character| caret + character.len_utf8())
                .unwrap_or(caret + 1);
            return (display, caret..end);
        }

        let length = display
            .chars()
            .map(|character| character.len_utf16() as u32)
            .sum();
        let Some((from, to)) =
            selection_on_line(self.editor.selection(), self.editor.cursor().line, length)
        else {
            // An empty selection at the end of the document still needs a marker.
            display.push(' ');
            let start = display.len() - 1;
            let end = display.len();
            return (display, start..end);
        };

        let from = utf16_to_byte(&display, from);
        let mut to = utf16_to_byte(&display, to);

        if to <= from {
            // Selecting only the line break: mark the end of the line.
            display.push(' ');
            to = display.len();
        }

        (display, from..to)
    }
}

impl EditorView {
    /// Length of the preedit text in UTF-16 code units.
    fn preedit_len_utf16(&self) -> usize {
        self.preedit.encode_utf16().count()
    }

    /// Flat UTF-16 offset of the caret in the text sent to the core.
    fn caret_utf16(&self) -> usize {
        self.editor.utf16_offset(self.editor.cursor())
    }

    fn clear_preedit(&mut self) {
        self.preedit.clear();
        self.preedit_selection = None;
    }

    /// Returns text in flat UTF-16 offsets, with the preedit spliced in at the caret.
    ///
    /// The preedit never enters the document, so the input method only sees it through
    /// this view.
    fn document_text(&self, range: std::ops::Range<usize>) -> String {
        if self.preedit.is_empty() {
            return self.editor.text_utf16(range);
        }

        let caret = self.caret_utf16();
        let preedit_end = caret + self.preedit_len_utf16();
        let mut text = String::new();

        if range.start < caret {
            text.push_str(&self.editor.text_utf16(range.start..range.end.min(caret)));
        }

        if range.start <= preedit_end && range.end > caret {
            let from = range.start.saturating_sub(caret).min(preedit_end - caret);
            let to = range.end.saturating_sub(caret).min(preedit_end - caret);
            let from = utf16_to_byte(&self.preedit, from as u32);
            let to = utf16_to_byte(&self.preedit, to as u32);
            text.push_str(&self.preedit[from..to]);
        }

        if range.end > preedit_end {
            let start = range.start.max(preedit_end) - preedit_end;
            text.push_str(&self.editor.text_utf16(start..range.end - preedit_end));
        }

        text
    }

    fn commit_text(&mut self, text: &str) {
        if let Action::Edit(transaction) = self.editor.insert_commit(text) {
            self.send_transaction(transaction);
            self.ensure_cursor_visible();
        }
    }

    /// Digits used for line numbers. At least four, so small files do not shift.
    fn line_number_width(&self) -> usize {
        self.editor.line_count().max(1).to_string().len().max(4)
    }

    /// Width of the line number gutter. It grows with the number of lines.
    fn gutter_width(&self) -> f32 {
        let digits = self.line_number_width();
        digits as f32 * CHARACTER_WIDTH + CHARACTER_WIDTH * 1.2
    }

    /// First line drawn in the text area.
    fn first_visible_line(&self) -> usize {
        self.scroll_top
    }

    /// Moves the viewport just enough to show the cursor.
    fn ensure_cursor_visible(&mut self) {
        self.scroll_top = scroll_to_show(
            self.editor.cursor().line as usize,
            self.scroll_top,
            self.viewport_rows,
            self.editor.line_count(),
        );
    }

    fn clamp_scroll(&mut self) {
        self.scroll_top = clamp_scroll_top(
            self.scroll_top,
            self.viewport_rows,
            self.editor.line_count(),
        );
    }
}

impl Focusable for EditorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for EditorView {
    fn text_for_range(
        &mut self,
        range_utf16: std::ops::Range<usize>,
        adjusted_range: &mut Option<std::ops::Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let length = self.editor.len_utf16() + self.preedit_len_utf16();
        let end = range_utf16.end.min(length);
        let start = range_utf16.start.min(end);
        adjusted_range.replace(start..end);
        Some(self.document_text(start..end))
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        if self.preedit.is_empty() {
            let selection = self.editor.selection();
            let start = self.editor.utf16_offset(selection.start);
            let end = self.editor.utf16_offset(selection.end);
            return Some(UTF16Selection {
                range: start..end.max(start),
                reversed: false,
            });
        }

        let caret = self.caret_utf16();
        let range = self
            .preedit_selection
            .clone()
            .unwrap_or_else(|| 0..self.preedit_len_utf16());

        Some(UTF16Selection {
            range: caret + range.start..caret + range.end,
            reversed: false,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<std::ops::Range<usize>> {
        if self.preedit.is_empty() {
            return None;
        }

        let caret = self.caret_utf16();
        Some(caret..caret + self.preedit_len_utf16())
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.preedit.is_empty() {
            return;
        }

        self.clear_preedit();
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        _range_utf16: Option<std::ops::Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.frame_log {
            eprintln!("vy: commit {text:?}");
        }

        // The marked text is kept outside the document, so the range only tells us what
        // the platform believes it replaces. The commit always lands at the caret.
        let had_preedit = !self.preedit.is_empty();
        self.clear_preedit();

        if text.is_empty() {
            if had_preedit {
                cx.notify();
            }
            return;
        }

        self.commit_text(text);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range_utf16: Option<std::ops::Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<std::ops::Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editor.mode() != Mode::Insert {
            // Typing in Normal or Select is a command, not text. Drop any preedit left
            // over from a mode change during composition.
            let had_preedit = !self.preedit.is_empty();
            self.clear_preedit();
            if had_preedit {
                cx.notify();
            }
            return;
        }

        self.preedit.clear();
        self.preedit.push_str(new_text);
        self.preedit_selection = new_selected_range_utf16;
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: std::ops::Range<usize>,
        element_bounds: Bounds<Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let position = self.editor.position_at_utf16(range_utf16.start);
        let line = self.editor.line(position.line as usize)?;
        let prefix = line.get(..utf16_to_byte(&line, position.character))?;

        let mut width = px(0.);
        if !prefix.is_empty() {
            let run = TextRun {
                len: prefix.len(),
                font: gpui::font("monospace"),
                color: rgb(0xd4d4d4).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let shaped = window.text_system().shape_line(
                SharedString::from(prefix.to_string()),
                window.rem_size(),
                &[run],
                None,
            );
            width = shaped.x_for_index(prefix.len());
        }

        let row = (position.line as usize).saturating_sub(self.first_visible_line());
        let origin = element_bounds.origin
            + point(
                px(self.gutter_width()) + width,
                px(row as f32 * LINE_HEIGHT),
            );
        Some(Bounds::new(origin, size(px(2.), px(LINE_HEIGHT))))
    }

    fn character_index_for_point(
        &mut self,
        _point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

impl Render for EditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let started = Instant::now();

        // Fit the viewport to the window before choosing which lines to draw.
        let height = f32::from(window.viewport_size().height) - CHROME_HEIGHT;
        self.viewport_rows = ((height / LINE_HEIGHT).floor().max(1.0)) as usize;
        self.clamp_scroll();

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

        let element = div()
            .id("vy-editor")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
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
                    .relative()
                    .flex_1()
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .child({
                        let view = cx.entity();
                        canvas(
                            |_, _, _| (),
                            move |bounds, _, window, cx| {
                                let focus = view.read(cx).focus_handle.clone();

                                if !view.read(cx).input_registered {
                                    view.update(cx, |view, _| {
                                        view.input_registered = true;
                                        if view.frame_log {
                                            eprintln!("vy: input bounds={bounds:?}");
                                        }
                                    });
                                }

                                window.handle_input(
                                    &focus,
                                    ElementInputHandler::new(bounds, view.clone()),
                                    cx,
                                );
                            },
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                    })
                    .children(self.visible_lines()),
            )
            .child(footer);

        self.log_frame(started);
        element
    }
}

impl EditorView {
    fn log_frame(&mut self, started: Instant) {
        if !self.frame_log {
            return;
        }

        let now = Instant::now();
        let build = now.duration_since(started).as_secs_f64() * 1000.0;

        match self.last_frame.replace(now) {
            Some(last) => {
                let gap = now.duration_since(last).as_secs_f64() * 1000.0;
                eprintln!("vy: frame gap={gap:.1}ms build={build:.2}ms");
            }
            None => eprintln!("vy: frame gap=- build={build:.2}ms"),
        }
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
            .expect("could not open the window");

        window
            .update(cx, |view, window, _cx| {
                view.focus_handle.focus(window);
            })
            .ok();

        cx.activate(true);
    });
}

/// Converts a GPUI keystroke into an editor key.
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

    // The committed text reaches Insert mode through the input handler, so commands use
    // the key name. Shift is applied here because key names are unshifted.
    let mut characters = keystroke.key.chars();
    let character = characters.next()?;
    if characters.next().is_some() {
        return None;
    }

    if keystroke.modifiers.shift && character.is_ascii_lowercase() {
        return Some(Key::Char(character.to_ascii_uppercase()));
    }

    Some(Key::Char(character))
}

/// Returns the viewport top that keeps `line` inside the window.
fn scroll_to_show(line: usize, top: usize, rows: usize, total: usize) -> usize {
    let rows = rows.max(1);
    let top = if line < top {
        line
    } else if line >= top + rows {
        line + 1 - rows
    } else {
        top
    };

    clamp_scroll_top(top, rows, total)
}

/// Clamps the viewport top so the last screen stays full.
fn clamp_scroll_top(top: usize, rows: usize, total: usize) -> usize {
    top.min(total.saturating_sub(rows.max(1)))
}

/// Moves the viewport by whole lines. Positive values go towards later lines.
fn scroll_lines(top: usize, lines: isize, rows: usize, total: usize) -> usize {
    let max_top = total.saturating_sub(rows.max(1)) as isize;
    (top as isize + lines).clamp(0, max_top) as usize
}

/// Converts a UTF-16 offset to a UTF-8 byte offset. A position inside a surrogate
/// pair is rounded down to the start of the character.
fn utf16_to_byte(text: &str, units: u32) -> usize {
    let mut count = 0u32;

    for (index, character) in text.char_indices() {
        if count >= units || units < count + character.len_utf16() as u32 {
            return index;
        }
        count += character.len_utf16() as u32;
    }

    text.len()
}

/// The part of one line covered by the selection. [`None`] if the line is outside it.
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
        // Selecting only the line break: let the caller draw a marker.
        return (to >= length).then_some((length, length));
    }

    Some((from.min(length), to))
}

fn value_of<T: Serialize>(value: &T) -> Value {
    // rmpv's serde support turns structs into tuples, so go through rmp-serde.
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

/// Resolves the URI of the document to open.
///
/// Arguments starting with `file:` or `vy:` are used as-is. Other arguments are made
/// absolute and prefixed with `file:`. With no argument, opens an empty `vy:untitled`.
fn document_uri() -> String {
    match std::env::args().nth(1) {
        Some(argument) if argument.starts_with("file:") || argument.starts_with("vy:") => argument,
        Some(path) => match std::fs::canonicalize(&path) {
            Ok(absolute) => format!("file://{}", absolute.display()),
            // Let a file that does not exist yet open with the path as given.
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
    fn converts_utf16_units_to_byte_offsets() {
        assert_eq!(utf16_to_byte("a😀b", 0), 0);
        assert_eq!(utf16_to_byte("a😀b", 1), 1);
        // A position inside a surrogate pair is rounded down.
        assert_eq!(utf16_to_byte("a😀b", 2), 1);
        assert_eq!(utf16_to_byte("a😀b", 3), 5);
        assert_eq!(utf16_to_byte("a😀b", 4), 6);
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
    fn scrolls_only_when_the_cursor_leaves_the_window() {
        // 100 lines, 10 rows, starting at the top.
        assert_eq!(scroll_to_show(5, 0, 10, 100), 0);
        assert_eq!(scroll_to_show(10, 0, 10, 100), 1);
        assert_eq!(scroll_to_show(42, 20, 10, 100), 33);
        assert_eq!(scroll_to_show(3, 20, 10, 100), 3);
    }

    #[test]
    fn keeps_the_last_screen_full() {
        assert_eq!(clamp_scroll_top(95, 10, 100), 90);
        assert_eq!(clamp_scroll_top(4, 10, 100), 4);
        assert_eq!(clamp_scroll_top(4, 10, 3), 0);
    }

    #[test]
    fn scrolls_by_lines_within_the_document() {
        assert_eq!(scroll_lines(20, 5, 10, 100), 25);
        assert_eq!(scroll_lines(20, -5, 10, 100), 15);
        assert_eq!(scroll_lines(2, -5, 10, 100), 0);
        assert_eq!(scroll_lines(88, 5, 10, 100), 90);
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
