//! Declarations produced by the `#[vy_api_trait]` macro.
//!
//! A namespace module lists its methods and the TypeScript types that describe
//! them. The codegen and the shim read these declarations; see
//! `docs/adr/0008-declare-plugin-api-with-macros.md`.

use std::any::TypeId;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::{Config, TS, TypeVisitor};

/// Any JSON value. Its TypeScript type is `unknown`.
///
/// Methods whose reply shape is not fixed (currently `config.get`) return this
/// instead of a named type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Json(pub Value);

impl TS for Json {
    type WithoutGenerics = Self;
    type OptionInnerType = Self;

    fn name(_: &Config) -> String {
        "unknown".to_string()
    }

    fn inline(_: &Config) -> String {
        "unknown".to_string()
    }
}

/// A TypeScript type a method or an event refers to.
///
/// The name is `TS::name`, so `Option<T>` becomes `T | null` and `Vec<T>`
/// becomes `Array<T>`. The collector adds the type and every type it depends
/// on to the generated files.
#[derive(Clone, Copy, Debug)]
pub struct TypeRef {
    /// The TypeScript name, for example `EditorSetModeParams`.
    pub name: fn() -> String,
    /// Adds this type and its transitive dependencies to `collector`.
    pub collect: fn(&mut TypeCollector),
}

/// The TypeScript files a declaration refers to.
///
/// `ts-rs` knows how to render a type and the types it depends on, but it only
/// exports them from generated tests. The codegen walks the declarations with
/// this collector instead, so every file comes from one place.
#[derive(Default)]
pub struct TypeCollector {
    files: BTreeMap<PathBuf, String>,
    named: BTreeSet<(String, PathBuf)>,
    seen: HashSet<TypeId>,
}

impl TypeCollector {
    /// Adds `T` and the types it depends on.
    pub fn insert<T: TS + 'static + ?Sized>(&mut self) {
        if !self.seen.insert(TypeId::of::<T>()) {
            return;
        }

        let config = Config::default();
        if let Some(path) = T::output_path()
            && let Ok(body) = T::export_to_string(&config)
        {
            self.named.insert((T::name(&config), path.clone()));
            self.files.insert(path, body);
        }

        struct Visit<'a>(&'a mut TypeCollector);

        impl TypeVisitor for Visit<'_> {
            fn visit<T: TS + 'static + ?Sized>(&mut self) {
                self.0.insert::<T>();
            }
        }

        // `visit_dependencies` walks the fields, but wrapper types such as
        // `Option<T>` and `Vec<T>` only visit their own dependencies there.
        // Their type parameter arrives through `visit_generics`.
        T::visit_dependencies(&mut Visit(self));
        T::visit_generics(&mut Visit(self));
    }

    /// The files to write, keyed by their path under the export directory.
    pub fn files(&self) -> &BTreeMap<PathBuf, String> {
        &self.files
    }

    /// The named types to import, keyed by TypeScript name.
    pub fn named(&self) -> impl Iterator<Item = (&str, &PathBuf)> {
        self.named.iter().map(|(name, path)| (name.as_str(), path))
    }

    /// Whether the collector found a type with this TypeScript name.
    pub fn contains(&self, name: &str) -> bool {
        self.named.iter().any(|(found, _)| found == name)
    }
}

/// Adds `T` and the types it depends on to `collector`.
pub fn collect<T: TS + 'static + ?Sized>(collector: &mut TypeCollector) {
    collector.insert::<T>();
}

/// One host method, as `#[vy_api_trait]` collected it.
///
/// `params` is `None` for a method without arguments. The names are computed
/// from the Rust types with `ts-rs`, so the declarations cannot drift from the
/// generated `.ts` files.
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
    /// Type name of the map that types the `args` field, if any.
    pub typed_args: Option<&'static str>,
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
    /// Hand-written sugar interface that `index.d.ts` extends, if any.
    pub sugar: Option<&'static str>,
}

/// The TypeScript name of a boundary type.
///
/// `ts-rs` maps `Option<T>` to `T | null`, `Vec<T>` to `Array<T>`, and `()` to
/// `null`, so the declaration needs no hand-written TypeScript strings.
pub fn ts_name<T: TS>() -> String {
    <T as TS>::name(&Config::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, TS)]
    struct Inner {
        value: u32,
    }

    #[derive(Serialize, Deserialize, TS)]
    struct Outer {
        inner: Option<Inner>,
        items: Vec<Inner>,
    }

    #[test]
    fn collects_types_behind_wrappers() {
        let mut collector = TypeCollector::default();
        collect::<Option<Inner>>(&mut collector);
        assert!(collector.contains("Inner"));
    }

    #[test]
    fn collects_field_types_recursively() {
        let mut collector = TypeCollector::default();
        collect::<Outer>(&mut collector);
        assert!(collector.contains("Outer"));
        assert!(collector.contains("Inner"));
    }
}
