//! Workspace-level URI resolution and metadata.

use std::path::{Path, PathBuf};

use vy_core::Uri;

use crate::plugin_host::WorkspaceInfo;

/// Resolves the URI of the document to open on startup.
///
/// Arguments starting with `file:` or `vy:` are used as-is. Other arguments are
/// resolved against `base_uri`, which is the workspace. With no argument, opens an
/// empty `vy:untitled`.
pub fn document_uri(file: Option<&str>, base_uri: Option<&str>, local: bool) -> String {
    match file {
        Some(argument) => resolve_uri(argument, base_uri, local),
        None => "vy:untitled".to_string(),
    }
}

/// Resolves an Ex argument to a document URI. Relative paths use the workspace root.
pub fn uri_from_argument(argument: &str, workspace: Option<&Path>) -> String {
    let base = workspace.map(|path| format!("file://{}", path.display()));
    resolve_uri(argument, base.as_deref(), true)
}

/// Resolves an Ex argument against a workspace URI.
///
/// The base is a URI, not a local path, so a remote `file:` URI is never
/// interpreted as a path on the client.
pub fn uri_from_argument_with_base(argument: &str, base_uri: Option<&str>) -> String {
    resolve_uri(argument, base_uri, true)
}

/// Resolves an Ex argument for a remote workspace.
///
/// The path is not canonicalized because it does not exist on this machine.
pub fn uri_from_argument_remote(argument: &str, base_uri: Option<&str>) -> String {
    resolve_uri(argument, base_uri, false)
}

fn resolve_uri(argument: &str, base_uri: Option<&str>, canonicalize: bool) -> String {
    if argument.starts_with("file:") || argument.starts_with("vy:") {
        return argument.to_string();
    }

    let path = PathBuf::from(argument);
    let path = if path.is_absolute() {
        path
    } else {
        let base = base_uri
            .and_then(|uri| Uri::parse(uri).ok())
            .and_then(|uri| uri.file_path());
        match base {
            Some(base) => base.join(path),
            None => path,
        }
    };

    if canonicalize && let Ok(absolute) = std::fs::canonicalize(&path) {
        return format!("file://{}", absolute.display());
    }
    format!("file://{}", path.display())
}

/// Computes the workspace URI from the current working directory.
pub fn workspace_uri() -> String {
    match std::env::current_dir() {
        Ok(directory) => format!("file://{}", directory.display()),
        Err(_) => "file:///".to_string(),
    }
}

/// The workspace values scripts see through `workspace.*`.
pub fn workspace_info() -> WorkspaceInfo {
    workspace_info_for(
        std::env::current_dir()
            .ok()
            .map(|path| format!("file://{}", path.display())),
    )
}

/// Builds workspace values for a URI. The name is the last path component.
pub fn workspace_info_for(root_uri: Option<String>) -> WorkspaceInfo {
    let name = root_uri
        .as_deref()
        .and_then(|uri| Uri::parse(uri).ok())
        .and_then(|uri| uri.file_path())
        .and_then(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
        });

    WorkspaceInfo {
        folders: root_uri.iter().cloned().collect(),
        root_uri,
        name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_ex_paths_against_the_workspace() {
        assert_eq!(
            uri_from_argument("README.md", Some(Path::new("/tmp/vy-ws"))),
            "file:///tmp/vy-ws/README.md"
        );
        assert_eq!(
            uri_from_argument("/nonexistent/file.txt", Some(Path::new("/tmp/vy-ws"))),
            "file:///nonexistent/file.txt"
        );
        assert_eq!(
            uri_from_argument("vy:untitled", Some(Path::new("/tmp/vy-ws"))),
            "vy:untitled"
        );
    }

    #[test]
    fn resolves_remote_paths_without_touching_the_local_filesystem() {
        assert_eq!(
            uri_from_argument_remote("src/main.rs", Some("file:///home/user/project")),
            "file:///home/user/project/src/main.rs"
        );
        assert_eq!(
            uri_from_argument_remote("/var/log/app.log", Some("file:///home/user/project")),
            "file:///var/log/app.log"
        );
        assert_eq!(
            uri_from_argument_remote("file:///tmp/a.txt", Some("file:///home/user/project")),
            "file:///tmp/a.txt"
        );
    }

    #[test]
    fn names_the_workspace_from_its_uri() {
        let info = workspace_info_for(Some("file:///home/user/project".to_string()));
        assert_eq!(info.name.as_deref(), Some("project"));
        assert_eq!(info.root_uri.as_deref(), Some("file:///home/user/project"));
        assert_eq!(info.folders, vec!["file:///home/user/project"]);
    }
}
