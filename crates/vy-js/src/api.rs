//! The `vy-api` module served to `init.ts`.
//!
//! QuickJS asks the runtime's loader for `vy-api`; [`VyLoader`] answers with
//! `shim.js`, which is compiled into the binary. The shim calls back into Rust
//! through `globalThis.__vy_host_call`, so every Promise is resolved by the host
//! and never depends on the network.

use rquickjs::loader::{ImportAttributes, Loader, Resolver};
use rquickjs::{Ctx, Error, Module, Result};

/// The module name scripts import.
pub const API_MODULE: &str = "vy-api";

/// Source of the shim module, generated from `shim.ts` by `pnpm generate:api`.
pub const SHIM_SOURCE: &str = include_str!("generated/shim.js");

/// Resolves `vy-api` and rejects everything else.
///
/// `init.ts` may not import files in Phase 2. External plugins will bring their
/// own resolver in Phase 3.
pub struct VyResolver;

impl Resolver for VyResolver {
    fn resolve<'js>(
        &mut self,
        _ctx: &Ctx<'js>,
        base: &str,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>,
    ) -> Result<String> {
        if name == API_MODULE {
            Ok(API_MODULE.to_string())
        } else {
            Err(Error::new_resolving_message(
                base,
                name,
                "only `vy-api` can be imported in Phase 2",
            ))
        }
    }
}

/// Serves the shim source for `vy-api`.
pub struct VyLoader;

impl Loader for VyLoader {
    fn load<'js>(
        &mut self,
        ctx: &Ctx<'js>,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>,
    ) -> Result<Module<'js, rquickjs::module::Declared>> {
        if name == API_MODULE {
            Module::declare(ctx.clone(), API_MODULE, SHIM_SOURCE)
        } else {
            Err(Error::new_loading_message(name, "unknown module"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shim_is_a_module_with_the_phase_two_namespaces() {
        assert!(SHIM_SOURCE.contains("export const commands"));
        assert!(SHIM_SOURCE.contains("export const config"));
        assert!(SHIM_SOURCE.contains("export const context"));
        assert!(SHIM_SOURCE.contains("export const document"));
        assert!(SHIM_SOURCE.contains("export const editor"));
        assert!(SHIM_SOURCE.contains("export const fs"));
        assert!(SHIM_SOURCE.contains("export const keymap"));
        assert!(SHIM_SOURCE.contains("export const log"));
        assert!(SHIM_SOURCE.contains("export const notifications"));
        assert!(SHIM_SOURCE.contains("export const process"));
        assert!(SHIM_SOURCE.contains("export const ui"));
        assert!(SHIM_SOURCE.contains("export const views"));
        assert!(SHIM_SOURCE.contains("export const workspace"));
        assert!(!SHIM_SOURCE.contains(": string"));
    }
}
