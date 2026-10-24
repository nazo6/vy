//! Settings keys exposed by the client.
//!
//! The declarations are the source of truth for the built-in defaults and for
//! the generated `SettingsKey.ts`. They live here rather than in `vy-app` so
//! the codegen can run without building the GPUI client.

use serde_json::{Map, Value};

/// One setting the client defines.
#[derive(Clone, Copy, Debug)]
pub struct SettingDecl {
    /// Dotted setting key, for example `ui.fontSize`.
    pub key: &'static str,
    /// One-line description emitted as JSDoc.
    pub doc: &'static str,
    /// Built-in default value.
    pub default: fn() -> Value,
}

impl SettingDecl {
    /// The built-in default for this setting.
    pub fn default_value(self) -> Value {
        (self.default)()
    }

    /// TypeScript type of the value.
    pub fn typescript_type(self) -> &'static str {
        match self.default_value() {
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            _ => "unknown",
        }
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

fn font_family() -> Value {
    Value::from(default_font_family())
}

fn font_size() -> Value {
    Value::from(14.0)
}

fn line_height() -> Value {
    Value::from(20.0)
}

fn line_numbers() -> Value {
    Value::from("absolute")
}

fn transaction_idle_ms() -> Value {
    Value::from(500)
}

fn sequence_timeout_ms() -> Value {
    Value::from(1000)
}

fn wrap() -> Value {
    Value::from(false)
}

/// Every setting the client defines, with its default and description.
pub const SETTINGS: &[SettingDecl] = &[
    SettingDecl {
        key: "ui.fontFamily",
        doc: "Font family used for editor text.",
        default: font_family,
    },
    SettingDecl {
        key: "ui.fontSize",
        doc: "Font size in points.",
        default: font_size,
    },
    SettingDecl {
        key: "ui.lineHeight",
        doc: "Line height in points, or a multiple of the font size when at most 4.",
        default: line_height,
    },
    SettingDecl {
        key: "ui.lineNumbers",
        doc: "Line number style: absolute, relative, or off.",
        default: line_numbers,
    },
    SettingDecl {
        key: "editor.transactionIdleMs",
        doc: "Idle time in milliseconds before an undo transaction is closed.",
        default: transaction_idle_ms,
    },
    SettingDecl {
        key: "editor.sequenceTimeoutMs",
        doc: "Time in milliseconds allowed between keys of one sequence.",
        default: sequence_timeout_ms,
    },
    SettingDecl {
        key: "editor.wrap",
        doc: "Whether to wrap lines that exceed the viewport width.",
        default: wrap,
    },
];

/// The built-in defaults for every setting.
pub fn defaults() -> Map<String, Value> {
    let mut values = Map::new();
    for setting in SETTINGS {
        values.insert(setting.key.to_string(), setting.default_value());
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_setting_has_a_default_and_doc() {
        for setting in SETTINGS {
            assert!(!setting.doc.trim().is_empty(), "{} has no doc", setting.key);
            assert_ne!(setting.typescript_type(), "unknown", "{}", setting.key);
        }
        assert_eq!(defaults().len(), SETTINGS.len());
    }
}
