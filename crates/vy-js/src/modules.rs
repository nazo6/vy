//! Resolves and loads relative TypeScript modules for the script host.
//!
//! The loader only accepts relative specifiers. External dependencies must be
//! bundled before a plugin is loaded. Every resolved path is canonicalized and
//! checked against the root so a script cannot import files outside its
//! directory.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use rquickjs::loader::{ImportAttributes, Loader, Resolver};
use rquickjs::{Ctx, Error, Module, Result};

use crate::transpile::strip_types;

/// Extensions tried when a relative import has no extension.
const EXTENSIONS: &[&str] = &[".ts", ".js", ".mjs"];

/// Files loaded by one script runtime and their timestamps at load time.
///
/// The timestamps let the host notice a changed helper module without scanning
/// the whole plugin directory.
#[derive(Clone, Default)]
pub(crate) struct ModuleFiles {
    inner: Arc<Mutex<BTreeMap<PathBuf, Option<SystemTime>>>>,
}

impl ModuleFiles {
    pub(crate) fn insert(&self, path: PathBuf) {
        let stamp = file_stamp(&path);
        if let Ok(mut files) = self.inner.lock() {
            files.insert(path, stamp);
        }
    }

    pub(crate) fn snapshot(&self) -> Vec<PathBuf> {
        self.inner
            .lock()
            .map(|files| files.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub(crate) fn changed_files(&self) -> Vec<PathBuf> {
        let Ok(files) = self.inner.lock() else {
            return Vec::new();
        };
        files
            .iter()
            .filter(|(path, recorded)| file_stamp(path) != **recorded)
            .map(|(path, _)| path.clone())
            .collect()
    }
}

fn file_stamp(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
}

/// Resolver and loader for one script runtime.
#[derive(Clone)]
pub(crate) struct ModuleLoader {
    root: PathBuf,
    files: ModuleFiles,
}

impl ModuleLoader {
    pub(crate) fn new(root: PathBuf, files: ModuleFiles) -> Self {
        let root = std::fs::canonicalize(&root).unwrap_or_else(|_| {
            if root.is_absolute() {
                root
            } else {
                std::env::current_dir().unwrap_or_default().join(root)
            }
        });
        Self { root, files }
    }

    fn resolve_relative(&self, base: &str, name: &str) -> Result<String> {
        let base_dir = Path::new(base).parent().unwrap_or_else(|| Path::new("."));
        let requested = base_dir.join(name);
        let path = resolve_file(&requested).ok_or_else(|| {
            Error::new_resolving_message(base, name, format!("cannot resolve `{name}`"))
        })?;
        let canonical = std::fs::canonicalize(&path).map_err(|error| {
            Error::new_resolving_message(base, name, format!("cannot resolve `{name}`: {error}"))
        })?;

        if !canonical.starts_with(&self.root) {
            return Err(Error::new_resolving_message(
                base,
                name,
                format!("module `{name}` escapes the plugin root"),
            ));
        }
        if !is_supported(&canonical) {
            return Err(Error::new_resolving_message(
                base,
                name,
                format!("unsupported module extension: {}", canonical.display()),
            ));
        }
        Ok(canonical.to_string_lossy().into_owned())
    }
}

impl Resolver for ModuleLoader {
    fn resolve<'js>(
        &mut self,
        _ctx: &Ctx<'js>,
        base: &str,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>,
    ) -> Result<String> {
        if name.starts_with("./") || name.starts_with("../") {
            self.resolve_relative(base, name)
        } else {
            Err(Error::new_resolving_message(
                base,
                name,
                "only relative imports are supported; bundle external dependencies",
            ))
        }
    }
}

impl Loader for ModuleLoader {
    fn load<'js>(
        &mut self,
        ctx: &Ctx<'js>,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>,
    ) -> Result<Module<'js, rquickjs::module::Declared>> {
        let path = Path::new(name);
        let source = std::fs::read_to_string(path).map_err(|error| {
            Error::new_loading_message(name, format!("could not read {}: {error}", path.display()))
        })?;
        let code = strip_types(path, &source).map_err(|error| {
            Error::new_loading_message(name, format!("{}: {error}", path.display()))
        })?;
        self.files.insert(path.to_path_buf());
        Module::declare(ctx.clone(), name, code)
    }
}

/// Finds a file for `path`, trying the exact path, extensions, and directory
/// index files.
fn resolve_file(path: &Path) -> Option<PathBuf> {
    if path.is_file() {
        return Some(path.to_path_buf());
    }

    for extension in EXTENSIONS {
        let mut candidate = path.as_os_str().to_owned();
        candidate.push(extension);
        let candidate = PathBuf::from(candidate);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    if path.is_dir() {
        for index in ["index.ts", "index.js"] {
            let candidate = path.join(index);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

fn is_supported(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("ts" | "js" | "mjs")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_extensions_and_directory_indexes() {
        let root = std::env::temp_dir().join(format!("vy-modules-resolve-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("lib")).expect("create");
        std::fs::write(root.join("lib/util.ts"), "export const value = 1;\n").expect("write");
        std::fs::write(root.join("lib/index.js"), "export const index = 2;\n").expect("write");

        assert_eq!(
            resolve_file(&root.join("lib/util")),
            Some(root.join("lib/util.ts"))
        );
        assert_eq!(
            resolve_file(&root.join("lib")),
            Some(root.join("lib/index.js"))
        );
        assert_eq!(resolve_file(&root.join("missing")), None);

        let _ = std::fs::remove_dir_all(&root);
    }
}
