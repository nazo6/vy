//! Script and plugin host integration for .

use std::time::Duration;

use gpui::Context;
use vy_app::commands::{Invocation, Owner};
use vy_app::plugin::DiscoveredPlugin;
use vy_app::plugin_host::{self, DocumentInfo, HostAction, InitLoad, PluginHost, RequestContext};
use vy_core::Uri;
use vy_js::{HostRequest, ScriptEvent};

use super::{EditorView, Pending, PluginRuntime};
use crate::theme::Theme;

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
                        this.handle_host_request(cx, request);
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

    /// Polls for plugin runtimes that were created outside a GPUI task and
    /// starts their event and request pumps.
    pub(super) fn start_plugin_supervisor(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let updated = this.update(cx, |this, cx| this.start_pending_plugins(cx));
                if updated.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn start_pending_plugins(&mut self, cx: &mut Context<Self>) {
        let pending: Vec<String> = self
            .plugin_runtimes
            .iter()
            .filter(|(_, runtime)| !runtime.started)
            .map(|(id, _)| id.clone())
            .collect();
        for plugin_id in pending {
            if let Some(runtime) = self.plugin_runtimes.get_mut(&plugin_id) {
                runtime.started = true;
            }
            self.start_plugin_script(&plugin_id, cx);
        }
    }

    fn start_plugin_script(&mut self, plugin_id: &str, cx: &mut Context<Self>) {
        let Some(runtime) = self.plugin_runtimes.get(plugin_id) else {
            return;
        };
        let events = runtime.host.events();
        let requests = runtime.host.requests();
        let generation = runtime.generation;

        let id = plugin_id.to_string();
        cx.spawn(async move |this, cx| {
            while let Ok(event) = events.recv().await {
                let id = id.clone();
                let updated = this.update(cx, |this, cx| {
                    let active = this
                        .plugin_runtimes
                        .get(&id)
                        .is_some_and(|runtime| runtime.generation == generation);
                    if active {
                        this.handle_plugin_script_event(&id, event);
                        cx.notify();
                    }
                });
                if updated.is_err() {
                    return;
                }
            }
        })
        .detach();

        let id = plugin_id.to_string();
        cx.spawn(async move |this, cx| {
            while let Ok(request) = requests.recv().await {
                let id = id.clone();
                let updated = this.update(cx, |this, cx| {
                    let active = this
                        .plugin_runtimes
                        .get(&id)
                        .is_some_and(|runtime| runtime.generation == generation);
                    if active {
                        this.handle_plugin_host_request(cx, &id, request);
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

        let (env, warnings) = plugin_host::script_env_with_remote(
            &self.workspace_info(),
            &self.paths,
            self.remote_env(),
        );
        for warning in &warnings {
            eprintln!("vy: trusted.json: {warning}");
        }
        self.plugin = Some(PluginHost::spawn(env, self.paths.init_ts()));
        self.script_generation += 1;
        self.start_script(cx);
        self.evaluate_init();
    }

    /// Replaces one plugin runtime after its source changed.
    pub(super) fn reload_plugin(&mut self, plugin_id: &str) {
        let Some(plugin) = self
            .plugin_manager
            .state(plugin_id)
            .map(|state| state.plugin().clone())
        else {
            return;
        };
        let owner = Owner::Plugin(plugin_id.to_string());
        self.commands.unregister_owner(&owner);
        self.keymap.unbind_owner(&owner);
        self.plugin_runtimes.remove(plugin_id);
        self.plugin_manager.mark_active(plugin_id);
        self.register_contributed_commands();
        self.evaluate_discovered_plugin(&plugin);
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
        if let Some(workspace) = self.local_workspace.as_deref() {
            let ws_plugins = self.plugin_manager.plugins_for_workspace(workspace);
            for plugin in ws_plugins {
                self.evaluate_discovered_plugin(&plugin);
            }
        }
    }

    /// Pre-registers contributed commands from all trusted plugins into the command registry.
    pub(super) fn register_contributed_commands(&mut self) {
        for command in self.plugin_manager.contributed_commands() {
            let category = command
                .category
                .as_deref()
                .and_then(vy_app::commands::CommandCategory::parse);
            if let Err(error) = self.commands.register_contributed(
                command.id,
                command.title,
                &command.plugin_id,
                category,
                command.ex_names,
            ) {
                eprintln!("vy: plugins: {}: {error}", command.plugin_id);
            }
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
        if let Some(workspace) = self.local_workspace.as_deref() {
            let ws_plugins = self.plugin_manager.plugins_for_workspace(workspace);
            for plugin in ws_plugins {
                if plugin.manifest.id == plugin_id {
                    self.evaluate_discovered_plugin(&plugin);
                    return;
                }
            }
        }
    }

    /// Creates a runtime for one plugin and evaluates its frontend entry point.
    pub(super) fn evaluate_discovered_plugin(&mut self, plugin: &DiscoveredPlugin) {
        let plugin_id = plugin.manifest.id.clone();
        if self.plugin_runtimes.contains_key(&plugin_id) {
            return;
        }

        // Discovery validated the entry at startup. Revalidate here because a
        // reload can pick up an edited bundle.
        let plugin = match plugin.revalidate() {
            Ok(plugin) => plugin,
            Err(error) => {
                eprintln!("vy: plugin '{plugin_id}' validation failed: {error}");
                self.plugin_manager.mark_failed(&plugin_id, error);
                return;
            }
        };
        let Some(main) = plugin.main_path() else {
            return;
        };
        if self.frame_log
            && let Some(bundle) = plugin
                .main_entry
                .as_ref()
                .and_then(|entry| entry.bundle.as_ref())
        {
            eprintln!(
                "vy: plugin '{plugin_id}' bundle {} bytes sha256 {}",
                bundle.size, bundle.sha256
            );
        }

        let env = plugin_host::plugin_script_env(
            &self.workspace_info(),
            self.remote_env(),
            &plugin_id,
            &plugin.manifest.version,
            self.plugin_manager.capabilities(&plugin_id),
        );
        let mut host = PluginHost::spawn_in(env, main.clone(), plugin.dir.clone());

        match host.evaluate_file(main) {
            InitLoad::Loaded => {
                eprintln!("vy: loaded plugin '{plugin_id}'");
            }
            InitLoad::Missing => {
                eprintln!("vy: plugin '{plugin_id}' main entry not found");
                self.plugin_manager
                    .mark_failed(&plugin_id, "main entry not found".to_string());
                return;
            }
            InitLoad::Failed(message) => {
                eprintln!("vy: plugin '{plugin_id}' failed: {message}");
                self.plugin_manager.mark_failed(&plugin_id, message);
                return;
            }
        }

        self.next_plugin_generation += 1;
        let generation = self.next_plugin_generation;
        self.plugin_runtimes.insert(
            plugin_id,
            PluginRuntime {
                host,
                generation,
                started: false,
            },
        );
    }

    pub(super) fn handle_plugin_script_event(&mut self, plugin_id: &str, event: ScriptEvent) {
        match event {
            ScriptEvent::Evaluated => {
                self.status = format!("plugin {plugin_id} loaded").into();
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
                eprintln!("vy: plugin {plugin_id}: {message}");
                self.status = format!("plugin {plugin_id}: {message}").into();
                if self.running_command.take().is_some() {
                    self.drain_command_queue();
                }
            }
            ScriptEvent::Stopped => {
                self.status = format!("plugin {plugin_id} stopped").into();
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
    pub(super) fn handle_host_request(&mut self, cx: &mut Context<Self>, request: HostRequest) {
        self.handle_host_request_for(cx, None, request);
    }

    pub(super) fn handle_plugin_host_request(
        &mut self,
        cx: &mut Context<Self>,
        plugin_id: &str,
        request: HostRequest,
    ) {
        self.handle_host_request_for(cx, Some(plugin_id), request);
    }

    fn handle_host_request_for(
        &mut self,
        cx: &mut Context<Self>,
        plugin_id: Option<&str>,
        request: HostRequest,
    ) {
        self.activate_active_view();
        let document = self.document_info();
        let workspace = self.workspace_info();

        let (script, env, owner) = match plugin_id {
            Some(plugin_id) => {
                let Some(runtime) = self.plugin_runtimes.get(plugin_id) else {
                    request
                        .respond_error("RemoteDisconnected", "the plugin runtime is not running");
                    return;
                };
                (
                    runtime.host.script(),
                    runtime.host.env(),
                    Owner::Plugin(plugin_id.to_string()),
                )
            }
            None => {
                let Some(plugin) = self.plugin.as_ref() else {
                    request.respond_error("RemoteDisconnected", "the script host is not running");
                    return;
                };
                (plugin.script(), plugin.env(), Owner::Init)
            }
        };

        // Snapshot UI focus before taking the mutable editor borrow: the
        // script thread answers `ui.focused()` from this snapshot, not from
        // live view state, because the request is handled off the view.
        let ui_focus = self.ui_focus_info();
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
            script: Some(script),
            env,
            owner,
            statusline: Some(&mut self.statusline),
            gutter: Some(&mut self.gutter),
            hover: Some(&mut self.hover),
            ui_focus,
        };
        let outcome = plugin_host::handle_request(context, request);

        if let Some(status) = outcome.status.clone() {
            self.show_notification(status, cx);
        }

        if !outcome.config_changed.is_empty() {
            self.emit_config_change_to_scripts(outcome.config_changed.clone());
            let idle = Duration::from_millis(self.config.transaction_idle_ms());
            let timeout = Duration::from_millis(self.config.sequence_timeout_ms());
            self.set_transaction_idle(idle);
            self.keymap.set_sequence_timeout(timeout);
            self.config_arc = std::sync::Arc::new(self.config.clone());

            let theme = if self.config.theme() == "light" {
                Theme::light()
            } else {
                Theme::dark()
            };
            self.update_theme(theme, cx);
        }

        self.store_active_view();

        match outcome.action {
            HostAction::None => {}
            HostAction::Edit(transaction) => {
                self.send_transaction(transaction);
                self.clamp_active_views();
            }
            HostAction::Undo => self.send_history::<vy_proto::BufferUndo>(Pending::Undo),
            HostAction::Redo => self.send_history::<vy_proto::BufferRedo>(Pending::Redo),
            HostAction::Pane(command) => {
                let invocation = Invocation::palette(command.id());
                self.run_pane_command(command, &invocation);
            }
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
            HostAction::Setting { command, args } => {
                let invocation = args
                    .map(|a| Invocation::script(command.id(), a))
                    .unwrap_or_else(|| Invocation::palette(command.id()));
                self.run_setting_command(command, &invocation);
            }
            HostAction::RenderWidgetTree { view_id, tree } => {
                self.activate_plugins_for_view(&view_id);
                for pane in self.panes.values_mut() {
                    if pane.view_type == view_id {
                        pane.set_widget_tree(Some(tree.clone()));
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
            HostAction::ServerRpc { call, request } => {
                let Some(client) = self.client.as_mut() else {
                    request.respond_error("RemoteDisconnected", "not connected to vy-server");
                    return;
                };
                let id = client.request_call(call);
                self.pending.insert(
                    id,
                    Pending::HostCall {
                        request,
                        runtime: plugin_id.map(str::to_string),
                    },
                );
            }
            HostAction::Execute { id, args } => self.execute_script_command(&id, args),
        }

        self.ensure_cursor_visible();
    }

    /// Sends a config change to every script runtime.
    pub(super) fn emit_config_change_to_scripts(&self, keys: Vec<String>) {
        if let Some(plugin) = self.plugin.as_ref() {
            plugin.emit_config_change(keys.clone());
        }
        for runtime in self.plugin_runtimes.values() {
            runtime.host.emit_config_change(keys.clone());
        }
    }

    /// Sends one typed event to every script runtime.
    pub(super) fn emit_to_scripts<E: vy_host_api::HostEvent>(&self, payload: &E::Payload) {
        if let Some(plugin) = self.plugin.as_ref() {
            plugin.script().emit_event::<E>(payload);
        }
        for runtime in self.plugin_runtimes.values() {
            runtime.host.script().emit_event::<E>(payload);
        }
    }

    /// Sends a process event to the runtime that spawned the process. A stream
    /// without a recorded owner falls back to every runtime.
    pub(super) fn emit_process_event<E: vy_host_api::HostEvent>(
        &self,
        owner: &Option<Option<String>>,
        payload: &E::Payload,
    ) {
        match owner {
            Some(Some(plugin_id)) => {
                if let Some(runtime) = self.plugin_runtimes.get(plugin_id) {
                    runtime.host.script().emit_event::<E>(payload);
                }
            }
            Some(None) => {
                if let Some(plugin) = self.plugin.as_ref() {
                    plugin.script().emit_event::<E>(payload);
                }
            }
            None => self.emit_to_scripts::<E>(payload),
        }
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

    /// Checks if active editor or selections changed and emits events to every
    /// script runtime.
    pub(super) fn check_and_emit_editor_events(&mut self) {
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
            if let Some(plugin) = self.plugin.as_ref() {
                plugin.emit_active_editor_change(info.clone());
            }
            for runtime in self.plugin_runtimes.values() {
                runtime.host.emit_active_editor_change(info.clone());
            }
        }

        // 2. Check selections change
        let current_selections = self.editor().selections().to_vec();
        if self.last_reported_selections.as_ref() != Some(&current_selections) {
            self.last_reported_selections = Some(current_selections.clone());
            if let Some(plugin) = self.plugin.as_ref() {
                plugin.emit_selection_change(current_selections.clone());
            }
            for runtime in self.plugin_runtimes.values() {
                runtime
                    .host
                    .emit_selection_change(current_selections.clone());
            }
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
