//! Client implementation of the `document` host trait.

use vy_core::Position;
use vy_host_api::document::{
    DocumentApi, DocumentGetTextParams, DocumentLineAtParams, DocumentOffsetAtParams,
    DocumentPositionAtParams,
};
use vy_host_api::{CallError, Reply};

use super::ClientHost;

impl DocumentApi for ClientHost<'_, '_, '_> {
    fn get_text(&mut self, params: DocumentGetTextParams) -> Result<Reply<String>, CallError> {
        let text = match params.range {
            Some(range) => {
                let start = self.call.context.editor.utf16_offset(range.start);
                let end = self.call.context.editor.utf16_offset(range.end).max(start);
                self.call.context.editor.text_utf16(start..end)
            }
            None => self.call.context.editor.text(),
        };
        Ok(Reply::Value(text))
    }

    fn line_at(
        &mut self,
        params: DocumentLineAtParams,
    ) -> Result<Reply<Option<String>>, CallError> {
        Ok(Reply::Value(self.call.context.editor.line(params.line)))
    }

    fn position_at(
        &mut self,
        params: DocumentPositionAtParams,
    ) -> Result<Reply<Position>, CallError> {
        Ok(Reply::Value(
            self.call.context.editor.position_at_utf16(params.offset),
        ))
    }

    fn offset_at(&mut self, params: DocumentOffsetAtParams) -> Result<Reply<usize>, CallError> {
        Ok(Reply::Value(
            self.call.context.editor.utf16_offset(params.position),
        ))
    }
}
