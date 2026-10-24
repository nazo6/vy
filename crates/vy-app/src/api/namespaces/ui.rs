//! `ui` namespace.
//!
//! The widget builders live in the shim; this module only declares the
//! namespace so the generated declarations can extend `UiSugar`.

/// Builds native widget trees and updates UI slots.
#[vy_api_schema::vy_api_namespace("ui")]
mod api {}

pub use self::api::{EVENTS, METHODS, NAMESPACE, dispatch};
