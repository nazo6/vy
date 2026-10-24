//! `ui` namespace.
//!
//! The widget builders live in the shim; the shared declaration only groups
//! the slot namespaces.

/// Builds native widget trees and updates UI slots.
#[vy_api_schema::vy_api_namespace("ui", shared = vy_client_api::namespaces::ui)]
mod api {}

pub use self::api::{EVENTS, METHODS, NAMESPACE, dispatch};
