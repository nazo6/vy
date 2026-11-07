//! Library interface for generating and checking plugin API TypeScript declarations.

use std::error::Error;
use std::io;
use std::path::{Path, PathBuf};

use vy_api_schema::codegen::{self, Output};
use vy_client_api::command::{
    ArgKind, BuiltinCommand, CommandMeta, DocumentCommand, PaneCommand, RangeSpec, SettingCommand,
    UiActionCommand, UiCommand, ViewCommand, display_title,
};
use vy_client_api::settings::SETTINGS;

const REGENERATE: &str = "pnpm generate:api";

pub fn generate_client(output: &Output) -> Result<(), Box<dyn Error>> {
    let methods = vy_client_api::client_host_api_methods();
    let namespaces = vy_client_api::client_host_api_namespaces();
    let events = vy_client_api::client_host_api_events();
    std::fs::create_dir_all(&output.types_dir)?;
    write_builtin_commands(&output.types_dir)?;
    write_command_metadata(&output.types_dir)?;
    write_settings_keys(&output.types_dir)?;
    codegen::export_to(output, &methods, &namespaces, &events)?;
    Ok(())
}

pub fn check_client(expected: &Output) -> Result<(), Box<dyn Error>> {
    let root = std::env::temp_dir().join(format!("vy-api-codegen-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);

    let actual = Output {
        types_dir: root.join("vy-api/types"),
        api_dir: root.join("vy-api"),
    };

    let result = (|| -> Result<(), Box<dyn Error>> {
        generate_client(&actual)?;
        compare_tree(&actual.types_dir, &expected.types_dir)?;
        compare_tree(&actual.api_dir, &expected.api_dir)?;
        check_shims(expected)?;
        Ok(())
    })();

    let _ = std::fs::remove_dir_all(&root);
    result
}

/// Checks that `packages/vy-api/shim.d.ts` and `crates/vy-js/src/generated/shim.js` exist and are in sync.
pub fn check_shims(expected: &Output) -> io::Result<()> {
    let mut differences = Vec::new();
    let shim_dts = expected.api_dir.join("shim.d.ts");
    let shim_js = expected
        .api_dir
        .join("../../crates/vy-js/src/generated/shim.js");
    let target_shim_js = expected
        .api_dir
        .join("../../target/api-shim/crates/vy-js/src/shim.js");
    let target_shim_dts = expected
        .api_dir
        .join("../../target/api-shim/crates/vy-js/src/shim.d.ts");

    if !shim_dts.exists() {
        differences.push(format!("missing: {}", shim_dts.display()));
    } else {
        let content = std::fs::read_to_string(&shim_dts)?;
        if !content.contains("generated from crates/vy-js/src/shim.ts") {
            differences.push(format!("invalid header in: {}", shim_dts.display()));
        }
        if target_shim_dts.exists() {
            let emitted_dts = std::fs::read_to_string(&target_shim_dts)?;
            let header = "// This file was generated from crates/vy-js/src/shim.ts by tsc.\n\
                          // Do not edit it manually. Regenerate with `pnpm generate:api`.\n\n";
            let expected_content = format!(
                "{}{}",
                header,
                emitted_dts.replace("../../../packages/vy-api/", "./")
            );
            if content != expected_content {
                differences.push(format!("differs: {}", shim_dts.display()));
            }
        }
    }

    if !shim_js.exists() {
        differences.push(format!("missing: {}", shim_js.display()));
    } else if target_shim_js.exists() {
        let actual_bytes = std::fs::read(&shim_js)?;
        let expected_bytes = std::fs::read(&target_shim_js)?;
        if actual_bytes != expected_bytes {
            differences.push(format!("differs: {}", shim_js.display()));
        }
    }

    if differences.is_empty() {
        Ok(())
    } else {
        Err(io::Error::other(differences.join("\n")))
    }
}

/// Compares every generated file with its committed copy.
pub fn compare_tree(generated: &Path, committed: &Path) -> io::Result<()> {
    let mut differences = Vec::new();
    for relative in files_under(generated)? {
        let actual = generated.join(&relative);
        let expected = committed.join(&relative);
        match std::fs::read(&expected) {
            Ok(body) if body == std::fs::read(&actual)? => {}
            Ok(_) => differences.push(format!("differs: {}", expected.display())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                differences.push(format!("missing: {}", expected.display()));
            }
            Err(error) => return Err(error),
        }
    }

    if differences.is_empty() {
        Ok(())
    } else {
        Err(io::Error::other(differences.join("\n")))
    }
}

fn files_under(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_files(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_files(root, &path, files)?;
        } else {
            files.push(
                path.strip_prefix(root)
                    .expect("path under root")
                    .to_path_buf(),
            );
        }
    }
    Ok(())
}

fn header() -> String {
    format!(
        "// This file was generated by vy-api-codegen. Do not edit it manually.\n\
         // Regenerate with `{REGENERATE}`.\n\n"
    )
}

pub fn write_builtin_commands(directory: &Path) -> io::Result<()> {
    let mut ids: Vec<&str> = Vec::new();
    ids.extend(BuiltinCommand::all().iter().map(|command| command.id()));
    ids.extend(PaneCommand::all().iter().map(|command| command.id()));
    ids.extend(DocumentCommand::all().iter().map(|command| command.id()));
    ids.extend(UiCommand::all().iter().map(|command| command.id()));
    ids.extend(UiActionCommand::all().iter().map(|command| command.id()));
    ids.extend(ViewCommand::all().iter().map(|command| command.id()));
    ids.extend(SettingCommand::all().iter().map(|command| command.id()));
    ids.sort_unstable();
    ids.dedup();

    let mut output = header();
    output.push_str("/** Arguments of each command built into the client. */\n");
    output.push_str("export interface BuiltinCommandMap {\n");
    for id in &ids {
        output.push_str(&format!("  \"{id}\": unknown;\n"));
    }
    output.push_str("}\n\n");
    output.push_str("/** Identifiers of the commands built into the client. */\n");
    output.push_str("export type BuiltinCommandId = keyof BuiltinCommandMap;\n");

    std::fs::write(directory.join("BuiltinCommandMap.ts"), output)
}

pub fn write_command_metadata(directory: &Path) -> io::Result<()> {
    let mut output = header();
    output.push_str("import type { ArgKind, CommandCategory } from \"./bindings\";\n\n");
    output.push_str("/** One declared positional argument. */\n");
    output.push_str("export interface CommandArgMetadata {\n  name: string;\n  kind: ArgKind;\n  choices: string[];\n  required: boolean;\n  rest: boolean;\n}\n\n");
    output.push_str("/** Metadata of one command built into the client. */\n");
    output.push_str("export interface CommandMetadata {\n  id: string;\n  title: string;\n  category: CommandCategory;\n  palette: \"listed\" | \"hidden\";\n  exNames: string[];\n  args: CommandArgMetadata[];\n  range: \"none\" | \"currentLine\" | \"selection\" | \"wholeDocument\";\n  bang: boolean;\n}\n\n");
    output.push_str("/** Every command built into the client. */\n");
    output.push_str("export const COMMAND_METADATA: CommandMetadata[] = [\n");

    for (id, title, meta) in builtin_metadata() {
        output.push_str("  {\n");
        output.push_str(&format!("    id: \"{}\",\n", escape(id)));
        output.push_str(&format!("    title: \"{}\",\n", escape(&title)));
        output.push_str(&format!(
            "    category: \"{}\",\n",
            category_name(meta.category)
        ));
        output.push_str(&format!(
            "    palette: \"{}\",\n",
            match meta.palette {
                vy_client_api::command::PaletteVisibility::Listed => "listed",
                vy_client_api::command::PaletteVisibility::Hidden => "hidden",
            }
        ));
        output.push_str(&format!(
            "    exNames: [{}],\n",
            meta.ex_names
                .iter()
                .map(|name| format!("\"{name}\""))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        output.push_str("    args: [");
        for (index, arg) in meta.args.args().iter().enumerate() {
            if index > 0 {
                output.push_str(", ");
            }
            output.push_str(&format!(
                "{{ name: \"{}\", kind: \"{}\", choices: [{}], required: {}, rest: {} }}",
                escape(arg.name),
                arg_kind_name(arg.kind),
                arg.choices
                    .iter()
                    .map(|choice| format!("\"{choice}\""))
                    .collect::<Vec<_>>()
                    .join(", "),
                arg.required,
                arg.rest
            ));
        }
        output.push_str("],\n");
        output.push_str(&format!("    range: \"{}\",\n", range_name(meta.range)));
        output.push_str(&format!("    bang: {},\n", meta.bang));
        output.push_str("  },\n");
    }

    output.push_str("];\n");
    std::fs::write(directory.join("CommandMetadata.ts"), output)
}

fn builtin_metadata() -> Vec<(&'static str, String, CommandMeta)> {
    let mut commands = Vec::new();
    for command in BuiltinCommand::all() {
        commands.push((command.id(), display_title(command.id()), command.meta()));
    }
    for command in PaneCommand::all() {
        commands.push((command.id(), display_title(command.id()), command.meta()));
    }
    for command in DocumentCommand::all() {
        commands.push((command.id(), command.title().to_string(), command.meta()));
    }
    for command in UiCommand::all() {
        commands.push((command.id(), command.title().to_string(), command.meta()));
    }
    for command in UiActionCommand::all() {
        commands.push((command.id(), command.title().to_string(), command.meta()));
    }
    for command in ViewCommand::all() {
        commands.push((command.id(), display_title(command.id()), command.meta()));
    }
    for command in SettingCommand::all() {
        commands.push((command.id(), command.title().to_string(), command.meta()));
    }
    commands.sort_by(|left, right| left.0.cmp(right.0));
    commands
}

fn category_name(category: vy_client_api::command::CommandCategory) -> &'static str {
    match category {
        vy_client_api::command::CommandCategory::File => "file",
        vy_client_api::command::CommandCategory::Edit => "edit",
        vy_client_api::command::CommandCategory::Selection => "selection",
        vy_client_api::command::CommandCategory::View => "view",
        vy_client_api::command::CommandCategory::Pane => "pane",
        vy_client_api::command::CommandCategory::App => "app",
        vy_client_api::command::CommandCategory::Settings => "settings",
        vy_client_api::command::CommandCategory::Plugin => "plugin",
    }
}

fn arg_kind_name(kind: ArgKind) -> &'static str {
    match kind {
        ArgKind::Text => "text",
        ArgKind::Path => "path",
        ArgKind::Document => "document",
        ArgKind::Number => "number",
        ArgKind::Choice => "choice",
        ArgKind::Any => "any",
    }
}

fn range_name(range: RangeSpec) -> &'static str {
    match range {
        RangeSpec::None => "none",
        RangeSpec::CurrentLine => "currentLine",
        RangeSpec::Selection => "selection",
        RangeSpec::WholeDocument => "wholeDocument",
    }
}

fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn write_settings_keys(directory: &Path) -> io::Result<()> {
    let mut output = header();
    output.push_str("/** Value type of every setting the client defines. */\n");
    output.push_str("export interface SettingsMap {\n");
    for setting in SETTINGS {
        output.push_str(&format!("  /** {} */\n", setting.doc));
        output.push_str(&format!(
            "  \"{}\": {};\n",
            setting.key,
            setting.typescript_type()
        ));
    }
    output.push_str("}\n\n");
    output.push_str("/** Names of every setting the client defines. */\n");
    output.push_str("export type SettingsKey = keyof SettingsMap;\n");

    std::fs::write(directory.join("SettingsKey.ts"), output)
}
