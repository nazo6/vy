//! Native widget tree models for custom `ui` views.
//!
//! Defined in `docs/specs/ui.md` and `docs/adr/0005-native-plugin-ui.md`. The tree
//! crosses the plugin boundary, so the renderer and the declarations share
//! these types.

use serde::{Deserialize, Serialize};

use crate::color::ThemeColor;

/// Layout container direction or kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum ContainerKind {
    Column,
    Row,
    Stack,
}

/// Spacing between the children of a container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum Gap {
    Sm,
    Md,
    Lg,
}

/// Font a text widget asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum Font {
    Mono,
}

/// Visual weight of a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum ButtonVariant {
    Primary,
    Danger,
}

/// One row of a list or table.
///
/// `text` is copied by `vy.ui.yank`; when it is absent the label is used.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ListItem {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
}

impl ListItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: None,
            text: None,
        }
    }

    /// The text `vy.ui.yank` copies.
    pub fn yank_text(&self) -> &str {
        self.text.as_deref().unwrap_or(&self.label)
    }
}

/// One node of a tree widget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TreeNode {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub children: Vec<TreeNode>,
}

impl TreeNode {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            text: None,
            children: Vec::new(),
        }
    }

    pub fn with_children(mut self, children: Vec<TreeNode>) -> Self {
        self.children = children;
        self
    }

    /// The text `vy.ui.yank` copies.
    pub fn yank_text(&self) -> &str {
        self.text.as_deref().unwrap_or(&self.label)
    }
}

/// A node in the declarative widget tree rendered by the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WidgetNode {
    /// Vertical, horizontal, or stacked layout container.
    Container {
        #[serde(default)]
        id: Option<String>,
        kind: ContainerKind,
        #[serde(default)]
        gap: Option<Gap>,
        #[serde(default)]
        children: Vec<WidgetNode>,
    },
    /// Static or dynamic text item.
    Text {
        #[serde(default)]
        id: Option<String>,
        text: String,
        #[serde(default)]
        color: Option<ThemeColor>,
        #[serde(default)]
        font: Option<Font>,
    },
    /// Clickable action button triggering a registered command.
    Button {
        id: String,
        label: String,
        command: String,
        #[serde(default)]
        variant: Option<ButtonVariant>,
    },
    /// Text input field.
    TextInput {
        #[serde(default)]
        id: Option<String>,
        #[serde(default)]
        value: String,
        #[serde(default)]
        placeholder: Option<String>,
    },
    /// Scrollable list with one selected row.
    List {
        id: String,
        #[serde(default)]
        items: Vec<ListItem>,
        /// Whether `x` marks several rows instead of replacing the selection.
        #[serde(default)]
        multiple: bool,
    },
    /// Collapsible tree. `vy.ui.toggle` expands and collapses.
    Tree {
        id: String,
        #[serde(default)]
        nodes: Vec<TreeNode>,
        #[serde(default)]
        multiple: bool,
    },
    /// Table drawn from rows of cells. Selection works like a list.
    Table {
        id: String,
        #[serde(default)]
        columns: Vec<String>,
        #[serde(default)]
        items: Vec<ListItem>,
        #[serde(default)]
        multiple: bool,
    },
    /// Visual separator line.
    Separator {
        #[serde(default)]
        id: Option<String>,
    },
}

/// The role of a widget that can take keyboard focus.
///
/// The keymap exposes this as `uiRole`, so a binding can be limited to one kind
/// of widget ([ADR 0024](../../docs/adr/0024-keyboard-first-plugin-ui.md) §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum UiRole {
    List,
    Tree,
    Table,
    TextInput,
    Button,
}

impl UiRole {
    /// Name used by `when` expressions.
    pub fn name(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Tree => "tree",
            Self::Table => "table",
            Self::TextInput => "textInput",
            Self::Button => "button",
        }
    }

    /// Whether the role holds a selection of items.
    pub fn has_selection(self) -> bool {
        matches!(self, Self::List | Self::Tree | Self::Table)
    }
}

impl WidgetNode {
    /// The node id, when it has one.
    pub fn id(&self) -> Option<&str> {
        match self {
            Self::Container { id, .. }
            | Self::Text { id, .. }
            | Self::TextInput { id, .. }
            | Self::Separator { id } => id.as_deref(),
            Self::Button { id, .. }
            | Self::List { id, .. }
            | Self::Tree { id, .. }
            | Self::Table { id, .. } => Some(id.as_str()),
        }
    }

    /// The focus role of this node, or `None` when it cannot take focus.
    ///
    /// Focus needs a stable `id`, so an unidentified node is never focusable.
    pub fn focus_role(&self) -> Option<UiRole> {
        match self {
            Self::Button { .. } => Some(UiRole::Button),
            Self::List { .. } => Some(UiRole::List),
            Self::Tree { .. } => Some(UiRole::Tree),
            Self::Table { .. } => Some(UiRole::Table),
            Self::TextInput { id, .. } if id.is_some() => Some(UiRole::TextInput),
            _ => None,
        }
    }

    /// Child nodes in tree order.
    ///
    /// Lists, trees, and tables are leaves for focus purposes: their rows are
    /// data, not widgets.
    pub fn children(&self) -> &[Self] {
        match self {
            Self::Container { children, .. } => children,
            _ => &[],
        }
    }

    /// Whether `vy.ui.toggle` marks several rows instead of one.
    pub fn multiple(&self) -> bool {
        match self {
            Self::List { multiple, .. }
            | Self::Tree { multiple, .. }
            | Self::Table { multiple, .. } => *multiple,
            _ => false,
        }
    }
}
