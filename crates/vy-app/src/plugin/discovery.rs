//! Plugin directory discovery.
//!
//! Scans a plugins directory (typically `~/.config/vy/plugins`) for subdirectories
//! containing a valid `plugin.json`.

use std::path::{Path, PathBuf};

use crate::plugin::manifest::{PluginManifest, PluginManifestError};

/// A plugin found on the filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredPlugin {
    /// The parsed manifest.
    pub manifest: PluginManifest,
    /// The plugin's root directory.
    pub dir: PathBuf,
}

impl DiscoveredPlugin {
    /// Absolute path to the frontend entry point if declared.
    pub fn main_path(&self) -> Option<PathBuf> {
        self.manifest.main.as_ref().map(|main| self.dir.join(main))
    }

    /// Absolute path to the backend entry point if declared.
    pub fn backend_path(&self) -> Option<PathBuf> {
        self.manifest
            .backend
            .as_ref()
            .map(|backend| self.dir.join(backend))
    }
}

/// Discovers all plugins in the given directory.
///
/// Returns a list of successfully discovered plugins and a list of warnings
/// for directories that failed to parse.
pub fn discover_plugins(plugins_dir: &Path) -> (Vec<DiscoveredPlugin>, Vec<(PathBuf, String)>) {
    let mut plugins = Vec::new();
    let mut warnings = Vec::new();

    let Ok(entries) = std::fs::read_dir(plugins_dir) else {
        return (plugins, warnings);
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let manifest_path = path.join("plugin.json");
        if !manifest_path.is_file() {
            continue;
        }

        match load_manifest(&manifest_path) {
            Ok(manifest) => {
                plugins.push(DiscoveredPlugin {
                    manifest,
                    dir: path,
                });
            }
            Err(err) => {
                warnings.push((path, err.to_string()));
            }
        }
    }

    // Sort plugins by id for deterministic order.
    plugins.sort_by(|a, b| a.manifest.id.cmp(&b.manifest.id));

    (plugins, warnings)
}

fn load_manifest(path: &Path) -> Result<PluginManifest, PluginManifestError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| PluginManifestError::Json(format!("cannot read file: {e}")))?;
    PluginManifest::parse(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_valid_plugins_and_ignores_non_plugins() {
        let temp_dir =
            std::env::temp_dir().join(format!("vy_discovery_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // 1. Valid plugin
        let p1 = temp_dir.join("plugin-one");
        std::fs::create_dir_all(&p1).unwrap();
        std::fs::write(
            p1.join("plugin.json"),
            r#"{"id": "plugin.one", "name": "One", "version": "0.1.0", "main": "dist/main.js"}"#,
        )
        .unwrap();

        // 2. Directory without plugin.json
        let p2 = temp_dir.join("empty-dir");
        std::fs::create_dir_all(&p2).unwrap();

        // 3. Plugin with broken json
        let p3 = temp_dir.join("broken-plugin");
        std::fs::create_dir_all(&p3).unwrap();
        std::fs::write(p3.join("plugin.json"), "{ invalid }").unwrap();

        let (plugins, warnings) = discover_plugins(&temp_dir);
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].manifest.id, "plugin.one");
        assert_eq!(plugins[0].main_path(), Some(p1.join("dist/main.js")));

        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].0, p3);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
