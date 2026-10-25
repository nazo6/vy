//! Focus and input method (IME) composition handling for `EditorView`.

use gpui::{
    App, Bounds, Context, EntityInputHandler, FocusHandle, Focusable, Pixels, ScrollStrategy,
    SharedString, TextRun, UTF16Selection, Window, point, px, size,
};
use vy_app::editor::{Action, Mode};
use vy_app::palette::PaletteMode;
use vy_app::text_layout::{byte_to_utf16, utf16_to_byte};

use super::EditorView;
use crate::theme::ActiveTheme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveInput {
    Editor,
    Palette,
    Ex,
    None,
}

impl EditorView {
    fn active_input_target(&self) -> ActiveInput {
        if self.palette.is_some() {
            ActiveInput::Palette
        } else if self.ex.is_some() {
            ActiveInput::Ex
        } else if self
            .active_view()
            .map(|view| view.mode() == Mode::Insert)
            .unwrap_or(false)
        {
            ActiveInput::Editor
        } else {
            ActiveInput::None
        }
    }

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
    fn accepts_text_input(&self, _window: &mut Window, _cx: &mut Context<Self>) -> bool {
        !matches!(self.active_input_target(), ActiveInput::None)
    }

    fn text_for_range(
        &mut self,
        range_utf16: std::ops::Range<usize>,
        adjusted_range: &mut Option<std::ops::Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        match self.active_input_target() {
            ActiveInput::Palette => {
                let palette = self.palette.as_ref()?;
                let full = format!("{}{}", palette.query, palette.preedit);
                let len = full.encode_utf16().count();
                let end = range_utf16.end.min(len);
                let start = range_utf16.start.min(end);
                adjusted_range.replace(start..end);
                let start_b = utf16_to_byte(&full, start as u32);
                let end_b = utf16_to_byte(&full, end as u32);
                Some(full[start_b..end_b].to_string())
            }
            ActiveInput::Ex => {
                let ex = self.ex.as_ref()?;
                let full = format!("{}{}", ex.input, ex.preedit);
                let len = full.encode_utf16().count();
                let end = range_utf16.end.min(len);
                let start = range_utf16.start.min(end);
                adjusted_range.replace(start..end);
                let start_b = utf16_to_byte(&full, start as u32);
                let end_b = utf16_to_byte(&full, end as u32);
                Some(full[start_b..end_b].to_string())
            }
            ActiveInput::Editor => {
                self.activate_active_view();
                let length = self.editor().len_utf16() + self.preedit_len_utf16();
                let end = range_utf16.end.min(length);
                let start = range_utf16.start.min(end);
                adjusted_range.replace(start..end);
                Some(self.document_text(start..end))
            }
            ActiveInput::None => None,
        }
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        match self.active_input_target() {
            ActiveInput::Palette => {
                let palette = self.palette.as_ref()?;
                let query_len = palette.query.encode_utf16().count();
                let preedit_len = palette.preedit.encode_utf16().count();
                Some(UTF16Selection {
                    range: query_len..query_len + preedit_len,
                    reversed: false,
                })
            }
            ActiveInput::Ex => {
                let ex = self.ex.as_ref()?;
                let input_len = ex.input.encode_utf16().count();
                let preedit_len = ex.preedit.encode_utf16().count();
                Some(UTF16Selection {
                    range: input_len..input_len + preedit_len,
                    reversed: false,
                })
            }
            ActiveInput::Editor => {
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
            ActiveInput::None => None,
        }
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<std::ops::Range<usize>> {
        match self.active_input_target() {
            ActiveInput::Palette => {
                let palette = self.palette.as_ref()?;
                if palette.preedit.is_empty() {
                    return None;
                }
                let start = palette.query.encode_utf16().count();
                let len = palette.preedit.encode_utf16().count();
                Some(start..start + len)
            }
            ActiveInput::Ex => {
                let ex = self.ex.as_ref()?;
                if ex.preedit.is_empty() {
                    return None;
                }
                let start = ex.input.encode_utf16().count();
                let len = ex.preedit.encode_utf16().count();
                Some(start..start + len)
            }
            ActiveInput::Editor => {
                if self.preedit_len_utf16() == 0 {
                    return None;
                }
                let caret = self.caret_utf16();
                Some(caret..caret + self.preedit_len_utf16())
            }
            ActiveInput::None => None,
        }
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.active_input_target() {
            ActiveInput::Palette => {
                if let Some(palette) = self.palette.as_mut() {
                    palette.preedit.clear();
                    cx.notify();
                }
            }
            ActiveInput::Ex => {
                if let Some(ex) = self.ex.as_mut() {
                    ex.preedit.clear();
                    cx.notify();
                }
            }
            ActiveInput::Editor => {
                if self.preedit_len_utf16() > 0 {
                    self.clear_preedit();
                    cx.notify();
                }
            }
            ActiveInput::None => {}
        }
    }

    fn replace_text_in_range(
        &mut self,
        _range_utf16: Option<std::ops::Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.active_input_target() {
            ActiveInput::Palette => {
                if let Some(palette) = self.palette.as_mut() {
                    palette.preedit.clear();
                    if !text.is_empty() {
                        palette.push_str(text);
                    }
                    self.palette_scroll_handle
                        .scroll_to_item(0, ScrollStrategy::Top);
                    cx.notify();
                }
            }
            ActiveInput::Ex => {
                if let Some(ex) = self.ex.as_mut() {
                    ex.preedit.clear();
                    if !text.is_empty() {
                        ex.push_str(text);
                    }
                    cx.notify();
                }
            }
            ActiveInput::Editor => {
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
            ActiveInput::None => {}
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range_utf16: Option<std::ops::Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<std::ops::Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.active_input_target() {
            ActiveInput::Palette => {
                if let Some(palette) = self.palette.as_mut() {
                    palette.preedit.clear();
                    palette.preedit.push_str(new_text);
                    cx.notify();
                }
            }
            ActiveInput::Ex => {
                if let Some(ex) = self.ex.as_mut() {
                    ex.preedit.clear();
                    ex.preedit.push_str(new_text);
                    cx.notify();
                }
            }
            ActiveInput::Editor => {
                self.activate_active_view();
                if self.editor().mode() != Mode::Insert {
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
            ActiveInput::None => {}
        }
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: std::ops::Range<usize>,
        element_bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let font_family = self.config.font_family();
        let font_size = px(self.config.font_size());
        let text_color = cx.theme().editor.text.into();

        match self.active_input_target() {
            ActiveInput::Palette => {
                let palette = self.palette.as_ref()?;
                let prefix = match &palette.mode {
                    PaletteMode::Approval { plugin_name, .. } => {
                        format!("Approve {plugin_name}? > ")
                    }
                    _ => "> ".to_string(),
                };
                let full_prefix = format!("{prefix}{}", palette.query);
                let mut width = px(0.);
                if !full_prefix.is_empty() {
                    let run = TextRun {
                        len: full_prefix.len(),
                        font: gpui::font(font_family),
                        color: text_color,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    let shaped = window.text_system().shape_line(
                        SharedString::from(full_prefix.clone()),
                        font_size,
                        &[run],
                        None,
                    );
                    width = shaped.x_for_index(full_prefix.len());
                }
                let palette_w =
                    px(vy_app::palette::PALETTE_MAX_WIDTH
                        .min(f32::from(element_bounds.size.width)));
                let origin_x = element_bounds.origin.x
                    + (element_bounds.size.width - palette_w) / 2.0
                    + px(16.0)
                    + width;
                let origin_y = element_bounds.origin.y + px(20.0);
                Some(Bounds::new(
                    point(origin_x, origin_y),
                    size(px(2.), px(self.line_height())),
                ))
            }
            ActiveInput::Ex => {
                let ex = self.ex.as_ref()?;
                let full_prefix = format!(":{}", ex.input);
                let mut width = px(0.);
                if !full_prefix.is_empty() {
                    let run = TextRun {
                        len: full_prefix.len(),
                        font: gpui::font(font_family),
                        color: text_color,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    let shaped = window.text_system().shape_line(
                        SharedString::from(full_prefix.clone()),
                        font_size,
                        &[run],
                        None,
                    );
                    width = shaped.x_for_index(full_prefix.len());
                }
                let origin_x = element_bounds.origin.x + px(8.0) + width;
                let origin_y = element_bounds.bottom();
                Some(Bounds::new(
                    point(origin_x, origin_y),
                    size(px(2.), px(self.line_height())),
                ))
            }
            ActiveInput::Editor => {
                self.activate_active_view();
                let document = self.active_document()?;
                let pane = self.active_pane_state()?;
                let is_wrap = pane.is_wrap(&self.config);
                let position = document.editor.position_at_utf16(range_utf16.start);
                let target_line = position.line as usize;
                let gutter_w = px(self.gutter_width(document.editor.line_count()));
                let titlebar_h = px(crate::ui::titlebar::TITLEBAR_HEIGHT);

                if !is_wrap {
                    if target_line < pane.scroll_top {
                        return None;
                    }
                    let row = target_line - pane.scroll_top;
                    if row >= pane.viewport_rows {
                        return None;
                    }
                    let line = document.editor.line(target_line)?;
                    let prefix = line.get(..utf16_to_byte(&line, position.character))?;
                    let mut width = px(0.);
                    if !prefix.is_empty() {
                        let run = TextRun {
                            len: prefix.len(),
                            font: gpui::font(font_family),
                            color: text_color,
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        };
                        let shaped = window.text_system().shape_line(
                            SharedString::from(prefix.to_string()),
                            font_size,
                            &[run],
                            None,
                        );
                        width = shaped.x_for_index(prefix.len());
                    }
                    let scroll_left_px = px(pane.scroll_left as f32 * self.char_width);
                    let origin_x = element_bounds.origin.x + gutter_w + width - scroll_left_px;
                    let origin_y =
                        element_bounds.origin.y + titlebar_h + px(row as f32 * self.line_height());
                    Some(Bounds::new(
                        point(origin_x, origin_y),
                        size(px(2.), px(self.line_height())),
                    ))
                } else {
                    if target_line < pane.scroll_top {
                        return None;
                    }
                    let cols = pane.viewport_cols.max(1);
                    let mut visual_row = 0;

                    for l in pane.scroll_top..target_line {
                        let text = document.editor.line(l).unwrap_or_default();
                        let chunks = vy_app::text_layout::chunk_line(&text, cols);
                        let count = if l == pane.scroll_top {
                            chunks.len().saturating_sub(pane.scroll_sub_line)
                        } else {
                            chunks.len()
                        };
                        visual_row += count;
                        if visual_row >= pane.viewport_rows {
                            return None;
                        }
                    }

                    let text = document.editor.line(target_line).unwrap_or_default();
                    let chunks = vy_app::text_layout::chunk_line(&text, cols);
                    let target_utf16 = position.character as usize;

                    let (chunk_idx, chunk) = chunks.iter().enumerate().find(|(idx, chk)| {
                        let is_last = *idx + 1 >= chunks.len();
                        if is_last {
                            target_utf16 >= chk.utf16_range.start
                        } else {
                            target_utf16 >= chk.utf16_range.start
                                && target_utf16 < chk.utf16_range.end
                        }
                    })?;

                    if target_line == pane.scroll_top {
                        if chunk_idx < pane.scroll_sub_line {
                            return None;
                        }
                        visual_row += chunk_idx - pane.scroll_sub_line;
                    } else {
                        visual_row += chunk_idx;
                    }

                    if visual_row >= pane.viewport_rows {
                        return None;
                    }

                    let chunk_sub_utf16 = target_utf16.saturating_sub(chunk.utf16_range.start);
                    let chunk_text = &text[chunk.byte_range.clone()];
                    let chunk_byte_offset = utf16_to_byte(chunk_text, chunk_sub_utf16 as u32);
                    let chunk_prefix = chunk_text.get(..chunk_byte_offset).unwrap_or("");

                    let mut width = px(0.);
                    if !chunk_prefix.is_empty() {
                        let run = TextRun {
                            len: chunk_prefix.len(),
                            font: gpui::font(font_family),
                            color: text_color,
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        };
                        let shaped = window.text_system().shape_line(
                            SharedString::from(chunk_prefix.to_string()),
                            font_size,
                            &[run],
                            None,
                        );
                        width = shaped.x_for_index(chunk_prefix.len());
                    }

                    let origin_x = element_bounds.origin.x + gutter_w + width;
                    let origin_y = element_bounds.origin.y
                        + titlebar_h
                        + px(visual_row as f32 * self.line_height());
                    Some(Bounds::new(
                        point(origin_x, origin_y),
                        size(px(2.), px(self.line_height())),
                    ))
                }
            }
            ActiveInput::None => None,
        }
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let active = self.active_input_target();
        if !matches!(active, ActiveInput::Editor) {
            return None;
        }

        self.activate_active_view();
        let document = self.active_document()?;
        let pane = self.active_pane_state()?;
        let is_wrap = pane.is_wrap(&self.config);
        let lh = self.line_height();
        let titlebar_h = crate::ui::titlebar::TITLEBAR_HEIGHT;
        let font_family = self.config.font_family();
        let font_size = px(self.config.font_size());
        let text_color = cx.theme().editor.text.into();
        let gutter_w = self.gutter_width(document.editor.line_count());

        let rel_y = (f32::from(point.y) - titlebar_h).max(0.0);
        let visual_row = (rel_y / lh).floor() as usize;

        if !is_wrap {
            let line_idx = pane.scroll_top + visual_row;
            if line_idx >= document.editor.line_count() {
                return Some(document.editor.len_utf16());
            }
            let line = document.editor.line(line_idx).unwrap_or_default();
            let scroll_left_px = pane.scroll_left as f32 * self.char_width;
            let rel_x = (f32::from(point.x) - gutter_w + scroll_left_px).max(0.0);

            let run = TextRun {
                len: line.len(),
                font: gpui::font(font_family),
                color: text_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let shaped = window.text_system().shape_line(
                SharedString::from(line.clone()),
                font_size,
                &[run],
                None,
            );
            let byte_idx = shaped.index_for_x(px(rel_x)).unwrap_or(0).min(line.len());
            let char_offset = byte_to_utf16(&line, byte_idx);
            let pos = vy_core::Position::new(line_idx as u32, char_offset);
            Some(document.editor.utf16_offset(pos))
        } else {
            let cols = pane.viewport_cols.max(1);
            let line_count = document.editor.line_count();
            let mut current_vrow = 0;
            let mut target_line = pane.scroll_top;
            let mut target_chunk = None;

            'outer: while target_line < line_count {
                let text = document.editor.line(target_line).unwrap_or_default();
                let chunks = vy_app::text_layout::chunk_line(&text, cols);
                let start_chk = if target_line == pane.scroll_top {
                    pane.scroll_sub_line
                } else {
                    0
                };
                for (_chk_idx, chunk) in chunks.iter().enumerate().skip(start_chk) {
                    if current_vrow == visual_row {
                        target_chunk = Some((target_line, chunk.clone()));
                        break 'outer;
                    }
                    current_vrow += 1;
                }
                target_line += 1;
            }

            let (line_idx, chunk) = target_chunk.unwrap_or_else(|| {
                let last = line_count.saturating_sub(1);
                let text = document.editor.line(last).unwrap_or_default();
                let chunks = vy_app::text_layout::chunk_line(&text, cols);
                let last_chk = chunks
                    .last()
                    .cloned()
                    .unwrap_or(vy_app::text_layout::LineChunk {
                        byte_range: 0..0,
                        utf16_range: 0..0,
                    });
                (last, last_chk)
            });

            let full_line = document.editor.line(line_idx).unwrap_or_default();
            let chunk_text = full_line.get(chunk.byte_range.clone()).unwrap_or("");
            let rel_x = (f32::from(point.x) - gutter_w).max(0.0);

            let run = TextRun {
                len: chunk_text.len(),
                font: gpui::font(font_family),
                color: text_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let shaped = window.text_system().shape_line(
                SharedString::from(chunk_text.to_string()),
                font_size,
                &[run],
                None,
            );
            let byte_in_chunk = shaped
                .index_for_x(px(rel_x))
                .unwrap_or(0)
                .min(chunk_text.len());
            let char_in_chunk = byte_to_utf16(chunk_text, byte_in_chunk);
            let total_char = chunk.utf16_range.start as u32 + char_in_chunk;
            let pos = vy_core::Position::new(line_idx as u32, total_char);
            Some(document.editor.utf16_offset(pos))
        }
    }
}
