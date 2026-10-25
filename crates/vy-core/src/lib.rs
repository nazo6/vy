//! The editor core: documents, buffers, positions, and undo.
//!
//! It does not depend on rendering. It is used by headless tests and `vy-server`.

pub mod buffer;
pub mod edit;
pub mod layout;
pub mod position;
pub mod text;
pub mod uri;

pub use buffer::{Applied, Buffer, DEFAULT_UNDO_LIMIT, DocumentId, Version};
pub use edit::{EditError, TextEdit};
pub use layout::{
    Layout, LayoutError, LayoutNode, PaneId, SplitDirection, WindowId, set_boundary_ratio,
};
pub use position::{Position, Range, Selection};
pub use text::{Text, byte_to_utf16, utf16_to_byte};
pub use uri::{Uri, UriError};
