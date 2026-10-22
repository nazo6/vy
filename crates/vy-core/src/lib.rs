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
pub use layout::{Layout, LayoutError, LayoutNode, PaneId, SplitDirection, WindowId};
pub use position::{Position, Range, Selection};
pub use text::Text;
pub use uri::{Uri, UriError};
