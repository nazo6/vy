//! Client-side pieces of `vy-app` that do not depend on GPUI.
//!
//! The binary in `main.rs` owns the window and rendering. Everything here can be
//! tested without a display.

pub mod api;
pub mod commands;
pub mod config;
pub mod editor;
pub mod ex;
pub mod geometry;
pub mod keymap;
pub mod palette;
pub mod paths;
pub mod plugin;
pub mod plugin_host;
pub mod rpc;
pub mod slots;
pub mod state;
pub mod statusline;
pub mod text_layout;
pub mod when;
pub mod widget;
pub mod workspace;
