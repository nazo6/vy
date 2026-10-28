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
            Self::Button { id, .. } => Some(id.as_str()),
        }
    }

    /// The focus role of this node, or `None` when it cannot take focus.
    ///
    /// Focus needs a stable `id`, so an unidentified node is never focusable.
    pub fn focus_role(&self) -> Option<UiRole> {
        match self {
            Self::Button { .. } => Some(UiRole::Button),
            Self::TextInput { id, .. } if id.is_some() => Some(UiRole::TextInput),
            _ => None,
        }
    }

    /// Child nodes in tree order.
    pub fn children(&self) -> &[Self] {
        match self {
            Self::Container { children, .. } => children,
            _ => &[],
        }
    }
}
