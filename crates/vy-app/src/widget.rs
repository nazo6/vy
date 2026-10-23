//! Native widget tree models for custom `ui` views.
//!
//! Defined in `docs/ui.md` and `docs/adr/0005-native-plugin-ui.md`. The tree
//! crosses the plugin boundary, so the types live in `vy-api-schema` and are
//! re-exported here for the renderer.

pub use vy_api_schema::{ContainerKind, WidgetNode};
