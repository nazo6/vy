//! Script and plugin host integration for .

use std::time::Duration;

use gpui::Context;
use vy_app::commands::Invocation;
use vy_app::plugin_host::{self, DocumentInfo, HostAction, InitLoad, PluginHost, RequestContext};
use vy_app::workspace::workspace_info;
use vy_core::Uri;
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
        self.init_plugins();
    }

    /// Initializes trusted plugins: pre-registers contributed commands and activates
    /// plugins triggered by `onStartup` or `onWorkspaceContains`.
    pub(super) fn init_plugins(&mut self) {
        self.register_contributed_commands();

        // 1. Activate onStartup plugins
        let startup = self.plugin_manager.plugins_for_startup();
        for plugin in startup {
            self.evaluate_discovered_plugin(&plugin);
        }

        // 2. Activate onWorkspaceContains plugins if workspace is open
        if let Some(workspace) = self.workspace.as_deref() {
            let ws_plugins = self.plugin_manager.plugins_for_workspace(workspace);
            for plugin in ws_plugins {
                self.evaluate_discovered_plugin(&plugin);
            }
        }
    }

    /// Pre-registers contributed commands from all trusted plugins into the command registry.
    pub(super) fn register_contributed_commands(&mut self) {
        for (cmd_id, title, plugin_id) in self.plugin_manager.contributed_commands() {
            let _ = self
                .commands
                .register_contributed(cmd_id, title, &plugin_id);
        }
    }

    /// Checks if executing `command_id` triggers any lazy-loaded plugin and activates them.
    pub(super) fn activate_plugins_for_command(&mut self, command_id: &str) {
        let to_activate = self.plugin_manager.plugins_for_command(command_id);
        for plugin in to_activate {
            self.evaluate_discovered_plugin(&plugin);
        }
    }

    /// Checks if displaying `view_id` triggers any lazy-loaded plugin and activates them.
    pub(super) fn activate_plugins_for_view(&mut self, view_id: &str) {
        let to_activate = self.plugin_manager.plugins_for_view(view_id);
        for plugin in to_activate {
            self.evaluate_discovered_plugin(&plugin);
        }
    }

    /// Checks if opening `language_id` triggers any lazy-loaded plugin and activates them.
    pub(super) fn activate_plugins_for_language(&mut self, language_id: &str) {
        let to_activate = self.plugin_manager.plugins_for_language(language_id);
        for plugin in to_activate {
            self.evaluate_discovered_plugin(&plugin);
        }
    }

    /// Handles post-approval setup for a plugin: registers its commands and triggers
    /// activation if it matches startup or the current workspace/document.
    pub(super) fn on_plugin_approved(&mut self, plugin_id: &str) {
        self.register_contributed_commands();

        // Activate if it has onStartup
        let startup = self.plugin_manager.plugins_for_startup();
        for plugin in startup {
            if plugin.manifest.id == plugin_id {
                self.evaluate_discovered_plugin(&plugin);
                return;
            }
        }

        // Activate if workspace matches
        if let Some(workspace) = self.workspace.as_deref() {
            let ws_plugins = self.plugin_manager.plugins_for_workspace(workspace);
            for plugin in ws_plugins {
                if plugin.manifest.id == plugin_id {
                    self.evaluate_discovered_plugin(&plugin);
                    return;
                }
            }
        }
    }

    /// Evaluates a single discovered plugin's frontend entry point.
    pub(super) fn evaluate_discovered_plugin(&mut self, plugin: &vy_app::plugin::DiscoveredPlugin) {
        let Some(main) = plugin.main_path() else {
            return;
        };
        let Some(host) = self.plugin.as_mut() else {
            return;
        };

        match host.evaluate_file(main) {
            InitLoad::Loaded => {
                eprintln!("vy: loaded plugin '{}'", plugin.manifest.id);
            }
            InitLoad::Missing => {
                eprintln!("vy: plugin '{}' main entry not found", plugin.manifest.id);
            }
            InitLoad::Failed(msg) => {
                eprintln!("vy: plugin '{}' failed: {msg}", plugin.manifest.id);
            }
        }
    }

    pub(super) fn handle_script_event(&mut self, event: ScriptEvent) {
        match event {
            ScriptEvent::Evaluated => {
                self.status = "init.ts loaded".into();
            }
            ScriptEvent::EvalFinished { result } => {
                self.status = result.into();
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
            env: plugin.env(),
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
            HostAction::Document { command, args } => {
                let invocation = args
                    .map(|a| Invocation::script(command.id(), a))
                    .unwrap_or_else(|| Invocation::palette(command.id()));
                self.run_document_command(command, &invocation);
            }
            HostAction::Ui { command, args } => {
                let invocation = args
                    .map(|a| Invocation::script(command.id(), a))
                    .unwrap_or_else(|| Invocation::palette(command.id()));
                self.run_ui_command(command, &invocation);
            }
            HostAction::RenderWidgetTree { view_id, tree } => {
                self.activate_plugins_for_view(&view_id);
                for pane in self.panes.values_mut() {
                    if pane.view_type == view_id {
                        pane.widget_tree = Some(tree.clone());
                    }
                }
                self.view_widget_trees.insert(view_id, tree);
            }
            HostAction::ShowPicker {
                id,
                items,
                placeholder,
                request,
            } => {
                self.open_custom_picker(id, items, placeholder, request);
            }
            HostAction::ServerRpc {
                method,
                params,
                request,
            } => {
                let Some(client) = self.client.as_mut() else {
                    request.respond_error("RemoteDisconnected", "not connected to vy-server");
                    return;
                };
                let id = client.request(&method, params);
                self.pending.insert(id, Pending::HostCall { request });
            }
        }

        self.ensure_cursor_visible();
    }

    pub(super) fn document_info(&self) -> Option<DocumentInfo> {
        let document = self.active_document()?;
        let language_id = detect_language_id(&document.uri).to_string();
        Some(DocumentInfo {
            id: document.id,
            uri: Uri::parse(document.uri.clone()).ok()?,
            language_id,
        })
    }

    /// Checks if active editor or selections changed and emits events to the script thread.
    pub(super) fn check_and_emit_editor_events(&mut self) {
        let Some(plugin) = self.plugin.as_ref() else {
            return;
        };

        // 1. Check active document change
        let current_doc_id = self.active_document().map(|d| d.id);
        if current_doc_id != self.last_reported_document_id {
            self.last_reported_document_id = current_doc_id;
            let info = self.document_info().map(|doc| vy_js::DocumentInfoReply {
                id: doc.id,
                uri: doc.uri.clone(),
                language_id: doc.language_id.clone(),
                version: self.editor().version(),
                line_count: self.editor().line_count(),
                end: self.editor().position_at_utf16(self.editor().len_utf16()),
            });
            plugin.emit_active_editor_change(info);
        }

        // 2. Check selections change
        let current_selections = self.editor().selections().to_vec();
        if self.last_reported_selections.as_ref() != Some(&current_selections) {
            self.last_reported_selections = Some(current_selections.clone());
            plugin.emit_selection_change(current_selections);
        }
    }
}

/// Detects a basic language identifier from a file URI or path extension.
pub fn detect_language_id(uri: &str) -> &'static str {
    let path = uri.split('?').next().unwrap_or(uri);
    if let Some(ext) = path.rsplit('.').next() {
        match ext.to_lowercase().as_str() {
            "rs" => "rust",
            "ts" | "mts" | "cts" => "typescript",
            "js" | "mjs" | "cjs" => "javascript",
            "json" => "json",
            "md" | "markdown" => "markdown",
            "py" => "python",
            "toml" => "toml",
            "yaml" | "yml" => "yaml",
            "html" | "htm" => "html",
            "css" => "css",
            "sh" | "bash" => "shellscript",
            "c" | "h" => "c",
            "cpp" | "hpp" | "cc" => "cpp",
            _ => "text",
        }
    } else {
        "text"
    }
}
