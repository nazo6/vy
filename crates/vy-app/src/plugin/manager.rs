//! High-level plugin manager managing discovered plugins and capability approval workflow.

use std::collections::{HashMap, VecDeque};

use vy_capabilities::{Roots, TrustedStore};

use crate::paths::Paths;
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

/// The runtime status of a known plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginState {
    /// Fully trusted and ready to be activated.
    Trusted(DiscoveredPlugin),
    /// Pending approval from user before it can be loaded.
    PendingApproval {
        plugin: DiscoveredPlugin,
        missing: Vec<String>,
    },
    /// Denied by the user; will not be loaded.
    Denied(DiscoveredPlugin),
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
            // If already explicitly denied in this session, keep denied
            if let Some(PluginState::Denied(_)) = self.plugins.get(&id) {
                continue;
            }

            match evaluate_plugin_trust(&plugin.manifest, &self.trusted_store, &self.roots) {
                Ok(PluginTrustStatus::Trusted) => {
                    self.plugins.insert(id, PluginState::Trusted(plugin));
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

    /// Approves a plugin's capabilities, moves it to `Trusted`, and persists `trusted.json`.
    pub fn approve(&mut self, plugin_id: &str) -> Result<(), String> {
        self.approval_queue.retain(|r| r.plugin_id != plugin_id);

        let Some(state) = self.plugins.remove(plugin_id) else {
            return Err(format!("unknown plugin '{plugin_id}'"));
        };

        let plugin = match state {
            PluginState::PendingApproval { plugin, .. } => plugin,
            PluginState::Trusted(plugin) => plugin,
            PluginState::Denied(plugin) => plugin,
        };

        approve_plugin(
            &plugin.manifest,
            &mut self.trusted_store,
            &self.paths.trusted_json(),
        )
        .map_err(|e| format!("failed to save trusted.json: {e}"))?;

        self.plugins
            .insert(plugin_id.to_string(), PluginState::Trusted(plugin));
        Ok(())
    }

    /// Denies a plugin's approval request, moving it to `Denied`.
    pub fn deny(&mut self, plugin_id: &str) {
        self.approval_queue.retain(|r| r.plugin_id != plugin_id);

        if let Some(state) = self.plugins.remove(plugin_id) {
            let plugin = match state {
                PluginState::PendingApproval { plugin, .. } => plugin,
                PluginState::Trusted(plugin) => plugin,
                PluginState::Denied(plugin) => plugin,
            };
            self.plugins
                .insert(plugin_id.to_string(), PluginState::Denied(plugin));
        }
    }

    /// Returns all plugins currently in the `Trusted` state.
    pub fn trusted_plugins(&self) -> Vec<&DiscoveredPlugin> {
        self.plugins
            .values()
            .filter_map(|state| match state {
                PluginState::Trusted(plugin) => Some(plugin),
                _ => None,
            })
            .collect()
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
    }
}
