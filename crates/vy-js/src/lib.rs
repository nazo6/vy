//! QuickJS host for `init.ts`.
//!
//! The crate is independent of GPUI and of the RPC client. It owns the script
//! thread, the transpiler, and the global `vy` shim. The client turns its state
//! into [`vm::HostRequest`] replies.
//!
//! Phase 2 runs only `init.ts`, which is trusted. External plugins and the
//! approval UI come in Phase 3; [`vy_capabilities`] has the parsing and trust
//! records they will use.

pub mod api;
pub mod bundle;
pub mod document;
mod modules;
pub mod transpile;
pub mod vm;

pub use bundle::{BundleSyntaxError, validate_esm};
pub use document::DocumentInfoReply;
pub use transpile::{TranspileError, strip_types};
pub use vm::{
    HostReply, HostRequest, RemoteEnv, ScriptEnv, ScriptEvent, ScriptHost, ScriptTask, WorkspaceEnv,
};
