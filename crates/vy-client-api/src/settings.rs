//! Settings keys exposed by the client.
//!
//! The declarations are the source of truth for the built-in defaults and for
//! the generated `SettingsKey.ts`. They live here rather than in `vy-app` so
//! the codegen can run without building the GPUI client.

use serde_json::{Map, Value};

/// Where a setting is stored when it is changed at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingScope {
    /// Session-wide memory layer. `:set` and `:setglobal` both write it.
    Global,
    /// Current window only. `:set` writes it; `:setglobal` writes the memory layer.
    Window,
}

impl SettingScope {
    pub fn label(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Window => "window",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "global" => Some(Self::Global),
            "window" => Some(Self::Window),
            _ => None,
        }
    }
}

/// The value type of a setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingKind {
    Bool,
    Number,
    Text,
    /// One of a fixed list of strings.
    Choice(&'static [&'static str]),
}

impl SettingKind {
    /// The TypeScript type of the value.
    pub fn typescript_type(self) -> String {
        match self {
            Self::Bool => "boolean".to_string(),
            Self::Number => "number".to_string(),
            Self::Text => "string".to_string(),
            Self::Choice(choices) => choices
                .iter()
                .map(|choice| format!("\"{choice}\""))
                .collect::<Vec<_>>()
                .join(" | "),
        }
    }

    /// Whether `value` matches this kind.
    pub fn accepts(self, value: &Value) -> bool {
        match self {
            Self::Bool => value.is_boolean(),
            Self::Number => value.as_f64().is_some_and(f64::is_finite),
            Self::Text => value.is_string(),
            Self::Choice(choices) => value.as_str().is_some_and(|text| choices.contains(&text)),
        }
    }

    /// Parses the text typed after `:set` or in the settings prompt.
    pub fn parse(self, text: &str) -> Option<Value> {
        let text = text.trim();
        match self {
            Self::Bool => match text {
                "true" | "on" | "yes" | "1" => Some(Value::Bool(true)),
                "false" | "off" | "no" | "0" => Some(Value::Bool(false)),
                _ => None,
            },
            Self::Number => text
                .parse::<f64>()
                .ok()
                .filter(|number| number.is_finite())
                .map(Value::from),
            Self::Text => Some(Value::from(text)),
            Self::Choice(choices) => choices
                .iter()
                .find(|choice| **choice == text)
                .map(|choice| Value::from(*choice)),
        }
    }
}

/// One setting the client defines.
#[derive(Clone, Copy, Debug)]
pub struct SettingDecl {
    /// Dotted setting key, for example `ui.fontSize`.
    pub key: &'static str,
    /// One-line description emitted as JSDoc.
    pub doc: &'static str,
    /// Built-in default value.
    pub default: fn() -> Value,
    /// Where runtime changes are stored.
    pub scope: SettingScope,
    /// The value type accepted by the setting.
    pub kind: SettingKind,
}

impl SettingDecl {
    /// The built-in default for this setting.
    pub fn default_value(self) -> Value {
        (self.default)()
    }

    /// The last segment of the key. Used as the name after `:set`.
    pub fn name(self) -> &'static str {
        self.key.rsplit('.').next().unwrap_or(self.key)
    }

    /// TypeScript type of the value.
    pub fn typescript_type(self) -> String {
        self.kind.typescript_type()
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

fn theme() -> Value {
    Value::from("dark")
}

fn empty_color() -> Value {
    Value::from("")
}

/// Every setting the client defines, with its default and description.
pub const SETTINGS: &[SettingDecl] = &[
    SettingDecl {
        key: "ui.fontFamily",
        doc: "Font family used for editor text.",
        default: font_family,
        scope: SettingScope::Global,
        kind: SettingKind::Text,
    },
    SettingDecl {
        key: "ui.fontSize",
        doc: "Font size in points.",
        default: font_size,
        scope: SettingScope::Global,
        kind: SettingKind::Number,
    },
    SettingDecl {
        key: "ui.lineHeight",
        doc: "Line height in points, or a multiple of the font size when at most 4.",
        default: line_height,
        scope: SettingScope::Global,
        kind: SettingKind::Number,
    },
    SettingDecl {
        key: "ui.lineNumbers",
        doc: "Line number style: absolute, relative, or off.",
        default: line_numbers,
        scope: SettingScope::Global,
        kind: SettingKind::Choice(&["absolute", "relative", "off"]),
    },
    SettingDecl {
        key: "ui.theme",
        doc: "Color theme: dark or light.",
        default: theme,
        scope: SettingScope::Global,
        kind: SettingKind::Choice(&["dark", "light"]),
    },
    SettingDecl {
        key: "ui.activePaneBorder",
        doc: "Border color of the active pane as #rrggbb. Empty follows the theme.",
        default: empty_color,
        scope: SettingScope::Global,
        kind: SettingKind::Text,
    },
    SettingDecl {
        key: "ui.inactivePaneBorder",
        doc: "Border color of inactive panes as #rrggbb. Empty follows the theme.",
        default: empty_color,
        scope: SettingScope::Global,
        kind: SettingKind::Text,
    },
    SettingDecl {
        key: "ui.focusBorder",
        doc: "Focus ring color as #rrggbb. Empty follows the theme.",
        default: empty_color,
        scope: SettingScope::Global,
        kind: SettingKind::Text,
    },
    SettingDecl {
        key: "ui.selectionBackground",
        doc: "Background of the focused row as #rrggbb. Empty follows the theme.",
        default: empty_color,
        scope: SettingScope::Global,
        kind: SettingKind::Text,
    },
    SettingDecl {
        key: "ui.selectionForeground",
        doc: "Text color of the focused row as #rrggbb. Empty follows the theme.",
        default: empty_color,
        scope: SettingScope::Global,
        kind: SettingKind::Text,
    },
    SettingDecl {
        key: "editor.transactionIdleMs",
        doc: "Idle time in milliseconds before an undo transaction is closed.",
        default: transaction_idle_ms,
        scope: SettingScope::Global,
        kind: SettingKind::Number,
    },
    SettingDecl {
        key: "editor.sequenceTimeoutMs",
        doc: "Time in milliseconds allowed between keys of one sequence.",
        default: sequence_timeout_ms,
        scope: SettingScope::Global,
        kind: SettingKind::Number,
    },
    SettingDecl {
        key: "editor.wrap",
        doc: "Whether to wrap lines that exceed the viewport width.",
        default: wrap,
        scope: SettingScope::Window,
        kind: SettingKind::Bool,
    },
];

/// Finds a setting by its exact key.
pub fn find(key: &str) -> Option<&'static SettingDecl> {
    SETTINGS.iter().find(|setting| setting.key == key)
}

/// Resolves a setting name to an exact key.
///
/// The full dotted key or the last segment is accepted. A last segment that
/// matches more than one setting is ambiguous and returns an error.
pub fn resolve(name: &str) -> Result<&'static SettingDecl, String> {
    if let Some(setting) = find(name) {
        return Ok(setting);
    }
    let matches: Vec<&SettingDecl> = SETTINGS
        .iter()
        .filter(|setting| setting.name() == name)
        .collect();
    match matches.as_slice() {
        [] => Err(format!("unknown setting: {name}")),
        [setting] => Ok(setting),
        _ => Err(format!(
            "ambiguous setting `{name}`: {}",
            matches
                .iter()
                .map(|setting| setting.key)
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// What one `:set` / `:setglobal` argument asks for.
#[derive(Debug, Clone)]
pub enum SettingEdit {
    /// `:set` with no argument.
    OpenSettings,
    /// `:set name?`: report the effective value and its source.
    Inspect(&'static SettingDecl),
    /// `:set name`, `:set noname`, or `:set name=value`.
    Apply(&'static SettingDecl, Value),
}

impl PartialEq for SettingEdit {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::OpenSettings, Self::OpenSettings) => true,
            (Self::Inspect(left), Self::Inspect(right)) => left.key == right.key,
            (Self::Apply(left, left_value), Self::Apply(right, right_value)) => {
                left.key == right.key && left_value == right_value
            }
            _ => false,
        }
    }
}

impl Eq for SettingEdit {}

/// Parses the argument of `:set` / `:setglobal`.
///
/// The caller decides where to store the value; this only resolves the setting
/// and converts the text.
pub fn parse_set_option(option: &str) -> Result<SettingEdit, String> {
    let option = option.trim();
    if option.is_empty() {
        return Ok(SettingEdit::OpenSettings);
    }

    if let Some(name) = option.strip_suffix('?') {
        return Ok(SettingEdit::Inspect(resolve(name.trim())?));
    }

    if let Some((name, raw)) = option.split_once('=') {
        let decl = resolve(name.trim())?;
        let value = decl
            .kind
            .parse(raw)
            .ok_or_else(|| format!("invalid value for {}: {raw}", decl.key))?;
        return Ok(SettingEdit::Apply(decl, value));
    }

    let (name, negate) = match option.strip_prefix("no") {
        Some(rest) => (rest, true),
        None => (option, false),
    };
    let decl = resolve(name)?;
    match decl.kind {
        SettingKind::Bool => Ok(SettingEdit::Apply(decl, Value::Bool(!negate))),
        _ if negate => Err(format!("{} is not a boolean setting", decl.key)),
        _ => Err(format!(
            "use `{} = <value>` to change {}",
            decl.name(),
            decl.key
        )),
    }
}

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
            assert!(
                setting.kind.accepts(&setting.default_value()),
                "{} default does not match its kind",
                setting.key
            );
            assert!(
                !setting.typescript_type().is_empty(),
                "{} has no type",
                setting.key
            );
        }
        assert_eq!(defaults().len(), SETTINGS.len());
    }

    #[test]
    fn resolves_names_and_parses_values() {
        assert_eq!(
            resolve("wrap").map(|setting| setting.key),
            Ok("editor.wrap")
        );
        assert_eq!(
            resolve("ui.fontSize").map(|setting| setting.key),
            Ok("ui.fontSize")
        );
        assert!(resolve("missing").is_err());
        assert!(resolve("theme").is_ok());

        let bool_setting = find("editor.wrap").unwrap();
        assert_eq!(bool_setting.name(), "wrap");
        assert_eq!(bool_setting.kind.parse("true"), Some(Value::Bool(true)));
        assert_eq!(bool_setting.kind.parse("off"), Some(Value::Bool(false)));
        assert_eq!(bool_setting.kind.parse("maybe"), None);

        let choice = find("ui.lineNumbers").unwrap().kind;
        assert_eq!(choice.parse("relative"), Some(Value::from("relative")));
        assert_eq!(choice.parse("nope"), None);
    }

    #[test]
    fn parses_set_options() {
        assert_eq!(parse_set_option(""), Ok(SettingEdit::OpenSettings));

        let wrap = find("editor.wrap").unwrap();
        assert_eq!(
            parse_set_option("wrap"),
            Ok(SettingEdit::Apply(wrap, Value::Bool(true)))
        );
        assert_eq!(
            parse_set_option("nowrap"),
            Ok(SettingEdit::Apply(wrap, Value::Bool(false)))
        );
        assert_eq!(
            parse_set_option("wrap=off"),
            Ok(SettingEdit::Apply(wrap, Value::Bool(false)))
        );
        assert_eq!(parse_set_option("wrap?"), Ok(SettingEdit::Inspect(wrap)));

        let theme = find("ui.theme").unwrap();
        assert_eq!(
            parse_set_option("theme=light"),
            Ok(SettingEdit::Apply(theme, Value::from("light")))
        );
        assert_eq!(
            parse_set_option("ui.theme=dark"),
            Ok(SettingEdit::Apply(theme, Value::from("dark")))
        );
        assert!(parse_set_option("theme").is_err());
        assert!(parse_set_option("notheme").is_err());
        assert!(parse_set_option("theme=nope").is_err());
        assert!(parse_set_option("missing").is_err());
    }
}
