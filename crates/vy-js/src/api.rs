//! Source of the `vy` global installed by the script host.
//!
//! The shim is compiled from `shim.ts` by `pnpm generate:api`. It has no
//! imports and assigns `globalThis.vy` when evaluated. User scripts never
//! import it.

/// Source of the shim module, generated from `shim.ts` by `pnpm generate:api`.
pub const SHIM_SOURCE: &str = include_str!("generated/shim.js");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shim_installs_the_global_api() {
        assert!(SHIM_SOURCE.contains("globalThis.vy"));
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
