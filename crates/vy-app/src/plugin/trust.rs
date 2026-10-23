//! Capability checking and approval storage for plugins.
//!
//! Compares a plugin manifest's declared capabilities with records in `trusted.json`.
//! If a plugin declares permissions that have not yet been approved, it transitions
//! to `PendingApproval` until the user decides to approve or deny them.

use std::path::Path;

use vy_capabilities::{Capability, CapabilityError, CapabilitySet, Roots, TrustedStore};

use crate::plugin::manifest::PluginManifest;

/// Trust status of a plugin based on its declared capabilities and `trusted.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginTrustStatus {
    /// All declared capabilities are already granted, or the plugin declares no permissions.
    Trusted,
    /// Plugin requires user approval for one or more capabilities.
    Pending { missing: Vec<Capability> },
}

/// Evaluates whether a plugin is trusted according to `trusted.json`.
pub fn evaluate_plugin_trust(
    manifest: &PluginManifest,
    store: &TrustedStore,
    roots: &Roots,
) -> Result<PluginTrustStatus, CapabilityError> {
    if manifest.permissions.is_empty() {
        return Ok(PluginTrustStatus::Trusted);
    }

    let declared = CapabilitySet::from_declarations(&manifest.permissions)?;
    let missing = store.pending(&manifest.id, &declared, roots);

    if missing.is_empty() {
        Ok(PluginTrustStatus::Trusted)
    } else {
        Ok(PluginTrustStatus::Pending { missing })
    }
}

/// Approves a plugin's declared capabilities and persists the record to `trusted.json`.
pub fn approve_plugin(
    manifest: &PluginManifest,
    store: &mut TrustedStore,
    trusted_file: &Path,
) -> std::io::Result<()> {
    let declared = CapabilitySet::from_declarations(&manifest.permissions).unwrap_or_default();
    store.approve(&manifest.id, &manifest.version, &declared);
    store.save(trusted_file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_without_permissions_is_automatically_trusted() {
        let manifest = PluginManifest {
            id: "test.plugin".into(),
            name: "Test".into(),
            version: "0.1.0".into(),
            description: None,
            engines: None,
            main: None,
            backend: None,
            permissions: vec![],
            activation_events: vec![],
            contributes: Default::default(),
        };

        let store = TrustedStore::default();
        let roots = Roots::default();

        let status = evaluate_plugin_trust(&manifest, &store, &roots).unwrap();
        assert_eq!(status, PluginTrustStatus::Trusted);
    }

    #[test]
    fn unapproved_permissions_are_pending_until_approved() {
        let manifest = PluginManifest {
            id: "test.plugin".into(),
            name: "Test".into(),
            version: "0.1.0".into(),
            description: None,
            engines: None,
            main: None,
            backend: None,
            permissions: vec!["fs:read:${workspace}/**".into()],
            activation_events: vec![],
            contributes: Default::default(),
        };

        let mut store = TrustedStore::default();
        let roots = Roots::new(Some("/home/user/project".into()), None);

        // Before approval: Pending
        let status = evaluate_plugin_trust(&manifest, &store, &roots).unwrap();
        match status {
            PluginTrustStatus::Pending { missing } => {
                assert_eq!(missing.len(), 1);
                assert_eq!(missing[0].kind(), "fs:read");
            }
            _ => panic!("expected pending status"),
        }

        // Approve and verify: Trusted
        let temp_dir = std::env::temp_dir().join(format!("vy_trust_test_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let trusted_path = temp_dir.join("trusted.json");

        approve_plugin(&manifest, &mut store, &trusted_path).unwrap();

        let status = evaluate_plugin_trust(&manifest, &store, &roots).unwrap();
        assert_eq!(status, PluginTrustStatus::Trusted);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
