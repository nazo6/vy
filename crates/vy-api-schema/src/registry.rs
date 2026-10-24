//! Declarations produced by the `#[vy_api_namespace]` macro.
//!
//! A namespace module lists its methods and the TypeScript types that describe
//! them. The codegen and the shim read these declarations; see
//! `docs/adr/0008-declare-plugin-api-with-macros.md`.

use ts_rs::{Config, TS};

/// One host method, as `#[vy_api_namespace]` collected it.
///
/// `params` is `None` for a method without arguments. The names are computed
/// from the Rust types with `ts-rs`, so the declarations cannot drift from the
/// generated `.ts` files.
#[derive(Clone, Copy, Debug)]
pub struct MethodDecl {
    /// Wire name, for example `editor.mode`.
    pub method: &'static str,
    /// TypeScript name of the parameter object, if the method takes one.
    pub params: Option<fn() -> String>,
    /// TypeScript type of the reply value.
    pub result: fn() -> String,
    /// One-line description emitted as JSDoc.
    pub doc: &'static str,
    /// Whether the shim provides a hand-written wrapper for this method.
    pub manual: bool,
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
