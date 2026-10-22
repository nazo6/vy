//! Server RPC handling and message loop for `EditorView`.

use std::time::{Duration, Instant};

use gpui::Context;
use rmpv::Value;
use vy_app::editor::{Transaction, ViewState};
use vy_app::geometry::collect_pane_ids;
use vy_app::rpc::{Client, decode_value, value_of};
use vy_app::state::{DocumentState, PaneState};
use vy_core::{LayoutNode, Selection, TextEdit};
use vy_proto::{RpcMessage, RpcResult, get, object};

use super::{EditorView, Pending};

impl EditorView {
    /// Receives messages as they arrive. The reader thread pushes into an async
    /// channel, so the UI only wakes when there is something to handle.
    pub(super) fn start_receiving(&self, cx: &mut Context<Self>) {
        let Some(receiver) = self.client.as_ref().map(Client::receiver) else {
            return;
        };

        cx.spawn(async move |this, cx| {
            while let Ok(message) = receiver.recv().await {
                let updated = this.update(cx, |this, cx| {
                    this.handle(message);
                    cx.notify();
                });
                if updated.is_err() {
                    return;
                }
            }

            // The reader stopped: the server closed the connection.
            let _ = this.update(cx, |this, cx| {
                if this.client.is_some() {
                    this.status = "lost the connection to vy-server; restart it manually".into();
                    this.client = None;
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn handle(&mut self, message: RpcMessage) {
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
                Pending::Close | Pending::Focus => {}
                Pending::EditorOpen => self.editor_opened(&value),
                Pending::Save => self.saved(&value),
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
            "layout/did_change" => self.received_layout(&params),
            other => {
                eprintln!("vy: unsupported notification: {other}");
            }
        }
    }

    /// Applies the layout the core confirmed. The client never draws a tree the core
    /// has not accepted, so this is the only place panes are added or removed.
    fn received_layout(&mut self, params: &Value) {
        let revision = get(params, "revision")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        if revision < self.layout_revision {
            return;
        }

        self.layout_revision = revision;
        self.window_id = get(params, "window_id")
            .and_then(Value::as_u64)
            .unwrap_or(self.window_id);
        self.layout = get(params, "layout").and_then(decode_value::<LayoutNode>);
        let next_active = get(params, "active_pane").and_then(Value::as_u64);
        self.switch_active_pane(next_active);

        let mut pane_ids = Vec::new();
        if let Some(layout) = &self.layout {
            collect_pane_ids(layout, &mut pane_ids);
        }
        for pane_id in &pane_ids {
            self.panes
                .entry(*pane_id)
                .or_insert_with(|| PaneState::new(*pane_id, None));
        }
        self.panes.retain(|pane_id, _| pane_ids.contains(pane_id));

        let used: Vec<u64> = self
            .panes
            .values()
            .filter_map(|pane| pane.document_id)
            .collect();
        self.documents.retain(|id, _| used.contains(id));
        self.activate_active_view();
    }

    fn open_document(&mut self) {
        let uri = self.initial_uri.clone();
        self.request(
            "pane/open",
            object([("uri", Value::from(uri))]),
            Pending::Open,
        );
    }

    fn opened(&mut self, value: &Value) {
        let (Some(pane_id), Some(document_id)) = (
            get(value, "pane_id").and_then(Value::as_u64),
            get(value, "document_id").and_then(Value::as_u64),
        ) else {
            self.status = "pane/open response has no pane or document".into();
            return;
        };

        let inherited = if self.documents.contains_key(&document_id) {
            self.active_view().unwrap_or_default()
        } else {
            ViewState::default()
        };
        let inherited_scroll = if self.documents.contains_key(&document_id) {
            self.active_pane_state()
                .map(|pane| pane.scroll_top)
                .unwrap_or_default()
        } else {
            0
        };
        match self.panes.entry(pane_id) {
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                entry.get_mut().document_id = Some(document_id);
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                let mut pane = PaneState::new(pane_id, Some(document_id));
                pane.view = inherited;
                pane.scroll_top = inherited_scroll;
                entry.insert(pane);
            }
        }
        if self.active_pane.is_none() {
            self.active_pane = Some(pane_id);
            self.activate_active_view();
        }

        if !self.documents.contains_key(&document_id) {
            let uri = get(value, "uri")
                .and_then(Value::as_str)
                .unwrap_or("vy:untitled")
                .to_string();
            let idle = Duration::from_millis(self.config.transaction_idle_ms());
            let mut document = DocumentState::new(document_id, uri, "", 0);
            document.editor.set_transaction_idle(idle);
            self.documents.insert(document_id, document);
        }

        self.request(
            "buffer/snapshot",
            object([("document_id", Value::from(document_id))]),
            Pending::Snapshot,
        );
    }

    fn editor_opened(&mut self, value: &Value) {
        let (Some(pane_id), Some(document_id)) = (
            get(value, "pane_id").and_then(Value::as_u64),
            get(value, "document_id").and_then(Value::as_u64),
        ) else {
            self.status = "editor/open response has no pane or document".into();
            return;
        };
        let replaced = get(value, "replaced")
            .and_then(Value::as_bool)
            .unwrap_or(true);

        // The old pane state belongs to the old document.
        self.store_active_view();

        if !self.documents.contains_key(&document_id) {
            let uri = get(value, "uri")
                .and_then(Value::as_str)
                .unwrap_or("vy:untitled")
                .to_string();
            let idle = Duration::from_millis(self.config.transaction_idle_ms());
            let mut document = DocumentState::new(document_id, uri, "", 0);
            document.editor.set_transaction_idle(idle);
            self.documents.insert(document_id, document);
        }

        let pane = self
            .panes
            .entry(pane_id)
            .or_insert_with(|| PaneState::new(pane_id, None));
        pane.document_id = Some(document_id);
        if replaced {
            pane.view = ViewState::default();
            pane.preedit.clear();
            pane.preedit_selection = None;
            pane.scroll_top = 0;
        }

        self.active_pane = Some(pane_id);
        self.activate_active_view();
        self.request(
            "buffer/snapshot",
            object([("document_id", Value::from(document_id))]),
            Pending::Snapshot,
        );
    }

    fn saved(&mut self, value: &Value) {
        let path = get(value, "path").and_then(Value::as_str).unwrap_or("-");
        self.status = format!("saved {path}").into();
    }

    fn received_snapshot(&mut self, value: &Value) {
        let started = Instant::now();
        let Some(document_id) = get(value, "document_id").and_then(Value::as_u64) else {
            self.status = "snapshot has no document_id".into();
            return;
        };
        let text = get(value, "text")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let version = get(value, "version")
            .and_then(Value::as_u64)
            .unwrap_or_default();

        let mut reset = false;
        if let Some(document) = self.documents.get_mut(&document_id) {
            // A second pane on the same buffer asks for a snapshot too. Keep the
            // per-pane cursor when the copy is already at that version.
            if !document.loaded || document.editor.version() != version {
                document.editor.reset(text, version);
                reset = true;
            }
            document.loaded = true;
            if self.frame_log {
                eprintln!(
                    "vy: snapshot {} lines in {:.1}ms",
                    document.editor.line_count(),
                    started.elapsed().as_secs_f64() * 1000.0
                );
            }
        }
        if reset {
            self.reset_views_for_document(document_id);
            self.activate_active_view();
        }

        let (line_count, version) = self
            .documents
            .get(&document_id)
            .map(|document| (document.editor.line_count(), document.editor.version()))
            .unwrap_or_default();
        self.status = format!(
            "{} lines / version {} / pane {}",
            line_count,
            version,
            self.active_pane
                .map(|pane| pane.to_string())
                .unwrap_or_else(|| "-".into())
        )
        .into();

        // `init.ts` reads documents and settings, so it runs after the first
        // snapshot and before any user command.
        if !self.init_evaluated {
            self.init_evaluated = true;
            self.evaluate_init();
        }
    }

    fn received_history(&mut self, value: &Value) {
        self.activate_active_view();
        let version = get(value, "version")
            .and_then(Value::as_u64)
            .unwrap_or_else(|| self.editor().version());
        let batches = get(value, "edits")
            .and_then(decode_value::<Vec<Vec<TextEdit>>>)
            .unwrap_or_default();
        let selection = get(value, "selection")
            .and_then(decode_value::<Vec<Selection>>)
            .unwrap_or_default();

        let result = {
            let Some(document) = self.active_document_mut() else {
                return;
            };
            document.editor.apply_history(&batches, version, &selection)
        };
        if let Err(error) = result {
            self.status = format!("could not apply the history: {error}").into();
        }

        self.store_active_view();
        if let Some(document_id) = self.active_document_id() {
            self.clamp_views_for_document(document_id);
            self.activate_active_view();
        }
    }

    pub(super) fn request(&mut self, method: &str, params: Value, pending: Pending) {
        let Some(client) = self.client.as_mut() else {
            return;
        };
        let id = client.request(method, params);
        self.pending.insert(id, pending);
    }

    pub(super) fn send_transaction(&mut self, transaction: Transaction) {
        let Some(document_id) = self.active_document_id() else {
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

    pub(super) fn send_history(&mut self, method: &str, pending: Pending) {
        let Some(document_id) = self.active_document_id() else {
            return;
        };
        let Some(client) = self.client.as_mut() else {
            return;
        };

        let id = client.request(method, object([("document_id", Value::from(document_id))]));
        self.pending.insert(id, pending);
    }
}
