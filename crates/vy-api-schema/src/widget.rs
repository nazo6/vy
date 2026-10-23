//! Native widget tree models for custom `ui` views.
//!
//! Defined in `docs/ui.md` and [`ADR 0005`](../../docs/adr/0005-native-plugin-ui.md).
//! The tree crosses the plugin boundary, so the types live here and the client
//! re-exports them.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Layout container direction or kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum ContainerKind {
    Column,
    Row,
    Stack,
}

/// A node in the declarative widget tree rendered by the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export)]
pub enum WidgetNode {
    /// Vertical, horizontal, or stacked layout container.
    Container {
        id: Option<String>,
        kind: ContainerKind,
        #[serde(default)]
        gap: Option<String>,
        #[serde(default)]
        children: Vec<WidgetNode>,
    },
    /// Static or dynamic text item.
    Text {
        id: Option<String>,
        text: String,
        #[serde(default)]
        color: Option<String>,
        #[serde(default)]
        font: Option<String>,
    },
    /// Clickable action button triggering a registered command.
    Button {
        id: String,
        label: String,
        command: String,
        #[serde(default)]
        variant: Option<String>,
    },
    /// Text input field.
    TextInput {
        id: String,
        #[serde(default)]
        value: String,
        #[serde(default)]
        placeholder: Option<String>,
    },
    /// Visual separator line.
    Separator { id: Option<String> },
}
