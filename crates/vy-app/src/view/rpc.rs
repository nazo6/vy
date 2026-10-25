//! Server RPC handling and message loop for `EditorView`.

use std::time::{Duration, Instant};

use gpui::Context;
use rmpv::Value;
use vy_app::editor::{Transaction, ViewState};
use vy_app::geometry::collect_pane_ids;
use vy_app::rpc::Client;
use vy_app::state::{DocumentState, PaneState};
use vy_proto::{
    BufferDidChange, BufferResync, BufferSnapshot, BufferUndo, DidChangeParams, DocumentIdParams,
    DocumentSave, DocumentSaveParams, DocumentSavedResult, EditorOpen, EditorOpenParams,
    HistoryResult, Initialize,
    InitializeParams, Initialized, LayoutDidChange, LayoutDidChangeNotification, LayoutGet,
    LayoutGetResult, PROTOCOL_VERSION, PaneOpen, PaneOpenParams, PaneOpenResult, RpcMessage,
    RpcMethod, RpcNotification, RpcResult, SnapshotResult, SyncDocument, SyncRequest,
    SyncRequestParams, SyncRequestResult, decode_notification, decode_reply, decode_value,
};

use super::{EditorView, Pending};

impl EditorView {
    /// Starts the first connection and the reconnect loop.
    pub(super) fn connect(&mut self, cx: &mut Context<Self>) {
        self.connection_generation += 1;
        match Client::connect(self.connection.clone()) {
            Ok(mut client) => {
                let id = client.request::<Initialize>(InitializeParams {
                    protocol_version: PROTOCOL_VERSION,
                    client_version: env!("CARGO_PKG_VERSION").into(),
                    client_capabilities: vec!["session-restore".to_string()],
                    workspace: Some(self.workspace_uri.clone()),
                    resume_session: None,
                });
                self.pending.clear();
                self.pending.insert(id, Pending::Initialize);
                self.client = Some(client);
                self.reconnecting = false;
                self.status = "connected; restoring the session".into();
                self.start_receiving(cx);
            }
            Err(error) => {
                self.client = None;
                self.status = format!(
                    "could not start vy-server for {}: {error}",
                    self.connection_name
                )
                .into();
                self.schedule_reconnect(cx);
            }
        }
    }

    /// Drops the current client and connects again, without waiting for the
    /// reconnect backoff. Used by `vy.reconnect_server`.
    pub(super) fn reconnect_now(&mut self, cx: &mut Context<Self>) {
        self.connection_generation += 1;
        self.client = None;
        self.pending.clear();
        self.reconnecting = false;
        self.connect(cx);
    }

    /// Receives messages as they arrive. The reader thread pushes into an async
    /// channel, so the UI only wakes when there is something to handle.
    pub(super) fn start_receiving(&self, cx: &mut Context<Self>) {
        let Some(receiver) = self.client.as_ref().map(Client::receiver) else {
            return;
        };
        let generation = self.connection_generation;

        cx.spawn(async move |this, cx| {
            while let Ok(message) = receiver.recv().await {
                let updated = this.update(cx, |this, cx| {
                    if this.connection_generation != generation {
                        return;
                    }
                    this.handle(message);
                    cx.notify();
                });
                if updated.is_err() {
                    return;
                }
            }

            // The reader stopped: the server closed the connection.
            let _ = this.update(cx, |this, cx| {
                if this.connection_generation == generation {
                    this.handle_disconnect(cx);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn handle_disconnect(&mut self, cx: &mut Context<Self>) {
        self.client = None;
        for (_, pending) in self.pending.drain() {
            if let Pending::HostCall { request, .. } = pending {
                request.respond_error("RemoteDisconnected", "the connection to the core closed");
            }
        }
        self.stream_owners.clear();
        self.status = format!(
            "lost the connection to {}; reconnecting",
            self.connection_name
        )
        .into();
        self.schedule_reconnect(cx);
    }

    fn schedule_reconnect(&mut self, cx: &mut Context<Self>) {
        if self.reconnecting {
            return;
        }
        self.reconnecting = true;
        let generation = self.connection_generation;

        cx.spawn(async move |this, cx| {
            let mut attempt = 0u32;
            loop {
                let delay = reconnect_delay(attempt);
                cx.background_executor().timer(delay).await;

                let connected = this.update(cx, |this, cx| {
                    if this.connection_generation != generation {
                        return true;
                    }
                    this.try_reconnect(cx)
                });

                match connected {
                    Ok(true) => break,
                    Ok(false) => attempt = attempt.saturating_add(1),
                    Err(_) => break,
                }
            }

            let _ = this.update(cx, |this, _| {
                this.reconnecting = false;
            });
        })
        .detach();
    }

    /// One reconnect attempt. Returns whether it succeeded.
    fn try_reconnect(&mut self, cx: &mut Context<Self>) -> bool {
        match Client::connect(self.connection.clone()) {
            Ok(mut client) => {
                self.connection_generation += 1;
                let id = client.request::<Initialize>(InitializeParams {
                    protocol_version: PROTOCOL_VERSION,
                    client_version: env!("CARGO_PKG_VERSION").into(),
                    client_capabilities: vec!["session-restore".to_string()],
                    workspace: Some(self.workspace_uri.clone()),
                    resume_session: None,
                });
                self.pending.clear();
                self.pending.insert(id, Pending::Initialize);
                self.client = Some(client);
                self.reconnecting = false;
                self.status = "reconnected; restoring the session".into();
                self.start_receiving(cx);
                true
            }
            Err(error) => {
                self.status = format!("reconnect failed: {error}").into();
                false
            }
        }
    }

    pub(super) fn handle(&mut self, message: RpcMessage) {
        match message {
            RpcMessage::Response { id, result } => self.handle_response(id, result),
            RpcMessage::Notification { method, params } => {
                self.handle_notification(&method, params)
            }
            RpcMessage::Stream {
                stream_id, payload, ..
            } => {
                self.handle_stream(stream_id, payload);
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
            RpcResult::Err { code, message, .. } => match pending {
                Pending::HostCall { request, .. } => {
                    request.respond_error(&code.to_string(), message);
                }
                Pending::Sync => {
                    self.status = format!("{code}: {message}").into();
                    // The session could not be restored, but the frontend
                    // settings from `init.ts` still have to apply.
                    self.ensure_init_evaluated();
                }
                Pending::Close => {
                    self.status = message.into();
                }
                _ => {
                    self.status = format!("{code}: {message}").into();
                }
            },
            RpcResult::Ok { value } => match pending {
                Pending::HostCall { request, runtime } => {
                    let json_val = decode_value::<serde_json::Value>(&value)
                        .unwrap_or(serde_json::Value::Null);
                    if let Some(pid) = json_val
                        .get("pid")
                        .and_then(serde_json::Value::as_u64)
                        .map(|pid| pid as u32)
                    {
                        self.stream_owners.insert(pid, runtime);
                    }
                    request.respond_value(json_val);
                }
                Pending::Initialize => {
                    if let Some(init) = decode_reply::<Initialize>(&value)
                        && let Some(client) = self.client.as_mut()
                    {
                        client.set_session_id(init.session_id);
                    }
                }
                Pending::Layout => match decode_reply::<LayoutGet>(&value) {
                    Some(result) => self.received_layout_get(result),
                    None => self.status = "layout/get response could not be decoded".into(),
                },
                Pending::Sync => match decode_reply::<SyncRequest>(&value) {
                    Some(result) => self.received_sync(result),
                    None => self.status = "sync/request response could not be decoded".into(),
                },
                Pending::Open => match decode_reply::<PaneOpen>(&value) {
                    Some(opened) => self.opened(opened),
                    None => self.status = "pane/open response has no pane or document".into(),
                },
                Pending::Snapshot => match decode_reply::<BufferSnapshot>(&value) {
                    Some(snapshot) => self.received_snapshot(snapshot),
                    None => self.status = "snapshot could not be decoded".into(),
                },
                Pending::Undo | Pending::Redo => match decode_reply::<BufferUndo>(&value) {
                    Some(history) => self.received_history(history),
                    None => self.status = "history could not be decoded".into(),
                },
                Pending::Close | Pending::Focus | Pending::Resize => {}
                Pending::EditorOpen => match decode_reply::<EditorOpen>(&value) {
                    Some(opened) => self.editor_opened(opened),
                    None => self.status = "editor/open response has no pane or document".into(),
                },
                Pending::Save => match decode_reply::<DocumentSave>(&value) {
                    Some(saved) => self.saved(saved),
                    None => self.status = "saved -".into(),
                },
                Pending::SaveThenQuit => match decode_reply::<DocumentSave>(&value) {
                    Some(saved) => {
                        self.saved(saved);
                        self.request_quit(false);
                    }
                    None => self.status = "saved -".into(),
                },
                Pending::SaveForQuit { remaining } => match decode_reply::<DocumentSave>(&value)
                {
                    Some(saved) => {
                        self.saved(saved);
                        let mut remaining = remaining;
                        if let Some(next) = remaining.first().copied() {
                            remaining.remove(0);
                            self.request::<DocumentSave>(
                                DocumentSaveParams {
                                    document_id: next,
                                    force: Some(false),
                                },
                                Pending::SaveForQuit { remaining },
                            );
                        } else {
                            self.close_requested = true;
                        }
                    }
                    None => self.status = "saved -".into(),
                },
            },
        }
    }

    fn handle_stream(&mut self, stream_id: u64, payload: vy_proto::StreamPayload) {
        let pid = stream_id as u32;
        let owner = self.stream_owners.get(&pid).cloned();
        match payload {
            vy_proto::StreamPayload::Chunk(val) => {
                let json =
                    decode_value::<serde_json::Value>(&val).unwrap_or(serde_json::Value::Null);
                let channel = json
                    .get("channel")
                    .and_then(|v| v.as_str())
                    .unwrap_or("stdout");
                let data = json
                    .get("data")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let payload = vy_host_api::process::ProcessData { pid, data };
                if channel == "stderr" {
                    self.emit_process_event::<vy_host_api::process::OnStderr>(&owner, &payload);
                } else {
                    self.emit_process_event::<vy_host_api::process::OnStdout>(&owner, &payload);
                }
            }
            vy_proto::StreamPayload::End { summary } => {
                let exit_code = summary
                    .and_then(|s| decode_value::<serde_json::Value>(&s))
                    .and_then(|s| s.get("exit_code").and_then(|v| v.as_i64()))
                    .unwrap_or(0) as i32;
                self.emit_process_event::<vy_host_api::process::OnExit>(
                    &owner,
                    &vy_host_api::process::ProcessExit { pid, exit_code },
                );
                self.stream_owners.remove(&pid);
            }
            vy_proto::StreamPayload::Error { message, .. } => {
                self.emit_process_event::<vy_host_api::process::OnStderr>(
                    &owner,
                    &vy_host_api::process::ProcessData { pid, data: message },
                );
                self.emit_process_event::<vy_host_api::process::OnExit>(
                    &owner,
                    &vy_host_api::process::ProcessExit { pid, exit_code: -1 },
                );
                self.stream_owners.remove(&pid);
            }
        }
    }

    fn handle_notification(&mut self, method: &str, params: Value) {
        if method == Initialized::METHOD {
            self.status = "connected; restoring the session".into();
            self.request::<LayoutGet>((), Pending::Layout);
        } else if method == BufferResync::METHOD {
            match decode_notification::<BufferResync>(&params) {
                Some(resync) => self.received_snapshot(SnapshotResult {
                    document_id: resync.document_id,
                    version: resync.version,
                    line_count: None,
                    text: resync.text,
                }),
                None => self.status = "buffer/resync could not be decoded".into(),
            }
        } else if method == LayoutDidChange::METHOD {
            match decode_notification::<LayoutDidChange>(&params) {
                Some(layout) => self.received_layout(layout),
                None => self.status = "layout/did_change could not be decoded".into(),
            }
        } else {
            eprintln!("vy: unsupported notification: {method}");
        }
    }

    /// Applies the layout the core confirmed. The client never draws a tree the core
    /// has not accepted, so this is the only place panes are added or removed.
    fn received_layout(&mut self, layout_msg: LayoutDidChangeNotification) {
        if layout_msg.revision < self.layout_revision {
            return;
        }

        self.layout_revision = layout_msg.revision;
        if let Some(window_id) = layout_msg.window_id {
            self.window_id = window_id;
        }
        self.layout = layout_msg.layout;
        let next_active = layout_msg.active_pane;
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
        self.request::<PaneOpen>(
            PaneOpenParams {
                view: Some("editor".into()),
                uri,
                placement: None,
            },
            Pending::Open,
        );
    }

    /// Applies the layout the core restored after a reconnect.
    ///
    /// The client keeps its text and cursor by URI; document and pane IDs may
    /// differ from the previous connection.
    fn received_layout_get(&mut self, result: LayoutGetResult) {
        if result.layout.is_none() && result.panes.is_empty() {
            self.layout = None;
            self.layout_revision = 0;
            self.panes.clear();
            self.documents.clear();
            self.active_pane = None;
            self.open_initial_after_restore = false;
            self.open_document();
            return;
        }

        let mut previous: std::collections::HashMap<String, DocumentState> = self
            .documents
            .drain()
            .map(|(_, document)| (document.uri.clone(), document))
            .collect();

        self.layout = result.layout;
        self.layout_revision = result.revision;
        if let Some(window_id) = result.window_id {
            self.window_id = window_id;
        }

        let idle = Duration::from_millis(self.config.transaction_idle_ms());
        let mut panes = std::collections::HashMap::new();
        let mut documents = std::collections::HashMap::new();
        for info in result.panes {
            let mut pane = PaneState::new(info.pane_id, None);
            // The core saves only the view id. The tree is client state, so put
            // back the last one a script rendered for this view.
            pane.widget_tree = self.view_widget_trees.get(&info.view).cloned();
            pane.view_type = info.view;
            if let Some(document_id) = info.document_id {
                if let std::collections::hash_map::Entry::Vacant(entry) =
                    documents.entry(document_id)
                {
                    let uri = info.uri.unwrap_or_else(|| "vy:untitled".to_string());
                    let mut document = previous
                        .remove(&uri)
                        .unwrap_or_else(|| DocumentState::new(document_id, uri.clone(), "", 0));
                    document.id = document_id;
                    document.editor.set_transaction_idle(idle);
                    entry.insert(document);
                }
                pane.document_id = Some(document_id);
            }
            panes.insert(info.pane_id, pane);
        }

        self.active_pane = result.active_pane.or_else(|| panes.keys().copied().next());
        self.panes = panes;
        self.documents = documents;
        self.activate_active_view();
        self.send_sync();
    }

    /// Sends the client's open documents after a reconnect. Only dirty documents
    /// carry their text; the core reads the rest from disk.
    fn send_sync(&mut self) {
        let documents: Vec<SyncDocument> = self
            .documents
            .values()
            .map(|document| SyncDocument {
                uri: document.uri.clone(),
                version: document.editor.version(),
                saved_version: document.saved_version,
                text: document.is_dirty().then(|| document.editor.text()),
            })
            .collect();

        if documents.is_empty() {
            self.status = "reconnected; there are no open documents".into();
            self.ensure_init_evaluated();
            return;
        }

        self.status = format!("restoring {} document(s)", documents.len()).into();
        self.request::<SyncRequest>(SyncRequestParams { documents }, Pending::Sync);
    }

    /// Applies the text and versions returned by `sync/request`.
    fn received_sync(&mut self, result: SyncRequestResult) {
        let idle = Duration::from_millis(self.config.transaction_idle_ms());
        let mut documents = std::collections::HashMap::new();
        for document in result.documents {
            let mut state = DocumentState::new(
                document.document_id,
                document.uri,
                &document.text,
                document.version,
            );
            state.saved_version = document.saved_version;
            state.loaded = true;
            state.editor.set_transaction_idle(idle);
            documents.insert(document.document_id, state);
        }

        self.documents = documents;
        for pane in self.panes.values_mut() {
            if let Some(document_id) = pane.document_id
                && !self.documents.contains_key(&document_id)
            {
                pane.document_id = None;
            }
        }

        let ids: Vec<u64> = self.documents.keys().copied().collect();
        for document_id in ids {
            self.clamp_views_for_document(document_id);
        }
        self.activate_active_view();
        self.status = format!("reconnected; restored {} document(s)", self.documents.len()).into();

        if self.open_initial_after_restore {
            self.open_initial_after_restore = false;
            let already_open = self
                .documents
                .values()
                .any(|document| document.uri == self.initial_uri);
            if !already_open && self.initial_uri != "vy:untitled" {
                self.request::<EditorOpen>(
                    EditorOpenParams {
                        uri: self.initial_uri.clone(),
                        pane_id: self.active_pane,
                    },
                    Pending::EditorOpen,
                );
                // `init.ts` reads the active document, so wait for the snapshot
                // of the startup file. A restored session has no snapshot of
                // its own.
                return;
            }
        }

        self.ensure_init_evaluated();
    }

    /// Evaluates `init.ts` once the client has a document to read.
    ///
    /// A session restored from the core has no first snapshot, so the sync reply
    /// is the point where `init.ts` can run. Later reconnects keep the
    /// registrations from the first evaluation.
    fn ensure_init_evaluated(&mut self) {
        if self.init_evaluated {
            return;
        }
        self.init_evaluated = true;
        self.evaluate_init();
    }

    fn opened(&mut self, opened: PaneOpenResult) {
        let pane_id = opened.pane_id;
        let document_id = opened.document_id;

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
                let pane = entry.get_mut();
                pane.document_id = Some(document_id);
                if let Some(view_type) = self.pending_view_for_next_open.take() {
                    pane.widget_tree = self.view_widget_trees.get(&view_type).cloned();
                    pane.view_type = view_type;
                }
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                let mut pane = PaneState::new(pane_id, Some(document_id));
                pane.view = inherited;
                pane.scroll_top = inherited_scroll;
                if let Some(view_type) = self.pending_view_for_next_open.take() {
                    pane.widget_tree = self.view_widget_trees.get(&view_type).cloned();
                    pane.view_type = view_type;
                }
                entry.insert(pane);
            }
        }
        if self.active_pane.is_none() {
            self.active_pane = Some(pane_id);
            self.activate_active_view();
        }

        if !self.documents.contains_key(&document_id) {
            let uri = if opened.uri.is_empty() {
                "vy:untitled".into()
            } else {
                opened.uri
            };
            let idle = Duration::from_millis(self.config.transaction_idle_ms());
            let mut document = DocumentState::new(document_id, uri, "", 0);
            document.editor.set_transaction_idle(idle);
            self.documents.insert(document_id, document);
        }

        self.request::<BufferSnapshot>(DocumentIdParams { document_id }, Pending::Snapshot);
    }

    fn editor_opened(&mut self, opened: PaneOpenResult) {
        let pane_id = opened.pane_id;
        let document_id = opened.document_id;
        let replaced = opened.replaced;

        // The old pane state belongs to the old document.
        self.store_active_view();

        if !self.documents.contains_key(&document_id) {
            let uri = if opened.uri.is_empty() {
                "vy:untitled".into()
            } else {
                opened.uri
            };
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
        self.request::<BufferSnapshot>(DocumentIdParams { document_id }, Pending::Snapshot);
    }

    fn saved(&mut self, saved: DocumentSavedResult) {
        if let Some(document) = self.documents.get_mut(&saved.document_id) {
            document.saved_version = saved.version;
        }
        self.status = format!("saved {}", saved.path).into();
    }

    fn received_snapshot(&mut self, snapshot: SnapshotResult) {
        let started = Instant::now();
        let document_id = snapshot.document_id;
        let text = snapshot.text;
        let version = snapshot.version;

        let mut reset = false;
        if let Some(document) = self.documents.get_mut(&document_id) {
            // A second pane on the same buffer asks for a snapshot too. Keep the
            // per-pane cursor when the copy is already at that version.
            if !document.loaded || document.editor.version() != version {
                document.editor.reset(&text, version);
                document.saved_version = version;
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

        if let Some(document) = self.documents.get(&document_id) {
            let lang = super::plugin::detect_language_id(&document.uri);
            self.activate_plugins_for_language(lang);
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
        self.ensure_init_evaluated();
    }

    fn received_history(&mut self, history: HistoryResult) {
        self.activate_active_view();
        let version = history.version;
        let batches = history.edits;
        let selection = history.selection;

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

    pub(super) fn request<M: RpcMethod>(&mut self, params: M::Params, pending: Pending) {
        let Some(client) = self.client.as_mut() else {
            return;
        };
        let id = client.request::<M>(params);
        self.pending.insert(id, pending);
    }

    pub(super) fn send_transaction(&mut self, transaction: Transaction) {
        let Some(document_id) = self.active_document_id() else {
            return;
        };
        let Some(client) = self.client.as_mut() else {
            return;
        };

        client.notify::<BufferDidChange>(DidChangeParams {
            document_id,
            base_version: transaction.base_version,
            version: transaction.base_version + 1,
            transaction_id: transaction.transaction_id,
            edits: transaction.edits,
            selection_before: transaction.selection_before,
            selection_after: transaction.selection_after,
        });
    }

    pub(super) fn send_history<M: RpcMethod<Params = DocumentIdParams>>(
        &mut self,
        pending: Pending,
    ) {
        let Some(document_id) = self.active_document_id() else {
            return;
        };
        let Some(client) = self.client.as_mut() else {
            return;
        };

        let id = client.request::<M>(DocumentIdParams { document_id });
        self.pending.insert(id, pending);
    }
}

/// Backoff for automatic reconnects. The first retry is quick because a local
/// server usually restarts immediately.
fn reconnect_delay(attempt: u32) -> Duration {
    let millis = 500u64.saturating_mul(1 << attempt.min(4));
    Duration::from_millis(millis.min(5_000))
}
