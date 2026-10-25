use super::*;
use crate::keymap::{KeyContext, Keymap, KeymapOutcome};

fn at(line: u32, character: u32) -> Position {
    Position::new(line, character)
}

/// Runs keys through the default keymap and the editor, like `vy-app` does.
struct Harness {
    editor: super::Editor,
    keymap: Keymap,
}

/// The tests below use `Editor` as the harness so they exercise the keymap too.
type Editor = Harness;

impl Harness {
    fn with_text(text: impl AsRef<str>, version: u64) -> Self {
        Self {
            editor: super::Editor::with_text(text, version),
            keymap: Keymap::load_default(Duration::from_millis(500)).expect("keymap"),
        }
    }

    fn context(&self) -> KeyContext {
        let selection = self.editor.selection();
        KeyContext {
            mode: self.editor.mode(),
            has_selection: selection.start != selection.end,
            operator_pending: self.editor.operator_pending(),
            text_object_pending: self.editor.text_object_pending(),
            ..KeyContext::default()
        }
    }

    fn handle_key(&mut self, key: Key) -> Action {
        if self.editor.mode() == Mode::Insert
            && let Key::Char(character) = key
        {
            let mut buffer = [0u8; 4];
            return self
                .editor
                .insert_commit(character.encode_utf8(&mut buffer));
        }

        match self.keymap.feed(key, &self.context()) {
            KeymapOutcome::Command { id, args, .. } => {
                let command = BuiltinCommand::parse(&id).expect("built-in command");
                self.editor.execute(command, args)
            }
            KeymapOutcome::Pending | KeymapOutcome::Ignored => Action::None,
        }
    }
}

impl std::ops::Deref for Harness {
    type Target = super::Editor;

    fn deref(&self) -> &Self::Target {
        &self.editor
    }
}

impl std::ops::DerefMut for Harness {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.editor
    }
}

fn typed(editor: &mut Harness, text: &str) -> Vec<Transaction> {
    let mut transactions = Vec::new();
    for character in text.chars() {
        if let Action::Edit(transaction) = editor.handle_key(Key::Char(character)) {
            transactions.push(transaction);
        }
    }
    transactions
}

fn last_edit(editor: &mut Harness, key: Key) -> Transaction {
    match editor.handle_key(key) {
        Action::Edit(transaction) => transaction,
        other => panic!("expected an edit: {other:?}"),
    }
}

fn press(editor: &mut Harness, keys: &str) {
    for character in keys.chars() {
        editor.handle_key(Key::Char(character));
    }
}

#[test]
fn starts_in_normal_mode_with_one_character_selected() {
    let editor = Editor::with_text("hello", 3);
    assert_eq!(editor.mode(), Mode::Normal);
    assert_eq!(editor.cursor(), at(0, 0));
    assert_eq!(editor.selection(), Range::new(at(0, 0), at(0, 1)));
    assert_eq!(editor.version(), 3);
}

#[test]
fn inserts_text_in_insert_mode() {
    let mut editor = Editor::with_text("abc", 0);
    assert_eq!(editor.handle_key(Key::Char('i')), Action::None);
    assert_eq!(editor.mode(), Mode::Insert);

    let transactions = typed(&mut editor, "XY");
    assert_eq!(editor.text(), "XYabc");
    assert_eq!(editor.cursor(), at(0, 2));
    assert_eq!(editor.version(), 2);

    assert_eq!(transactions[0].base_version, 0);
    assert_eq!(transactions[0].edits.len(), 1);
    assert_eq!(transactions[0].edits[0].text, "X");
    assert_eq!(transactions[1].base_version, 1);
}

#[test]
fn groups_consecutive_typing_into_one_transaction() {
    let mut editor = Editor::with_text("", 0);
    editor.handle_key(Key::Char('i'));
    let transactions = typed(&mut editor, "abc");

    assert_eq!(
        transactions[0].transaction_id,
        transactions[1].transaction_id
    );
    assert_eq!(
        transactions[1].transaction_id,
        transactions[2].transaction_id
    );

    // A mode change starts a new entry for the next edit.
    editor.handle_key(Key::Esc);
    editor.handle_key(Key::Char('i'));
    let next = last_edit(&mut editor, Key::Char('d'));
    assert_ne!(next.transaction_id, transactions[0].transaction_id);
}

#[test]
fn deletes_a_character_with_x() {
    let mut editor = Editor::with_text("あ😀い", 0);
    editor.handle_key(Key::Char('l'));
    // Moves one character even for a surrogate pair; the selection is 2 UTF-16 units.
    assert_eq!(editor.selection(), Range::new(at(0, 1), at(0, 3)));

    // First delete the leading character.
    let mut editor = Editor::with_text("あ😀い", 0);
    let deleted = last_edit(&mut editor, Key::Char('x'));
    assert_eq!(deleted.edits[0].range, Range::new(at(0, 0), at(0, 1)));
    assert_eq!(editor.text(), "😀い");

    // The cursor stays at the deletion point.
    assert_eq!(editor.cursor(), at(0, 0));
    assert_eq!(editor.selection(), Range::new(at(0, 0), at(0, 2)));
}

#[test]
fn selects_and_deletes_with_v_and_d() {
    let mut editor = Editor::with_text("abcdef", 0);
    editor.handle_key(Key::Char('v'));
    assert_eq!(editor.mode(), Mode::Select);

    editor.handle_key(Key::Char('l'));
    editor.handle_key(Key::Char('l'));
    assert_eq!(editor.selection(), Range::new(at(0, 0), at(0, 3)));

    let transaction = last_edit(&mut editor, Key::Char('d'));
    assert_eq!(transaction.edits[0].range, Range::new(at(0, 0), at(0, 3)));
    assert_eq!(editor.text(), "def");
    assert_eq!(editor.mode(), Mode::Normal);
    assert_eq!(editor.cursor(), at(0, 0));
}

#[test]
fn moves_by_word() {
    let mut editor = Editor::with_text("foo bar baz", 0);

    editor.handle_key(Key::Char('w'));
    assert_eq!(editor.cursor(), at(0, 4));
    editor.handle_key(Key::Char('w'));
    assert_eq!(editor.cursor(), at(0, 8));
    editor.handle_key(Key::Char('b'));
    assert_eq!(editor.cursor(), at(0, 4));
    editor.handle_key(Key::Char('e'));
    assert_eq!(editor.cursor(), at(0, 6));
}

#[test]
fn handles_gg_and_shift_g() {
    let mut editor = Editor::with_text("one\ntwo\nthree", 0);
    editor.handle_key(Key::Char('G'));
    assert_eq!(editor.cursor(), at(2, 0));

    editor.handle_key(Key::Char('g'));
    editor.handle_key(Key::Char('g'));
    assert_eq!(editor.cursor(), at(0, 0));
}

#[test]
fn moves_vertically_and_clamps_the_column() {
    let mut editor = Editor::with_text("long line\nx\nthird", 0);
    editor.handle_key(Key::Char('$'));
    assert_eq!(editor.cursor(), at(0, 8));

    editor.handle_key(Key::Char('j'));
    assert_eq!(editor.cursor(), at(1, 0));
    editor.handle_key(Key::Char('k'));
    assert_eq!(editor.cursor(), at(0, 8));
}

#[test]
fn backspace_deletes_the_previous_character() {
    let mut editor = Editor::with_text("あい", 0);
    editor.handle_key(Key::Char('a'));
    assert_eq!(editor.cursor(), at(0, 1));

    let transaction = last_edit(&mut editor, Key::Backspace);
    assert_eq!(transaction.edits[0].range, Range::new(at(0, 0), at(0, 1)));
    assert_eq!(editor.text(), "い");
    assert_eq!(editor.cursor(), at(0, 0));
}

#[test]
fn enter_inserts_a_line_break() {
    let mut editor = Editor::with_text("ab", 0);
    editor.handle_key(Key::Char('i'));
    editor.handle_key(Key::Enter);
    assert_eq!(editor.text(), "\nab");
    assert_eq!(editor.cursor(), at(1, 0));
    assert_eq!(editor.line_count(), 2);
}

#[test]
fn esc_leaves_insert_mode_and_selects_the_previous_character() {
    let mut editor = Editor::with_text("abc", 0);
    editor.handle_key(Key::Char('a'));
    editor.handle_key(Key::Esc);

    assert_eq!(editor.mode(), Mode::Normal);
    assert_eq!(editor.selection(), Range::new(at(0, 1), at(0, 2)));
}

#[test]
fn applies_history_from_the_core() {
    let mut editor = Editor::with_text("aXYc", 1);
    let batches = vec![vec![TextEdit::new(Range::new(at(0, 1), at(0, 3)), "b")]];
    editor
        .apply_history(&batches, 2, &[Selection::caret(at(0, 1))])
        .expect("history");

    assert_eq!(editor.text(), "abc");
    assert_eq!(editor.version(), 2);
    assert_eq!(editor.cursor(), at(0, 1));
}

#[test]
fn keeps_the_version_when_a_history_batch_fails() {
    let mut editor = Editor::with_text("abc", 1);
    let batches = vec![vec![TextEdit::new(Range::new(at(0, 5), at(0, 6)), "x")]];

    assert!(editor.apply_history(&batches, 2, &[]).is_err());
    assert_eq!(editor.text(), "abc");
    assert_eq!(editor.version(), 1);
}

#[test]
fn applies_script_edits_in_any_order() {
    let mut editor = Editor::with_text("one\ntwo\n", 0);
    let transaction = editor
        .apply_edits(vec![
            TextEdit::new(Range::new(at(1, 0), at(1, 3)), "TWO"),
            TextEdit::new(Range::empty(at(0, 0)), "# "),
        ])
        .expect("apply");

    assert_eq!(editor.text(), "# one\nTWO\n");
    assert_eq!(transaction.base_version, 0);
    assert_eq!(editor.version(), 1);
}

#[test]
fn reset_replaces_the_text_and_version() {
    let mut editor = Editor::with_text("old", 4);
    editor.reset("新しい\n本文", 9);

    assert_eq!(editor.text(), "新しい\n本文");
    assert_eq!(editor.version(), 9);
    assert_eq!(editor.mode(), Mode::Normal);
    assert_eq!(editor.selection(), Range::new(at(0, 0), at(0, 1)));
}

#[test]
fn reports_undo_and_redo_keys() {
    let mut editor = Editor::with_text("abc", 0);
    assert_eq!(editor.handle_key(Key::Char('u')), Action::Undo);
    assert_eq!(editor.handle_key(Key::Ctrl('r')), Action::Redo);
}

#[test]
fn inserts_committed_text_only_in_insert_mode() {
    let mut editor = Editor::with_text("ab", 0);
    assert_eq!(editor.insert_commit("x"), Action::None);
    assert_eq!(editor.text(), "ab");

    editor.handle_key(Key::Char('i'));
    let committed = editor.insert_commit("日本");
    let Action::Edit(transaction) = committed else {
        panic!("expected an edit: {committed:?}");
    };

    assert_eq!(editor.text(), "日本ab");
    assert_eq!(editor.cursor(), at(0, 2));
    assert_eq!(transaction.edits.len(), 1);
    assert_eq!(editor.version(), 1);
}

#[test]
fn converts_positions_to_flat_utf16_offsets() {
    let editor = Editor::with_text("a😀b\n日本", 7);

    assert_eq!(editor.len_utf16(), 7);
    assert_eq!(editor.utf16_offset(at(0, 3)), 3);
    assert_eq!(editor.position_at_utf16(6), at(1, 1));
    assert_eq!(editor.text_utf16(4..7), "\n日本");
}

#[test]
fn yanks_a_word_and_pastes_it() {
    let mut editor = Editor::with_text("foo bar", 0);

    press(&mut editor, "yw");
    assert_eq!(editor.register_text(), "foo ");

    press(&mut editor, "$");
    last_edit(&mut editor, Key::Char('p'));
    assert_eq!(editor.text(), "foo barfoo ");
}

#[test]
fn deletes_and_pastes_whole_lines() {
    let mut editor = Editor::with_text("one\ntwo\nthree", 0);

    press(&mut editor, "dd");
    assert_eq!(editor.text(), "two\nthree");
    assert_eq!(editor.register_text(), "one");

    last_edit(&mut editor, Key::Char('p'));
    assert_eq!(editor.text(), "two\none\nthree");
    assert_eq!(editor.cursor(), at(1, 0));

    press(&mut editor, "yy");
    assert_eq!(editor.register_text(), "one");
}

#[test]
fn deletes_text_objects() {
    let mut editor = Editor::with_text("foo bar", 0);
    press(&mut editor, "diw");
    assert_eq!(editor.text(), " bar");

    let mut editor = Editor::with_text("foo bar", 0);
    press(&mut editor, "daw");
    assert_eq!(editor.text(), "bar");

    let mut editor = Editor::with_text("say \"hello\" now", 0);
    for _ in 0..6 {
        editor.handle_key(Key::Char('l'));
    }
    press(&mut editor, "di\"");
    assert_eq!(editor.text(), "say \"\" now");
}

#[test]
fn operator_motions_use_linewise_ranges() {
    let mut editor = Editor::with_text("a\nb\nc", 0);
    press(&mut editor, "dj");
    assert_eq!(editor.text(), "c");

    let mut editor = Editor::with_text("abc\ndef", 0);
    press(&mut editor, "d$");
    assert_eq!(editor.text(), "\ndef");
}

#[test]
fn word_motions_stop_at_punctuation() {
    let mut editor = Editor::with_text("foo_bar.baz()", 0);

    press(&mut editor, "w");
    assert_eq!(editor.cursor(), at(0, 7));
    press(&mut editor, "w");
    assert_eq!(editor.cursor(), at(0, 8));
    press(&mut editor, "w");
    assert_eq!(editor.cursor(), at(0, 11));
}

#[test]
fn escape_cancels_a_pending_operator() {
    let mut editor = Editor::with_text("abc", 0);
    press(&mut editor, "d");
    assert!(editor.operator_pending());

    editor.handle_key(Key::Esc);
    assert!(!editor.operator_pending());
    assert_eq!(editor.text(), "abc");
}

#[test]
fn applies_counts_to_motions() {
    let mut editor = Editor::with_text("abcdef\nghijkl\nmnopqr", 0);

    press(&mut editor, "3l");
    assert_eq!(editor.cursor(), at(0, 3));

    press(&mut editor, "2j");
    assert_eq!(editor.cursor(), at(2, 3));

    press(&mut editor, "3gg");
    assert_eq!(editor.cursor(), at(2, 0));

    press(&mut editor, "2G");
    assert_eq!(editor.cursor(), at(1, 0));

    press(&mut editor, "1G");
    assert_eq!(editor.cursor(), at(0, 0));
}

#[test]
fn counts_delete_and_leave_the_text_in_the_register() {
    let mut editor = Editor::with_text("abcdef", 0);

    editor.handle_key(Key::Char('3'));
    let transaction = last_edit(&mut editor, Key::Char('x'));
    assert_eq!(transaction.edits.len(), 1);
    assert_eq!(editor.text(), "def");

    // `p` inserts the unnamed register after the cursor.
    last_edit(&mut editor, Key::Char('p'));
    assert_eq!(editor.text(), "dabcef");
    assert_eq!(editor.cursor(), at(0, 3));
}

#[test]
fn named_registers_keep_their_own_text() {
    let mut editor = Editor::with_text("bc\ndef", 0);

    press(&mut editor, "\"b");
    last_edit(&mut editor, Key::Char('x'));
    assert_eq!(editor.text(), "c\ndef");

    press(&mut editor, "j");
    assert_eq!(editor.cursor(), at(1, 0));

    press(&mut editor, "\"b");
    last_edit(&mut editor, Key::Char('P'));
    assert_eq!(editor.text(), "c\nbdef");
}

#[test]
fn dot_repeats_the_last_delete() {
    let mut editor = Editor::with_text("abcdef", 0);

    editor.handle_key(Key::Char('2'));
    last_edit(&mut editor, Key::Char('x'));
    assert_eq!(editor.text(), "cdef");

    last_edit(&mut editor, Key::Char('.'));
    assert_eq!(editor.text(), "ef");
}

#[test]
fn dot_replays_the_insert_session() {
    let mut editor = Editor::with_text("", 0);

    press(&mut editor, "i");
    typed(&mut editor, "ab");
    editor.handle_key(Key::Esc);
    assert_eq!(editor.text(), "ab");
    assert_eq!(editor.cursor(), at(0, 1));

    last_edit(&mut editor, Key::Char('.'));
    assert_eq!(editor.text(), "aabb");
}

#[test]
fn dot_keeps_the_first_change_when_insert_is_empty() {
    let mut editor = Editor::with_text("abc", 0);

    last_edit(&mut editor, Key::Char('x'));
    press(&mut editor, "i");
    editor.handle_key(Key::Esc);
    last_edit(&mut editor, Key::Char('.'));
    assert_eq!(editor.text(), "c");
}

#[test]
fn insert_mode_end_moves_past_the_last_character() {
    let mut editor = Editor::with_text("abc\nde", 0);

    press(&mut editor, "i");
    editor.handle_key(Key::End);
    assert_eq!(editor.cursor(), at(0, 3));

    typed(&mut editor, "X");
    assert_eq!(editor.text(), "abcX\nde");
}

#[test]
fn view_state_can_be_swapped_between_panes() {
    let mut editor = Editor::with_text("one\ntwo\nthree", 0);
    let first = editor.view_state();

    editor.set_selections(&[Selection::caret(at(2, 1))]);
    let second = editor.view_state();
    assert_eq!(second.cursor(), at(2, 1));

    editor.set_view_state(first);
    assert_eq!(editor.cursor(), at(0, 0));

    editor.set_view_state(second);
    assert_eq!(editor.cursor(), at(2, 1));
}

#[test]
fn a_fresh_pane_still_selects_one_character_in_normal_mode() {
    let mut editor = Editor::with_text("abc", 0);
    let mut state = editor.view_state();
    state.anchor = at(0, 0);
    state.head = at(0, 0);

    editor.set_view_state(state);
    assert_eq!(editor.selection(), Range::new(at(0, 0), at(0, 1)));
}

#[test]
fn clamps_a_stored_view_state_to_a_shorter_document() {
    let mut editor = Editor::with_text("one\ntwo\nthree", 0);
    editor.set_selections(&[Selection::caret(at(2, 4))]);
    let stored = editor.view_state();

    editor.reset("a", 1);
    let clamped = editor.clamp_view_state(stored);
    assert_eq!(clamped.cursor(), at(0, 1));
}

#[test]
fn gv_restores_the_last_visual_range() {
    let mut editor = Editor::with_text("one two", 0);

    press(&mut editor, "v");
    press(&mut editor, "e");
    let range = editor.selection();
    editor.set_last_visual(range);
    press(&mut editor, "Esc");

    press(&mut editor, "g");
    press(&mut editor, "v");
    assert_eq!(editor.mode(), Mode::Select);
    assert_eq!(editor.selection(), range);
}

#[test]
fn insert_at_line_end_with_shift_a() {
    let mut editor = Editor::with_text("hello\nworld", 0);
    press(&mut editor, "l");
    assert_eq!(editor.cursor(), at(0, 1));

    press(&mut editor, "A");
    assert_eq!(editor.mode(), Mode::Insert);
    assert_eq!(editor.cursor(), at(0, 5));

    typed(&mut editor, "!");
    assert_eq!(editor.text(), "hello!\nworld");
}

#[test]
fn insert_at_line_start_with_shift_i() {
    let mut editor = Editor::with_text("  hello\nworld", 0);
    press(&mut editor, "I");
    assert_eq!(editor.mode(), Mode::Insert);
    assert_eq!(editor.cursor(), at(0, 2));

    typed(&mut editor, "!");
    assert_eq!(editor.text(), "  !hello\nworld");

    // On an empty line, inserts at column 0.
    let mut editor = Editor::with_text("", 0);
    press(&mut editor, "I");
    assert_eq!(editor.cursor(), at(0, 0));
}

#[test]
fn opens_line_below_with_o_and_replays() {
    let mut editor = Editor::with_text("first\nsecond", 0);
    press(&mut editor, "o");
    assert_eq!(editor.mode(), Mode::Insert);
    assert_eq!(editor.cursor(), at(1, 0));
    assert_eq!(editor.text(), "first\n\nsecond");

    typed(&mut editor, "new");
    editor.handle_key(Key::Esc);
    assert_eq!(editor.mode(), Mode::Normal);
    assert_eq!(editor.text(), "first\nnew\nsecond");

    // Dot repeats the open-below and typed content.
    press(&mut editor, ".");
    assert_eq!(editor.text(), "first\nnew\nnew\nsecond");
}

#[test]
fn opens_line_above_with_shift_o_and_replays() {
    let mut editor = Editor::with_text("first\nsecond", 0);
    press(&mut editor, "j");
    assert_eq!(editor.cursor(), at(1, 0));

    press(&mut editor, "O");
    assert_eq!(editor.mode(), Mode::Insert);
    assert_eq!(editor.cursor(), at(1, 0));
    assert_eq!(editor.text(), "first\n\nsecond");

    typed(&mut editor, "new");
    editor.handle_key(Key::Esc);
    assert_eq!(editor.mode(), Mode::Normal);
    assert_eq!(editor.text(), "first\nnew\nsecond");

    // Dot repeats the open-above and typed content.
    press(&mut editor, ".");
    assert_eq!(editor.text(), "first\nnew\nnew\nsecond");
}

#[test]
fn select_mode_transitions_with_shift_a_and_shift_i() {
    let mut editor = Editor::with_text("  hello", 0);
    press(&mut editor, "v");
    press(&mut editor, "e");
    assert_eq!(editor.mode(), Mode::Select);

    press(&mut editor, "A");
    assert_eq!(editor.mode(), Mode::Insert);
    assert_eq!(editor.cursor(), at(0, 7));

    let mut editor = Editor::with_text("  hello", 0);
    press(&mut editor, "v");
    press(&mut editor, "e");
    press(&mut editor, "I");
    assert_eq!(editor.mode(), Mode::Insert);
    assert_eq!(editor.cursor(), at(0, 2));
}

#[test]
fn dot_repeats_open_below_with_count() {
    let mut editor = Editor::with_text("first\nsecond", 0);
    press(&mut editor, "o");
    typed(&mut editor, "x");
    editor.handle_key(Key::Esc);
    assert_eq!(editor.text(), "first\nx\nsecond");

    // 3. repeats open-below and repeats "x" 3 times
    press(&mut editor, "3");
    press(&mut editor, ".");
    assert_eq!(editor.text(), "first\nx\nxxx\nsecond");
}

#[test]
fn visual_mode_symmetric_motion_and_toggle() {
    let mut editor = Editor::with_text("abcdef", 0);
    press(&mut editor, "l");
    press(&mut editor, "l");
    assert_eq!(editor.cursor(), at(0, 2));

    // Entering visual mode selects the character under cursor
    press(&mut editor, "v");
    assert_eq!(editor.mode(), Mode::Select);
    assert_eq!(editor.cursor(), at(0, 2));
    assert_eq!(editor.selection(), Range::new(at(0, 2), at(0, 3)));

    // Moving right expands selection to the right
    press(&mut editor, "l");
    assert_eq!(editor.cursor(), at(0, 3));
    assert_eq!(editor.selection(), Range::new(at(0, 2), at(0, 4)));

    // Moving left shrinks selection back to the anchor character
    press(&mut editor, "h");
    assert_eq!(editor.cursor(), at(0, 2));
    assert_eq!(editor.selection(), Range::new(at(0, 2), at(0, 3)));

    // Moving left past the anchor expands selection to the left, keeping anchor included
    press(&mut editor, "h");
    assert_eq!(editor.cursor(), at(0, 1));
    assert_eq!(editor.selection(), Range::new(at(0, 1), at(0, 3)));

    press(&mut editor, "h");
    assert_eq!(editor.cursor(), at(0, 0));
    assert_eq!(editor.selection(), Range::new(at(0, 0), at(0, 3)));

    // Pressing v again toggles back to normal mode at the current cursor
    press(&mut editor, "v");
    assert_eq!(editor.mode(), Mode::Normal);
    assert_eq!(editor.cursor(), at(0, 0));
}

#[test]
fn visual_line_mode_selection_and_delete() {
    let mut editor = Editor::with_text("first\nsecond\nthird\n", 0);

    // Shift-V enters line-wise visual mode
    editor.handle_key(Key::Char('V'));
    assert_eq!(editor.mode(), Mode::SelectLine);
    assert_eq!(editor.selection(), Range::new(at(0, 0), at(1, 0)));

    // Moving down expands by whole lines
    press(&mut editor, "j");
    assert_eq!(editor.cursor(), at(1, 0));
    assert_eq!(editor.selection(), Range::new(at(0, 0), at(2, 0)));

    // Delete removes the selected lines and stores them linewise
    let transaction = last_edit(&mut editor, Key::Char('d'));
    assert_eq!(transaction.edits[0].range, Range::new(at(0, 0), at(2, 0)));
    assert_eq!(editor.text(), "third\n");
    assert_eq!(editor.mode(), Mode::Normal);
    assert_eq!(editor.cursor(), at(0, 0));

    // Pasting pastes linewise below current line
    editor.handle_key(Key::Char('p'));
    assert_eq!(editor.text(), "third\nfirst\nsecond\n");
}

#[test]
fn visual_block_mode_selection_and_delete() {
    let mut editor = Editor::with_text("12345\nabcde\n!@#$%\n", 0);
    press(&mut editor, "l");
    assert_eq!(editor.cursor(), at(0, 1));

    // Ctrl-V enters visual block mode
    editor.handle_key(Key::Ctrl('v'));
    assert_eq!(editor.mode(), Mode::SelectBlock);

    // Expand 1 line down and 2 characters right (cols 1..=3)
    press(&mut editor, "j");
    press(&mut editor, "l");
    press(&mut editor, "l");
    assert_eq!(editor.cursor(), at(1, 3));

    // Delete removes the rectangular block from each line
    editor.handle_key(Key::Char('d'));
    assert_eq!(editor.text(), "15\nae\n!@#$%\n");
    assert_eq!(editor.mode(), Mode::Normal);
    assert_eq!(editor.cursor(), at(0, 1));
}

#[test]
fn visual_mode_switching_between_variants() {
    let mut editor = Editor::with_text("hello\nworld", 0);

    // v -> Select
    press(&mut editor, "v");
    assert_eq!(editor.mode(), Mode::Select);

    // V -> SelectLine
    editor.handle_key(Key::Char('V'));
    assert_eq!(editor.mode(), Mode::SelectLine);

    // Ctrl-v -> SelectBlock
    editor.handle_key(Key::Ctrl('v'));
    assert_eq!(editor.mode(), Mode::SelectBlock);

    // v -> Select
    press(&mut editor, "v");
    assert_eq!(editor.mode(), Mode::Select);

    // Esc -> Normal
    editor.handle_key(Key::Esc);
    assert_eq!(editor.mode(), Mode::Normal);
}

#[test]
fn display_motions_move_by_display_line_when_wrapped() {
    let mut editor = Editor::with_text("0123456789abcdef\nsecond line", 0);
    editor.set_wrap_width(Some(10));

    // Move to character '3' (display col 3)
    press(&mut editor, "3l");
    assert_eq!(editor.cursor(), at(0, 3));

    // 'gj' moves down to chunk 1 of line 0 at display col 3 ('d' at utf16 offset 13)
    press(&mut editor, "gj");
    assert_eq!(editor.cursor(), at(0, 13));

    // 'gj' again moves to line 1 at display col 3 ('o' at utf16 offset 3)
    press(&mut editor, "gj");
    assert_eq!(editor.cursor(), at(1, 3));

    // 'gk' moves back up to chunk 1 of line 0
    press(&mut editor, "gk");
    assert_eq!(editor.cursor(), at(0, 13));

    // 'gk' moves back to chunk 0 of line 0
    press(&mut editor, "gk");
    assert_eq!(editor.cursor(), at(0, 3));
}

#[test]
fn display_motions_with_count() {
    let mut editor = Editor::with_text("0123456789abcdef\nsecond line", 0);
    editor.set_wrap_width(Some(10));

    press(&mut editor, "3l");
    assert_eq!(editor.cursor(), at(0, 3));

    // '2gj' skips the next chunk and lands on line 1
    press(&mut editor, "2gj");
    assert_eq!(editor.cursor(), at(1, 3));

    // '2gk' goes back 2 visual rows to chunk 0 of line 0
    press(&mut editor, "2gk");
    assert_eq!(editor.cursor(), at(0, 3));
}

#[test]
fn display_motions_preserve_display_col_across_short_lines() {
    let mut editor = Editor::with_text("0123456789abcdef\nxy\n0123456789", 0);
    editor.set_wrap_width(Some(10));

    press(&mut editor, "5l");
    assert_eq!(editor.cursor(), at(0, 5));

    // Down to chunk 1 of line 0 (char 'f', offset 15, display col 5)
    press(&mut editor, "gj");
    assert_eq!(editor.cursor(), at(0, 15));

    // Down to line 1 "xy", which has only 2 characters; clamps to 'y' at offset 1
    press(&mut editor, "gj");
    assert_eq!(editor.cursor(), at(1, 1));

    // Down to line 2 "0123456789", restores preferred display col 5 ('5' at offset 5)
    press(&mut editor, "gj");
    assert_eq!(editor.cursor(), at(2, 5));
}

#[test]
fn display_motions_behave_like_regular_motions_when_nowrap() {
    let mut editor = Editor::with_text("hello\nworld", 0);
    editor.set_wrap_width(None);

    press(&mut editor, "2l");
    assert_eq!(editor.cursor(), at(0, 2));

    press(&mut editor, "gj");
    assert_eq!(editor.cursor(), at(1, 2));

    press(&mut editor, "gk");
    assert_eq!(editor.cursor(), at(0, 2));
}

#[test]
fn display_motions_with_operator_delete() {
    let mut editor = Editor::with_text("0123456789abcdef\nsecond line", 0);
    editor.set_wrap_width(Some(10));

    press(&mut editor, "3l");
    assert_eq!(editor.cursor(), at(0, 3));

    // 'dgj' deletes from character '3' (offset 3) to character 'd' (offset 13)
    press(&mut editor, "dgj");
    assert_eq!(editor.text(), "012def\nsecond line");
    assert_eq!(editor.cursor(), at(0, 3));
}

#[test]
fn display_motions_with_wide_characters() {
    let mut editor = Editor::with_text("あいうえおかきくけこ", 0);
    editor.set_wrap_width(Some(10));

    // "あいうえお" is 5 chars * 2 width = 10 cols (chunk 0)
    // "かきくけこ" is 5 chars * 2 width = 10 cols (chunk 1)
    // Move to 'う' (offset 2, display col 4)
    press(&mut editor, "2l");
    assert_eq!(editor.cursor(), at(0, 2));

    // 'gj' should land on 'く' (offset 7, display col 4)
    press(&mut editor, "gj");
    assert_eq!(editor.cursor(), at(0, 7));

    // 'gk' should land back on 'う' (offset 2, display col 4)
    press(&mut editor, "gk");
    assert_eq!(editor.cursor(), at(0, 2));
}
