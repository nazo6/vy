//! Client-side pieces of `vy-app` that do not depend on GPUI.
//!
//! The binary in `main.rs` owns the window and rendering. Everything here can be
//! tested without a display.

pub mod commands;
pub mod config;
pub mod editor;
pub mod keymap;
pub mod paths;
pub mod rpc;
pub mod when;
