//! Types and the method table shared by the `vy-api` shim and the host.
//!
//! This crate is the single source of truth for the plugin API boundary
//! ([ADR 0007](../../docs/adr/0007-rust-typed-plugin-api.md)). TypeScript
//! declarations are generated from here, and the host parses every call with
//! these types, so JavaScript and Rust cannot drift apart.
//!
//! Nothing here may depend on GPUI or rquickjs: the client and the script host
//! both use these types.

mod calls;
mod mode;
mod picker;
mod slots;
mod widget;

pub use calls::*;
pub use mode::Mode;
pub use picker::PickerItem;
pub use slots::StatuslineAlignment;
pub use widget::{ContainerKind, WidgetNode};
