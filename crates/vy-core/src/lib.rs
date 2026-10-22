//! エディタのコアです。文書、バッファ、位置、アンドゥを持ちます。
//!
//! 描画には依存しません。ヘッドレスのテストと `vy-server` の両方で使います。

pub mod buffer;
pub mod edit;
pub mod position;
pub mod uri;

pub use buffer::{Applied, Buffer, DEFAULT_UNDO_LIMIT, DocumentId, Version};
pub use edit::{EditError, TextEdit};
pub use position::{Position, Range, Selection};
pub use uri::{Uri, UriError};
