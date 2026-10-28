//! Host defaults for the `vy.ui.*` actions.
//!
//! Movement and selection run entirely in the client so a key press does not
//! wait for the command queue and a script round trip
//! ([ADR 0024](../../../docs/adr/0024-keyboard-first-plugin-ui.md) §5). A
//! plugin overrides an action by mapping its command id in `views.register`;
//! that path is not implemented yet, so every action uses the host default.

use crate::editor::Mode;
use crate::overlay::Overlay;
use crate::palette::PaletteState;
use crate::ui_list::{self, ListState};
use crate::widget::{UiRole, WidgetNode};

/// Arguments handed to a plugin command for one action (ADR 0024 §5).
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiActionArgs {
    pub action: String,
    pub view_id: String,
    pub pane_id: u64,
    pub role: Option<UiRole>,
    pub focus_id: Option<String>,
    pub selected_ids: Vec<String>,
    pub anchor_id: Option<String>,
    pub expanded_ids: Vec<String>,
    /// Tree depth of the cursor row.
    pub depth: usize,
    pub query: String,
    pub count: Option<u32>,
    pub register: Option<char>,
}

/// What one action asks the view to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiActionOutcome {
    /// Nothing changed.
    None,
    /// The action was handled; the view should redraw.
    Handled,
    /// The action moved widget focus.
    Focus { forward: bool },
    /// The action asked to leave insert mode.
    LeaveInsert,
    /// The row's command should run. The view owns command execution.
    RunCommand(String),
    /// The action was not handled, so the caller falls through to `Esc` layers.
    Unhandled,
}

/// Runs one action against the rows of a widget.
///
/// `page` is the number of rows a page scrolls; the view supplies the visible
/// row count.
pub fn apply_to_widget(
    node: &WidgetNode,
    state: &mut ListState,
    action: &str,
    count: Option<u32>,
    page: usize,
) -> UiActionOutcome {
    let rows = ui_list::rows(node, state);
    let visible: Vec<&str> = rows.iter().map(|row| row.id.as_str()).collect();
    let role = node.focus_role();
    let step = count.unwrap_or(1).max(1) as isize;

    let cursor_index = state
        .cursor()
        .and_then(|cursor| visible.iter().position(|id| *id == cursor));

    let move_to = |state: &mut ListState, index: Option<usize>| {
        if let Some(index) = index
            && let Some(id) = visible.get(index)
        {
            state.move_cursor(*id, node.multiple());
            return UiActionOutcome::Handled;
        }
        UiActionOutcome::None
    };

    match action {
        "next" => {
            let target = cursor_index.map_or(Some(0), |index| {
                let next = index as isize + step;
                (!visible.is_empty()).then(|| next.clamp(0, visible.len() as isize - 1) as usize)
            });
            move_to(state, target)
        }
        "prev" => {
            let target = cursor_index.map(|index| (index as isize - step).max(0) as usize);
            move_to(state, target)
        }
        "first" => move_to(state, (!visible.is_empty()).then_some(0)),
        "last" => move_to(state, visible.len().checked_sub(1)),
        "pageDown" => {
            let step = page.max(1) * count.unwrap_or(1).max(1) as usize;
            let target = cursor_index.map_or(Some(0), |index| {
                Some((index + step).min(visible.len().saturating_sub(1)))
            });
            move_to(state, target)
        }
        "pageUp" => {
            let step = page.max(1) * count.unwrap_or(1).max(1) as usize;
            let target = cursor_index.map(|index| index.saturating_sub(step));
            move_to(state, target)
        }
        "toggle" => {
            let Some(cursor) = state.cursor().map(str::to_string) else {
                return UiActionOutcome::None;
            };
            // A row command owns the meaning of toggling. That is how a file
            // explorer expands a directory: the plugin fetches the children and
            // renders a new tree, so the host cannot guess.
            if let Some(command) = ui_list::command_for(node, &cursor) {
                return UiActionOutcome::RunCommand(command);
            }
            if role == Some(UiRole::Tree) {
                state.toggle_expanded(cursor);
            } else if node.multiple() {
                state.toggle(cursor);
            } else {
                state.select_single(cursor);
            }
            UiActionOutcome::Handled
        }
        "expand" | "collapse" => {
            let Some(cursor) = state.cursor().map(str::to_string) else {
                return UiActionOutcome::None;
            };
            let expands = action == "expand";
            // A row command owns expansion for the same reason as `toggle`:
            // only the plugin can load the children.
            if let Some(command) = ui_list::command_for(node, &cursor) {
                if role != Some(UiRole::Tree) {
                    return if expands {
                        UiActionOutcome::RunCommand(command)
                    } else {
                        UiActionOutcome::Unhandled
                    };
                }
                // A tree row reports whether it is open through its own data, so
                // repeating `l` on an open row does nothing.
                let is_open = ui_list::is_expanded(node, &cursor, state);
                if is_open != expands {
                    return UiActionOutcome::RunCommand(command);
                }
                return UiActionOutcome::None;
            }
            if role != Some(UiRole::Tree) {
                return UiActionOutcome::Unhandled;
            }
            let expanded = state.is_expanded(&cursor);
            if expanded == expands {
                return UiActionOutcome::None;
            }
            state.toggle_expanded(cursor);
            UiActionOutcome::Handled
        }
        // `activate`, `yank`, `cancel`, and the focus actions need data the
        // widget alone does not hold, so the caller handles them.
        _ => UiActionOutcome::Unhandled,
    }
}

/// The text `vy.ui.yank` copies: every marked row, or the cursor row.
pub fn yank_text(node: &WidgetNode, state: &ListState) -> Option<String> {
    let mut ids: Vec<String> = state.selected().cloned().collect();
    if ids.is_empty()
        && let Some(cursor) = state.cursor()
    {
        ids.push(cursor.to_string());
    }
    let texts: Vec<String> = ids
        .iter()
        .filter_map(|id| ui_list::yank_text(node, id))
        .collect();
    (!texts.is_empty()).then(|| texts.join("\n"))
}

/// Whether closing the overlay should report "not handled" to the Esc layers.
///
/// The host default for `cancel` restores temporary state (a query, marks, a
/// preview, a temporary expansion). When there is nothing to restore the
/// surface closes, which is why the caller needs to know.
pub fn cancel_palette(palette: &mut PaletteState) -> UiActionOutcome {
    if palette.input.is_empty() && palette.selected == 0 {
        return UiActionOutcome::Unhandled;
    }
    palette.input.clear();
    UiActionOutcome::Handled
}

/// The action name a `vy.ui.*` command runs.
pub fn action_name_of_overlay(overlay: &Overlay) -> Option<&'static str> {
    let _ = overlay;
    None
}

/// Whether a widget role supports an action.
pub fn role_supports(role: Option<UiRole>, action: &str) -> bool {
    match action {
        "expand" | "collapse" => role == Some(UiRole::Tree),
        "toggle" => role.is_some_and(UiRole::has_selection) || role == Some(UiRole::Tree),
        "yank" => role.is_some_and(UiRole::has_selection),
        "next" | "prev" | "pageDown" | "pageUp" | "first" | "last" => {
            role.is_some_and(UiRole::has_selection)
        }
        "activate" | "cancel" | "focusNext" | "focusPrev" => true,
        _ => false,
    }
}

/// The mode the widget leaves when `cancel` runs on a text input.
pub fn cancel_mode(role: Option<UiRole>, mode: Mode) -> Option<Mode> {
    (role == Some(UiRole::TextInput) && mode == Mode::Insert).then_some(Mode::Normal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::ListItem;

    fn list() -> WidgetNode {
        WidgetNode::List {
            id: "results".to_string(),
            items: vec![
                ListItem::new("a", "Alpha"),
                ListItem::new("b", "Beta"),
                ListItem::new("c", "Gamma"),
                ListItem::new("d", "Delta"),
                ListItem::new("e", "Epsilon"),
            ],
            multiple: true,
        }
    }

    fn tree() -> WidgetNode {
        WidgetNode::Tree {
            id: "files".to_string(),
            nodes: vec![
                crate::widget::TreeNode::new("src", "src")
                    .with_children(vec![crate::widget::TreeNode::new("main", "main.ts")]),
                crate::widget::TreeNode::new("readme", "README.md"),
            ],
            multiple: false,
        }
    }

    fn selected(state: &ListState) -> Vec<String> {
        state.selected().cloned().collect()
    }

    #[test]
    fn moves_the_cursor_with_clamping() {
        let node = list();
        let mut state = ListState::new();
        state.normalize(&ui_list::all_ids(&node));
        assert_eq!(state.cursor(), Some("a"));

        assert_eq!(
            apply_to_widget(&node, &mut state, "next", None, 10),
            UiActionOutcome::Handled
        );
        assert_eq!(state.cursor(), Some("b"));

        // A count moves several rows.
        apply_to_widget(&node, &mut state, "next", Some(2), 10);
        assert_eq!(state.cursor(), Some("d"));

        // The last row is a boundary, not a wrap.
        apply_to_widget(&node, &mut state, "next", Some(9), 10);
        assert_eq!(state.cursor(), Some("e"));
        apply_to_widget(&node, &mut state, "next", None, 10);
        assert_eq!(state.cursor(), Some("e"));

        apply_to_widget(&node, &mut state, "prev", None, 10);
        assert_eq!(state.cursor(), Some("d"));
        apply_to_widget(&node, &mut state, "prev", Some(9), 10);
        assert_eq!(state.cursor(), Some("a"));

        apply_to_widget(&node, &mut state, "last", None, 10);
        assert_eq!(state.cursor(), Some("e"));
        apply_to_widget(&node, &mut state, "first", None, 10);
        assert_eq!(state.cursor(), Some("a"));
    }

    #[test]
    fn pages_by_the_visible_row_count() {
        let node = list();
        let mut state = ListState::new();
        state.normalize(&ui_list::all_ids(&node));

        apply_to_widget(&node, &mut state, "pageDown", None, 2);
        assert_eq!(state.cursor(), Some("c"));
        apply_to_widget(&node, &mut state, "pageDown", None, 2);
        assert_eq!(state.cursor(), Some("e"));
        apply_to_widget(&node, &mut state, "pageUp", None, 2);
        assert_eq!(state.cursor(), Some("c"));
        apply_to_widget(&node, &mut state, "pageUp", None, 9);
        assert_eq!(state.cursor(), Some("a"));
    }

    #[test]
    fn marks_rows_only_when_the_list_allows_it() {
        let node = list();
        let mut state = ListState::new();
        state.normalize(&ui_list::all_ids(&node));
        state.select_single("b");

        // Toggling a marked row unmarks it.
        apply_to_widget(&node, &mut state, "toggle", None, 10);
        assert!(selected(&state).is_empty());

        // A multiple-selection list keeps the empty mark set while the cursor
        // moves, so `next` does not silently mark every row it visits.
        apply_to_widget(&node, &mut state, "next", None, 10);
        assert_eq!(state.cursor(), Some("c"));
        assert!(selected(&state).is_empty());

        apply_to_widget(&node, &mut state, "toggle", None, 10);
        assert_eq!(selected(&state), vec!["c".to_string()]);

        state.select_single("b");
        state.toggle("c");
        assert_eq!(selected(&state), vec!["b".to_string(), "c".to_string()]);
        assert_eq!(yank_text(&node, &state).as_deref(), Some("Beta\nGamma"));
    }

    #[test]
    fn a_single_selection_list_keeps_one_mark() {
        let node = WidgetNode::List {
            id: "results".to_string(),
            items: vec![ListItem::new("a", "Alpha"), ListItem::new("b", "Beta")],
            multiple: false,
        };
        let mut state = ListState::new();
        state.normalize(&ui_list::all_ids(&node));
        state.select_single("a");
        apply_to_widget(&node, &mut state, "next", None, 10);
        apply_to_widget(&node, &mut state, "toggle", None, 10);
        assert_eq!(selected(&state), vec!["b".to_string()]);
    }

    #[test]
    fn a_tree_toggles_expansion_instead_of_marking() {
        let node = tree();
        let mut state = ListState::new();
        state.normalize(&ui_list::all_ids(&node));
        assert_eq!(state.cursor(), Some("src"));

        assert_eq!(
            apply_to_widget(&node, &mut state, "toggle", None, 10),
            UiActionOutcome::Handled
        );
        assert!(state.is_expanded("src"));
        assert_eq!(state.selected_count(), 1);

        // `expand` and `collapse` are tree-only and report "no change".
        assert_eq!(
            apply_to_widget(&node, &mut state, "expand", None, 10),
            UiActionOutcome::None
        );
        assert_eq!(
            apply_to_widget(&node, &mut state, "collapse", None, 10),
            UiActionOutcome::Handled
        );
        assert!(!state.is_expanded("src"));
    }

    #[test]
    fn list_actions_that_need_the_view_are_left_to_the_caller() {
        let node = list();
        let mut state = ListState::new();
        state.normalize(&ui_list::all_ids(&node));
        for action in ["activate", "yank", "cancel"] {
            assert_eq!(
                apply_to_widget(&node, &mut state, action, None, 10),
                UiActionOutcome::Unhandled,
                "{action}"
            );
        }
        // A list has no expansion, so `expand` is unhandled too.
        assert_eq!(
            apply_to_widget(&node, &mut state, "expand", None, 10),
            UiActionOutcome::Unhandled
        );
    }

    #[test]
    fn yanks_the_cursor_when_nothing_is_marked() {
        let node = list();
        let mut state = ListState::new();
        state.normalize(&ui_list::all_ids(&node));
        state.reset();
        state.select_single("d");
        assert_eq!(yank_text(&node, &state).as_deref(), Some("Delta"));
    }

    #[test]
    fn a_row_command_owns_toggle_and_expand() {
        let node = WidgetNode::List {
            id: "results".to_string(),
            items: vec![
                ListItem {
                    id: "open".to_string(),
                    label: "Open".to_string(),
                    description: None,
                    text: None,
                    command: Some("example.open".to_string()),
                },
                ListItem::new("plain", "Plain"),
            ],
            multiple: false,
        };
        let mut state = ListState::new();
        state.normalize(&ui_list::all_ids(&node));
        state.select_single("open");

        // The row command runs instead of the host marking the row.
        assert_eq!(
            apply_to_widget(&node, &mut state, "toggle", None, 10),
            UiActionOutcome::RunCommand("example.open".to_string())
        );

        // A row without a command still toggles through the host.
        state.select_single("plain");
        assert_eq!(
            apply_to_widget(&node, &mut state, "toggle", None, 10),
            UiActionOutcome::Handled
        );
    }

    #[test]
    fn a_tree_row_command_runs_on_expand_and_collapse() {
        let node = WidgetNode::Tree {
            id: "files".to_string(),
            nodes: vec![crate::widget::TreeNode {
                id: "src".to_string(),
                label: "src".to_string(),
                text: None,
                command: Some("example.tree.toggle_dir".to_string()),
                expanded: Some(false),
                children: vec![],
            }],
            multiple: false,
        };
        let mut state = ListState::new();
        state.normalize(&ui_list::all_ids(&node));

        // `l` asks the plugin to expand.
        assert_eq!(
            apply_to_widget(&node, &mut state, "expand", None, 10),
            UiActionOutcome::RunCommand("example.tree.toggle_dir".to_string())
        );

        // `x` delegates too, because the plugin owns the children.
        assert_eq!(
            apply_to_widget(&node, &mut state, "toggle", None, 10),
            UiActionOutcome::RunCommand("example.tree.toggle_dir".to_string())
        );

        // The row is closed, so `h` has nothing to close.
        assert_eq!(
            apply_to_widget(&node, &mut state, "collapse", None, 10),
            UiActionOutcome::None
        );
    }

    #[test]
    fn an_open_tree_row_runs_its_command_on_collapse() {
        let node = WidgetNode::Tree {
            id: "files".to_string(),
            nodes: vec![crate::widget::TreeNode {
                id: "src".to_string(),
                label: "src".to_string(),
                text: None,
                command: Some("example.tree.toggle_dir".to_string()),
                expanded: Some(true),
                children: vec![],
            }],
            multiple: false,
        };
        let mut state = ListState::new();
        state.normalize(&ui_list::all_ids(&node));

        assert_eq!(
            apply_to_widget(&node, &mut state, "collapse", None, 10),
            UiActionOutcome::RunCommand("example.tree.toggle_dir".to_string())
        );
        // `l` on an open row does nothing.
        assert_eq!(
            apply_to_widget(&node, &mut state, "expand", None, 10),
            UiActionOutcome::None
        );
    }

    #[test]
    fn roles_limit_which_actions_apply() {
        assert!(role_supports(Some(UiRole::Tree), "expand"));
        assert!(!role_supports(Some(UiRole::List), "expand"));
        assert!(role_supports(Some(UiRole::List), "toggle"));
        assert!(role_supports(Some(UiRole::Table), "yank"));
        assert!(!role_supports(Some(UiRole::Button), "yank"));
        assert!(role_supports(Some(UiRole::TextInput), "activate"));
        assert!(!role_supports(None, "toggle"));
    }

    #[test]
    fn cancel_leaves_insert_only_from_a_text_input() {
        assert_eq!(
            cancel_mode(Some(UiRole::TextInput), Mode::Insert),
            Some(Mode::Normal)
        );
        assert_eq!(cancel_mode(Some(UiRole::TextInput), Mode::Normal), None);
        assert_eq!(cancel_mode(Some(UiRole::List), Mode::Insert), None);
    }

    #[test]
    fn cancel_reports_whether_the_palette_had_temporary_state() {
        let mut palette = PaletteState::new();
        assert_eq!(cancel_palette(&mut palette), UiActionOutcome::Unhandled);
        palette.push_char('a');
        assert_eq!(cancel_palette(&mut palette), UiActionOutcome::Handled);
        assert!(palette.input.is_empty());
        assert_eq!(cancel_palette(&mut palette), UiActionOutcome::Unhandled);
    }
}
