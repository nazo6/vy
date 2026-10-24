//! Persisted layout and open documents.
//!
//! The core owns the layout, so it also owns the saved copy. The file is keyed by
//! the workspace URI and lives under `<data>/workspace-state/`. Unsaved text is
//! not stored here; the client restores it with `sync/request`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use vy_core::{Layout, LayoutNode, PaneId, Uri};

use super::{Pane, Session};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct SavedSession {
    pub window_id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<LayoutNode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_pane: Option<PaneId>,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub panes: Vec<SavedPane>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct SavedPane {
    pub pane_id: PaneId,
    pub view: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
}

impl Session {
    /// Loads the layout saved for the current workspace. Missing or broken files
    /// leave the session empty so the client can open its startup document.
    pub(super) fn restore_session(&mut self) {
        let Some(path) = self.state_path.clone() else {
            return;
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "could not read the saved layout");
                return;
            }
        };
        let saved: SavedSession = match serde_json::from_str(&text) {
            Ok(saved) => saved,
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "could not parse the saved layout");
                return;
            }
        };

        let layout = Layout::restore(
            saved.window_id,
            saved.layout,
            saved.active_pane,
            saved.revision,
        );
        let mut panes = HashMap::new();
        for pane_id in layout.leaves() {
            let saved_pane = saved.panes.iter().find(|pane| pane.pane_id == pane_id);
            let view = saved_pane
                .map(|pane| pane.view.clone())
                .unwrap_or_else(|| "editor".to_string());
            let document_id = saved_pane
                .and_then(|pane| pane.uri.as_deref())
                .and_then(|uri| Uri::parse(uri).ok())
                .and_then(|uri| match self.document_for_uri(uri) {
                    Ok((document_id, _, _, _)) => Some(document_id),
                    Err((code, message)) => {
                        tracing::warn!(pane_id, %code, %message, "could not reopen a document");
                        None
                    }
                });
            panes.insert(pane_id, Pane { view, document_id });
        }

        self.next_pane_id = panes.keys().max().copied().unwrap_or(0) + 1;
        self.panes = panes;
        self.layout = layout;
    }

    /// Writes the current layout. Failures are logged; they never fail a request.
    pub(super) fn persist_session(&self) {
        let Some(path) = &self.state_path else {
            return;
        };

        let mut panes: Vec<SavedPane> = self
            .panes
            .iter()
            .map(|(pane_id, pane)| SavedPane {
                pane_id: *pane_id,
                view: pane.view.clone(),
                uri: pane
                    .document_id
                    .and_then(|document_id| self.documents.get(&document_id))
                    .map(|buffer| buffer.uri().as_str().to_string()),
            })
            .collect();
        panes.sort_by_key(|pane| pane.pane_id);

        let saved = SavedSession {
            window_id: self.layout.window_id(),
            layout: self.layout.root().cloned(),
            active_pane: self.layout.active_pane(),
            revision: self.layout.revision(),
            panes,
        };
        let text = match serde_json::to_string_pretty(&saved) {
            Ok(text) => text,
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "could not encode the layout");
                return;
            }
        };
        if let Some(parent) = path.parent()
            && let Err(error) = std::fs::create_dir_all(parent)
        {
            tracing::warn!(path = %parent.display(), %error, "could not create the state directory");
            return;
        }
        if let Err(error) = std::fs::write(path, text) {
            tracing::warn!(path = %path.display(), %error, "could not write the layout");
        }
    }
}

/// Path of the saved layout for one workspace.
pub(super) fn workspace_state_path(data_dir: &Path, workspace: &Uri) -> PathBuf {
    data_dir
        .join("workspace-state")
        .join(format!("{}.json", workspace_hash(workspace.as_str())))
}

/// A stable 64-bit FNV-1a hash, used only for the file name.
fn workspace_hash(value: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_the_workspace_stably() {
        assert_eq!(
            workspace_hash("file:///home/user/project"),
            workspace_hash("file:///home/user/project")
        );
        assert_ne!(
            workspace_hash("file:///home/user/a"),
            workspace_hash("file:///home/user/b")
        );
        assert_eq!(workspace_hash("x").len(), 16);
    }

    #[test]
    fn puts_the_state_under_workspace_state() {
        let path = workspace_state_path(
            Path::new("/data/vy"),
            &Uri::parse("file:///home/user/project").expect("uri"),
        );
        assert!(path.starts_with("/data/vy/workspace-state"));
        assert_eq!(
            path.extension().and_then(|value| value.to_str()),
            Some("json")
        );
    }
}
