//! Plugin activation events.
//!
//! Plugins specify when they should be activated in `activationEvents`.
//! This enables lazy activation so that plugins are only loaded into QuickJS
//! when needed (e.g. when their commands are called, views opened, or files of
//! their target language are edited).

use std::path::Path;

/// An activation trigger declared by a plugin.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ActivationEvent {
    /// Activated immediately on editor startup.
    Startup,
    /// Activated when a specific command is executed.
    Command(String),
    /// Activated when a specific view is opened or focused.
    View(String),
    /// Activated when a document of this language is opened.
    Language(String),
    /// Activated when a webview of this kind is opened.
    Webview(String),
    /// Activated when the workspace root contains a file matching this glob.
    WorkspaceContains(String),
}

impl ActivationEvent {
    /// Parses an activation event string from `plugin.json`.
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        if value == "onStartup" {
            Some(Self::Startup)
        } else if let Some(rest) = value.strip_prefix("onCommand:") {
            let rest = rest.trim();
            if !rest.is_empty() {
                Some(Self::Command(rest.to_string()))
            } else {
                None
            }
        } else if let Some(rest) = value.strip_prefix("onView:") {
            let rest = rest.trim();
            if !rest.is_empty() {
                Some(Self::View(rest.to_string()))
            } else {
                None
            }
        } else if let Some(rest) = value.strip_prefix("onLanguage:") {
            let rest = rest.trim();
            if !rest.is_empty() {
                Some(Self::Language(rest.to_string()))
            } else {
                None
            }
        } else if let Some(rest) = value.strip_prefix("onWebview:") {
            let rest = rest.trim();
            if !rest.is_empty() {
                Some(Self::Webview(rest.to_string()))
            } else {
                None
            }
        } else if let Some(rest) = value.strip_prefix("onWorkspaceContains:") {
            let rest = rest.trim();
            if !rest.is_empty() {
                Some(Self::WorkspaceContains(rest.to_string()))
            } else {
                None
            }
        } else {
            None
        }
    }
}

/// Checks if a workspace directory contains any file matching the pattern.
///
/// Supports simple filename checks and basic recursive glob matching.
pub fn workspace_contains(workspace: &Path, pattern: &str) -> bool {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return false;
    }

    // Direct file check if no wildcards
    if !pattern.contains('*') && !pattern.contains('?') {
        return workspace.join(pattern).exists();
    }

    // Walk directory with shallow limit to avoid hanging on large projects
    check_glob_in_dir(workspace, pattern, 0)
}

fn check_glob_in_dir(dir: &Path, pattern: &str, depth: usize) -> bool {
    if depth > 4 {
        return false;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();

        if file_name.starts_with('.') {
            continue;
        }

        if glob_match(pattern, file_name) {
            return true;
        }

        if path.is_dir() && check_glob_in_dir(&path, pattern, depth + 1) {
            return true;
        }
    }
    false
}

/// Simple glob matching for patterns like `*.rs`, `Cargo.toml`.
fn glob_match(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(suffix) = pattern.strip_prefix('*') {
        text.ends_with(suffix)
    } else if let Some(prefix) = pattern.strip_suffix('*') {
        text.starts_with(prefix)
    } else {
        pattern == text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_activation_events() {
        assert_eq!(
            ActivationEvent::parse("onStartup"),
            Some(ActivationEvent::Startup)
        );
        assert_eq!(
            ActivationEvent::parse("onCommand:vy.tree.toggle"),
            Some(ActivationEvent::Command("vy.tree.toggle".into()))
        );
        assert_eq!(
            ActivationEvent::parse("onView:vy.tree"),
            Some(ActivationEvent::View("vy.tree".into()))
        );
        assert_eq!(
            ActivationEvent::parse("onLanguage:rust"),
            Some(ActivationEvent::Language("rust".into()))
        );
        assert_eq!(
            ActivationEvent::parse("onWorkspaceContains:Cargo.toml"),
            Some(ActivationEvent::WorkspaceContains("Cargo.toml".into()))
        );
        assert_eq!(ActivationEvent::parse("unknownEvent"), None);
        assert_eq!(ActivationEvent::parse("onCommand:"), None);
    }

    #[test]
    fn checks_workspace_contains() {
        let temp_dir = std::env::temp_dir().join(format!("vy_act_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        std::fs::write(temp_dir.join("Cargo.toml"), "[package]").unwrap();
        let src_dir = temp_dir.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(src_dir.join("main.rs"), "fn main() {}").unwrap();

        assert!(workspace_contains(&temp_dir, "Cargo.toml"));
        assert!(workspace_contains(&temp_dir, "*.rs"));
        assert!(!workspace_contains(&temp_dir, "package.json"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
