//! Plugin manifest (`plugin.json`) schema and parsing.
//!
//! Each plugin declares its identity, entry points, capabilities, activation
//! events, and contributed commands in `plugin.json` as specified in
//! `docs/specs/plugin-system.md` §3.1.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use vy_capabilities::Capability;

/// Errors that can occur when parsing or validating a plugin manifest.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PluginManifestError {
    #[error("manifest json error: {0}")]
    Json(String),
    #[error("plugin id cannot be empty")]
    EmptyId,
    #[error("invalid plugin id '{0}': only alphanumeric characters, '-', '_', and '.' are allowed")]
    InvalidId(String),
    #[error("plugin name cannot be empty")]
    EmptyName,
    #[error("plugin version cannot be empty")]
    EmptyVersion,
    #[error("invalid capability '{value}': {reason}")]
    InvalidCapability { value: String, reason: String },
    #[error("invalid engines.vy '{value}': {reason}")]
    InvalidEngine { value: String, reason: String },
    #[error(
        "invalid activation event '{0}': format must be 'onStartup', 'onCommand:<id>', 'onView:<id>', 'onLanguage:<id>', or 'onWorkspaceContains:<glob>'"
    )]
    InvalidActivationEvent(String),
}

/// Target engine requirements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginEngines {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vy: Option<String>,
}

/// A command contributed by the plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContributedCommand {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Short Ex names reserved by the plugin. Only trusted plugins register them.
    #[serde(default, rename = "exNames", skip_serializing_if = "Vec::is_empty")]
    pub ex_names: Vec<String>,
}

/// Contributed features such as commands and configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginContributes {
    #[serde(default)]
    pub commands: Vec<ContributedCommand>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configuration: Option<serde_json::Value>,
}

/// A parsed `plugin.json` manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engines: Option<PluginEngines>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default, rename = "activationEvents")]
    pub activation_events: Vec<String>,
    #[serde(default)]
    pub contributes: PluginContributes,
}

impl PluginManifest {
    /// Parses a manifest from a JSON string.
    pub fn parse(json_str: &str) -> Result<Self, PluginManifestError> {
        let manifest: Self =
            serde_json::from_str(json_str).map_err(|e| PluginManifestError::Json(e.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validates fields of the manifest.
    pub fn validate(&self) -> Result<(), PluginManifestError> {
        if self.id.trim().is_empty() {
            return Err(PluginManifestError::EmptyId);
        }
        if !self
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
        {
            return Err(PluginManifestError::InvalidId(self.id.clone()));
        }
        if self.name.trim().is_empty() {
            return Err(PluginManifestError::EmptyName);
        }
        if self.version.trim().is_empty() {
            return Err(PluginManifestError::EmptyVersion);
        }

        for perm in &self.permissions {
            if let Err(err) = Capability::parse(perm) {
                return Err(PluginManifestError::InvalidCapability {
                    value: perm.clone(),
                    reason: err.to_string(),
                });
            }
        }

        if let Some(requirement) = self
            .engines
            .as_ref()
            .and_then(|engines| engines.vy.as_deref())
            && let Err(error) = semver::VersionReq::parse(requirement)
        {
            return Err(PluginManifestError::InvalidEngine {
                value: requirement.to_string(),
                reason: error.to_string(),
            });
        }

        for event in &self.activation_events {
            validate_activation_event(event)?;
        }

        Ok(())
    }
}

fn validate_activation_event(event: &str) -> Result<(), PluginManifestError> {
    if event == "onStartup" {
        return Ok(());
    }
    let valid_prefixes = [
        "onCommand:",
        "onView:",
        "onLanguage:",
        "onWebview:",
        "onWorkspaceContains:",
    ];
    for prefix in valid_prefixes {
        if let Some(_rest) = event.strip_prefix(prefix).filter(|s| !s.trim().is_empty()) {
            return Ok(());
        }
    }
    Err(PluginManifestError::InvalidActivationEvent(
        event.to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_manifest() {
        let json = r#"{
            "id": "example.hello",
            "name": "Hello",
            "version": "0.1.0",
            "engines": { "vy": ">=0.1.0" },
            "main": "dist/frontend.js",
            "backend": "dist/backend.js",
            "permissions": ["fs:read:${workspace}/**"],
            "activationEvents": ["onCommand:example.hello.greet", "onStartup"],
            "contributes": {
                "commands": [{ "id": "example.hello.greet", "title": "Hello" }]
            }
        }"#;

        let manifest = PluginManifest::parse(json).unwrap();
        assert_eq!(manifest.id, "example.hello");
        assert_eq!(manifest.name, "Hello");
        assert_eq!(manifest.version, "0.1.0");
        assert_eq!(manifest.main.as_deref(), Some("dist/frontend.js"));
        assert_eq!(manifest.backend.as_deref(), Some("dist/backend.js"));
        assert_eq!(manifest.permissions, vec!["fs:read:${workspace}/**"]);
        assert_eq!(manifest.activation_events.len(), 2);
        assert_eq!(manifest.contributes.commands.len(), 1);
        assert_eq!(manifest.contributes.commands[0].id, "example.hello.greet");
    }

    #[test]
    fn rejects_invalid_id() {
        let json = r#"{
            "id": "example/hello",
            "name": "Hello",
            "version": "0.1.0"
        }"#;
        assert!(PluginManifest::parse(json).is_err());
    }

    #[test]
    fn rejects_invalid_permission() {
        let json = r#"{
            "id": "example.hello",
            "name": "Hello",
            "version": "0.1.0",
            "permissions": ["invalid_kind:something"]
        }"#;
        assert!(PluginManifest::parse(json).is_err());
    }

    #[test]
    fn rejects_invalid_activation_event() {
        let json = r#"{
            "id": "example.hello",
            "name": "Hello",
            "version": "0.1.0",
            "activationEvents": ["randomEvent"]
        }"#;
        assert!(PluginManifest::parse(json).is_err());
    }

    #[test]
    fn rejects_invalid_engine_requirement() {
        let json = r#"{
            "id": "example.hello",
            "name": "Hello",
            "version": "0.1.0",
            "engines": { "vy": "not a version" }
        }"#;
        assert!(matches!(
            PluginManifest::parse(json),
            Err(PluginManifestError::InvalidEngine { .. })
        ));
    }
}
