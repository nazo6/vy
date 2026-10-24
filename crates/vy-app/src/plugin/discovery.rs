//! Plugin directory discovery.
//!
//! Scans a plugins directory (typically `~/.config/vy/plugins`) for subdirectories
//! containing a valid `plugin.json`, then validates entry points and engine
//! requirements. See ADR 0021.

use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

use crate::plugin::bundle::{BundleInfo, MAX_BUNDLE_BYTES, validate_bundle};
use crate::plugin::manifest::{PluginManifest, PluginManifestError};

/// A validated frontend or backend entry point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginEntry {
    /// Canonical path inside the plugin directory.
    pub path: PathBuf,
    /// Bundle metadata for `.js` / `.mjs` entries. `None` for source form.
    pub bundle: Option<BundleInfo>,
}

/// A plugin found on the filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredPlugin {
    /// The parsed manifest.
    pub manifest: PluginManifest,
    /// The canonical plugin root directory.
    pub dir: PathBuf,
    /// The validated frontend entry point if declared.
    pub main_entry: Option<PluginEntry>,
    /// The validated backend entry point if declared.
    pub backend_entry: Option<PluginEntry>,
}

impl DiscoveredPlugin {
    /// Absolute path to the frontend entry point if declared.
    pub fn main_path(&self) -> Option<PathBuf> {
        self.main_entry.as_ref().map(|entry| entry.path.clone())
    }

    /// Absolute path to the backend entry point if declared.
    pub fn backend_path(&self) -> Option<PathBuf> {
        self.backend_entry.as_ref().map(|entry| entry.path.clone())
    }

    /// Whether the frontend is loaded from TypeScript sources.
    pub fn is_source(&self) -> bool {
        self.main_entry
            .as_ref()
            .is_some_and(|entry| entry.bundle.is_none())
    }

    /// Re-runs entry validation against the current files.
    ///
    /// Discovery validates at startup, but a bundle can change before a reload.
    /// The view calls this before creating a new runtime.
    pub fn revalidate(&self) -> Result<Self, String> {
        load_plugin(&self.dir, self.manifest.clone())
    }
}

/// Discovers all plugins in the given directory.
///
/// Returns a list of successfully discovered plugins and a list of warnings
/// for directories that failed to parse or validate.
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
            Ok(manifest) => match load_plugin(&path, manifest) {
                Ok(plugin) => plugins.push(plugin),
                Err(error) => warnings.push((path, error)),
            },
            Err(err) => warnings.push((path, err.to_string())),
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

fn load_plugin(dir: &Path, manifest: PluginManifest) -> Result<DiscoveredPlugin, String> {
    check_engine(&manifest)?;

    let root = std::fs::canonicalize(dir)
        .map_err(|error| format!("cannot read plugin directory: {error}"))?;
    if !root.is_dir() {
        return Err("plugin path is not a directory".to_string());
    }

    let main_entry = manifest
        .main
        .as_deref()
        .map(|entry| load_entry(&root, entry, EntryKind::Frontend))
        .transpose()?;
    let backend_entry = manifest
        .backend
        .as_deref()
        .map(|entry| load_entry(&root, entry, EntryKind::Backend))
        .transpose()?;

    Ok(DiscoveredPlugin {
        manifest,
        dir: root,
        main_entry,
        backend_entry,
    })
}

fn check_engine(manifest: &PluginManifest) -> Result<(), String> {
    let Some(requirement) = manifest
        .engines
        .as_ref()
        .and_then(|engines| engines.vy.as_deref())
    else {
        return Ok(());
    };
    let requirement = semver::VersionReq::parse(requirement)
        .map_err(|error| format!("invalid engines.vy: {error}"))?;
    let current = semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|error| format!("invalid Vy version: {error}"))?;
    if !requirement.matches(&current) {
        return Err(format!("requires vy {requirement}, but this is {current}"));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    Frontend,
    Backend,
}

fn load_entry(root: &Path, declared: &str, kind: EntryKind) -> Result<PluginEntry, String> {
    let relative = safe_entry_path(declared).ok_or_else(|| {
        format!("entry `{declared}` must be a relative path inside the plugin directory")
    })?;
    let canonical = std::fs::canonicalize(root.join(relative))
        .map_err(|error| format!("cannot read entry `{declared}`: {error}"))?;
    if !canonical.starts_with(root) {
        return Err(format!("entry `{declared}` escapes the plugin directory"));
    }
    if !canonical.is_file() {
        return Err(format!("entry `{declared}` is not a file"));
    }

    let extension = Path::new(declared)
        .extension()
        .and_then(OsStr::to_str)
        .unwrap_or_default();
    match (kind, extension) {
        (EntryKind::Frontend, "ts") => Ok(PluginEntry {
            path: canonical,
            bundle: None,
        }),
        (EntryKind::Frontend, "js" | "mjs") | (EntryKind::Backend, "js" | "mjs") => {
            let bundle = validate_bundle(&canonical, MAX_BUNDLE_BYTES)
                .map_err(|error| format!("entry `{declared}`: {error}"))?;
            Ok(PluginEntry {
                path: canonical,
                bundle: Some(bundle),
            })
        }
        (EntryKind::Backend, "ts") => Err(format!(
            "backend `{declared}` must be a bundled .js or .mjs file"
        )),
        (EntryKind::Backend, _) => Err(format!("backend `{declared}` must be .js or .mjs")),
        (EntryKind::Frontend, _) => Err(format!(
            "entry `{declared}` must be .ts (source), .js, or .mjs (bundle)"
        )),
    }
}

fn safe_entry_path(declared: &str) -> Option<&Path> {
    let path = Path::new(declared);
    if path.as_os_str().is_empty() || path.is_absolute() {
        return None;
    }
    let mut components = path.components();
    if components.any(|component| !matches!(component, Component::Normal(_))) {
        return None;
    }
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn directory(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("vy-discovery-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).expect("create temp directory");
        directory
    }

    fn write_manifest(dir: &Path, manifest: &str) {
        fs::write(dir.join("plugin.json"), manifest).expect("write manifest");
    }

    fn write_entry(dir: &Path, relative: &str, source: &str) {
        let path = dir.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create entry directory");
        }
        fs::write(path, source).expect("write entry");
    }

    #[test]
    fn discovers_valid_plugins_and_ignores_non_plugins() {
        let temp_dir = directory("valid");
        let p1 = temp_dir.join("plugin-one");
        fs::create_dir_all(&p1).expect("create plugin");
        write_manifest(
            &p1,
            r#"{"id": "plugin.one", "name": "One", "version": "0.1.0", "main": "dist/main.js"}"#,
        );
        write_entry(&p1, "dist/main.js", "export const answer = 42;\n");

        let empty = temp_dir.join("empty-dir");
        fs::create_dir_all(&empty).expect("create empty");

        let broken = temp_dir.join("broken-plugin");
        fs::create_dir_all(&broken).expect("create broken");
        write_manifest(&broken, "{ invalid }");

        let (plugins, warnings) = discover_plugins(&temp_dir);
        assert_eq!(plugins.len(), 1);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].0, broken);

        let plugin = &plugins[0];
        assert_eq!(plugin.manifest.id, "plugin.one");
        assert_eq!(
            plugin.main_path(),
            Some(fs::canonicalize(&p1).unwrap().join("dist/main.js"))
        );
        let bundle = plugin
            .main_entry
            .as_ref()
            .and_then(|entry| entry.bundle.as_ref())
            .expect("bundle metadata");
        assert!(bundle.size > 0);
        assert_eq!(bundle.sha256.len(), 64);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn accepts_source_entries_without_bundle_metadata() {
        let temp_dir = directory("source");
        let plugin = temp_dir.join("plugin");
        fs::create_dir_all(&plugin).expect("create plugin");
        write_manifest(
            &plugin,
            r#"{"id": "plugin.source", "name": "Source", "version": "0.1.0", "main": "frontend.ts"}"#,
        );
        write_entry(
            &plugin,
            "frontend.ts",
            "export const answer: number = 42;\n",
        );

        let (plugins, warnings) = discover_plugins(&temp_dir);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(plugins.len(), 1);
        assert!(plugins[0].is_source());
        assert!(plugins[0].main_entry.as_ref().unwrap().bundle.is_none());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn rejects_paths_outside_the_plugin_directory() {
        let temp_dir = directory("path");
        let plugin = temp_dir.join("plugin");
        fs::create_dir_all(&plugin).expect("create plugin");
        write_manifest(
            &plugin,
            r#"{"id": "plugin.path", "name": "Path", "version": "0.1.0", "main": "../outside.ts"}"#,
        );
        write_entry(&temp_dir, "outside.ts", "export {};\n");

        let (plugins, warnings) = discover_plugins(&temp_dir);
        assert!(plugins.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].1.contains("relative path"), "{warnings:?}");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn revalidation_rejects_an_edited_bundle_with_imports() {
        let temp_dir = directory("revalidate");
        let plugin = temp_dir.join("plugin");
        fs::create_dir_all(&plugin).expect("create plugin");
        write_manifest(
            &plugin,
            r#"{"id": "plugin.revalidate", "name": "Revalidate", "version": "0.1.0", "main": "dist/frontend.js"}"#,
        );
        write_entry(&plugin, "dist/frontend.js", "export const answer = 42;\n");

        let (plugins, warnings) = discover_plugins(&temp_dir);
        assert!(warnings.is_empty(), "{warnings:?}");
        let plugin = &plugins[0];

        write_entry(&plugin.dir, "dist/frontend.js", "import \"zod\";\n");
        let error = plugin.revalidate().expect_err("edited bundle should fail");
        assert!(error.contains("zod"), "{error}");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn rejects_bundles_with_remaining_imports() {
        let temp_dir = directory("imports");
        let plugin = temp_dir.join("plugin");
        fs::create_dir_all(&plugin).expect("create plugin");
        write_manifest(
            &plugin,
            r#"{"id": "plugin.imports", "name": "Imports", "version": "0.1.0", "main": "dist/frontend.js"}"#,
        );
        write_entry(&plugin, "dist/frontend.js", "import \"zod\";\n");

        let (plugins, warnings) = discover_plugins(&temp_dir);
        assert!(plugins.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].1.contains("zod"), "{warnings:?}");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn rejects_typescript_backends() {
        let temp_dir = directory("backend-ts");
        let plugin = temp_dir.join("plugin");
        fs::create_dir_all(&plugin).expect("create plugin");
        write_manifest(
            &plugin,
            r#"{"id": "plugin.backend", "name": "Backend", "version": "0.1.0", "main": "frontend.ts", "backend": "backend.ts"}"#,
        );
        write_entry(&plugin, "frontend.ts", "export {};\n");
        write_entry(&plugin, "backend.ts", "export {};\n");

        let (plugins, warnings) = discover_plugins(&temp_dir);
        assert!(plugins.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].1.contains("bundled"), "{warnings:?}");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn rejects_incompatible_engines() {
        let temp_dir = directory("engines");
        let plugin = temp_dir.join("plugin");
        fs::create_dir_all(&plugin).expect("create plugin");
        write_manifest(
            &plugin,
            r#"{"id": "plugin.engine", "name": "Engine", "version": "0.1.0", "engines": {"vy": ">=99.0.0"}, "main": "frontend.ts"}"#,
        );
        write_entry(&plugin, "frontend.ts", "export {};\n");

        let (plugins, warnings) = discover_plugins(&temp_dir);
        assert!(plugins.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].1.contains("requires vy"), "{warnings:?}");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn rejects_missing_entries() {
        let temp_dir = directory("missing");
        let plugin = temp_dir.join("plugin");
        fs::create_dir_all(&plugin).expect("create plugin");
        write_manifest(
            &plugin,
            r#"{"id": "plugin.missing", "name": "Missing", "version": "0.1.0", "main": "frontend.ts"}"#,
        );

        let (plugins, warnings) = discover_plugins(&temp_dir);
        assert!(plugins.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].1.contains("cannot read entry"), "{warnings:?}");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_that_escape_the_plugin_directory() {
        let temp_dir = directory("symlink");
        let plugin = temp_dir.join("plugin");
        fs::create_dir_all(&plugin).expect("create plugin");
        write_manifest(
            &plugin,
            r#"{"id": "plugin.symlink", "name": "Symlink", "version": "0.1.0", "main": "frontend.ts"}"#,
        );
        write_entry(&temp_dir, "outside.ts", "export {};\n");
        std::os::unix::fs::symlink(temp_dir.join("outside.ts"), plugin.join("frontend.ts"))
            .expect("create symlink");

        let (plugins, warnings) = discover_plugins(&temp_dir);
        assert!(plugins.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].1.contains("escapes"), "{warnings:?}");

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
