//! Client-side document and pane state models.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;
use vy_core::PaneId;

use crate::config::{Config, SettingSource};
use crate::editor::{Editor, ViewState};
use crate::focus_path::{self, FocusPath};
use crate::keymap::{KeyContext, OverlayKind, PaneKind};
use crate::ui_list::ListState;
use crate::widget::{UiRole, WidgetNode};

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
    /// Ids from the widget tree root down to the focused widget.
    ///
    /// Empty when the pane shows an editor or nothing in its tree has focus.
    pub focus_path: FocusPath,
    /// Cursor, marks, and expansion of each list, tree, and table widget,
    /// keyed by the widget id. Kept across redraws.
    pub list_states: BTreeMap<String, ListState>,
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
            focus_path: FocusPath::new(),
            list_states: BTreeMap::new(),
        }
    }

    /// What this pane shows, as the keymap sees it.
    ///
    /// A pane showing a registered view is a `ui` pane; the view id goes in
    /// `viewType`. The pane kinds `webview` and `terminal` exist in the keymap
    /// context but no pane produces them yet.
    pub fn pane_kind(&self) -> PaneKind {
        if self.widget_tree.is_some() || self.view_type != "editor" {
            PaneKind::Ui
        } else {
            PaneKind::Editor
        }
    }

    /// The id of the view this pane shows, or empty for an editor pane.
    pub fn context_view_type(&self) -> String {
        if self.pane_kind() == PaneKind::Editor {
            String::new()
        } else {
            self.view_type.clone()
        }
    }

    /// The focusable node current focus resolves to, if any.
    pub fn focused_widget(&self) -> Option<&WidgetNode> {
        let tree = self.widget_tree.as_ref()?;
        focus_path::focused_node(tree, &self.focus_path)
    }

    pub fn ui_role(&self) -> Option<UiRole> {
        self.focused_widget().and_then(WidgetNode::focus_role)
    }

    pub fn ui_focus(&self) -> Option<&str> {
        self.ui_role()?;
        self.focus_path.focused()
    }

    /// Drops focus when the tree no longer contains the focused widget.
    pub fn normalize_focus(&mut self) {
        if self.focus_path.is_empty() {
            return;
        }
        self.focus_path = focus_path::normalize(self.widget_tree.as_ref(), &self.focus_path);
    }

    /// Moves focus to the next or previous focusable widget, wrapping.
    pub fn step_focus(&mut self, forward: bool) {
        let Some(tree) = self.widget_tree.as_ref() else {
            self.focus_path = FocusPath::new();
            return;
        };
        self.focus_path = focus_path::step(tree, &self.focus_path, forward);
    }

    /// Focuses the widget at `path`, or clears focus when it is not focusable.
    pub fn set_focus_path(&mut self, path: FocusPath) {
        self.focus_path = if self
            .widget_tree
            .as_ref()
            .is_some_and(|tree| focus_path::focused_node(tree, &path).is_some())
        {
            path
        } else {
            FocusPath::new()
        };
    }

    /// Fills the pane-owned values of a keymap context.
    ///
    /// `uiRole` and `uiFocus` are set only while a focusable widget has focus,
    /// so `uiRole == 'list'` stays false in an editor pane and in a UI pane
    /// whose focus is on the tree container.
    pub fn apply_key_context(&self, context: &mut KeyContext) {
        context.pane_kind = self.pane_kind();
        context.view_type = self.context_view_type();
        context.ui_role = self.ui_role();
        context.ui_focus = self.ui_focus().map(str::to_string);
        context.ui_selection_count = if context.ui_role.is_some_and(UiRole::has_selection) {
            self.ui_selection_count()
        } else {
            0
        };
    }

    /// Number of selected items in the focused widget.
    pub fn ui_selection_count(&self) -> usize {
        let Some((id, _)) = self.focused_list() else {
            return 0;
        };
        self.list_state(id).map_or(0, ListState::selected_count)
    }

    /// Selection state of one widget in this pane's tree.
    ///
    /// The state is created on demand and kept across redraws, so the cursor
    /// and marked rows survive a new tree from the plugin.
    pub fn list_state_mut(&mut self, widget_id: &str) -> &mut ListState {
        self.list_states.entry(widget_id.to_string()).or_default()
    }

    pub fn list_state(&self, widget_id: &str) -> Option<&ListState> {
        self.list_states.get(widget_id)
    }

    /// The focused widget, when it has a selection role.
    pub fn focused_list(&self) -> Option<(&str, &WidgetNode)> {
        let widget = self.focused_widget()?;
        let role = widget.focus_role()?;
        if !role.has_selection() {
            return None;
        }
        Some((self.focus_path.focused()?, widget))
    }

    /// Widget focus and list selection, for change detection.
    ///
    /// The view compares this before and after a key press so a command that
    /// only moves focus or a list cursor still requests a frame.
    pub fn ui_snapshot(&self) -> (FocusPath, BTreeMap<String, ListState>) {
        (self.focus_path.clone(), self.list_states.clone())
    }

    /// Keeps every list state in step with a freshly rendered tree.
    ///
    /// Rows that disappeared are dropped, and a widget that left the tree
    /// loses its state so a later tree with the same id starts clean. A widget
    /// that is new to the tree gets a state with its first row selected.
    pub fn normalize_list_states(&mut self) {
        let Some(tree) = self.widget_tree.clone() else {
            self.list_states.clear();
            return;
        };

        let mut selection_ids = BTreeSet::new();
        let mut nodes = Vec::new();
        focus_path::walk_widgets(&tree, &mut |node| {
            nodes.push(node.clone());
            if let Some(id) = node.id()
                && node.focus_role().is_some_and(UiRole::has_selection)
            {
                selection_ids.insert(id.to_string());
            }
        });
        self.list_states.retain(|id, _| selection_ids.contains(id));

        for node in &nodes {
            let Some(id) = node.id() else {
                continue;
            };
            if !selection_ids.contains(id) {
                continue;
            }
            let state = self.list_states.entry(id.to_string()).or_default();
            state.normalize(&crate::ui_list::all_ids(node));
        }
    }

    /// Whether the keymap context should say the editor owns focus.
    pub fn context_editor_focus(&self, overlay: OverlayKind) -> bool {
        overlay == OverlayKind::None && self.pane_kind() == PaneKind::Editor
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

    fn panel_tree() -> WidgetNode {
        WidgetNode::Container {
            id: Some("panel".to_string()),
            kind: crate::widget::ContainerKind::Column,
            gap: None,
            children: vec![
                WidgetNode::Button {
                    id: "run".to_string(),
                    label: "Run".to_string(),
                    command: "example.run".to_string(),
                    variant: None,
                },
                WidgetNode::TextInput {
                    id: Some("query".to_string()),
                    value: String::new(),
                    placeholder: None,
                },
            ],
        }
    }

    #[test]
    fn an_editor_pane_exposes_no_widget_context() {
        let pane = PaneState::new(1, Some(1));
        let mut context = KeyContext::default();
        pane.apply_key_context(&mut context);

        assert_eq!(context.pane_kind, PaneKind::Editor);
        assert_eq!(context.view_type, "");
        assert_eq!(context.ui_role, None);
        assert_eq!(context.ui_focus, None);
        assert!(pane.context_editor_focus(OverlayKind::None));
        assert!(!pane.context_editor_focus(OverlayKind::Palette));
    }

    #[test]
    fn a_ui_pane_reports_its_view_and_focused_widget() {
        let mut pane = PaneState::new(1, Some(1));
        pane.view_type = "example.panel".to_string();
        pane.widget_tree = Some(panel_tree());
        pane.set_focus_path(FocusPath::from_ids(["panel", "query"]));

        let mut context = KeyContext::default();
        pane.apply_key_context(&mut context);

        assert_eq!(context.pane_kind, PaneKind::Ui);
        assert_eq!(context.view_type, "example.panel");
        assert_eq!(context.ui_role, Some(UiRole::TextInput));
        assert_eq!(context.ui_focus.as_deref(), Some("query"));
        assert!(!pane.context_editor_focus(OverlayKind::None));

        // Without widget focus, `uiRole` is unset but the pane is still a UI pane.
        pane.set_focus_path(FocusPath::new());
        pane.apply_key_context(&mut context);
        assert_eq!(context.pane_kind, PaneKind::Ui);
        assert_eq!(context.ui_role, None);
        assert_eq!(context.ui_focus, None);
    }

    #[test]
    fn focus_steps_through_the_tree_and_recovers_after_a_redraw() {
        let mut pane = PaneState::new(1, Some(1));
        pane.widget_tree = Some(panel_tree());

        pane.step_focus(true);
        assert_eq!(pane.focus_path, FocusPath::from_ids(["panel", "run"]));
        pane.step_focus(true);
        assert_eq!(pane.focus_path, FocusPath::from_ids(["panel", "query"]));
        pane.step_focus(true);
        assert_eq!(pane.focus_path, FocusPath::from_ids(["panel", "run"]));
        pane.step_focus(false);
        assert_eq!(pane.focus_path, FocusPath::from_ids(["panel", "query"]));

        // A redraw replaces the tree; the focused widget is gone.
        pane.widget_tree = Some(WidgetNode::Container {
            id: Some("panel".to_string()),
            kind: crate::widget::ContainerKind::Column,
            gap: None,
            children: vec![WidgetNode::Button {
                id: "run".to_string(),
                label: "Run".to_string(),
                command: "example.run".to_string(),
                variant: None,
            }],
        });
        pane.normalize_focus();
        assert_eq!(pane.focus_path, FocusPath::from_ids(["panel", "run"]));

        // Dropping the tree clears focus.
        pane.widget_tree = None;
        pane.normalize_focus();
        assert_eq!(pane.focus_path, FocusPath::new());
    }

    #[test]
    fn negative_wrap_inheritance_is_unchanged_by_focus_state() {
        let mut config = Config::with_defaults();
        let mut pane = PaneState::new(1, None);
        pane.view_type = "example.panel".to_string();
        pane.widget_tree = Some(panel_tree());

        config.set("editor.wrap", Value::Bool(true));
        assert!(pane.is_wrap(&config));
    }

    #[test]
    fn a_focused_list_reports_its_selected_count() {
        let mut pane = PaneState::new(1, Some(1));
        pane.view_type = "example.results".to_string();
        pane.widget_tree = Some(WidgetNode::List {
            id: "results".to_string(),
            items: vec![
                crate::widget::ListItem::new("a", "Alpha"),
                crate::widget::ListItem::new("b", "Beta"),
            ],
            multiple: true,
        });
        pane.set_focus_path(FocusPath::from_ids(["results"]));
        pane.normalize_list_states();

        let mut context = KeyContext::default();
        pane.apply_key_context(&mut context);
        assert_eq!(context.ui_role, Some(UiRole::List));
        // A new list starts with its first row selected.
        assert_eq!(context.ui_selection_count, 1);

        pane.list_state_mut("results").toggle("b");
        pane.apply_key_context(&mut context);
        assert_eq!(context.ui_selection_count, 2);
        assert_eq!(pane.ui_selection_count(), 2);
    }

    #[test]
    fn selection_count_is_zero_without_a_focused_selection_widget() {
        let mut pane = PaneState::new(1, Some(1));
        pane.view_type = "example.panel".to_string();
        pane.widget_tree = Some(panel_tree());
        pane.normalize_list_states();

        // No widget focus yet.
        assert_eq!(pane.ui_selection_count(), 0);

        // Focus on a button, not a list.
        pane.set_focus_path(FocusPath::from_ids(["panel", "run"]));
        assert_eq!(pane.ui_selection_count(), 0);
    }

    #[test]
    fn list_states_follow_a_redrawn_tree() {
        let mut pane = PaneState::new(1, Some(1));
        pane.view_type = "example.results".to_string();
        pane.widget_tree = Some(WidgetNode::List {
            id: "results".to_string(),
            items: vec![
                crate::widget::ListItem::new("a", "Alpha"),
                crate::widget::ListItem::new("b", "Beta"),
            ],
            multiple: false,
        });
        pane.normalize_list_states();
        pane.list_state_mut("results").select_single("b");
        assert_eq!(
            pane.list_state("results").and_then(ListState::cursor),
            Some("b")
        );

        // The new tree drops row "b"; the cursor falls back to the first row.
        pane.widget_tree = Some(WidgetNode::List {
            id: "results".to_string(),
            items: vec![crate::widget::ListItem::new("a", "Alpha")],
            multiple: false,
        });
        pane.normalize_list_states();
        assert_eq!(
            pane.list_state("results").and_then(ListState::cursor),
            Some("a")
        );

        // The widget leaves the tree; its state goes too.
        pane.widget_tree = Some(panel_tree());
        pane.normalize_list_states();
        assert!(pane.list_state("results").is_none());
        assert!(pane.list_states.is_empty());
    }

    #[test]
    fn a_snapshot_changes_when_focus_or_a_list_cursor_moves() {
        let mut pane = PaneState::new(1, Some(1));
        pane.view_type = "example.panel".to_string();
        pane.widget_tree = Some(panel_tree());
        pane.normalize_list_states();

        // Moving focus alone changes the snapshot, which is what makes the
        // view repaint for `Tab` instead of waiting for the next cursor blink.
        let before = pane.ui_snapshot();
        pane.step_focus(true);
        let after = pane.ui_snapshot();
        assert_ne!(before, after);

        // Moving a list cursor alone changes it too.
        let before = pane.ui_snapshot();
        pane.list_state_mut("results").select_single("b");
        assert_ne!(before, pane.ui_snapshot());

        // A key that changes nothing keeps it equal.
        let before = pane.ui_snapshot();
        assert_eq!(before, pane.ui_snapshot());
    }
}
