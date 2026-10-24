//! Declarations shared by the `#[vy_api_namespace]` macro and the codegen.
//!
//! The API types live with their handlers in `vy-app`; this crate only holds
//! the runtime metadata the macro emits (`MethodDecl`, `EventDecl`,
//! `NamespaceDecl`) and the codegen that writes the TypeScript declarations.
//! See `docs/adr/0008-declare-plugin-api-with-macros.md`.
//!
//! Nothing here may depend on GPUI or rquickjs.

pub mod codegen;
mod registry;

pub use registry::{
    EventDecl, Json, MethodDecl, NamespaceDecl, TypeCollector, TypeRef, collect, ts_name,
};
pub use vy_api_macros::{
    vy_api_event, vy_api_method, vy_api_namespace, vy_api_registry, vy_api_trait,
};
