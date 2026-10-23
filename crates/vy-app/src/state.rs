//! Client-side document and pane state models.

use std::time::Duration;
use vy_core::PaneId;

use crate::editor::{Editor, ViewState};

/// Default rows drawn before measuring the window.
pub const VISIBLE_LINES: usize = 30;

/// A buffer the client draws. The core owns the authoritative text.
pub struct DocumentState {
    pub id: u64,
    pub uri: String,
    pub editor: Editor,
    /// Version when this document was last saved or opened from disk.
    pub saved_version: u64,
    /// Whether a snapshot has filled this document.
    pub loaded: bool,
}

impl DocumentState {
    pub fn new(id: u64, uri: String, text: &str, version: u64) -> Self {
        let mut editor = Editor::with_text(text, version);
        editor.set_transaction_idle(Duration::from_millis(500));
        Self {
            id,
            uri,
            editor,
            saved_version: version,
            loaded: false,
        }
    }

    /// Whether the document has unsaved edits.
    pub fn is_dirty(&self) -> bool {
        self.editor.version() != self.saved_version
    }
}

/// One pane in the client's view tree.
#[derive(Debug)]
pub struct PaneState {
    #[allow(dead_code)]
    pub id: PaneId,
    pub document_id: Option<u64>,
    /// Cursor, selection, and mode of this pane.
    pub view: ViewState,
    /// Text the input method is still converting. It never reaches the core.
    pub preedit: String,
    /// Range the input method selected inside the preedit, in UTF-16 code units.
    pub preedit_selection: Option<std::ops::Range<usize>>,
    /// First line drawn in this pane.
    pub scroll_top: usize,
    /// Rows that fit in this pane. Updated on every render.
    pub viewport_rows: usize,
}

impl PaneState {
    pub fn new(id: PaneId, document_id: Option<u64>) -> Self {
        Self {
            id,
            document_id,
            view: ViewState::default(),
            preedit: String::new(),
            preedit_selection: None,
            scroll_top: 0,
            viewport_rows: VISIBLE_LINES,
        }
    }
}
