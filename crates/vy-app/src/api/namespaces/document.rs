//! `document` namespace.

/// Reads the text and offsets of the active document.
#[vy_api_schema::vy_api_namespace("document")]
mod api {
    use vy_api_schema::{
        DocumentGetTextParams, DocumentLineAtParams, DocumentOffsetAtParams,
        DocumentPositionAtParams,
    };
    use vy_core::Position;

    use crate::api::{Call, CallError, Reply};

    /// Returns the document text, optionally limited to a range.
    #[vy_api_schema::vy_api_method]
    fn get_text(
        call: &mut Call<'_, '_>,
        params: DocumentGetTextParams,
    ) -> Result<Reply<String>, CallError> {
        let text = match params.range {
            Some(range) => {
                let start = call.context.editor.utf16_offset(range.start);
                let end = call.context.editor.utf16_offset(range.end).max(start);
                call.context.editor.text_utf16(start..end)
            }
            None => call.context.editor.text(),
        };
        Ok(Reply::Value(text))
    }

    /// Returns the text of a line, or null when the line is out of range.
    #[vy_api_schema::vy_api_method]
    fn line_at(
        call: &mut Call<'_, '_>,
        params: DocumentLineAtParams,
    ) -> Result<Reply<Option<String>>, CallError> {
        Ok(Reply::Value(call.context.editor.line(params.line)))
    }

    /// Converts a UTF-16 offset to a position.
    #[vy_api_schema::vy_api_method]
    fn position_at(
        call: &mut Call<'_, '_>,
        params: DocumentPositionAtParams,
    ) -> Result<Reply<Position>, CallError> {
        Ok(Reply::Value(
            call.context.editor.position_at_utf16(params.offset),
        ))
    }

    /// Converts a position to a UTF-16 offset.
    #[vy_api_schema::vy_api_method]
    fn offset_at(
        call: &mut Call<'_, '_>,
        params: DocumentOffsetAtParams,
    ) -> Result<Reply<usize>, CallError> {
        Ok(Reply::Value(
            call.context.editor.utf16_offset(params.position),
        ))
    }
}

pub use self::api::{EVENTS, METHODS, NAMESPACE, dispatch};
