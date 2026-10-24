//! Validation for plugin bundles.
//!
//! A bundle is a single ES module that already contains every dependency. It
//! must not request another module at runtime: `vy` is a global, and Vy does not
//! resolve `node_modules` or CommonJS. This module parses the bundle with Oxc
//! and reports the first remaining module request.

use oxc_allocator::Allocator;
use oxc_ast::ast::{CallExpression, Expression};
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_span::SourceType;

use crate::transpile::{diagnostic_offset, position};

/// A bundle that cannot be loaded as a single ES module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleSyntaxError {
    pub message: String,
    pub line: Option<u32>,
    /// Zero-based column in UTF-8 bytes.
    pub column: Option<u32>,
}

impl BundleSyntaxError {
    fn new(source: &str, offset: usize, message: impl Into<String>) -> Self {
        let (line, column) = position(source, offset);
        Self {
            message: message.into(),
            line,
            column,
        }
    }
}

impl std::fmt::Display for BundleSyntaxError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.line, self.column) {
            (Some(line), Some(column)) => {
                write!(formatter, "{}:{}: {}", line, column + 1, self.message)
            }
            _ => formatter.write_str(&self.message),
        }
    }
}

impl std::error::Error for BundleSyntaxError {}

/// Checks that `source` is a standalone ES module.
///
/// TypeScript and JSX are rejected because a bundle is already JavaScript. The
/// parser is forced to the module goal so top-level `await` and `export` work.
pub fn validate_esm(source: &str) -> Result<(), BundleSyntaxError> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::mjs()).parse();

    if let Some(diagnostic) = parsed.diagnostics.first() {
        return Err(BundleSyntaxError::new(
            source,
            diagnostic_offset(diagnostic),
            diagnostic.message.to_string(),
        ));
    }

    let first_import = parsed
        .module_record
        .requested_modules
        .iter()
        .flat_map(|(specifier, occurrences)| {
            occurrences
                .first()
                .map(|occurrence| (specifier.as_str(), occurrence.span.start))
        })
        .min_by_key(|(_, start)| *start);
    if let Some((specifier, start)) = first_import {
        return Err(BundleSyntaxError::new(
            source,
            start as usize,
            format!("bundle must not contain imports; found `{specifier}`"),
        ));
    }

    if let Some(dynamic) = parsed.module_record.dynamic_imports.first() {
        return Err(BundleSyntaxError::new(
            source,
            dynamic.span.start as usize,
            "bundle must not use dynamic import(); bundle every dependency",
        ));
    }

    let mut finder = RequireFinder::default();
    finder.visit_program(&parsed.program);
    if let Some((target, start)) = finder.found {
        let message = match target {
            Some(target) => format!("bundle must not call require(); found `{target}`"),
            None => "bundle must not call require(); bundle every dependency".to_string(),
        };
        return Err(BundleSyntaxError::new(source, start as usize, message));
    }

    Ok(())
}

/// Finds the first call to a global-looking `require`.
#[derive(Default)]
struct RequireFinder {
    found: Option<(Option<String>, u32)>,
}

impl<'a> Visit<'a> for RequireFinder {
    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        if self.found.is_none()
            && let Expression::Identifier(identifier) = &call.callee
            && identifier.name.as_str() == "require"
        {
            let target = call.arguments.first().and_then(|argument| {
                let expression = argument.as_expression()?;
                match expression {
                    Expression::StringLiteral(literal) => {
                        literal.value.as_str().map(str::to_string)
                    }
                    _ => None,
                }
            });
            self.found = Some((target, call.span.start));
        }
        walk::walk_call_expression(self, call);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error(source: &str) -> BundleSyntaxError {
        validate_esm(source).expect_err("bundle should be rejected")
    }

    #[test]
    fn accepts_a_standalone_esm_bundle() {
        let source = r#"
export const answer = 42;
await vy.notifications.show({ message: "ready" });
"#;
        assert_eq!(validate_esm(source), Ok(()));
    }

    #[test]
    fn rejects_static_imports() {
        let error = error("import { z } from \"zod\";\n");
        assert!(error.message.contains("zod"), "{error}");
    }

    #[test]
    fn rejects_side_effect_imports() {
        let error = error("import \"./helper.js\";\n");
        assert!(error.message.contains("./helper.js"), "{error}");
    }

    #[test]
    fn rejects_reexports() {
        let error = error("export { z } from \"zod\";\n");
        assert!(error.message.contains("zod"), "{error}");
    }

    #[test]
    fn rejects_dynamic_imports() {
        let error = error("const mod = import(\"zod\");\n");
        assert!(error.message.contains("dynamic import"), "{error}");
    }

    #[test]
    fn rejects_commonjs_require() {
        let error = error("const z = require(\"zod\");\n");
        assert!(error.message.contains("require"), "{error}");
        assert!(error.message.contains("zod"), "{error}");
    }

    #[test]
    fn rejects_typescript_syntax() {
        let error = error("const answer: number = 42;\n");
        assert!(!error.message.is_empty());
    }

    #[test]
    fn reports_a_position() {
        let error = error("export const answer = 42;\nimport \"zod\";\n");
        assert_eq!(error.line, Some(2));
        assert!(error.column.is_some());
    }
}
