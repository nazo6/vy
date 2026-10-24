//! Writes the TypeScript declarations from the declarations that
//! `#[vy_api_namespace]` generated.
//!
//! `HostMethods.ts` lists every call; `packages/vy-api/index.d.ts` declares the
//! namespaces scripts import. Hand-written sugar lives in
//! `packages/vy-api/overrides.d.ts` and is composed here. See
//! `docs/adr/0008-declare-plugin-api-with-macros.md`.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;

use crate::EventDecl;
use crate::MethodDecl;
use crate::NamespaceDecl;

/// Tokens that are types in the emitted signature rather than imported names.
const TYPE_KEYWORDS: &[&str] = &[
    "Array", "boolean", "never", "null", "number", "Record", "string", "unknown", "void",
];

/// One namespace in the tree built from the declarations.
#[derive(Default)]
struct Namespace<'a> {
    methods: Vec<&'a MethodDecl>,
    events: Vec<&'a EventDecl>,
    children: BTreeMap<&'a str, Namespace<'a>>,
    decl: Option<&'a NamespaceDecl>,
}

impl<'a> Namespace<'a> {
    fn insert(&mut self, path: &[&'a str], method: &'a MethodDecl) {
        match path.split_first() {
            None => self.methods.push(method),
            Some((head, rest)) => {
                self.children.entry(head).or_default().insert(rest, method);
            }
        }
    }

    fn insert_event(&mut self, path: &[&'a str], event: &'a EventDecl) {
        match path.split_first() {
            None => self.events.push(event),
            Some((head, rest)) => {
                self.children
                    .entry(head)
                    .or_default()
                    .insert_event(rest, event);
            }
        }
    }

    fn declare(&mut self, path: &[&'a str], decl: &'a NamespaceDecl) {
        match path.split_first() {
            None => self.decl = Some(decl),
            Some((head, rest)) => {
                self.children.entry(head).or_default().declare(rest, decl);
            }
        }
    }

    /// Whether the generated declarations need an interface for this node.
    fn is_visible(&self) -> bool {
        self.decl.and_then(|decl| decl.sugar).is_some()
            || !self.events.is_empty()
            || self.methods.iter().any(|method| !method.manual)
            || self.children.values().any(Namespace::is_visible)
    }
}

/// Writes every generated file. The export test calls this.
pub fn export(methods: &[&MethodDecl], namespaces: &[&NamespaceDecl], events: &[&EventDecl]) {
    let types_dir = export_dir();
    std::fs::create_dir_all(&types_dir).expect("create the TypeScript export directory");
    write_host_methods(&types_dir, methods);
    write_host_events(&types_dir, events);
    write_barrel(&types_dir);

    let api_dir = types_dir
        .parent()
        .map_or_else(|| types_dir.clone(), Path::to_path_buf)
        .join("vy-api");
    std::fs::create_dir_all(&api_dir).expect("create the vy-api directory");
    std::fs::write(
        api_dir.join("index.d.ts"),
        render_api(methods, namespaces, events),
    )
    .expect("write index.d.ts");
    std::fs::write(api_dir.join("docs.d.ts"), render_docs()).expect("write docs.d.ts");
}

/// `TS_RS_EXPORT_DIR` when set by cargo, otherwise the repository default.
fn export_dir() -> PathBuf {
    std::env::var("TS_RS_EXPORT_DIR").map_or_else(
        |_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/vy-api-types"),
        PathBuf::from,
    )
}

fn write_host_methods(directory: &Path, methods: &[&MethodDecl]) {
    let mut sorted = methods.to_vec();
    sorted.sort_by_key(|method| method.method);

    let mut output = header();
    for name in used_types(&sorted) {
        let file = type_file(&name);
        output.push_str(&format!("import type {{ {name} }} from \"./{file}\";\n"));
    }
    output.push('\n');

    output.push_str("// Parameters and reply of every host call, keyed by method name.\n");
    output.push_str("export interface HostMethods {\n");
    for method in sorted {
        push_doc(&mut output, "  ", method.doc);
        let params = method
            .params
            .map(|params| params())
            .unwrap_or_else(|| "Record<string, never>".to_string());
        output.push_str(&format!(
            "  \"{}\": {{ params: {}; result: {} }};\n",
            method.method,
            params,
            (method.result)()
        ));
    }
    output.push_str("}\n\n");
    output.push_str("// Wire names of every host call.\n");
    output.push_str("export type HostMethodName = keyof HostMethods;\n");

    std::fs::write(directory.join("HostMethods.ts"), output).expect("write HostMethods.ts");
}

fn write_host_events(directory: &Path, events: &[&EventDecl]) {
    let mut sorted = events.to_vec();
    sorted.sort_by_key(|event| event.topic);

    let mut output = header();
    let payloads: BTreeSet<String> = sorted.iter().map(|event| (event.payload)()).collect();
    for name in &payloads {
        let file = type_file(name);
        output.push_str(&format!("import type {{ {name} }} from \"./{file}\";\n"));
    }
    output.push('\n');
    output.push_str("// Payload of every event, keyed by topic.\n");
    output.push_str("export interface HostEvents {\n");
    for event in sorted {
        push_doc(&mut output, "  ", event.doc);
        output.push_str(&format!(
            "  \"{}\": {{ payload: {} }};\n",
            event.topic,
            (event.payload)()
        ));
    }
    output.push_str("}\n\n");
    output.push_str("// Wire names of every event.\n");
    output.push_str("export type HostEventName = keyof HostEvents;\n");
    std::fs::write(directory.join("HostEvents.ts"), output).expect("write HostEvents.ts");
}

fn write_barrel(directory: &Path) {
    let mut names: Vec<String> = std::fs::read_dir(directory)
        .expect("read the TypeScript export directory")
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("ts") {
                return None;
            }
            let name = path.file_stem()?.to_str()?.to_string();
            (name != "index").then_some(name)
        })
        .collect();
    names.sort();

    let mut output = header();
    for name in names {
        output.push_str(&format!("export * from \"./{name}\";\n"));
    }
    std::fs::write(directory.join("index.ts"), output).expect("write index.ts");
}

/// Renders `packages/vy-api/index.d.ts`.
fn render_api(
    methods: &[&MethodDecl],
    namespaces: &[&NamespaceDecl],
    events: &[&EventDecl],
) -> String {
    let mut tree = Namespace::default();
    for method in methods {
        let path: Vec<&str> = method.method.split('.').collect();
        tree.insert(&path[..path.len() - 1], method);
    }
    for event in events {
        let path: Vec<&str> = event.namespace.split('.').collect();
        tree.insert_event(&path, event);
    }
    for decl in namespaces {
        let path: Vec<&str> = decl.path.split('.').collect();
        tree.declare(&path, decl);
    }

    let mut output = header();

    let generated: Vec<&MethodDecl> = methods
        .iter()
        .copied()
        .filter(|method| !method.manual)
        .collect();
    let mut generated_types = used_types(&generated);
    for event in events {
        generated_types.insert((event.payload)());
    }
    if !generated_types.is_empty() {
        output.push_str("import type {\n");
        for name in &generated_types {
            output.push_str(&format!("  {name},\n"));
        }
        output.push_str("} from \"../vy-api-types\";\n\n");
    }

    let sugars: BTreeSet<&str> = namespaces.iter().filter_map(|decl| decl.sugar).collect();
    if !sugars.is_empty() {
        output.push_str("import type {\n");
        for name in &sugars {
            output.push_str(&format!("  {name},\n"));
        }
        output.push_str("} from \"./overrides\";\n\n");
    }

    output.push_str("export * from \"../vy-api-types\";\n");
    output.push_str("export * from \"./overrides\";\n\n");

    output.push_str("/** A registration or event subscription that can be released. */\n");
    output.push_str("export interface Disposable {\n  dispose(): void;\n}\n\n");

    output.push_str("/** Augment this interface to type the arguments of your own commands. */\n");
    output.push_str("export interface CommandMap {}\n\n");

    for (name, namespace) in &tree.children {
        if namespace.is_visible() {
            render_namespace(&mut output, &[*name], namespace);
        }
    }

    output.push_str("declare global {\n");
    output.push_str("  /** The Vy client API exposed in the global scope. */\n");
    output.push_str("  const vy: typeof import(\"vy-api\");\n");
    output.push_str("}\n");

    output
}

/// Renders the TypeDoc entry point.
fn render_docs() -> String {
    let mut output = header();
    output.push_str("export * from \"./index\";\n");
    output.push_str("export * from \"./overrides\";\n");
    output
}

fn header() -> String {
    let mut output = String::new();
    output.push_str("// This file was generated by vy-api-schema. Do not edit it manually.\n");
    output.push_str("// Regenerate with `cargo test --workspace export_bindings`.\n\n");
    output
}

fn push_doc(output: &mut String, indent: &str, doc: &str) {
    output.push_str(&format!("{indent}/** {doc} */\n"));
}

fn render_namespace(output: &mut String, path: &[&str], namespace: &Namespace<'_>) {
    for (name, child) in &namespace.children {
        if child.is_visible() {
            let mut child_path = path.to_vec();
            child_path.push(*name);
            render_namespace(output, &child_path, child);
        }
    }
    let name = api_name(path);
    let extends = namespace
        .decl
        .and_then(|decl| decl.sugar)
        .map(|sugar| format!(" extends {sugar}"))
        .unwrap_or_default();
    if let Some(doc) = namespace.decl.map(|decl| decl.doc) {
        push_doc(output, "", doc);
    }
    output.push_str(&format!("export interface {name}{extends} {{\n"));

    for (child, child_namespace) in &namespace.children {
        if !child_namespace.is_visible() {
            continue;
        }
        let mut child_path = path.to_vec();
        child_path.push(*child);
        output.push_str(&format!("  readonly {child}: {};\n", api_name(&child_path)));
    }
    let mut methods = namespace.methods.clone();
    methods.sort_by_key(|method| method.method);
    for method in methods {
        if method.manual {
            continue;
        }
        push_doc(output, "  ", method.doc);
        output.push_str(&format!("  {}\n", signature(method)));
    }
    let mut events = namespace.events.clone();
    events.sort_by_key(|event| event.name);
    for event in events {
        push_doc(output, "  ", event.doc);
        output.push_str(&format!(
            "  {}(listener: (event: {}) => void): Disposable;\n",
            event.name,
            (event.payload)()
        ));
    }
    output.push_str("}\n");

    if path.len() == 1 {
        if let Some(doc) = namespace.decl.map(|decl| decl.doc) {
            push_doc(output, "", doc);
        }
        output.push_str(&format!("export const {}: {name};\n", path[0]));
    }
    output.push('\n');
}

/// A method declaration, with `Promise<void>` for calls that reply with null.
fn signature(method: &MethodDecl) -> String {
    let name = method.method.rsplit('.').next().unwrap_or(method.method);
    let params = match method.params {
        Some(params) => format!("params: {}", params()),
        None => String::new(),
    };
    let result = (method.result)();
    let result = if result == "null" {
        "void".to_string()
    } else {
        result
    };
    let general = format!("{name}({params}): Promise<{result}>;");

    if let Some(map) = method.typed_key {
        return format!(
            "{name}<K extends keyof {map}>(params: {{ key: K }}): Promise<{map}[K]>;\n  {general}"
        );
    }
    if let Some(map) = method.typed_args {
        return format!(
            "{name}<K extends keyof {map}>(params: {{ id: K; args: {map}[K] }}): Promise<unknown>;\n  {general}"
        );
    }
    general
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

/// The file that exports a named type.
fn type_file(name: &str) -> &str {
    match name {
        "SettingsMap" | "SettingsKey" => "SettingsKey",
        other => other,
    }
}

/// Type names the generated signatures refer to.
fn used_types(methods: &[&MethodDecl]) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for method in methods {
        let sources = [
            method.params.map(|params| params()),
            Some((method.result)()),
        ];
        for source in sources.into_iter().flatten() {
            for token in source
                .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
            {
                if token.is_empty() || TYPE_KEYWORDS.contains(&token) {
                    continue;
                }
                names.insert(token.to_string());
            }
        }
        if let Some(map) = method.typed_key {
            names.insert(map.to_string());
        }
        if let Some(map) = method.typed_args
            && map != "CommandMap"
        {
            names.insert(map.to_string());
        }
    }
    names
}
