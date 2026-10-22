//! Locations of user files.
//!
//! The layout follows `docs/layout.md`. Environment variables override the platform
//! defaults so tests and development builds can use a separate tree.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Roots for configuration, data, and cache files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub config: PathBuf,
    pub data: PathBuf,
    pub cache: PathBuf,
}

impl Paths {
    /// Reads the roots from the environment, with the platform defaults as fallback.
    pub fn from_env() -> Self {
        Self {
            config: env_path("VY_CONFIG_HOME", || dirs::config_dir().map(app_dir)),
            data: env_path("VY_DATA_HOME", || dirs::data_dir().map(app_dir)),
            cache: env_path("VY_CACHE_HOME", || dirs::cache_dir().map(app_cache_dir)),
        }
    }

    /// Builds a set of roots under one directory. Used by tests.
    pub fn under(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref();
        Self {
            config: root.join("config"),
            data: root.join("data"),
            cache: root.join("cache"),
        }
    }

    pub fn init_ts(&self) -> PathBuf {
        self.config.join("init.ts")
    }

    pub fn settings_json(&self) -> PathBuf {
        self.config.join("settings.json")
    }

    pub fn trusted_json(&self) -> PathBuf {
        self.config.join("trusted.json")
    }

    pub fn plugins_dir(&self) -> PathBuf {
        self.config.join("plugins")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.cache.join("logs")
    }

    /// `<workspace>/.vy/settings.json`, which only holds values and never code.
    pub fn workspace_settings(&self, workspace: impl AsRef<Path>) -> PathBuf {
        workspace.as_ref().join(".vy").join("settings.json")
    }

    /// Modification time of the newest settings file, if any exists.
    pub fn settings_stamp(&self, workspace: Option<&Path>) -> Option<SystemTime> {
        let project = workspace.map(|ws| self.workspace_settings(ws));
        let mut newest: Option<SystemTime> = None;

        for path in std::iter::once(self.settings_json()).chain(project) {
            if let Ok(modified) = std::fs::metadata(&path).and_then(|metadata| metadata.modified())
            {
                newest = Some(newest.map_or(modified, |current| current.max(modified)));
            }
        }

        newest
    }
}

fn env_path(variable: &str, fallback: impl FnOnce() -> Option<PathBuf>) -> PathBuf {
    if let Some(value) = std::env::var_os(variable) {
        return PathBuf::from(value);
    }
    fallback().unwrap_or_else(|| PathBuf::from(".vy"))
}

/// Name of the directory included in the platform roots.
const APP_DIR: &str = "vy";

/// Appends [`APP_DIR`] to a platform base directory such as `~/.config`.
fn app_dir(base: PathBuf) -> PathBuf {
    base.join(APP_DIR)
}

/// Appends [`APP_DIR`] to a platform cache directory.
///
/// `docs/layout.md` puts the Windows cache one level deeper (`%LOCALAPPDATA%\vy\cache`).
fn app_cache_dir(base: PathBuf) -> PathBuf {
    let dir = app_dir(base);
    if cfg!(windows) {
        dir.join("cache")
    } else {
        dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_user_file_locations() {
        let paths = Paths::under("/tmp/vy-test");
        assert_eq!(
            paths.init_ts(),
            PathBuf::from("/tmp/vy-test/config/init.ts")
        );
        assert_eq!(
            paths.settings_json(),
            PathBuf::from("/tmp/vy-test/config/settings.json")
        );
        assert_eq!(paths.logs_dir(), PathBuf::from("/tmp/vy-test/cache/logs"));
        assert_eq!(
            paths.workspace_settings("/project"),
            PathBuf::from("/project/.vy/settings.json")
        );
    }

    #[test]
    fn platform_roots_include_the_app_directory() {
        assert_eq!(app_dir(PathBuf::from("/base")), PathBuf::from("/base/vy"));

        let cache = app_cache_dir(PathBuf::from("/base"));
        if cfg!(windows) {
            assert_eq!(cache, PathBuf::from("/base/vy/cache"));
        } else {
            assert_eq!(cache, PathBuf::from("/base/vy"));
        }
    }
}
