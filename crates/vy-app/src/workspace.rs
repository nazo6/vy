//! Workspace-level URI resolution and metadata.

use std::path::{Path, PathBuf};

use crate::plugin_host::WorkspaceInfo;

/// Resolves the URI of the document to open on startup.
///
/// Arguments starting with `file:` or `vy:` are used as-is. Other arguments are made
/// absolute and prefixed with `file:`. With no argument, opens an empty `vy:untitled`.
pub fn document_uri() -> String {
    match std::env::args().nth(1) {
        Some(argument) if argument.starts_with("file:") || argument.starts_with("vy:") => argument,
        Some(path) => match std::fs::canonicalize(&path) {
            Ok(absolute) => format!("file://{}", absolute.display()),
            // Let a file that does not exist yet open with the path as given.
            Err(_) => format!("file://{path}"),
        },
        None => "vy:untitled".to_string(),
    }
}

/// Resolves an Ex argument to a document URI. Relative paths use the workspace root.
pub fn uri_from_argument(argument: &str, workspace: Option<&Path>) -> String {
    if argument.starts_with("file:") || argument.starts_with("vy:") {
        return argument.to_string();
    }

    let path = PathBuf::from(argument);
    let path = if path.is_absolute() {
        path
    } else {
        workspace
            .map(Path::to_path_buf)
            .unwrap_or_default()
            .join(path)
    };

    match std::fs::canonicalize(&path) {
        Ok(absolute) => format!("file://{}", absolute.display()),
        Err(_) => format!("file://{}", path.display()),
    }
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
    let root = std::env::current_dir().ok();
    let root_uri = root
        .as_ref()
        .map(|path| format!("file://{}", path.display()));
    let name = root
        .as_ref()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .map(str::to_string);

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
}
