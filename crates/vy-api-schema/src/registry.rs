//! Declarations produced by the `#[vy_api_trait]` macro.
//!
//! A namespace module lists its methods and the TypeScript types that describe
//! them. The codegen and the shim read these declarations; see
//! `docs/adr/0008-declare-plugin-api-with-macros.md` and `docs/adr/0023-specta-api-types.md`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use specta::Type;

/// Any JSON value. Its TypeScript type is `unknown`.
///
/// Methods whose reply shape is not fixed (currently `config.get`) return this
/// instead of a named type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Json(pub Value);

impl Type for Json {
    fn definition(types: &mut specta::Types) -> specta::datatype::DataType {
        <specta_typescript::Unknown as Type>::definition(types)
    }
}

/// A TypeScript type a method or an event refers to.
///
/// The name is rendered with Specta, so `Option<T>` becomes `T | null` and
/// `Vec<T>` becomes `T[]`. The register function adds the type and every
/// transitive type it depends on to the `specta::Types` registry.
#[derive(Clone, Copy, Debug)]
pub struct TypeRef {
    /// The TypeScript name, for example `EditorSetModeParams`.
    pub name: fn() -> String,
    /// Adds this type and its transitive dependencies to `types`.
    pub register: fn(&mut specta::Types),
}

/// Registers `T` and all types it transitively depends on into `types`.
pub fn register<T: Type>(types: &mut specta::Types) {
    <T as Type>::definition(types);
}

/// The TypeScript name of a boundary type.
///
/// Specta maps `Option<T>` to `T | null`, `Vec<T>` to `T[]`, and `()` to
/// `null`, so the declaration needs no hand-written TypeScript strings.
pub fn ts_name<T: Type>() -> String {
    let mut types = specta::Types::default();
    let dt = <T as Type>::definition(&mut types);
    let exporter: specta_typescript::Exporter = specta_typescript::Typescript::default().into();
    specta_typescript::primitives::reference(&exporter, &types, &dt)
        .unwrap_or_else(|_| "unknown".to_string())
}

/// One host method, as `#[vy_api_trait]` collected it.
#[derive(Clone, Copy, Debug)]
pub struct MethodDecl {
    /// Wire name, for example `editor.mode`.
    pub method: &'static str,
    /// TypeScript name of the parameter object, if the method takes one.
    pub params: Option<TypeRef>,
    /// TypeScript type of the reply value.
    pub result: TypeRef,
    /// One-line description emitted as JSDoc.
    pub doc: &'static str,
    /// Whether the shim provides a hand-written wrapper for this method.
    pub manual: bool,
    /// Type name of the map that keys this method's parameter, if any.
    pub typed_key: Option<&'static str>,
}

/// One event the host can emit to the script.
#[derive(Clone, Copy, Debug)]
pub struct EventDecl {
    /// Wire topic, for example `editor.onDidChangeSelection`.
    pub topic: &'static str,
    /// Subscription method name, for example `onDidChangeSelection`.
    pub name: &'static str,
    /// Namespace that exposes the subscription.
    pub namespace: &'static str,
    /// TypeScript type of the payload.
    pub payload: TypeRef,
    /// One-line description emitted as JSDoc.
    pub doc: &'static str,
}

/// One namespace in the generated TypeScript surface.
#[derive(Clone, Copy, Debug)]
pub struct NamespaceDecl {
    /// Dotted path, for example `ui.statusline`.
    pub path: &'static str,
    /// Description emitted as JSDoc for the namespace.
    pub doc: &'static str,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, Deserialize, Type)]
    struct Inner {
        value: u32,
    }

    #[derive(Serialize, Deserialize, Type)]
    struct Outer {
        inner: Option<Inner>,
        items: Vec<Inner>,
    }

    #[test]
    fn registers_types_recursively() {
        let mut types = specta::Types::default();
        register::<Outer>(&mut types);
        let ts = specta_typescript::Typescript::default()
            .export(&types, &specta_serde::Format)
            .unwrap();
        assert!(ts.contains("export type Inner"));
        assert!(ts.contains("export type Outer"));
    }

    #[test]
    fn formats_type_names() {
        assert_eq!(ts_name::<Inner>(), "Inner");
        assert_eq!(ts_name::<Option<Inner>>(), "Inner | null");
        assert_eq!(ts_name::<Vec<Inner>>(), "Inner[]");
        assert_eq!(ts_name::<()>(), "null");
        assert_eq!(ts_name::<Json>(), "unknown");
    }
}
