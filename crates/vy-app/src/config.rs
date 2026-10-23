//! Settings resolution.
//!
//! Four layers are resolved, and later layers win: built-in defaults, the user
//! `settings.json`, the project `.vy/settings.json`, and values set from `init.ts`
//! (memory only). See `docs/plugin-system.md`.

use std::collections::BTreeSet;
use std::path::Path;

use serde::Serialize;
use serde_json::{Map, Value};

use crate::paths::Paths;

const LAYERS: usize = 4;

/// A layer of settings. Later layers take precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Default,
    User,
    Project,
    Memory,
}

impl Layer {
    fn index(self) -> usize {
        match self {
            Self::Default => 0,
            Self::User => 1,
            Self::Project => 2,
            Self::Memory => 3,
        }
    }
}

/// How line numbers are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::IntoStaticStr, strum::EnumString)]
#[strum(serialize_all = "lowercase")]
pub enum LineNumbers {
    Absolute,
    Relative,
    Off,
}

impl LineNumbers {
    pub fn parse(value: &str) -> Option<Self> {
        value.parse().ok()
    }

    /// Returns the label for `line`, or [`None`] when the gutter is hidden.
    pub fn label(self, line: usize, cursor_line: usize) -> Option<String> {
        match self {
            Self::Absolute => Some((line + 1).to_string()),
            Self::Relative if line == cursor_line => Some((line + 1).to_string()),
            Self::Relative => Some(line.abs_diff(cursor_line).to_string()),
            Self::Off => None,
        }
    }
}

/// Every layer's value for one key, plus the value that is in effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inspect {
    pub default: Option<Value>,
    pub user: Option<Value>,
    pub project: Option<Value>,
    pub memory: Option<Value>,
    pub effective: Option<Value>,
}

/// Resolved settings.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Config {
    layers: [Map<String, Value>; LAYERS],
}

impl Config {
    /// Creates a configuration that only has the built-in defaults.
    pub fn with_defaults() -> Self {
        let mut config = Self::default();
        config.layers[Layer::Default.index()] = defaults();
        config
    }

    /// Reads the user and project settings. Missing files are not an error; broken
    /// files produce a warning and leave that layer empty.
    pub fn load(paths: &Paths, workspace: Option<&Path>) -> (Self, Vec<String>) {
        let mut config = Self::with_defaults();
        let warnings = config.reload(paths, workspace);
        (config, warnings)
    }

    /// Re-reads the user and project layers, keeping the defaults and memory layer.
    pub fn reload(&mut self, paths: &Paths, workspace: Option<&Path>) -> Vec<String> {
        let mut warnings = Vec::new();
        self.layers[Layer::User.index()] =
            read_settings(&paths.settings_json(), &mut warnings).unwrap_or_default();
        self.layers[Layer::Project.index()] = match workspace {
            Some(workspace) => read_settings(&paths.workspace_settings(workspace), &mut warnings)
                .unwrap_or_default(),
            None => Map::new(),
        };
        warnings
    }

    /// Returns the effective value of a key.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.layers.iter().rev().find_map(|layer| layer.get(key))
    }

    /// Returns every layer's value for a key.
    pub fn inspect(&self, key: &str) -> Inspect {
        Inspect {
            default: self.layers[Layer::Default.index()].get(key).cloned(),
            user: self.layers[Layer::User.index()].get(key).cloned(),
            project: self.layers[Layer::Project.index()].get(key).cloned(),
            memory: self.layers[Layer::Memory.index()].get(key).cloned(),
            effective: self.get(key).cloned(),
        }
    }

    /// Sets a value in the memory layer. Returns whether the effective value changed.
    pub fn set(&mut self, key: impl Into<String>, value: Value) -> bool {
        let key = key.into();
        let before = self.get(&key).cloned();
        self.layers[Layer::Memory.index()].insert(key.clone(), value);
        before != self.get(&key).cloned()
    }

    /// Drops every value set from `init.ts`. Returns whether anything was removed.
    pub fn clear_memory(&mut self) -> bool {
        let memory = &mut self.layers[Layer::Memory.index()];
        let had_values = !memory.is_empty();
        memory.clear();
        had_values
    }

    /// Sets several memory values at once, as `config.set` does. Returns the keys
    /// whose effective value changed.
    pub fn apply(&mut self, values: &Map<String, Value>) -> Vec<String> {
        let mut changed = Vec::new();
        for (key, value) in values {
            if self.set(key.clone(), value.clone()) {
                changed.push(key.clone());
            }
        }
        changed.sort();
        changed
    }

    /// Replaces a whole layer. Used by reloads and tests.
    pub fn replace_layer(&mut self, layer: Layer, values: Map<String, Value>) {
        self.layers[layer.index()] = values;
    }

    /// Keys whose effective value differs from `previous`.
    pub fn changed_keys(&self, previous: &Config) -> Vec<String> {
        let mut keys = BTreeSet::new();
        for layer in &self.layers {
            keys.extend(layer.keys());
        }
        for layer in &previous.layers {
            keys.extend(layer.keys());
        }

        keys.into_iter()
            .filter(|key| self.get(key) != previous.get(key))
            .cloned()
            .collect()
    }

    pub fn font_family(&self) -> String {
        self.string("ui.fontFamily", default_font_family())
    }

    pub fn font_size(&self) -> f32 {
        self.number("ui.fontSize", 14.0) as f32
    }

    pub fn line_height(&self) -> f32 {
        let font_size = self.font_size();
        let min_line_height = (font_size * 1.15).round().max(font_size);
        let default_line_height = (font_size * (20.0 / 14.0)).round().max(min_line_height);

        let explicit = self.layers[Layer::User.index()..]
            .iter()
            .rev()
            .find_map(|layer| layer.get("ui.lineHeight"))
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && *value > 0.0);

        match explicit {
            Some(value) => {
                let height = if value <= 4.0 {
                    (value as f32 * font_size).round()
                } else {
                    value as f32
                };
                height.max(min_line_height)
            }
            None => default_line_height,
        }
    }

    pub fn line_numbers(&self) -> LineNumbers {
        self.get("ui.lineNumbers")
            .and_then(Value::as_str)
            .and_then(LineNumbers::parse)
            .unwrap_or(LineNumbers::Absolute)
    }

    pub fn transaction_idle_ms(&self) -> u64 {
        self.unsigned("editor.transactionIdleMs", 500)
    }

    pub fn sequence_timeout_ms(&self) -> u64 {
        self.unsigned("editor.sequenceTimeoutMs", 1000)
    }

    fn string(&self, key: &str, fallback: &str) -> String {
        self.get(key)
            .and_then(Value::as_str)
            .unwrap_or(fallback)
            .to_string()
    }

    fn number(&self, key: &str, fallback: f64) -> f64 {
        self.get(key)
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(fallback)
    }

    fn unsigned(&self, key: &str, fallback: u64) -> u64 {
        self.get(key)
            .and_then(Value::as_u64)
            .filter(|value| *value > 0)
            .unwrap_or(fallback)
    }
}

/// Default monospace font family for the current platform.
pub fn default_font_family() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Menlo"
    }
    #[cfg(target_os = "windows")]
    {
        "Consolas"
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        "monospace"
    }
}

fn defaults() -> Map<String, Value> {
    let mut values = Map::new();
    values.insert("ui.fontFamily".into(), Value::from(default_font_family()));
    values.insert("ui.fontSize".into(), Value::from(14.0));
    values.insert("ui.lineHeight".into(), Value::from(20.0));
    values.insert("ui.lineNumbers".into(), Value::from("absolute"));
    values.insert("editor.transactionIdleMs".into(), Value::from(500));
    values.insert("editor.sequenceTimeoutMs".into(), Value::from(1000));
    values
}

fn read_settings(path: &Path, warnings: &mut Vec<String>) -> Option<Map<String, Value>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            warnings.push(format!("could not read {}: {error}", path.display()));
            return None;
        }
    };

    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(values)) => Some(values),
        Ok(_) => {
            warnings.push(format!("{} must contain a JSON object", path.display()));
            None
        }
        Err(error) => {
            warnings.push(format!("could not parse {}: {error}", path.display()));
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("vy-config-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("temp dir");
        path
    }

    #[test]
    fn layers_take_precedence() {
        let mut config = Config::with_defaults();

        let mut user = Map::new();
        user.insert("ui.fontSize".into(), Value::from(18.0));
        config.replace_layer(Layer::User, user);

        let mut project = Map::new();
        project.insert("ui.fontSize".into(), Value::from(22.0));
        config.replace_layer(Layer::Project, project);

        assert_eq!(config.font_size(), 22.0);
        assert!(config.set("ui.fontSize", Value::from(30.0)));
        assert_eq!(config.font_size(), 30.0);

        assert!(config.clear_memory());
        assert_eq!(config.font_size(), 22.0);
        // Default line height scales proportionally with font size (~1.43x).
        assert_eq!(config.line_height(), 31.0);

        // Explicit line height in user layer overrides default scaling.
        let mut user = Map::new();
        user.insert("ui.lineHeight".into(), Value::from(36.0));
        config.replace_layer(Layer::User, user);
        assert_eq!(config.line_height(), 36.0);

        // Memory layer takes precedence over user layer.
        config.set("ui.lineHeight", Value::from(40.0));
        assert_eq!(config.line_height(), 40.0);
    }

    #[test]
    fn calculates_line_height_with_multipliers_and_clamps() {
        let mut config = Config::with_defaults();
        assert_eq!(config.font_size(), 14.0);
        assert_eq!(config.line_height(), 20.0);

        // Relative multiplier <= 4.0
        config.set("ui.lineHeight", Value::from(1.5));
        assert_eq!(config.line_height(), 21.0);

        // Clamp prevents lines from overlapping when explicit line height is too small
        config.set("ui.fontSize", Value::from(24.0));
        config.set("ui.lineHeight", Value::from(10.0));
        assert_eq!(config.line_height(), 28.0);
    }

    #[test]
    fn inspects_each_layer() {
        let mut config = Config::with_defaults();
        let mut user = Map::new();
        user.insert("ui.fontSize".into(), Value::from(18.0));
        config.replace_layer(Layer::User, user);
        config.set("ui.fontSize", Value::from(30.0));

        let inspect = config.inspect("ui.fontSize");
        assert_eq!(inspect.default, Some(Value::from(14.0)));
        assert_eq!(inspect.user, Some(Value::from(18.0)));
        assert_eq!(inspect.project, None);
        assert_eq!(inspect.memory, Some(Value::from(30.0)));
        assert_eq!(inspect.effective, Some(Value::from(30.0)));
    }

    #[test]
    fn reports_changed_keys() {
        let previous = Config::with_defaults();
        let mut current = previous.clone();
        current.set("ui.fontSize", Value::from(18.0));

        assert_eq!(current.changed_keys(&previous), vec!["ui.fontSize"]);
        assert!(current.changed_keys(&current).is_empty());
    }

    #[test]
    fn formats_line_numbers() {
        assert_eq!(LineNumbers::Absolute.label(4, 9), Some("5".into()));
        assert_eq!(LineNumbers::Relative.label(9, 9), Some("10".into()));
        assert_eq!(LineNumbers::Relative.label(4, 9), Some("5".into()));
        assert_eq!(LineNumbers::Relative.label(12, 9), Some("3".into()));
        assert_eq!(LineNumbers::Off.label(4, 9), None);
    }

    #[test]
    fn loads_user_and_project_settings() {
        let root = temp_dir("loads");
        let paths = Paths::under(&root);
        std::fs::create_dir_all(&paths.config).expect("config dir");
        std::fs::write(paths.settings_json(), r#"{"ui.fontSize": 18}"#).expect("write");

        let workspace = root.join("workspace");
        std::fs::create_dir_all(workspace.join(".vy")).expect("project dir");
        std::fs::write(
            paths.workspace_settings(&workspace),
            r#"{"ui.fontSize": 22}"#,
        )
        .expect("write");

        let (config, warnings) = Config::load(&paths, Some(&workspace));
        assert!(warnings.is_empty());
        assert_eq!(config.font_size(), 22.0);
        assert_eq!(
            config
                .inspect("ui.fontSize")
                .user
                .and_then(|value| value.as_f64()),
            Some(18.0)
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn warns_and_keeps_defaults_for_a_broken_file() {
        let root = temp_dir("broken");
        let paths = Paths::under(&root);
        std::fs::create_dir_all(&paths.config).expect("config dir");
        std::fs::write(paths.settings_json(), "[1, 2, 3]").expect("write");

        let (config, warnings) = Config::load(&paths, None);
        assert_eq!(warnings.len(), 1);
        assert_eq!(config.font_size(), 14.0);

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn resolves_default_font_family() {
        let mut config = Config::with_defaults();
        assert_eq!(config.font_family(), default_font_family());

        config.set("ui.fontFamily", Value::from("JetBrains Mono"));
        assert_eq!(config.font_family(), "JetBrains Mono");
    }

    fn export_dir() -> std::path::PathBuf {
        std::env::var("TS_RS_EXPORT_DIR").map_or_else(
            |_| {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../packages/vy-api-types")
            },
            std::path::PathBuf::from,
        )
    }

    /// Writes `SettingsKey.ts` for `packages/vy-api-types`.
    #[test]
    fn export_bindings_settings_keys() {
        let values = defaults();

        let mut output = String::new();
        output.push_str("// This file was generated by vy-app. Do not edit it manually.\n");
        output.push_str("// Regenerate with `cargo test --workspace export_bindings`.\n\n");
        output.push_str("/// Value type of every setting the client defines.\n");
        output.push_str("export interface SettingsMap {\n");
        for (key, value) in &values {
            let ty = match value {
                Value::Bool(_) => "boolean",
                Value::Number(_) => "number",
                Value::String(_) => "string",
                _ => "unknown",
            };
            output.push_str(&format!("  \"{key}\": {ty};\n"));
        }
        output.push_str("}\n\n");
        output.push_str("export type SettingsKey = keyof SettingsMap;\n");

        let directory = export_dir();
        std::fs::create_dir_all(&directory).expect("create the export directory");
        std::fs::write(directory.join("SettingsKey.ts"), output).expect("write SettingsKey.ts");
    }
}
