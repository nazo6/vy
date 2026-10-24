//! Writes the TypeScript declarations from the declarations that
//! `#[vy_api_trait]` generated.
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
use crate::TypeCollector;

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

/// Where the generated client declarations are written.
#[derive(Clone, Debug)]
pub struct Output {
    /// Directory for per-type `.ts` files, for example `packages/vy-api-types`.
    pub types_dir: PathBuf,
    /// Directory for the namespace declarations, for example `packages/vy-api`.
    pub api_dir: PathBuf,
}

impl Output {
    /// The repository's committed output directories.
    ///
    /// `TS_RS_EXPORT_DIR` overrides the type directory, as it does for `ts-rs`.
    pub fn packages() -> Self {
        let types_dir = std::env::var("TS_RS_EXPORT_DIR").map_or_else(
            |_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/vy-api-types"),
            PathBuf::from,
        );
        let api_dir = types_dir
            .parent()
            .map_or_else(|| types_dir.clone(), Path::to_path_buf)
            .join("vy-api");
        Self { types_dir, api_dir }
    }
}

/// Writes every generated file into `output`.
pub fn export_to(
    output: &Output,
    methods: &[&MethodDecl],
    namespaces: &[&NamespaceDecl],
    events: &[&EventDecl],
) -> std::io::Result<()> {
    std::fs::create_dir_all(&output.types_dir)?;
    write_types(&output.types_dir, methods, events)?;
    write_host_methods(&output.types_dir, methods)?;
    write_host_events(&output.types_dir, events)?;
    write_barrel(&output.types_dir)?;

    std::fs::create_dir_all(&output.api_dir)?;
    std::fs::write(
        output.api_dir.join("index.d.ts"),
        render_api(methods, namespaces, events),
    )?;
    std::fs::write(output.api_dir.join("docs.d.ts"), render_docs())?;
    Ok(())
}

/// Writes every type the declarations refer to.
///
/// `ts-rs` generates a per-type export test, but the API needs one entry point
/// that writes all of them. The declarations carry the collector, so a type can
/// live in any crate and still be generated here.
fn write_types(
    directory: &Path,
    methods: &[&MethodDecl],
    events: &[&EventDecl],
) -> std::io::Result<()> {
    let mut collector = TypeCollector::default();
    for method in methods {
        if let Some(params) = method.params {
            (params.collect)(&mut collector);
        }
        (method.result.collect)(&mut collector);
    }
    for event in events {
        (event.payload.collect)(&mut collector);
    }

    for (path, body) in collector.files() {
        let target = directory.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, body)?;
    }
    Ok(())
}

fn write_host_methods(directory: &Path, methods: &[&MethodDecl]) -> std::io::Result<()> {
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
            .map(|params| (params.name)())
            .unwrap_or_else(|| "Record<string, never>".to_string());
        output.push_str(&format!(
            "  \"{}\": {{ params: {}; result: {} }};\n",
            method.method,
            params,
            (method.result.name)()
        ));
    }
    output.push_str("}\n\n");
    output.push_str("// Wire names of every host call.\n");
    output.push_str("export type HostMethodName = keyof HostMethods;\n");

    std::fs::write(directory.join("HostMethods.ts"), output)
}

fn write_host_events(directory: &Path, events: &[&EventDecl]) -> std::io::Result<()> {
    let mut sorted = events.to_vec();
    sorted.sort_by_key(|event| event.topic);

    let mut output = header();
    let payloads: BTreeSet<String> = sorted.iter().map(|event| (event.payload.name)()).collect();
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
            (event.payload.name)()
        ));
    }
    output.push_str("}\n\n");
    output.push_str("// Wire names of every event.\n");
    output.push_str("export type HostEventName = keyof HostEvents;\n");
    std::fs::write(directory.join("HostEvents.ts"), output)
}

fn write_barrel(directory: &Path) -> std::io::Result<()> {
    let mut names: Vec<String> = std::fs::read_dir(directory)?
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
    std::fs::write(directory.join("index.ts"), output)
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
        generated_types.insert((event.payload.name)());
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
    output.push_str("// Regenerate with `cargo run -p vy-api-codegen -- client`.\n\n");
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
            (event.payload.name)()
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
        Some(params) => format!("params: {}", (params.name)()),
        None => String::new(),
    };
    let result = (method.result.name)();
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
            method.params.map(|params| (params.name)()),
            Some((method.result.name)()),
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
