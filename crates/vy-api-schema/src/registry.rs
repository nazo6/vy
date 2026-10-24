//! Declarations produced by the `#[vy_api_namespace]` macro.
//!
//! A namespace module lists its methods and the TypeScript types that describe
//! them. The codegen and the shim read these declarations; see
//! `docs/adr/0008-declare-plugin-api-with-macros.md`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::{Config, TS};

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
    /// TypeScript name of the payload.
    pub payload: fn() -> String,
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
