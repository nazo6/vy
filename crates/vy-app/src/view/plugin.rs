//! Script and plugin host integration for .

use std::time::Duration;

use gpui::Context;
use vy_app::commands::Invocation;
use vy_app::plugin_host::{self, DocumentInfo, HostAction, InitLoad, PluginHost, RequestContext};
use vy_app::workspace::workspace_info;
use vy_js::{HostRequest, ScriptEvent};

use super::{EditorView, Pending};

impl EditorView {
    pub(super) fn start_script(&mut self, cx: &mut Context<Self>) {
        let Some(plugin) = self.plugin.as_ref() else {
            return;
        };
        let events = plugin.events();
        let requests = plugin.requests();
        let generation = self.script_generation;

        cx.spawn(async move |this, cx| {
            while let Ok(event) = events.recv().await {
                let updated = this.update(cx, |this, cx| {
                    if this.script_generation == generation {
                        this.handle_script_event(event);
                        cx.notify();
                    }
                });
                if updated.is_err() {
                    return;
                }
            }
        })
        .detach();

        cx.spawn(async move |this, cx| {
            while let Ok(request) = requests.recv().await {
                let updated = this.update(cx, |this, cx| {
                    if this.script_generation == generation {
                        this.handle_host_request(request);
                        cx.notify();
                    }
                });
                if updated.is_err() {
                    return;
                }
            }
        })
        .detach();
    }

    /// Starts a fresh runtime and evaluates `init.ts`. Registrations from the
    /// previous runtime are dropped first.
    pub(super) fn reload_init(&mut self, cx: &mut Context<Self>) {
        self.command_queue.clear();
        self.running_command = None;
        self.commands
            .unregister_owner(&vy_app::commands::Owner::Init);
        self.keymap.unbind_owner(&vy_app::commands::Owner::Init);
        self.config.clear_memory();

        let (env, warnings) = plugin_host::script_env(&workspace_info(), &self.paths);
        for warning in &warnings {
            eprintln!("vy: trusted.json: {warning}");
        }
        self.plugin = Some(PluginHost::spawn(env, self.paths.init_ts()));
        self.script_generation += 1;
        self.start_script(cx);
        self.evaluate_init();
    }

    /// Hands `init.ts` to the script thread. Missing files are not an error.
    pub(super) fn evaluate_init(&mut self) {
        let Some(plugin) = self.plugin.as_mut() else {
            return;
        };
        match plugin.evaluate_init() {
            InitLoad::Missing => {
                self.status = "no init.ts".into();
            }
            InitLoad::Loaded => {
                self.status = "loading init.ts".into();
            }
            InitLoad::Failed(message) => {
                eprintln!("vy: {message}");
                self.status = message.into();
            }
        }
    }

    pub(super) fn handle_script_event(&mut self, event: ScriptEvent) {
        match event {
            ScriptEvent::Evaluated => {
                self.status = "init.ts loaded".into();
            }
            ScriptEvent::CommandFinished { id, .. } => {
                if self.frame_log {
                    eprintln!("vy: command {id} finished");
                }
                if self.running_command.as_deref() == Some(id.as_str()) {
                    self.running_command = None;
                    self.drain_command_queue();
                }
            }
            ScriptEvent::Error { message } => {
                // The log file already has the message; the status line is the
                // shortest path to the user.
                eprintln!("vy: init.ts: {message}");
                self.status = format!("init.ts: {message}").into();
                // A failed command will not send CommandFinished. Do not leave the
                // queue blocked.
                if self.running_command.take().is_some() {
                    self.drain_command_queue();
                }
            }
            ScriptEvent::Stopped => {
                self.running_command = None;
                self.command_queue.clear();
                self.status = "the script host stopped".into();
            }
        }
    }

    /// Answers a call from the shim with the current client state.
    pub(super) fn handle_host_request(&mut self, request: HostRequest) {
        self.activate_active_view();
        let document = self.document_info();
        let workspace = workspace_info();
        let Some(plugin) = self.plugin.as_ref() else {
            request.respond_error("RemoteDisconnected", "the script host is not running");
            return;
        };

        let editor = {
            let id = self.active_document_id();
            match id.and_then(|id| self.documents.get_mut(&id)) {
                Some(document) => &mut document.editor,
                None => &mut self.fallback.editor,
            }
        };
        let context = RequestContext {
            editor,
            config: &mut self.config,
            keymap: &mut self.keymap,
            commands: &mut self.commands,
            document,
            workspace: &workspace,
            paths: &self.paths,
            script: Some(plugin.script()),
            statusline: Some(&mut self.statusline),
            gutter: Some(&mut self.gutter),
            hover: Some(&mut self.hover),
        };
        let outcome = plugin_host::handle_request(context, request);

        if let Some(status) = outcome.status.clone() {
            self.status = status.into();
        }

        if !outcome.config_changed.is_empty() {
            plugin.emit_config_change(outcome.config_changed.clone());
            let idle = Duration::from_millis(self.config.transaction_idle_ms());
            let timeout = Duration::from_millis(self.config.sequence_timeout_ms());
            self.set_transaction_idle(idle);
            self.keymap.set_sequence_timeout(timeout);
        }

        self.store_active_view();

        match outcome.action {
            HostAction::None => {}
            HostAction::Edit(transaction) => {
                self.send_transaction(transaction);
                self.clamp_active_views();
            }
            HostAction::Undo => self.send_history("buffer/undo", Pending::Undo),
            HostAction::Redo => self.send_history("buffer/redo", Pending::Redo),
            HostAction::Pane(command) => self.run_pane_command(command),
            HostAction::Document(command) => {
                self.run_document_command(command, &Invocation::palette(command.id()));
            }
            HostAction::Ui(command) => self.run_ui_command(command),
            HostAction::RenderWidgetTree { view_id: _, tree } => {
                if let Some(pane_id) = self.active_pane
                    && let Some(pane) = self.panes.get_mut(&pane_id)
                {
                    pane.view_type = "ui".to_string();
                    pane.widget_tree = Some(tree);
                }
            }
            HostAction::ShowPicker {
                id,
                items,
                placeholder,
                request,
            } => {
                self.open_custom_picker(id, items, placeholder, request);
            }
        }

        self.ensure_cursor_visible();
    }

    pub(super) fn document_info(&self) -> Option<DocumentInfo> {
        let document = self.active_document()?;
        Some(DocumentInfo {
            id: document.id,
            uri: document.uri.clone(),
            language_id: "text".to_string(),
        })
    }
}
