//! Client-side document and pane state models.

use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Duration;
use vy_core::PaneId;

use crate::config::{Config, SettingSource};
use crate::editor::{Editor, ViewState};
use crate::widget::WidgetNode;

/// Default rows drawn before measuring the window.
pub const VISIBLE_LINES: usize = 30;

/// A buffer the client draws. The core owns the authoritative text.
pub struct DocumentState {
    pub id: u64,
    pub uri: String,
    pub editor: Editor,
    /// Version when this document was last saved or opened from disk.
    pub saved_version: u64,
    /// Whether a snapshot has filled this document.
    pub loaded: bool,
}

impl DocumentState {
    pub fn new(id: u64, uri: String, text: &str, version: u64) -> Self {
        let mut editor = Editor::with_text(text, version);
        editor.set_transaction_idle(Duration::from_millis(500));
        Self {
            id,
            uri,
            editor,
            saved_version: version,
            loaded: false,
        }
    }

    /// Whether the document has unsaved edits.
    pub fn is_dirty(&self) -> bool {
        self.editor.version() != self.saved_version
    }
}

/// One pane in the client's view tree.
#[derive(Debug)]
pub struct PaneState {
    #[allow(dead_code)]
    pub id: PaneId,
    pub document_id: Option<u64>,
    /// Cursor, selection, and mode of this pane.
    pub view: ViewState,
    /// Text the input method is still converting. It never reaches the core.
    pub preedit: String,
    /// Range the input method selected inside the preedit, in UTF-16 code units.
    pub preedit_selection: Option<std::ops::Range<usize>>,
    /// First line drawn in this pane.
    pub scroll_top: usize,
    /// First column scrolled in this pane when wrap is disabled.
    pub scroll_left: usize,
    /// Visual row offset within the first line drawn in this pane when wrap is enabled.
    pub scroll_sub_line: usize,
    /// Rows that fit in this pane. Updated on every render.
    pub viewport_rows: usize,
    /// Columns that fit horizontally in this pane. Updated on every render.
    pub viewport_cols: usize,
    /// Window-scoped setting overrides for this pane.
    pub settings: BTreeMap<String, Value>,
    /// Kind of view displayed in this pane (e.g. "editor", "ui", "list", etc.).
    pub view_type: String,
    /// Root of the custom widget tree if this is a "ui" view.
    pub widget_tree: Option<WidgetNode>,
}

impl PaneState {
    pub fn new(id: PaneId, document_id: Option<u64>) -> Self {
        Self {
            id,
            document_id,
            view: ViewState::default(),
            preedit: String::new(),
            preedit_selection: None,
            scroll_top: 0,
            scroll_left: 0,
            scroll_sub_line: 0,
            viewport_rows: VISIBLE_LINES,
            viewport_cols: 80,
            settings: BTreeMap::new(),
            view_type: "editor".to_string(),
            widget_tree: None,
        }
    }

    /// Resolves a setting for this pane: the window override, then the
    /// user/project/memory layers, then the built-in default.
    pub fn effective_setting(&self, key: &str, config: &Config) -> (Option<Value>, SettingSource) {
        if let Some(value) = self.settings.get(key) {
            return (Some(value.clone()), SettingSource::Window);
        }
        config.effective_with_source(key)
    }

    /// Sets a window-scoped override for this pane.
    pub fn set_window_setting(&mut self, key: impl Into<String>, value: Value) {
        self.settings.insert(key.into(), value);
    }

    /// Removes a window-scoped override. Returns whether one existed.
    pub fn reset_window_setting(&mut self, key: &str) -> bool {
        self.settings.remove(key).is_some()
    }

    /// Whether line wrapping is active for this pane.
    pub fn is_wrap(&self, config: &Config) -> bool {
        self.effective_setting("editor.wrap", config)
            .0
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn pane_inherits_wrap_from_config_or_overrides() {
        let mut config = Config::with_defaults();
        let mut pane = PaneState::new(1, None);

        assert!(!pane.is_wrap(&config));

        pane.set_window_setting("editor.wrap", Value::Bool(true));
        assert!(pane.is_wrap(&config));
        assert_eq!(
            pane.effective_setting("editor.wrap", &config).1,
            SettingSource::Window
        );

        pane.set_window_setting("editor.wrap", Value::Bool(false));
        assert!(!pane.is_wrap(&config));

        assert!(pane.reset_window_setting("editor.wrap"));
        assert_eq!(
            pane.effective_setting("editor.wrap", &config).1,
            SettingSource::Default
        );

        config.set("editor.wrap", Value::Bool(true));
        assert!(pane.is_wrap(&config));
        assert_eq!(
            pane.effective_setting("editor.wrap", &config).1,
            SettingSource::Memory
        );
    }
}
