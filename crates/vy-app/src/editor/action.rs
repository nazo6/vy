//! Editor action outcomes and transaction payloads.

use vy_core::{Selection, TextEdit};

/// One change to send to the core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    pub transaction_id: u64,
    pub base_version: u64,
    pub edits: Vec<TextEdit>,
    pub selection_before: Vec<Selection>,
    pub selection_after: Vec<Selection>,
}

/// The result of handling a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    None,
    Edit(Transaction),
    Undo,
    Redo,
}
