//! Plugin management, manifest parsing, discovery, and capability approval.
//!
//! Implements external plugin loading and the trust model defined in
//! `docs/plugin-system.md` and `docs/layout.md`.

pub mod discovery;
pub mod manager;
pub mod manifest;
pub mod trust;

pub use discovery::{DiscoveredPlugin, discover_plugins};
pub use manager::{PluginApprovalRequest, PluginManager, PluginState};
pub use manifest::{
    ContributedCommand, PluginContributes, PluginEngines, PluginManifest, PluginManifestError,
};
pub use trust::{PluginTrustStatus, approve_plugin, evaluate_plugin_trust};
