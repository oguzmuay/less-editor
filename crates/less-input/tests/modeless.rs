//! Normal (modeless) mod: klasik editör davranışı (PLAN.md §2), aynı `Command` kümesi.

mod support;

use less_core::command::AppCommand;
use less_input::{EditMode, Engine};
use support::{H, modeless};

macro_rules! t {
    ($name:ident, $input:expr, $keys:expr, $want:expr) => {
        #[test]
        fn $name() {
            assert_eq!(modeless($input, $keys), $want);
        }
    };
}

t!(typing_inserts, "|", "hello", "hello|");
t!(typing_in_middle, "a|c", "b", "ab|c");
t!(enter_keeps_indent, "  a|", "<CR>b", "  a\n  b|");
t!(enter_after_brace_indents, "f {|}", "<CR>", "f {\n    |\n}");
t!(tab_inserts_spaces_to_stop, "ab|", "<Tab>", "ab  |");
t!(shift_tab_outdents_line, "    a|", "<S-Tab>", "a|");
t!(backspace, "ab|", "<BS>", "a|");
t!(backspace_at_start_noop, "|a", "<BS>", "|a");
t!(delete_forward, "|ab", "<Del>", "|b");
t!(left_right_wrap_lines_11, "a|\nb", "<Right><Right>", "a\nb|");
t!(left_wraps_back_11, "a\n|b", "<Left>", "a|\nb");
t!(up_on_first_line_goes_to_start, "ab|c", "<Up>", "|abc");
t!(down_on_last_line_goes_to_end, "a|bc", "<Down>", "abc|");
t!(down_keeps_column, "ab|c\nxyz", "<Down>", "abc\nxy|z");
t!(cmd_left_smart_home_12, "  ab|", "<D-Left>", "  |ab");
t!(cmd_left_twice_col0_12, "  ab|", "<D-Left><D-Left>", "|  ab");
t!(cmd_right_line_end_12, "|ab\nc", "<D-Right>", "ab|\nc");
t!(cmd_up_down_document_12, "a\nb|\nc", "<D-Down>", "a\nb\nc|");
t!(option_right_word_end_12, "|foo bar", "<M-Right>", "foo| bar");
t!(option_left_word_start_12, "foo bar|", "<M-Left>", "foo |bar");
t!(option_backspace_word, "foo bar|", "<M-BS>", "foo |");
t!(cmd_backspace_to_line_start, "  foo|", "<D-BS>", "  |");
t!(home_end, "a|bc", "<End>x<Home>y", "y|abcx");
t!(shift_select_then_type_replaces, "ab|cd", "<S-Right><S-Right>X", "abX|");
t!(shift_select_then_backspace, "|abc", "<S-Right><S-Right><BS>", "|c");
t!(left_collapses_selection_to_start, "a|bc", "<S-Right><S-Right><Left>", "a|bc");
t!(right_collapses_selection_to_end, "|abc", "<S-Right><Right>", "a|bc");
t!(select_all_then_type, "abc|", "<D-a>x", "x|");
t!(copy_paste_selection, "|ab", "<S-Right><D-c><Right><D-v>", "aa|b");
t!(cut_paste, "|ab", "<S-Right><D-x><Right><D-v>", "ba|");
t!(copy_empty_selection_copies_line, "|a\nb", "<D-c><Down><D-v>", "a\na\n|b");
t!(cut_empty_selection_cuts_line, "a|\nb", "<D-x>", "|b");
t!(undo_redo, "|", "ab<D-z>", "|");
t!(redo_shift_cmd_z, "|", "ab<D-z><D-Z>", "ab|");
t!(undo_typing_word_by_word, "|", "foo bar<D-z>", "foo |");
t!(alt_down_moves_line, "|a\nb", "<M-Down>", "b\n|a");
t!(cmd_slash_comments, "a|", "<D-/>", "# a|");
t!(multi_cursor_typing, "|a\nb", "<D-M-Down>x", "xa\nx|b");
t!(esc_collapses_multi_cursor, "|a\nb", "<D-M-Down><Esc>x", "a\nx|b");

#[test]
fn tab_with_multiline_selection_indents_lines() {
    let mut h = H::modeless("|a\nb");
    h.keys("<S-Down><S-Right><Tab>");
    assert_eq!(h.ed.text(), "    a\n    b");
}

#[test]
fn shift_down_selects_lines() {
    let mut h = H::modeless("|ab\ncd");
    h.keys("<S-Down>");
    assert_eq!(h.selected(), vec!["ab\n"]);
}

#[test]
fn cmd_s_writes() {
    let mut h = H::modeless("|a");
    h.keys("<D-s>");
    assert_eq!(h.apps, vec![AppCommand::Write { path: None, force: false }]);
}

#[test]
fn cmd_shortcuts_left_to_app() {
    let mut h = H::modeless("|a");
    h.keys("<D-1><D-k><D-t><C-x><F9>");
    assert_eq!(h.unhandled, 5);
    assert_eq!(h.ed.text(), "a");
}

#[test]
fn modeless_status_label() {
    assert_eq!(Engine::new(EditMode::Modeless).status().mode_label, "EDIT");
}

#[test]
fn switching_mode_keeps_buffer() {
    let mut h = H::vim("|abc");
    h.keys("x");
    for c in h.en.set_mode(EditMode::Modeless) {
        h.ed.apply(c, &mut h.s);
    }
    h.keys("Z");
    assert_eq!(h.show(), "Z|bc");
    for c in h.en.set_mode(EditMode::Vim) {
        h.ed.apply(c, &mut h.s);
    }
    h.keys("x");
    assert_eq!(h.show(), "|bc");
}

#[test]
fn set_mode_from_insert_closes_undo_group() {
    let mut h = H::vim("|a");
    h.keys("ixy");
    for c in h.en.set_mode(EditMode::Modeless) {
        h.ed.apply(c, &mut h.s);
    }
    h.keys("<D-z>");
    assert_eq!(h.ed.text(), "a");
}
