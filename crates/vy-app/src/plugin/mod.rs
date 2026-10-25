//! Plugin management, manifest parsing, discovery, and capability approval.
//!
//! Implements external plugin loading and the trust model defined in
//! `docs/specs/plugin-system.md` and `docs/specs/layout.md`.

pub mod activation;
pub mod bundle;
pub mod discovery;
pub mod manager;
pub mod manifest;
pub mod trust;

pub use activation::ActivationEvent;
pub use bundle::{BundleError, BundleInfo, MAX_BUNDLE_BYTES, validate_bundle};
pub use discovery::{DiscoveredPlugin, PluginEntry, discover_plugins};
pub use manager::{PluginApprovalRequest, PluginManager, PluginState};
pub use manifest::{
    ContributedCommand, PluginContributes, PluginEngines, PluginManifest, PluginManifestError,
};
pub use trust::{PluginTrustStatus, approve_plugin, evaluate_plugin_trust};
