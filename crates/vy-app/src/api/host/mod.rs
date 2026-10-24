//! The client implementation of the shared and client-only host traits.
//!
//! `ClientHost` owns the per-call state. Each namespace implements its trait in
//! its own file under this module.

mod commands;
mod config;
mod context;
mod document;
mod editor;
mod fs;
mod keymap;
mod log;
mod notifications;
mod process;
mod ui;
mod ui_gutter;
mod ui_hover;
mod ui_picker;
mod ui_statusline;
mod views;
mod workspace;

use vy_client_api::ClientHostApi;

use crate::api::Call;

/// Mutable client state exposed to one host call.
pub struct ClientHost<'a, 'b, 'c> {
    pub call: &'a mut Call<'b, 'c>,
}

impl vy_host_api::HostRoot for ClientHost<'_, '_, '_> {}

impl vy_host_api::HostApi for ClientHost<'_, '_, '_> {}

impl ClientHostApi for ClientHost<'_, '_, '_> {}
