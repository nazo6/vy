//! The empty `ui` namespace.
//!
//! Widget builders live in the shim. The namespace groups the slot namespaces.

/// Builds native widget trees and updates UI slots.
#[vy_api_schema::vy_api_trait("ui")]
pub trait UiApi {}
