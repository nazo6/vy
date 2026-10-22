//! Capability declarations and the trust store.
//!
//! A manifest lists capabilities as `kind:target` strings. The target is a glob
//! expanded with `${workspace}` and `${home}`. The user approves them once per
//! plugin; `trusted.json` records the approved set so a version bump only asks
//! again for the capabilities that were added.
//!
//! Phase 2 only evaluates `init.ts`, which is always trusted. The manifest and
//! approval UI land in Phase 3, but the parsing and matching live here so the
//! checks never depend on JavaScript.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Kinds known to the matcher. Longer kinds must be tried first.
const KINDS: [&str; 11] = [
    "fs:local:read",
    "fs:local:write",
    "fs:read",
    "fs:write",
    "process:exec",
    "process:local",
    "clipboard:read",
    "clipboard:write",
    "workspace:state",
    "secrets",
    "openExternal",
];

/// Roots used to expand `${workspace}` and `${home}` in targets.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Roots {
    pub workspace: Option<PathBuf>,
    pub home: Option<PathBuf>,
}

impl Roots {
    pub fn new(workspace: Option<PathBuf>, home: Option<PathBuf>) -> Self {
        Self { workspace, home }
    }
}

/// One declared capability.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Capability {
    kind: String,
    target: Option<String>,
}

impl Capability {
    /// Parses `kind` or `kind:target`. Unknown kinds are rejected so a typo in a
    /// manifest cannot silently grant nothing.
    pub fn parse(value: &str) -> Result<Self, CapabilityError> {
        let value = value.trim();
        if value.is_empty() {
            return Err(CapabilityError::Empty);
        }

        // `net` and `net:https://...` need the URL to stay together, so they are
        // handled with the known two-segment kinds below.
        for kind in KINDS {
            if let Some(target) = rest_after_kind(value, kind) {
                return Ok(Self {
                    kind: kind.to_string(),
                    target,
                });
            }
        }
        if let Some(target) = rest_after_kind(value, "net") {
            return Ok(Self {
                kind: "net".to_string(),
                target,
            });
        }

        Err(CapabilityError::UnknownKind(value.to_string()))
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn target(&self) -> Option<&str> {
        self.target.as_deref()
    }

    /// Expands the target with the roots. `None` stays `None` (any target).
    pub fn expanded_target(&self, roots: &Roots) -> Option<String> {
        self.target.as_ref().map(|target| expand(target, roots))
    }

    /// Whether this declaration covers `requested`. A declaration without a
    /// target covers every target of that kind.
    pub fn covers(&self, requested: &Capability, roots: &Roots) -> bool {
        if self.kind != requested.kind {
            return false;
        }
        let Some(pattern) = self.expanded_target(roots) else {
            return true;
        };
        let value = requested.expanded_target(roots).unwrap_or_default();
        glob_match(&pattern, &value)
    }
}

impl std::fmt::Display for Capability {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.target {
            Some(target) => write!(formatter, "{}:{target}", self.kind),
            None => formatter.write_str(&self.kind),
        }
    }
}

/// Splits `value` into a target when it starts with `kind` followed by `:` or
/// equals `kind`. `fs:read:${workspace}` matches `fs:read`, not `fs:readonly`.
fn rest_after_kind(value: &str, kind: &str) -> Option<Option<String>> {
    let rest = value.strip_prefix(kind)?;
    if rest.is_empty() {
        return Some(None);
    }
    let target = rest.strip_prefix(':')?;
    if target.is_empty() {
        return None;
    }
    Some(Some(target.to_string()))
}

/// Parses every capability in a manifest, collecting the first error.
pub fn parse_all(values: &[String]) -> Result<Vec<Capability>, CapabilityError> {
    values
        .iter()
        .map(|value| Capability::parse(value))
        .collect()
}

/// The capabilities a plugin needs. Used to compute what still needs approval.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapabilitySet {
    entries: BTreeSet<Capability>,
}

impl CapabilitySet {
    pub fn from_declarations(values: &[String]) -> Result<Self, CapabilityError> {
        Ok(Self {
            entries: parse_all(values)?.into_iter().collect(),
        })
    }

    pub fn insert(&mut self, capability: Capability) {
        self.entries.insert(capability);
    }

    pub fn iter(&self) -> impl Iterator<Item = &Capability> {
        self.entries.iter()
    }

    /// Whether a declared capability allows `requested`.
    pub fn has(&self, requested: &Capability, roots: &Roots) -> bool {
        self.entries
            .iter()
            .any(|declared| declared.covers(requested, roots))
    }

    /// Declarations that are not covered by `granted`.
    pub fn missing(&self, granted: &CapabilitySet, roots: &Roots) -> Vec<Capability> {
        self.entries
            .iter()
            .filter(|requested| !granted.has(requested, roots))
            .cloned()
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// One entry in `trusted.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustedPlugin {
    pub version: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

/// Approval records keyed by plugin id.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustedStore {
    #[serde(default)]
    pub plugins: BTreeMap<String, TrustedPlugin>,
}

impl TrustedStore {
    /// Reads the store. A missing file is an empty store; a broken file produces a
    /// warning and an empty store, so a bad file never blocks startup.
    pub fn load(path: &Path) -> (Self, Option<String>) {
        let Ok(text) = std::fs::read_to_string(path) else {
            return (Self::default(), None);
        };
        match serde_json::from_str(&text) {
            Ok(store) => (store, None),
            Err(error) => (
                Self::default(),
                Some(format!("could not read trusted.json: {error}")),
            ),
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }

    pub fn granted(&self, plugin_id: &str) -> CapabilitySet {
        let mut set = CapabilitySet::default();
        let Some(plugin) = self.plugins.get(plugin_id) else {
            return set;
        };
        for capability in &plugin.capabilities {
            if let Ok(capability) = Capability::parse(capability) {
                set.insert(capability);
            }
        }
        set
    }

    /// Capabilities that still need approval. A new version only asks for what it
    /// added; capabilities that disappeared are forgotten without a prompt.
    pub fn pending(
        &self,
        plugin_id: &str,
        declared: &CapabilitySet,
        roots: &Roots,
    ) -> Vec<Capability> {
        declared.missing(&self.granted(plugin_id), roots)
    }

    /// Records approval of `declared` for `version`.
    pub fn approve(&mut self, plugin_id: &str, version: &str, declared: &CapabilitySet) {
        let mut capabilities: Vec<String> = declared
            .iter()
            .map(|capability| capability.to_string())
            .collect();
        capabilities.sort();
        self.plugins.insert(
            plugin_id.to_string(),
            TrustedPlugin {
                version: version.to_string(),
                capabilities,
            },
        );
    }
}

/// Expands `${workspace}` and `${home}`. Unknown variables are left as they are.
pub fn expand(target: &str, roots: &Roots) -> String {
    let home = roots
        .home
        .clone()
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_default();

    target
        .replace(
            "${workspace}",
            &roots
                .workspace
                .clone()
                .unwrap_or_default()
                .display()
                .to_string(),
        )
        .replace("${home}", &home.display().to_string())
}

/// Matches a glob where `**` crosses path separators, `*` and `?` do not.
pub fn glob_match(pattern: &str, value: &str) -> bool {
    // `a/**` also matches `a`, so a directory capability covers the directory
    // itself and not only its children.
    if let Some(base) = pattern.strip_suffix("/**")
        && base == value
    {
        return true;
    }
    let pattern: Vec<char> = pattern.chars().collect();
    let value: Vec<char> = value.chars().collect();
    matches_from(&pattern, 0, &value, 0)
}

fn matches_from(pattern: &[char], mut p: usize, value: &[char], mut v: usize) -> bool {
    while p < pattern.len() {
        match pattern[p] {
            '*' => {
                let doubled = pattern.get(p + 1) == Some(&'*');
                let mut next = p + if doubled { 2 } else { 1 };
                // Collapse `***` and following separators: `**/` also matches
                // zero directories.
                while pattern.get(next) == Some(&'*') {
                    next += 1;
                }
                let mut separator = false;
                if doubled && pattern.get(next) == Some(&'/') {
                    next += 1;
                    separator = true;
                }
                if next == pattern.len() {
                    return doubled || !value[v..].contains(&'/');
                }
                let mut index = v;
                loop {
                    if matches_from(pattern, next, value, index) {
                        return true;
                    }
                    if index >= value.len() {
                        return false;
                    }
                    if !doubled && value[index] == '/' {
                        return false;
                    }
                    if separator && value[index] == '/' {
                        // Try the first segment without the separator, then
                        // skip ahead.
                        if matches_from(pattern, next, value, index + 1) {
                            return true;
                        }
                    }
                    index += 1;
                }
            }
            '?' => {
                if v >= value.len() || value[v] == '/' {
                    return false;
                }
            }
            character => {
                if v >= value.len() || value[v] != character {
                    return false;
                }
            }
        }
        p += 1;
        v += 1;
    }

    v == value.len()
}

/// Why a capability string could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityError {
    Empty,
    UnknownKind(String),
}

impl std::fmt::Display for CapabilityError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => formatter.write_str("a capability must not be empty"),
            Self::UnknownKind(value) => write!(formatter, "unknown capability: {value}"),
        }
    }
}

impl std::error::Error for CapabilityError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots() -> Roots {
        Roots::new(
            Some(PathBuf::from("/work/project")),
            Some(PathBuf::from("/home/user")),
        )
    }

    #[test]
    fn parses_one_and_two_segment_kinds() {
        assert_eq!(
            Capability::parse("fs:read:${workspace}/**").expect("parse"),
            Capability {
                kind: "fs:read".into(),
                target: Some("${workspace}/**".into())
            }
        );
        assert_eq!(
            Capability::parse("fs:local:write:${home}/**").expect("parse"),
            Capability {
                kind: "fs:local:write".into(),
                target: Some("${home}/**".into())
            }
        );
        assert_eq!(
            Capability::parse("secrets").expect("parse"),
            Capability {
                kind: "secrets".into(),
                target: None
            }
        );
        assert_eq!(
            Capability::parse("net:https://api.example.com").expect("parse"),
            Capability {
                kind: "net".into(),
                target: Some("https://api.example.com".into())
            }
        );
        assert_eq!(
            Capability::parse("fs:readonly"),
            Err(CapabilityError::UnknownKind("fs:readonly".into()))
        );
    }

    #[test]
    fn expands_workspace_and_home() {
        assert_eq!(
            expand("${workspace}/src/**", &roots()),
            "/work/project/src/**"
        );
        assert_eq!(expand("${home}/.config", &roots()), "/home/user/.config");
        assert_eq!(expand("${unknown}/x", &roots()), "${unknown}/x");
    }

    #[test]
    fn matches_targets_with_globs() {
        let roots = roots();
        let declared = Capability::parse("fs:read:${workspace}/**").expect("parse");
        let inside = Capability::parse("fs:read:${workspace}/src/lib.rs").expect("parse");
        let outside = Capability::parse("fs:read:/etc/passwd").expect("parse");

        assert!(declared.covers(&inside, &roots));
        assert!(!declared.covers(&outside, &roots));
        // A declaration without a target covers every target of its kind.
        assert!(
            Capability::parse("fs:read")
                .expect("parse")
                .covers(&outside, &roots)
        );
    }

    #[test]
    fn star_does_not_cross_directories_but_double_star_does() {
        assert!(glob_match("/a/*/c", "/a/b/c"));
        assert!(!glob_match("/a/*", "/a/b/c"));
        assert!(glob_match("/a/**", "/a/b/c"));
        assert!(glob_match("/a/**/c", "/a/b/c"));
        assert!(glob_match("/a/**/c", "/a/c"));
        assert!(glob_match("/a/**", "/a"));
        assert!(!glob_match("/a/*.rs", "/a/b/c.rs"));
        assert!(glob_match("/a/?", "/a/b"));
    }

    #[test]
    fn asks_only_for_capabilities_added_by_a_new_version() {
        let roots = roots();
        let first =
            CapabilitySet::from_declarations(&["fs:read:${workspace}/**".into()]).expect("set");
        let second = CapabilitySet::from_declarations(&[
            "fs:read:${workspace}/**".into(),
            "process:exec:cargo".into(),
        ])
        .expect("set");

        let mut store = TrustedStore::default();
        assert_eq!(store.pending("example.hello", &first, &roots).len(), 1);
        store.approve("example.hello", "0.1.0", &first);

        // Same declarations: nothing to approve.
        assert!(store.pending("example.hello", &first, &roots).is_empty());
        // A new version that adds one capability asks for that one only.
        let pending = store.pending("example.hello", &second, &roots);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].to_string(), "process:exec:cargo");
    }

    #[test]
    fn writes_and_reads_the_store() {
        let directory = std::env::temp_dir().join("vy-trusted-store-test");
        let path = directory.join("trusted.json");
        let _ = std::fs::remove_dir_all(&directory);

        let mut store = TrustedStore::default();
        store.approve("example.hello", "1.0.0", &CapabilitySet::default());
        store.save(&path).expect("save");

        let (loaded, warning) = TrustedStore::load(&path);
        assert!(warning.is_none());
        assert_eq!(loaded, store);

        let _ = std::fs::remove_dir_all(&directory);
    }
}
