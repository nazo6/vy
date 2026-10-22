//! Removes TypeScript-only syntax before QuickJS evaluates a script.
//!
//! `init.ts` is TypeScript, but the host only needs the runtime behavior. Only
//! types are stripped: interfaces, type aliases, annotations, generics, and
//! `as` casts. The output stays an ES module, so imports and exports work.
//!
//! Oxc parses and transforms the file. Nothing is written to disk.

use std::path::Path;

use oxc_allocator::Allocator;
use oxc_codegen::Codegen;
use oxc_diagnostics::OxcDiagnostic;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;
use oxc_transformer::{TransformOptions, Transformer};

/// A TypeScript file turned into JavaScript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranspileError {
    pub message: String,
    pub line: Option<u32>,
    /// Zero-based column in UTF-8 bytes.
    pub column: Option<u32>,
}

impl TranspileError {
    fn new(message: impl Into<String>, line: Option<u32>, column: Option<u32>) -> Self {
        Self {
            message: message.into(),
            line,
            column,
        }
    }
}

impl std::fmt::Display for TranspileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.line, self.column) {
            (Some(line), Some(column)) => {
                write!(formatter, "{}:{}: {}", line, column + 1, self.message)
            }
            _ => formatter.write_str(&self.message),
        }
    }
}

impl std::error::Error for TranspileError {}

/// Strips the type-only syntax from `source`.
///
/// `path` decides whether decorators and JSX follow TypeScript rules; it is only
/// used for diagnostics, so the file does not have to exist.
pub fn strip_types(path: &Path, source: &str) -> Result<String, TranspileError> {
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(path).unwrap_or_else(|_| SourceType::ts());

    let parsed = Parser::new(&allocator, source, source_type).parse();
    if let Some(diagnostic) = parsed.diagnostics.first() {
        let (line, column) = position(source, diagnostic_offset(diagnostic));
        return Err(TranspileError::new(
            diagnostic.message.to_string(),
            line,
            column,
        ));
    }

    let mut program = parsed.program;
    let scoping = SemanticBuilder::new()
        .build(&program)
        .semantic
        .into_scoping();

    let options = TransformOptions::default();
    let transformed =
        Transformer::new(&allocator, path, &options).build_with_scoping(scoping, &mut program);
    if let Some(diagnostic) = transformed.diagnostics.first() {
        let (line, column) = position(source, diagnostic_offset(diagnostic));
        return Err(TranspileError::new(
            diagnostic.message.to_string(),
            line,
            column,
        ));
    }

    Ok(Codegen::new().build(&program).code)
}

fn diagnostic_offset(diagnostic: &OxcDiagnostic) -> usize {
    diagnostic
        .labels
        .first()
        .map(|label| label.offset() as usize)
        .unwrap_or(0)
}

/// Converts a byte offset to a one-based line and a zero-based column.
fn position(source: &str, offset: usize) -> (Option<u32>, Option<u32>) {
    if offset > source.len() {
        return (None, None);
    }
    let mut line = 0u32;
    let mut column = 0u32;
    for (index, character) in source.char_indices() {
        if index >= offset {
            break;
        }
        if character == '\n' {
            line += 1;
            column = 0;
        } else {
            column += character.len_utf8() as u32;
        }
    }
    (Some(line + 1), Some(column))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn strip(source: &str) -> Result<String, TranspileError> {
        strip_types(&PathBuf::from("init.ts"), source)
    }

    #[test]
    fn removes_annotations_and_interfaces() {
        let source = r#"
interface Options {
  size: number;
}

function twice(value: number, label?: string): number {
  return value * 2;
}

const options: Options = { size: 2 };
const name = "vy" as string;
export const result = twice(options.size as number, name);
"#;

        let output = strip(source).expect("transpile");
        assert!(!output.contains("interface"), "{output}");
        assert!(!output.contains(": number"), "{output}");
        assert!(!output.contains(" as string"), "{output}");
        assert!(output.contains("function twice(value, label)"), "{output}");
        assert!(output.contains("export const result"), "{output}");
    }

    #[test]
    fn keeps_generics_and_imports_out_of_the_runtime() {
        let source = r#"
import { config } from "vy-api";

type Value<T> = T | null;

function first<T>(items: T[]): T | undefined {
  return items[0];
}

config.set({ "ui.fontSize": 16 });
export const picked = first<number>([1, 2, 3]);
"#;

        let output = strip(source).expect("transpile");
        assert!(output.contains("from \"vy-api\""), "{output}");
        assert!(output.contains("function first(items)"), "{output}");
        assert!(output.contains("first(["), "{output}");
        assert!(!output.contains("type Value"), "{output}");
    }

    #[test]
    fn reports_a_position_for_broken_input() {
        let error = strip("const value: = 1;").expect_err("syntax error");
        assert_eq!(error.line, Some(1));
        assert!(!error.message.is_empty());
    }

    #[test]
    fn leaves_plain_javascript_alone() {
        let source = "export const answer = 42;\n";
        assert_eq!(strip(source).expect("transpile"), source);
    }
}
