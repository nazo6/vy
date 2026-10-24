//! Declarations shared by the host-trait macros and the codegen.
//!
//! The API types live with their namespace traits in `vy-host-api` and
//! `vy-client-api`; this crate holds the runtime metadata the macros emit
//! (`MethodDecl`, `EventDecl`, `NamespaceDecl`) and the codegen that writes the
//! TypeScript declarations. See `docs/adr/0013-host-api-traits.md`.
//!
//! Nothing here may depend on GPUI or rquickjs.

pub mod codegen;
mod registry;

pub use registry::{
    EventDecl, Json, MethodDecl, NamespaceDecl, TypeCollector, TypeRef, collect, ts_name,
};
pub use vy_api_macros::{vy_api_event, vy_api_method, vy_api_trait};
