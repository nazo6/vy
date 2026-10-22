//! Focus and input method (IME) composition handling for `EditorView`.

use gpui::{
    App, Bounds, Context, EntityInputHandler, FocusHandle, Focusable, Pixels, SharedString,
    TextRun, UTF16Selection, Window, point, px, size,
};
use vy_app::editor::{Action, Mode};
use vy_app::text_layout::utf16_to_byte;

use super::EditorView;
use crate::theme::ActiveTheme;

impl EditorView {
    /// Length of the preedit text in UTF-16 code units.
    pub(super) fn preedit_len_utf16(&self) -> usize {
        self.active_pane_state()
            .map(|pane| pane.preedit.encode_utf16().count())
            .unwrap_or_default()
    }

    /// Flat UTF-16 offset of the caret in the text sent to the core.
    pub(super) fn caret_utf16(&self) -> usize {
        self.editor().utf16_offset(self.editor().cursor())
    }

    pub(super) fn clear_preedit(&mut self) {
        if let Some(pane) = self.active_pane_state_mut() {
            pane.preedit.clear();
            pane.preedit_selection = None;
        }
    }

    /// Returns text in flat UTF-16 offsets, with the preedit spliced in at the caret.
    pub(super) fn document_text(&self, range: std::ops::Range<usize>) -> String {
        let Some(document) = self.active_document() else {
            return String::new();
        };
        let Some(pane) = self.active_pane_state() else {
            return String::new();
        };
        if pane.preedit.is_empty() {
            return document.editor.text_utf16(range);
        }

        let preedit = &pane.preedit;
        let caret = self.caret_utf16();
        let preedit_end = caret + preedit.encode_utf16().count();
        let mut text = String::new();

        if range.start < caret {
            text.push_str(
                &document
                    .editor
                    .text_utf16(range.start..range.end.min(caret)),
            );
        }

        if range.start <= preedit_end && range.end > caret {
            let from = range.start.saturating_sub(caret).min(preedit_end - caret);
            let to = range.end.saturating_sub(caret).min(preedit_end - caret);
            let from = utf16_to_byte(preedit, from as u32);
            let to = utf16_to_byte(preedit, to as u32);
            text.push_str(&preedit[from..to]);
        }

        if range.end > preedit_end {
            let start = range.start.max(preedit_end) - preedit_end;
            text.push_str(&document.editor.text_utf16(start..range.end - preedit_end));
        }

        text
    }

    pub(super) fn commit_text(&mut self, text: &str) {
        self.activate_active_view();
        let action = self.editor_mut().insert_commit(text);
        self.store_active_view();
        self.clamp_active_views();
        if let Action::Edit(transaction) = action {
            self.send_transaction(transaction);
            self.ensure_cursor_visible();
        }
    }
}

impl Focusable for EditorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for EditorView {
    fn text_for_range(
        &mut self,
        range_utf16: std::ops::Range<usize>,
        adjusted_range: &mut Option<std::ops::Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        if self.palette.is_some() {
            return None;
        }
        self.activate_active_view();
        let length = self.editor().len_utf16() + self.preedit_len_utf16();
        let end = range_utf16.end.min(length);
        let start = range_utf16.start.min(end);
        adjusted_range.replace(start..end);
        Some(self.document_text(start..end))
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        if self.palette.is_some() {
            return None;
        }
        self.activate_active_view();
        let document = self.active_document()?;
        let pane = self.active_pane_state()?;
        if pane.preedit.is_empty() {
            let selection = document.editor.selection();
            let start = document.editor.utf16_offset(selection.start);
            let end = document.editor.utf16_offset(selection.end);
            return Some(UTF16Selection {
                range: start..end.max(start),
                reversed: false,
            });
        }

        let caret = self.caret_utf16();
        let range = pane
            .preedit_selection
            .clone()
            .unwrap_or_else(|| 0..self.preedit_len_utf16());

        Some(UTF16Selection {
            range: caret + range.start..caret + range.end,
            reversed: false,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<std::ops::Range<usize>> {
        if self.palette.is_some() || self.preedit_len_utf16() == 0 {
            return None;
        }

        let caret = self.caret_utf16();
        Some(caret..caret + self.preedit_len_utf16())
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.is_some() || self.preedit_len_utf16() == 0 {
            return;
        }

        self.clear_preedit();
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        _range_utf16: Option<std::ops::Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.palette.is_some() {
            return;
        }
        if self.frame_log {
            eprintln!("vy: commit {text:?}");
        }

        // The marked text is kept outside the document, so the range only tells us what
        // the platform believes it replaces. The commit always lands at the caret.
        let had_preedit = self.preedit_len_utf16() > 0;
        self.clear_preedit();

        if text.is_empty() {
            if had_preedit {
                cx.notify();
            }
            return;
        }

        self.commit_text(text);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range_utf16: Option<std::ops::Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<std::ops::Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.palette.is_some() {
            return;
        }
        self.activate_active_view();
        if self.editor().mode() != Mode::Insert {
            // Typing in Normal or Select is a command, not text. Drop any preedit left
            // over from a mode change during composition.
            let had_preedit = self.preedit_len_utf16() > 0;
            self.clear_preedit();
            if had_preedit {
                cx.notify();
            }
            return;
        }

        if let Some(pane) = self.active_pane_state_mut() {
            pane.preedit.clear();
            pane.preedit.push_str(new_text);
            pane.preedit_selection = new_selected_range_utf16;
        }
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: std::ops::Range<usize>,
        element_bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        if self.palette.is_some() {
            return None;
        }
        self.activate_active_view();
        let document = self.active_document()?;
        let scroll_top = self.active_pane_state().map(|pane| pane.scroll_top)?;
        let position = document.editor.position_at_utf16(range_utf16.start);
        let line = document.editor.line(position.line as usize)?;
        let prefix = line.get(..utf16_to_byte(&line, position.character))?;

        let mut width = px(0.);
        if !prefix.is_empty() {
            let run = TextRun {
                len: prefix.len(),
                font: gpui::font(self.config.font_family()),
                color: cx.theme().editor.text.into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let shaped = window.text_system().shape_line(
                SharedString::from(prefix.to_string()),
                px(self.config.font_size()),
                &[run],
                None,
            );
            width = shaped.x_for_index(prefix.len());
        }

        let row = (position.line as usize).saturating_sub(scroll_top);
        let origin = element_bounds.origin
            + point(
                px(self.gutter_width(document.editor.line_count())) + width,
                px(row as f32 * self.line_height()),
            );
        Some(Bounds::new(origin, size(px(2.), px(self.line_height()))))
    }

    fn character_index_for_point(
        &mut self,
        _point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}
