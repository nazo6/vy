//! Writes the TypeScript declarations from [`crate::HOST_METHODS`].
//!
//! `HostMethods.ts` lists every call; `packages/vy-api/index.d.ts` declares the
//! namespaces scripts import. Hand-written sugar (wrappers that keep JavaScript
//! state in the shim) lives in `packages/vy-api/overrides.d.ts` and is composed
//! here. See `docs/adr/0007-rust-typed-plugin-api.md`.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;

use crate::HOST_METHODS;
use crate::MethodSpec;

/// Namespaces the shim exports as constants.
const TOP_LEVEL: &[&str] = &[
    "commands",
    "keymap",
    "config",
    "editor",
    "workspace",
    "notifications",
    "log",
    "ui",
    "views",
];

/// Tokens that are types in the emitted signature rather than imported names.
const TYPE_KEYWORDS: &[&str] = &[
    "boolean", "never", "null", "number", "Record", "string", "unknown", "void",
];

/// Types scripts can import from `vy-api` even when no generated signature uses
/// them.
const EXTRA_TYPES: &[&str] = &[
    "ButtonVariant",
    "CommandId",
    "ContainerKind",
    "ErrorCode",
    "Font",
    "Gap",
    "LogLevel",
    "Mode",
    "PickerItem",
    "Position",
    "Range",
    "Selection",
    "SettingsKey",
    "SettingsMap",
    "StatuslineAlignment",
    "TextEdit",
    "ThemeColor",
    "Uri",
    "WidgetNode",
];

/// Types the hand-written declarations export.
const OVERRIDE_TYPES: &[&str] = &[
    "CommandArgs",
    "Disposable",
    "Document",
    "EditorEditBuilder",
    "GutterItem",
    "GutterItemOptions",
    "LogChannel",
    "PluginContext",
    "RemoteApi",
    "ShowPickerOptions",
    "StatuslineItem",
    "StatuslineItemOptions",
    "UiButtonOptions",
    "UiContainerOptions",
    "UiTextInputOptions",
    "UiTextOptions",
    "ViewKind",
    "ViewOptions",
];

/// Sugar interfaces `index.d.ts` extends, grouped by namespace path.
const SUGAR: &[(&[&str], &str)] = &[
    (&["commands"], "CommandsSugar"),
    (&["config"], "ConfigSugar"),
    (&["editor"], "EditorSugar"),
    (&["keymap"], "KeymapSugar"),
    (&["log"], "LogSugar"),
    (&["notifications"], "NotificationsSugar"),
    (&["ui"], "UiSugar"),
    (&["ui", "gutter"], "UiGutterSugar"),
    (&["ui", "picker"], "UiPickerSugar"),
    (&["ui", "statusline"], "UiStatuslineSugar"),
    (&["views"], "ViewsSugar"),
    (&["workspace"], "WorkspaceSugar"),
];

/// One namespace in the tree built from the method names.
#[derive(Default)]
struct Namespace<'a> {
    methods: Vec<&'a MethodSpec>,
    children: BTreeMap<&'a str, Namespace<'a>>,
}

impl<'a> Namespace<'a> {
    fn insert(&mut self, path: &[&'a str], spec: &'a MethodSpec) {
        match path.split_first() {
            None => {
                if !spec.manual {
                    self.methods.push(spec);
                }
            }
            Some((head, rest)) => {
                let child = self.children.entry(head).or_default();
                child.insert(rest, spec);
            }
        }
    }
}

/// Writes every generated file. The tests call this.
pub fn export() {
    let types_dir = export_dir();
    std::fs::create_dir_all(&types_dir).expect("create the TypeScript export directory");
    write_host_methods(&types_dir);

    let api_dir = types_dir
        .parent()
        .map_or_else(|| types_dir.clone(), Path::to_path_buf)
        .join("vy-api");
    std::fs::create_dir_all(&api_dir).expect("create the vy-api directory");
    std::fs::write(api_dir.join("index.d.ts"), render_api()).expect("write index.d.ts");
}

/// `TS_RS_EXPORT_DIR` when set by cargo, otherwise the repository default.
fn export_dir() -> PathBuf {
    std::env::var("TS_RS_EXPORT_DIR").map_or_else(
        |_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/vy-api-types"),
        PathBuf::from,
    )
}

fn write_host_methods(directory: &Path) {
    let mut output = header();
    for name in used_types(false) {
        output.push_str(&format!("import type {{ {name} }} from \"./{name}\";\n"));
    }
    output.push('\n');

    output.push_str("// Parameters and reply of every host call, keyed by method name.\n");
    output.push_str("export interface HostMethods {\n");
    for spec in HOST_METHODS {
        output.push_str(&format!(
            "  \"{}\": {{ params: {}; result: {} }};\n",
            spec.method, spec.params, spec.result
        ));
    }
    output.push_str("}\n\n");
    output.push_str("// Wire names of every host call.\n");
    output.push_str("export type HostMethodName = keyof HostMethods;\n");

    std::fs::write(directory.join("HostMethods.ts"), output).expect("write HostMethods.ts");
}

/// Renders `packages/vy-api/index.d.ts`.
fn render_api() -> String {
    let mut output = header();

    let generated_types = used_types(true);
    if !generated_types.is_empty() {
        output.push_str("import type {\n");
        for name in &generated_types {
            output.push_str(&format!("  {name},\n"));
        }
        output.push_str("} from \"../vy-api-types\";\n\n");
    }

    let mut sugars: BTreeSet<&str> = BTreeSet::new();
    for (path, sugar) in SUGAR {
        if declares_namespace(path) {
            sugars.insert(*sugar);
        }
    }
    if !sugars.is_empty() {
        output.push_str("import type {\n");
        for name in &sugars {
            output.push_str(&format!("  {name},\n"));
        }
        output.push_str("} from \"./overrides\";\n\n");
    }

    let mut exported = used_types(false);
    for name in EXTRA_TYPES {
        exported.insert(*name);
    }
    output.push_str("export type {\n");
    for name in &exported {
        output.push_str(&format!("  {name},\n"));
    }
    output.push_str("} from \"../vy-api-types\";\n\n");

    output.push_str("export type {\n");
    for name in OVERRIDE_TYPES {
        output.push_str(&format!("  {name},\n"));
    }
    output.push_str("} from \"./overrides\";\n\n");

    output.push_str("// Augment this interface to type the arguments of your own commands.\n");
    output.push_str("export interface CommandMap {}\n\n");

    let tree = build_tree();
    for &name in TOP_LEVEL {
        if let Some(namespace) = tree.children.get(name) {
            render_namespace(&mut output, &[name], namespace);
        }
    }

    output.push_str("export { context, remote } from \"./overrides\";\n");

    output
}

fn header() -> String {
    let mut output = String::new();
    output.push_str("// This file was generated by vy-api-schema. Do not edit it manually.\n");
    output.push_str("// Regenerate with `cargo test --workspace export_bindings`.\n\n");
    output
}

fn build_tree<'a>() -> Namespace<'a> {
    let mut tree = Namespace::default();
    for spec in HOST_METHODS {
        let path: Vec<&str> = spec.method.split('.').collect();
        // The last segment is the method name; the rest is its namespace.
        let namespace = &path[..path.len() - 1];
        tree.insert(namespace, spec);
    }
    tree
}

fn declares_namespace(path: &[&str]) -> bool {
    TOP_LEVEL.contains(&path[0])
}

fn render_namespace(output: &mut String, path: &[&str], namespace: &Namespace<'_>) {
    for (name, child) in &namespace.children {
        let mut child_path = path.to_vec();
        child_path.push(*name);
        render_namespace(output, &child_path, child);
    }
    let name = api_name(path);
    let extends = SUGAR
        .iter()
        .find(|(candidate, _)| *candidate == path)
        .map(|(_, sugar)| format!(" extends {sugar}"))
        .unwrap_or_default();
    output.push_str(&format!("export interface {name}{extends} {{\n"));

    for child in namespace.children.keys() {
        let mut child_path = path.to_vec();
        child_path.push(*child);
        output.push_str(&format!("  readonly {child}: {};\n", api_name(&child_path)));
    }
    let mut methods = namespace.methods.clone();
    methods.sort_by_key(|spec| spec.method);
    for spec in methods {
        output.push_str(&format!("  {}\n", signature(spec)));
    }
    output.push_str("}\n");

    if path.len() == 1 {
        output.push_str(&format!("export const {}: {name};\n", path[0]));
    }
    output.push('\n');
}

/// A method declaration, with `Promise<void>` for calls that reply with null.
fn signature(spec: &MethodSpec) -> String {
    let name = spec.method.rsplit('.').next().unwrap_or(spec.method);
    let params = if spec.has_params() {
        format!("params: {}", spec.params)
    } else {
        String::new()
    };
    let result = if spec.result == "null" {
        "void".to_string()
    } else {
        spec.result.to_string()
    };
    format!("{name}({params}): Promise<{result}>;")
}

fn api_name(path: &[&str]) -> String {
    let mut name = String::new();
    for segment in path {
        let mut characters = segment.chars();
        if let Some(first) = characters.next() {
            name.push(first.to_ascii_uppercase());
            name.push_str(characters.as_str());
        }
    }
    name.push_str("Api");
    name
}

/// Type names the generated signatures refer to.
fn used_types(generated_only: bool) -> BTreeSet<&'static str> {
    let mut names = BTreeSet::new();
    for spec in HOST_METHODS {
        if generated_only && spec.manual {
            continue;
        }
        for source in [spec.params, spec.result] {
            for token in source
                .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
            {
                if token.is_empty() || TYPE_KEYWORDS.contains(&token) {
                    continue;
                }
                names.insert(token);
            }
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_declarations_cover_generated_methods() {
        let output = render_api();
        for spec in HOST_METHODS {
            if spec.manual {
                continue;
            }
            let name = spec.method.rsplit('.').next().expect("method name");
            assert!(output.contains(name), "{} is missing", spec.method);
        }
        assert!(output.contains("export interface CommandMap"));
        assert!(output.contains("export const ui: UiApi;"));
    }

    #[test]
    fn exported_types_resolve_to_generated_files() {
        let directory = export_dir();
        for name in used_types(false) {
            let path = directory.join(format!("{name}.ts"));
            assert!(path.exists(), "{} is not exported", name);
        }
    }
}
