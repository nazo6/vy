//! Native widget tree models for custom `ui` views.
//!
//! Defined in `docs/ui.md` and [`ADR 0005`](../../docs/adr/0005-native-plugin-ui.md).
//! The tree crosses the plugin boundary, so the types live here and the client
//! re-exports them.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ThemeColor;

/// Layout container direction or kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum ContainerKind {
    Column,
    Row,
    Stack,
}

/// Spacing between the children of a container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Gap {
    Sm,
    Md,
    Lg,
}

/// Font a text widget asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Font {
    Mono,
}

/// Visual weight of a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum ButtonVariant {
    Primary,
    Danger,
}

/// A node in the declarative widget tree rendered by the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export)]
pub enum WidgetNode {
    /// Vertical, horizontal, or stacked layout container.
    Container {
        #[serde(default)]
        #[ts(optional)]
        id: Option<String>,
        kind: ContainerKind,
        #[serde(default)]
        #[ts(optional)]
        gap: Option<Gap>,
        #[serde(default)]
        children: Vec<WidgetNode>,
    },
    /// Static or dynamic text item.
    Text {
        #[serde(default)]
        #[ts(optional)]
        id: Option<String>,
        text: String,
        #[serde(default)]
        #[ts(optional)]
        color: Option<ThemeColor>,
        #[serde(default)]
        #[ts(optional)]
        font: Option<Font>,
    },
    /// Clickable action button triggering a registered command.
    Button {
        id: String,
        label: String,
        command: String,
        #[serde(default)]
        #[ts(optional)]
        variant: Option<ButtonVariant>,
    },
    /// Text input field.
    TextInput {
        #[serde(default)]
        #[ts(optional)]
        id: Option<String>,
        #[serde(default)]
        value: String,
        #[serde(default)]
        #[ts(optional)]
        placeholder: Option<String>,
    },
    /// Visual separator line.
    Separator {
        #[serde(default)]
        #[ts(optional)]
        id: Option<String>,
    },
}
