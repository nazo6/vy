//! Native widget tree models for custom `ui` views.
//!
//! Defined in `docs/ui.md` and `docs/adr/0005-native-plugin-ui.md`. The tree
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
