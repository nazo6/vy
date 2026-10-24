//! High-level plugin manager managing discovered plugins, capability approvals,
//! and lazy activation lifecycle.

use std::collections::{HashMap, VecDeque};
use std::path::Path;

use vy_capabilities::{Roots, TrustedStore};

use crate::paths::Paths;
use crate::plugin::activation::{ActivationEvent, workspace_contains};
use crate::plugin::discovery::{DiscoveredPlugin, discover_plugins};
use crate::plugin::trust::{PluginTrustStatus, approve_plugin, evaluate_plugin_trust};

/// A request for the user to approve a plugin's capabilities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginApprovalRequest {
    pub plugin_id: String,
    pub plugin_name: String,
    pub plugin_version: String,
    pub missing_capabilities: Vec<String>,
}

/// The runtime lifecycle status of a known plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginState {
    /// Pending approval from user before it can be loaded.
    PendingApproval {
        plugin: DiscoveredPlugin,
        missing: Vec<String>,
    },
    /// Denied by the user; will not be loaded.
    Denied(DiscoveredPlugin),
    /// Trusted, but not yet activated (idle/lazy).
    Loaded(DiscoveredPlugin),
    /// Successfully activated and running.
    Active(DiscoveredPlugin),
    /// Activation failed with an error message.
    Failed {
        plugin: DiscoveredPlugin,
        error: String,
    },
}

impl PluginState {
    /// Returns the underlying discovered plugin regardless of its current state.
    pub fn plugin(&self) -> &DiscoveredPlugin {
        match self {
            Self::PendingApproval { plugin, .. }
            | Self::Denied(plugin)
            | Self::Loaded(plugin)
            | Self::Active(plugin)
            | Self::Failed { plugin, .. } => plugin,
        }
    }

    /// Whether this plugin has been trusted (approved).
    pub fn is_trusted(&self) -> bool {
        matches!(self, Self::Loaded(_) | Self::Active(_))
    }

    /// Whether this plugin is currently active.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active(_))
    }
}

/// Manages plugin discovery, capability checking, and user approval requests.
pub struct PluginManager {
    paths: Paths,
    roots: Roots,
    trusted_store: TrustedStore,
    plugins: HashMap<String, PluginState>,
    approval_queue: VecDeque<PluginApprovalRequest>,
}

impl PluginManager {
    /// Creates a new plugin manager.
    pub fn new(paths: Paths, roots: Roots) -> Self {
        let (trusted_store, _warn) = TrustedStore::load(&paths.trusted_json());
        Self {
            paths,
            roots,
            trusted_store,
            plugins: HashMap::new(),
            approval_queue: VecDeque::new(),
        }
    }

    /// Scans the plugins directory and updates plugin states and approval queue.
    pub fn scan(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();
        let (discovered, disc_warnings) = discover_plugins(&self.paths.plugins_dir());

        for (path, warn) in disc_warnings {
            warnings.push(format!("{}: {}", path.display(), warn));
        }

        for plugin in discovered {
            let id = plugin.manifest.id.clone();
            // If already explicitly denied, active, or loaded in this session, keep status
            if self.plugins.get(&id).is_some_and(|state| {
                matches!(
                    state,
                    PluginState::Denied(_) | PluginState::Active(_) | PluginState::Loaded(_)
                )
            }) {
                continue;
            }

            match evaluate_plugin_trust(&plugin.manifest, &self.trusted_store, &self.roots) {
                Ok(PluginTrustStatus::Trusted) => {
                    self.plugins.insert(id, PluginState::Loaded(plugin));
                }
                Ok(PluginTrustStatus::Pending { missing }) => {
                    let missing_strings: Vec<String> =
                        missing.iter().map(|c| c.to_string()).collect();

                    let request = PluginApprovalRequest {
                        plugin_id: plugin.manifest.id.clone(),
                        plugin_name: plugin.manifest.name.clone(),
                        plugin_version: plugin.manifest.version.clone(),
                        missing_capabilities: missing_strings.clone(),
                    };

                    if !self.approval_queue.iter().any(|r| r.plugin_id == id) {
                        self.approval_queue.push_back(request);
                    }

                    self.plugins.insert(
                        id,
                        PluginState::PendingApproval {
                            plugin,
                            missing: missing_strings,
                        },
                    );
                }
                Err(err) => {
                    warnings.push(format!("plugin {id}: {err}"));
                }
            }
        }

        warnings
    }

    /// Returns the next pending approval request, if any.
    pub fn next_approval_request(&self) -> Option<&PluginApprovalRequest> {
        self.approval_queue.front()
    }

    /// Pops the next pending approval request.
    pub fn pop_approval_request(&mut self) -> Option<PluginApprovalRequest> {
        self.approval_queue.pop_front()
    }

    /// Approves a plugin's capabilities, moves it to `Loaded`, and persists `trusted.json`.
    pub fn approve(&mut self, plugin_id: &str) -> Result<(), String> {
        self.approval_queue.retain(|r| r.plugin_id != plugin_id);

        let Some(state) = self.plugins.remove(plugin_id) else {
            return Err(format!("unknown plugin '{plugin_id}'"));
        };

        let plugin = state.plugin().clone();

        approve_plugin(
            &plugin.manifest,
            &mut self.trusted_store,
            &self.paths.trusted_json(),
        )
        .map_err(|e| format!("failed to save trusted.json: {e}"))?;

        self.plugins
            .insert(plugin_id.to_string(), PluginState::Loaded(plugin));
        Ok(())
    }

    /// Denies a plugin's approval request, moving it to `Denied`.
    pub fn deny(&mut self, plugin_id: &str) {
        self.approval_queue.retain(|r| r.plugin_id != plugin_id);

        if let Some(state) = self.plugins.remove(plugin_id) {
            let plugin = state.plugin().clone();
            self.plugins
                .insert(plugin_id.to_string(), PluginState::Denied(plugin));
        }
    }

    /// Returns all plugins currently in a trusted state (`Loaded` or `Active`).
    pub fn trusted_plugins(&self) -> Vec<&DiscoveredPlugin> {
        self.plugins
            .values()
            .filter_map(|state| {
                if state.is_trusted() {
                    Some(state.plugin())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Returns all contributed commands from all trusted plugins.
    ///
    /// Each entry is `(command_id, command_title, plugin_id)`.
    pub fn contributed_commands(&self) -> Vec<(String, String, String)> {
        let mut commands = Vec::new();
        for state in self.plugins.values() {
            if state.is_trusted() {
                let plugin = state.plugin();
                for cmd in &plugin.manifest.contributes.commands {
                    commands.push((
                        cmd.id.clone(),
                        cmd.title.clone(),
                        plugin.manifest.id.clone(),
                    ));
                }
            }
        }
        commands.sort_by(|a, b| a.0.cmp(&b.0));
        commands
    }

    /// Collects and marks as active all plugins that should activate for `event`.
    pub fn activate_for_event(&mut self, event: &ActivationEvent) -> Vec<DiscoveredPlugin> {
        let mut to_activate = Vec::new();

        for (id, state) in &mut self.plugins {
            if let PluginState::Loaded(plugin) = state {
                let should_activate = plugin
                    .manifest
                    .activation_events
                    .iter()
                    .any(|raw| ActivationEvent::parse(raw).as_ref() == Some(event));

                if should_activate {
                    to_activate.push((id.clone(), plugin.clone()));
                }
            }
        }

        for (id, plugin) in &to_activate {
            self.plugins
                .insert(id.clone(), PluginState::Active(plugin.clone()));
        }

        to_activate.into_iter().map(|(_, p)| p).collect()
    }

    /// Collects plugins that declare `onStartup` and marks them `Active`.
    pub fn plugins_for_startup(&mut self) -> Vec<DiscoveredPlugin> {
        self.activate_for_event(&ActivationEvent::Startup)
    }

    /// Collects plugins that should activate when `command_id` is run and marks them `Active`.
    ///
    /// Matches explicit `onCommand:<command_id>` declarations as well as commands
    /// contributed in `contributes.commands`.
    pub fn plugins_for_command(&mut self, command_id: &str) -> Vec<DiscoveredPlugin> {
        let mut to_activate = Vec::new();

        for (id, state) in &mut self.plugins {
            if let PluginState::Loaded(plugin) = state {
                let matches_event = plugin.manifest.activation_events.iter().any(|raw| {
                    matches!(
                        ActivationEvent::parse(raw),
                        Some(ActivationEvent::Command(ref cmd)) if cmd == command_id
                    )
                });
                let matches_contributes = plugin
                    .manifest
                    .contributes
                    .commands
                    .iter()
                    .any(|cmd| cmd.id == command_id);

                if matches_event || matches_contributes {
                    to_activate.push((id.clone(), plugin.clone()));
                }
            }
        }

        for (id, plugin) in &to_activate {
            self.plugins
                .insert(id.clone(), PluginState::Active(plugin.clone()));
        }

        to_activate.into_iter().map(|(_, p)| p).collect()
    }

    /// Collects plugins that declare `onView:<view_id>` and marks them `Active`.
    pub fn plugins_for_view(&mut self, view_id: &str) -> Vec<DiscoveredPlugin> {
        self.activate_for_event(&ActivationEvent::View(view_id.to_string()))
    }

    /// Collects plugins that declare `onLanguage:<language_id>` and marks them `Active`.
    pub fn plugins_for_language(&mut self, language_id: &str) -> Vec<DiscoveredPlugin> {
        self.activate_for_event(&ActivationEvent::Language(language_id.to_string()))
    }

    /// Collects plugins that declare `onWorkspaceContains:<glob>` matching files in `workspace_path`.
    pub fn plugins_for_workspace(&mut self, workspace_path: &Path) -> Vec<DiscoveredPlugin> {
        let mut to_activate = Vec::new();

        for (id, state) in &mut self.plugins {
            if let PluginState::Loaded(plugin) = state {
                let should_activate = plugin.manifest.activation_events.iter().any(|raw| {
                    if let Some(ActivationEvent::WorkspaceContains(glob)) =
                        ActivationEvent::parse(raw)
                    {
                        workspace_contains(workspace_path, &glob)
                    } else {
                        false
                    }
                });

                if should_activate {
                    to_activate.push((id.clone(), plugin.clone()));
                }
            }
        }

        for (id, plugin) in &to_activate {
            self.plugins
                .insert(id.clone(), PluginState::Active(plugin.clone()));
        }

        to_activate.into_iter().map(|(_, p)| p).collect()
    }

    /// Marks a plugin as active.
    pub fn mark_active(&mut self, plugin_id: &str) {
        if let Some(state) = self.plugins.get_mut(plugin_id) {
            let plugin = state.plugin().clone();
            *state = PluginState::Active(plugin);
        }
    }

    /// Marks a plugin as failed.
    pub fn mark_failed(&mut self, plugin_id: &str, error: String) {
        if let Some(state) = self.plugins.get_mut(plugin_id) {
            let plugin = state.plugin().clone();
            *state = PluginState::Failed { plugin, error };
        }
    }

    /// Returns the state of a given plugin.
    pub fn state(&self, plugin_id: &str) -> Option<&PluginState> {
        self.plugins.get(plugin_id)
    }

    /// Returns the trusted store.
    pub fn trusted_store(&self) -> &TrustedStore {
        &self.trusted_store
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn manager_scans_and_handles_approval_workflow() {
        let temp_dir =
            std::env::temp_dir().join(format!("vy_plugin_mgr_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        let paths = Paths::under(&temp_dir);
        std::fs::create_dir_all(paths.plugins_dir()).unwrap();

        // Write a plugin that requires permission
        let p_dir = paths.plugins_dir().join("my-plugin");
        std::fs::create_dir_all(&p_dir).unwrap();
        std::fs::write(
            p_dir.join("plugin.json"),
            r#"{
                "id": "my.plugin",
                "name": "My Plugin",
                "version": "1.0.0",
                "permissions": ["fs:read:${workspace}/**"]
            }"#,
        )
        .unwrap();

        let roots = Roots::new(Some("/workspace".into()), None);
        let mut manager = PluginManager::new(paths.clone(), roots);

        let warnings = manager.scan();
        assert!(warnings.is_empty());

        // Check it is pending
        assert_eq!(manager.trusted_plugins().len(), 0);
        let request = manager.next_approval_request().unwrap();
        assert_eq!(request.plugin_id, "my.plugin");
        assert_eq!(
            request.missing_capabilities,
            vec!["fs:read:${workspace}/**"]
        );

        // Approve it
        manager.approve("my.plugin").unwrap();
        assert_eq!(manager.trusted_plugins().len(), 1);
        assert!(manager.next_approval_request().is_none());

        // Re-scanning keeps it trusted because trusted.json was saved
        let mut manager2 = PluginManager::new(paths, Roots::new(Some("/workspace".into()), None));
        manager2.scan();
        assert_eq!(manager2.trusted_plugins().len(), 1);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn discovers_example_greeter_plugin() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let repo_root = manifest_dir.parent().unwrap().parent().unwrap();
        let plugins_dir = repo_root.join("examples").join("plugins");

        let (plugins, warnings) = discover_plugins(&plugins_dir);
        assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");

        let greeter = plugins
            .iter()
            .find(|p| p.manifest.id == "example.greeter")
            .expect("example.greeter plugin should be discovered");

        assert_eq!(greeter.manifest.name, "Greeter Plugin");
        assert_eq!(
            greeter.manifest.permissions,
            vec!["fs:read:${workspace}/**", "clipboard:write"]
        );
        let main = greeter.main_path().expect("should have main path");
        assert!(
            main.is_file(),
            "frontend.js should exist at {}",
            main.display()
        );

        let tree = plugins
            .iter()
            .find(|p| p.manifest.id == "example.tree")
            .expect("example.tree plugin should be discovered");
        assert_eq!(tree.manifest.name, "File Explorer");
        assert_eq!(tree.manifest.permissions, vec!["fs:read:${workspace}/**"]);
        let tree_main = tree.main_path().expect("should have main path");
        assert!(
            tree_main.is_file(),
            "frontend.js should exist at {}",
            tree_main.display()
        );
    }

    #[test]
    fn lazy_activation_workflow() {
        let temp_dir =
            std::env::temp_dir().join(format!("vy_plugin_lazy_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        let paths = Paths::under(&temp_dir);
        std::fs::create_dir_all(paths.plugins_dir()).unwrap();

        // 1. Plugin with onStartup
        let p1 = paths.plugins_dir().join("startup-plugin");
        std::fs::create_dir_all(&p1).unwrap();
        std::fs::write(
            p1.join("plugin.json"),
            r#"{
                "id": "p.startup",
                "name": "Startup",
                "version": "1.0.0",
                "activationEvents": ["onStartup"]
            }"#,
        )
        .unwrap();

        // 2. Plugin with onCommand
        let p2 = paths.plugins_dir().join("cmd-plugin");
        std::fs::create_dir_all(&p2).unwrap();
        std::fs::write(
            p2.join("plugin.json"),
            r#"{
                "id": "p.cmd",
                "name": "Command",
                "version": "1.0.0",
                "activationEvents": ["onCommand:p.cmd.run"],
                "contributes": {
                    "commands": [{ "id": "p.cmd.run", "title": "Run Command" }]
                }
            }"#,
        )
        .unwrap();

        // 3. Plugin with onView
        let p3 = paths.plugins_dir().join("view-plugin");
        std::fs::create_dir_all(&p3).unwrap();
        std::fs::write(
            p3.join("plugin.json"),
            r#"{
                "id": "p.view",
                "name": "View",
                "version": "1.0.0",
                "activationEvents": ["onView:p.tree"]
            }"#,
        )
        .unwrap();

        let mut manager = PluginManager::new(paths, Roots::default());
        manager.scan();

        // Initially all are Loaded (no permissions required)
        assert_eq!(manager.trusted_plugins().len(), 3);
        assert!(!manager.state("p.startup").unwrap().is_active());
        assert!(!manager.state("p.cmd").unwrap().is_active());
        assert!(!manager.state("p.view").unwrap().is_active());

        // Contributed commands are visible before activation
        let cmds = manager.contributed_commands();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].0, "p.cmd.run");
        assert_eq!(cmds[0].1, "Run Command");

        // 1. Startup triggers only p.startup
        let act = manager.plugins_for_startup();
        assert_eq!(act.len(), 1);
        assert_eq!(act[0].manifest.id, "p.startup");
        assert!(manager.state("p.startup").unwrap().is_active());
        assert!(!manager.state("p.cmd").unwrap().is_active());

        // 2. Command triggers p.cmd
        let act = manager.plugins_for_command("p.cmd.run");
        assert_eq!(act.len(), 1);
        assert_eq!(act[0].manifest.id, "p.cmd");
        assert!(manager.state("p.cmd").unwrap().is_active());

        // Calling it again does not return already active plugin
        assert!(manager.plugins_for_command("p.cmd.run").is_empty());

        // 3. View triggers p.view
        let act = manager.plugins_for_view("p.tree");
        assert_eq!(act.len(), 1);
        assert_eq!(act[0].manifest.id, "p.view");
        assert!(manager.state("p.view").unwrap().is_active());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
