//! Vim motoru + editör uçtan uca: `|` imleç işareti, tuş dizisi, beklenen metin.

mod support;

use less_core::command::{AppCommand, SetValue};
use less_input::{Key, KeyEvent, Mods};
use support::{H, vim};

macro_rules! t {
    ($name:ident, $input:expr, $keys:expr, $want:expr) => {
        #[test]
        fn $name() {
            assert_eq!(vim($input, $keys), $want);
        }
    };
}

// ---- hareketler ----
t!(motion_h, "ab|cd", "hh", "|abcd");
t!(motion_h_stops_at_line_start, "ab\n|cd", "h", "ab\n|cd");
t!(motion_l_stops_at_last_char, "ab|cd", "lllll", "abc|d");
t!(motion_count_l, "|abcdef", "3l", "abc|def");
t!(motion_j_keeps_preferred_column, "abc\nde\nfgh", "lljj", "abc\nde\nfg|h");
t!(motion_k, "abc\nd|ef", "k", "a|bc\ndef");
t!(motion_j_on_last_line_stays, "a\n|b", "j", "a\n|b");
t!(motion_w, "|foo bar.baz", "w", "foo |bar.baz");
t!(motion_ww_punct, "|foo bar.baz", "ww", "foo bar|.baz");
t!(motion_big_w, "|foo bar.baz qux", "WW", "foo bar.baz |qux");
t!(motion_w_crosses_line, "fo|o\n  bar", "w", "foo\n  |bar");
t!(motion_w_stops_on_empty_line, "|foo\n\nbar", "w", "foo\n|\nbar");
t!(motion_b, "foo bar|", "b", "foo |bar");
t!(motion_big_b, "foo a.b|", "B", "foo |a.b");
t!(motion_e, "|foo bar", "e", "fo|o bar");
t!(motion_ee, "|foo bar", "ee", "foo ba|r");
t!(motion_big_e, "|a.b c", "E", "a.|b c");
t!(motion_ge, "foo b|ar", "ge", "fo|o bar");
t!(motion_zero, "  a|bc", "0", "|  abc");
t!(motion_caret, "  a|bc", "^", "  |abc");
t!(motion_dollar, "|  abc\nde", "$", "  ab|c\nde");
t!(motion_dollar_count, "|abc\nde", "2$", "abc\nd|e");
t!(motion_dollar_then_j_keeps_eol, "|abcd\nxy\nlonger", "$jj", "abcd\nxy\nlonge|r");
t!(motion_g_underscore, "|ab  ", "g_", "a|b  ");
t!(motion_gg, "a\nb\n|c", "gg", "|a\nb\nc");
t!(motion_big_g, "|a\nb\n  c", "G", "a\nb\n  |c");
t!(motion_count_g, "|a\nb\nc", "2G", "a\n|b\nc");
t!(motion_count_gg, "a\nb\n|c", "2gg", "a\n|b\nc");
t!(motion_f, "|a,b,c,d", "f,", "a|,b,c,d");
t!(motion_f_semicolon, "|a,b,c,d", "f,;", "a,b|,c,d");
t!(motion_f_comma, "|a,b,c,d", "f,;,", "a|,b,c,d");
t!(motion_count_t, "|a,b,c", "2t,", "a,|b,c");
t!(motion_t_semicolon_skips_adjacent, "|a,b,c", "t,;", "a,|b,c");
t!(motion_big_f, "a,b,c|,d", "F,", "a,b|,c,d");
t!(motion_big_t, "a,b,c|,d", "T,", "a,b,|c,d");
t!(motion_f_not_found_stays, "|abc", "fz", "|abc");
t!(motion_percent, "|(a [b] c)", "%", "(a [b] c|)");
t!(motion_percent_back, "|(a [b] c)", "%%", "|(a [b] c)");
t!(motion_percent_from_inside_line, "x |= f(a)", "%", "x = f(a|)");
t!(motion_paragraph_forward, "|a\nb\n\nc", "}", "a\nb\n|\nc");
t!(motion_paragraph_backward, "a\n\nb\n|c", "{", "a\n|\nb\nc");
t!(motion_search, "|foo bar foo", "/foo<CR>", "foo bar |foo");
t!(motion_search_n_wraps, "|foo bar foo", "/foo<CR>n", "|foo bar foo");
t!(motion_search_big_n, "|foo bar foo", "/foo<CR>N", "|foo bar foo");
t!(motion_search_backward, "foo bar |foo", "?bar<CR>", "foo |bar foo");
t!(motion_search_smartcase, "|Foo foo FOO", "/FOO<CR>", "Foo foo |FOO");
t!(motion_search_ignorecase, "|x Foo foo", "/foo<CR>", "x |Foo foo");
t!(motion_star, "|foo bar foo", "*", "foo bar |foo");
t!(motion_hash, "foo bar |foo", "#", "|foo bar foo");
t!(motion_star_whole_word, "|foo food foo", "*", "foo food |foo");
t!(motion_g_star_partial, "|foo food foo", "g*", "foo |food foo");
t!(motion_mark_backtick, "a|bc\ndef", "majll`a", "a|bc\ndef");
t!(motion_mark_quote_linewise, "  a|bc\ndef", "majll'a", "  |abc\ndef");
t!(motion_enter_next_line_first_non_blank, "|a\n  b", "<CR>", "a\n  |b");
t!(motion_minus, "  a\n|b", "-", "  |a\nb");
t!(arrow_left_wraps_to_previous_line_11, "ab\n|cd", "<Left>", "a|b\ncd");
t!(arrow_right_wraps_to_next_line_11, "a|b\ncd", "<Right>", "ab\n|cd");
t!(h_does_not_wrap_11, "ab\n|cd", "h", "ab\n|cd");
t!(cmd_left_first_non_blank_then_col0_12, "  ab|c", "<D-Left>", "  |abc");
t!(cmd_left_twice_goes_to_col0_12, "  ab|c", "<D-Left><D-Left>", "|  abc");
t!(cmd_right_line_end_12, "|abc", "<D-Right>", "ab|c");
t!(cmd_up_down_file_start_end_12, "a\n|b\nc", "<D-Down>", "a\nb\n|c");
t!(option_arrows_words_12, "|foo bar baz", "<M-Right><M-Right>", "foo bar| baz");

#[test]
fn motion_screen_top_middle_bottom() {
    let text = (0..20).map(|i| format!("l{i}")).collect::<Vec<_>>().join("\n");
    let mut h = H::vim(&text);
    h.ed.view.height = 10;
    h.ed.view.scrolloff = 0;
    h.keys("L");
    assert_eq!(h.ed.cursor_line_col().0, 9);
    h.keys("M");
    assert_eq!(h.ed.cursor_line_col().0, 4);
    h.keys("H");
    assert_eq!(h.ed.cursor_line_col().0, 0);
}

#[test]
fn motion_ctrl_d_scrolls_half_page() {
    let text = (0..40).map(|i| format!("l{i}")).collect::<Vec<_>>().join("\n");
    let mut h = H::vim(&text);
    h.ed.view.height = 10;
    h.ed.view.scrolloff = 0;
    h.keys("<C-d>");
    assert_eq!(h.ed.cursor_line_col().0, 5);
    assert_eq!(h.ed.view.top, 5);
    h.keys("<C-u>");
    assert_eq!(h.ed.cursor_line_col().0, 0);
}

#[test]
fn view_follows_cursor_with_scrolloff() {
    let text = (0..40).map(|i| format!("l{i}")).collect::<Vec<_>>().join("\n");
    let mut h = H::vim(&text);
    h.ed.view.height = 10;
    h.ed.view.scrolloff = 2;
    h.keys("20j");
    assert_eq!(h.ed.view.top, 13);
    h.keys("zt");
    assert_eq!(h.ed.view.top, 18);
    h.keys("zz");
    assert_eq!(h.ed.view.top, 15);
    h.keys("<C-e>");
    assert_eq!(h.ed.view.top, 16);
}

#[test]
fn search_not_found_reports_message_28() {
    let mut h = H::vim("|abc");
    h.keys("/zzz<CR>");
    assert_eq!(h.last_message().as_deref(), Some("Pattern not found: zzz"));
    assert_eq!(h.show(), "|abc");
}

#[test]
fn search_highlight_and_noh() {
    let mut h = H::vim("|ab ab");
    h.keys("/ab<CR>");
    assert_eq!(h.ed.search_matches(&h.s, 0, 0), vec![(0, 2), (3, 5)]);
    h.keys(":noh<CR>");
    assert!(h.ed.search_matches(&h.s, 0, 0).is_empty());
}

// ---- operatörler ----
t!(op_dw, "|foo bar", "dw", "|bar");
t!(op_dw_last_word_keeps_newline, "foo |bar\nbaz", "dw", "foo| \nbaz");
t!(op_d2w, "|a b c d", "d2w", "|c d");
t!(op_2dw, "|a b c d", "2dw", "|c d");
t!(op_counts_multiply, "|a b c d e f g", "2d3w", "|g");
t!(op_de, "|foo bar", "de", "| bar");
t!(op_db, "foo |bar", "db", "|bar");
t!(op_cw_acts_like_ce, "|foo bar", "cwxy<Esc>", "x|y bar");
t!(op_cw_on_last_char, "fo|o bar", "cwX<Esc>", "fo|X bar");
t!(op_c2w, "|a b c", "c2wX<Esc>", "|X c");
t!(op_cw_on_blank, "a|  b", "cwX<Esc>", "a|Xb");
t!(op_dd, "a\n|b\nc", "dd", "a\n|c");
t!(op_dd_last_line, "a\n|b", "dd", "|a");
t!(op_dd_only_line, "|a", "dd", "|");
t!(op_dd_first_non_blank_after, "|a\n  b", "dd", "  |b");
t!(op_3dd, "|a\nb\nc\nd", "3dd", "|d");
t!(op_dd_count_past_end, "a\n|b\nc", "5dd", "|a");
t!(op_dj, "|a\nb\nc", "dj", "|c");
t!(op_dk, "a\nb\n|c", "dk", "|a");
t!(op_dj_on_last_line_noop, "a\n|b", "dj", "a\n|b");
t!(op_x, "a|bc", "x", "a|c");
t!(op_3x, "|abcd", "3x", "|d");
t!(op_x_last_char, "ab|c", "x", "a|b");
t!(op_x_empty_line, "a\n|\nb", "x", "a\n|\nb");
t!(op_big_x, "ab|c", "X", "a|c");
t!(op_big_d, "a|bc", "D", "|a");
t!(op_big_c, "a|bc", "Cx<Esc>", "a|x");
t!(op_s, "|abc", "sX<Esc>", "|Xbc");
t!(op_cc_keeps_indent, "  |abc\nd", "ccX<Esc>", "  |X\nd");
t!(op_big_s, "  |abc", "SX<Esc>", "  |X");
t!(op_d_dollar, "a|bc", "d$", "|a");
t!(op_d_zero, "ab|c", "d0", "|c");
t!(op_d_caret, "  ab|c", "d^", "  |c");
t!(op_d_big_g, "a\n|b\nc", "dG", "|a");
t!(op_dgg, "a\nb\n|c", "dgg", "|");
t!(op_d_brace_linewise_rule, "|a\nb\n\nc", "d}", "|\nc");
t!(op_d_brace_mid_line_exclusive_to_eol, "a|a\nb\n\nc", "d}", "|a\n\nc");
t!(op_df, "|a,b", "df,", "|b");
t!(op_dt, "|ab,c", "dt,", "|,c");
t!(op_d_semicolon, "|a,b,c", "f,d;", "a|c");
t!(op_d_search, "|ab foo", "d/foo<CR>", "|foo");
t!(op_d_percent, "f|(a)b", "d%", "f|b");
t!(op_diw, "foo b|ar baz", "diw", "foo | baz");
t!(op_daw, "foo b|ar baz", "daw", "foo |baz");
t!(op_daw_last_word_takes_leading_space, "foo b|ar", "daw", "fo|o");
t!(op_ciw, "foo b|ar baz", "ciwX<Esc>", "foo |X baz");
t!(op_ci_quote, "say \"he|llo\" now", "ci\"X<Esc>", "say \"|X\" now");
t!(op_da_quote, "say \"he|llo\" now", "da\"", "say |now");
t!(op_ci_quote_before_quote, "|x = \"abc\"", "ci\"Y<Esc>", "x = \"|Y\"");
t!(op_di_paren, "f(a, |b)", "di(", "f(|)");
t!(op_da_paren, "f(a|b)", "da(", "|f");
t!(op_dib_nested, "(a (b|c) d)", "2dib", "(|)");
t!(op_ci_brace_multiline, "fn {\n    |x\n}", "ci{y<Esc>", "fn {\n    |y\n}");
t!(op_di_bracket, "[1, |2]", "di]", "[|]");
t!(op_dip, "|a\nb\n\nc", "dip", "|\nc");
t!(op_dap, "|a\nb\n\nc", "dap", "|c");
t!(op_dit, "<a>he|llo</a>", "dit", "<a>|</a>");
t!(op_dat, "x<a>he|llo</a>y", "dat", "x|y");
t!(op_indent, "|a", ">>", "    |a");
t!(op_indent_count, "|a\nb\nc", "2>>", "    |a\n    b\nc");
t!(op_indent_motion, "|a\nb", ">j", "    |a\n    b");
t!(op_outdent, "      |a", "<<", "  |a");
t!(op_indent_skips_empty_lines, "|a\n\nb", ">2j", "    |a\n\n    b");
t!(op_gu_iw, "f|oo bar", "gUiw", "|FOO bar");
t!(op_guu, "|ABC def", "guu", "|abc def");
t!(op_gu_doubled_long, "|abc", "gUgU", "|ABC");
t!(op_g_tilde_tilde, "|aBc", "g~~", "|AbC");
t!(op_gu_w, "|abc def", "gUw", "|ABC def");
t!(op_tilde, "|ab", "~", "A|b");
t!(op_tilde_count, "|abc", "3~", "AB|C");
t!(op_gcc_comments_line, "|foo", "gcc", "# |foo");
t!(op_gcc_toggles_back, "|foo", "gccgcc", "|foo");
t!(op_gc_motion, "|a\nb\nc", "gcj", "# |a\n# b\nc");
t!(op_gc_min_indent, "|  a\n    b", "gcj", "|  # a\n  #   b");
t!(op_gc_uncomment_mixed_comments_all, "|# a\nb", "gcj", "# |# a\n# b");
t!(op_equal_is_noop, "  |a", "==", "  |a");
t!(op_join, "|a\n  b", "J", "a| b");
t!(op_join_count, "|a\nb\nc", "3J", "a b| c");
t!(op_join_paren, "|f(\n)", "J", "f(|)");
t!(op_gj_no_space, "|a\n  b", "gJ", "a|  b");
t!(op_join_last_line_noop, "a\n|b", "J", "a\n|b");
t!(op_r, "|abc", "rx", "|xbc");
t!(op_r_count, "|abc", "3rx", "xx|x");
t!(op_r_count_too_long_noop, "|ab", "3rx", "|ab");
t!(op_r_enter, "a|bc", "r<CR>", "a\n|c");
t!(op_yy_p, "|a\nb", "yyp", "a\n|a\nb");
t!(op_yy_big_p, "|a\nb", "yyP", "|a\na\nb");
t!(op_yy_p_on_last_line, "a\n|b", "yyp", "a\nb\n|b");
t!(op_yw_big_p, "|foo bar", "ywP", "foo| foo bar");
t!(op_yl_p, "|ab", "ylp", "a|ab");
t!(op_3p, "|a", "yl3p", "aaa|a");
t!(op_xp_swaps_chars, "|ab", "xp", "b|a");
t!(op_ddp_swaps_lines, "|a\nb", "ddp", "b\n|a");
t!(op_yank_moves_to_start, "ab|c", "yb", "|abc");
t!(op_big_y_yanks_to_eol, "a|bc", "Y$p", "abcb|c");
t!(register_named, "|a\nb", "\"ayyj\"ap", "a\nb\n|a");
t!(register_append_upper, "|a\nb", "\"ayyj\"Ayy\"ap", "a\nb\n|a\nb");
t!(register_black_hole, "|a\nb", "yyj\"_ddp", "a\n|a");
t!(register_zero_keeps_yank, "|a\nb", "yyjdd\"0p", "a\n|a");
t!(register_one_gets_line_delete, "|a\nb\nc", "dddd\"2p", "c\n|a");
t!(register_small_delete, "|ab\ncd", "xjdd\"-p", "b|a");

#[test]
fn op_dw_on_multiple_lines_undo_restores_text() {
    let mut h = H::vim("|a b\nc");
    h.keys("3dwu");
    assert_eq!(h.ed.text(), "a b\nc");
}

// ---- geri al / yinele ----
t!(undo_x, "|abc", "xu", "|abc");
t!(undo_twice, "|abc", "xxuu", "|abc");
t!(undo_count, "|abcd", "xxx2u", "|bcd");
t!(redo, "|abc", "xu<C-r>", "|bc");
t!(undo_insert_session_is_one_step, "|", "ihello<Esc>u", "|");
t!(undo_change_and_insert_together, "|foo bar", "cwxyz<Esc>u", "|foo bar");
t!(undo_open_line, "|a", "ob<Esc>u", "|a");
t!(cmd_z_undo_in_vim_mode, "|abc", "x<D-z>", "|abc");

#[test]
fn undo_nothing_reports_message() {
    let mut h = H::vim("|a");
    h.keys("u");
    assert_eq!(h.last_message().as_deref(), Some("Already at oldest change"));
}

#[test]
fn dirty_flag_follows_undo_and_insert() {
    let mut h = H::vim("|a");
    assert!(!h.ed.buffer.is_dirty());
    h.keys("ix");
    assert!(h.ed.buffer.is_dirty());
    h.keys("<Esc>u");
    assert!(!h.ed.buffer.is_dirty());
}

// ---- insert ----
t!(insert_i, "a|b", "iX<Esc>", "a|Xb");
t!(insert_a, "a|b", "aX<Esc>", "ab|X");
t!(insert_a_on_empty_line, "|", "aX<Esc>", "|X");
t!(insert_big_a, "|ab", "AX<Esc>", "ab|X");
t!(insert_big_i, "  a|b", "IX<Esc>", "  |Xab");
t!(insert_o, "|a\nb", "oX<Esc>", "a\n|X\nb");
t!(insert_big_o, "a\n|b", "OX<Esc>", "a\n|X\nb");
t!(insert_o_keeps_indent, "    |a", "ob<Esc>", "    a\n    |b");
t!(insert_o_after_brace_indents, "fn |{", "ox<Esc>", "fn {\n    |x");
t!(insert_esc_moves_left, "|ab", "A<Esc>", "a|b");
t!(insert_count, "|", "3ix<Esc>", "xx|x");
t!(insert_count_o, "|a", "2ohi<Esc>", "a\nhi\nh|i");
t!(insert_enter_keeps_indent, "    |a", "A<CR>b<Esc>", "    a\n    |b");
t!(insert_enter_between_braces, "|{}", "a<CR>x<Esc>", "{\n    |x\n}");
t!(insert_backspace, "ab|c", "i<BS><Esc>", "|ac");
t!(insert_backspace_joins_lines, "a\n|b", "i<BS><Esc>", "|ab");
t!(insert_backspace_soft_tab, "    a\n        |x", "i<BS><Esc>", "    a\n   | x");
t!(insert_ctrl_w, "foo ba|r", "A<C-w><Esc>", "foo| ");
t!(insert_ctrl_u, "  foo|", "A<C-u><Esc>", " | ");
t!(insert_ctrl_r_register, "|foo", "\"ayiwA <C-r>a<Esc>", "foo fo|o");
t!(insert_tab_spaces, "|x", "i<Tab><Esc>", "   | x");
t!(insert_arrows_move, "|ab", "i<Right>X<Esc>", "a|Xb");
t!(insert_delete_forward, "|ab", "i<Del><Esc>", "|b");
t!(insert_ctrl_t_indents_line, "|a", "i<C-t><Esc>", "   | a");
t!(replace_mode, "|abc", "RXY<Esc>", "X|Yc");
t!(replace_mode_extends_at_eol, "|a", "RXY<Esc>", "X|Y");
t!(insert_alt_j_moves_line, "|a\nb", "i<M-j><Esc>", "b\n|a");

#[test]
fn insert_cmd_s_writes_without_leaving_insert() {
    let mut h = H::vim("|a");
    h.keys("ix<D-s>y<Esc>");
    assert_eq!(h.ed.text(), "xya");
    assert!(h.apps.contains(&AppCommand::Write { path: None, force: false }));
}

// ---- görsel mod ----
t!(visual_d, "a|bc\ndef", "vd", "a|c\ndef");
t!(visual_l_d, "ab|cd", "vld", "a|b");
t!(visual_o_swap, "a|bcde", "vllohd", "|e");
t!(visual_dollar_includes_newline, "a|b\ncd", "v$d", "a|cd");
t!(visual_line_d, "a\n|b\nc", "Vd", "a\n|c");
t!(visual_line_jd, "|a\nb\nc", "Vjd", "|c");
t!(visual_line_indent, "|a\nb", "Vj>", "    |a\n    b");
t!(visual_iw_d, "foo b|ar baz", "viwd", "foo | baz");
t!(visual_ip_switches_to_line, "|a\nb\n\nc", "vipd", "|\nc");
t!(visual_upper, "|abc", "vlU", "|ABc");
t!(visual_lower, "|ABC", "vlu", "|abC");
t!(visual_tilde, "|aBc", "vl~", "|Abc");
t!(visual_join, "|a\nb\nc", "VjjJ", "a b| c");
t!(visual_r, "|abc", "vlrx", "|xxc");
t!(visual_paste_replaces, "|x foo", "yiwwviwp", "x |x");
t!(visual_paste_line_over_line, "|a\nb", "yyjVp", "a\n|a");
t!(visual_y_moves_to_start, "ab|c", "vhhy", "|abc");
t!(visual_c, "|abc", "vlcX<Esc>", "|Xc");
t!(visual_big_d_linewise, "a|b\ncd", "vD", "|cd");
t!(visual_esc_exits, "|abc", "vl<Esc>x", "a|c");
t!(visual_count_motion, "|abcdef", "v3ld", "|ef");
t!(visual_gv_reselect, "|abc", "vl<Esc>gvd", "|c");
t!(visual_block_d, "|abc\ndef\nghi", "<C-v>jjd", "|bc\nef\nhi");
t!(visual_block_d_two_cols, "|abc\ndef", "<C-v>jld", "|c\nf");
t!(visual_block_insert, "|abc\ndef\nghi", "<C-v>jjIX<Esc>", "|Xabc\nXdef\nXghi");
t!(visual_block_append, "|abc\ndef", "<C-v>jAX<Esc>", "a|Xbc\ndXef");
t!(visual_block_change, "|abc\ndef\nghi", "<C-v>jlcZ<Esc>", "|Zc\nZf\nghi");
t!(visual_block_yank_paste, "|ab\ncd", "<C-v>jy$p", "ab|a\ncdc");
t!(visual_substitute_range, "|a\nb\nc", "Vj:s/$/!/<CR>", "a!\n|b!\nc");
t!(visual_cmd_slash_comments_selection, "|a\nb", "Vj<D-/>", "# |a\n# b");
t!(visual_switch_v_to_big_v, "a|b\ncd", "vVd", "|cd");
t!(visual_cmd_c_yanks_to_clipboard, "|ab", "vl<D-c>$p", "aba|b");

#[test]
fn visual_selection_spans() {
    let mut h = H::vim("|abc\ndef");
    h.keys("vl");
    assert_eq!(h.selected(), vec!["ab"]);
    h.keys("V");
    assert_eq!(h.selected(), vec!["abc\n"]);
    h.keys("<C-v>j");
    assert_eq!(h.selected(), vec!["ab", "de"]);
}

#[test]
fn visual_marks_set_on_exit() {
    let mut h = H::vim("a\n|b\nc");
    h.keys("Vj<Esc>");
    assert_eq!(h.ed.mark('<'), Some(2));
    assert_eq!(h.ed.mark('>'), Some(4));
}

// ---- ex komutları ----
t!(ex_substitute_first, "|a a\na", ":s/a/b/<CR>", "|b a\na");
t!(ex_substitute_global_all_lines, "|a a\na", ":%s/a/b/g<CR>", "b b\n|b");
t!(ex_substitute_groups, "|ab", ":s/\\(a\\)\\(b\\)/\\2\\1/<CR>", "|ba");
t!(ex_substitute_ampersand, "|ab", ":s/a/[&]/<CR>", "|[a]b");
t!(ex_substitute_delimiter, "|a/b", ":s#/#-#<CR>", "|a-b");
t!(ex_substitute_ignore_case_flag, "|A", ":s/a/b/i<CR>", "|b");
t!(ex_substitute_empty_uses_last_search, "|foo x", "/x<CR>:s//y/<CR>", "|foo y");
t!(ex_substitute_newline, "|a,b", ":s/,/\\r/<CR>", "a\n|b");
t!(ex_goto_line, "|a\nb\nc", ":2<CR>", "a\n|b\nc");
t!(ex_goto_last, "|a\nb\n  c", ":$<CR>", "a\nb\n  |c");
t!(ex_goto_relative, "|a\nb\nc", ":+2<CR>", "a\nb\n|c");
t!(ex_delete_line, "a\n|b\nc", ":d<CR>", "a\n|c");
t!(ex_delete_range, "a\nb\nc\nd", ":2,3d<CR>", "a\n|d");
t!(ex_yank_all_paste, "|a\nb", ":%y<CR>P", "|a\nb\na\nb");
t!(ex_join, "|a\nb", ":j<CR>", "a| b");
t!(ex_join_range, "a\nb\nc", ":1,3j<CR>", "a b| c");
t!(ex_indent, "|a", ":><CR>", "    |a");
t!(ex_visual_range_delete, "a\n|b\nc\nd", "Vj:d<CR>", "a\n|d");
t!(ex_count_prefills_range, "|a\nb\nc\nd", "2:d<CR>", "|c\nd");
t!(ex_undo, "|abc", "x:u<CR>", "|abc");
t!(ex_history_up, "|a a a", ":s/a/b/<CR>:<Up><CR>", "|b b a");
t!(ex_escape_cancels, "|a", ":d<Esc>", "|a");
t!(ex_backspace_on_empty_leaves, "|ab", ":<BS>x", "|b");

#[test]
fn ex_write_quit_commands_go_to_app_41() {
    let cases: [(&str, AppCommand); 7] = [
        (":w<CR>", AppCommand::Write { path: None, force: false }),
        (":w! a.txt<CR>", AppCommand::Write { path: Some("a.txt".into()), force: true }),
        (":q<CR>", AppCommand::Quit { force: false, all: false }),
        (":q!<CR>", AppCommand::Quit { force: true, all: false }),
        (":qa<CR>", AppCommand::Quit { force: false, all: true }),
        (":wq<CR>", AppCommand::WriteQuit { force: false, all: false, only_if_modified: false }),
        (":x<CR>", AppCommand::WriteQuit { force: false, all: false, only_if_modified: true }),
    ];
    for (keys, want) in cases {
        let mut h = H::vim("|a");
        h.keys(keys);
        assert_eq!(h.apps, vec![want], "{keys}");
    }
}

#[test]
fn ex_unknown_command_passes_through_24() {
    let mut h = H::vim("|a");
    h.keys(":Files<CR>:R my tab<CR>:e src/x.rs<CR>");
    assert_eq!(
        h.apps,
        vec![
            AppCommand::Ex("Files".into()),
            AppCommand::Ex("R my tab".into()),
            AppCommand::Edit { path: Some("src/x.rs".into()), force: false }
        ]
    );
}

#[test]
fn ex_set_options() {
    let mut h = H::vim("|\tx");
    h.keys(":set ts=2<CR>");
    assert_eq!(h.ed.buffer.tab_width, 2);
    h.keys(":set ts?<CR>");
    assert_eq!(h.last_message().as_deref(), Some("tabstop=2"));
    h.keys(":set noic<CR>");
    assert!(!h.s.search.ignore_case);
    h.keys(":set relativenumber<CR>");
    assert_eq!(h.apps.last(), Some(&AppCommand::Set { name: "relativenumber".into(), value: SetValue::On }));
}

#[test]
fn ex_status_shows_cmdline() {
    let mut h = H::vim("|a");
    h.keys(":wq");
    assert_eq!(h.en.status().cmdline.as_deref(), Some(":wq"));
    assert_eq!(h.en.status().mode_label, "COMMAND");
}

// ---- nokta tekrarı ----
t!(dot_x, "|abc", "x.", "|c");
t!(dot_dw, "|a b c", "dw.", "|c");
t!(dot_ciw, "|a b", "ciwX<Esc>w.", "X |X");
t!(dot_append, "|a\nb", "A;<Esc>j.", "a;\nb|;");
t!(dot_count_override, "|abcdef", "x3.", "|ef");
t!(dot_keeps_count, "|abcdef", "2x.", "|ef");
t!(dot_dd, "|a\nb\nc", "dd.", "|c");
t!(dot_indent, "|a", ">>.", "        |a");
t!(dot_insert, "|", "ihi<Esc>.", "hh|ii");
t!(dot_o, "|a", "ob<Esc>.", "a\nb\n|b");
t!(dot_ignores_yank, "|abc", "xyy.", "|c");
t!(dot_r, "|abc", "rxl.", "x|xc");
t!(dot_join, "|a\nb\nc", "J.", "a b| c");
t!(dot_tilde, "|abc", "~.", "AB|c");
t!(dot_paste, "|a", "ylp.", "aa|a");
t!(dot_with_register, "|a\nb", "\"add\"ap.", "b\na\n|a");

// ---- makrolar ----
t!(macro_record_replay, "|a\nb", "qaA!<Esc>jq@a", "a!\nb|!");
t!(macro_at_at, "|a\nb\nc", "qaA!<Esc>jq@a@@", "a!\nb!\nc|!");
t!(macro_count, "|a\nb\nc\nd", "qaA!<Esc>jq3@a", "a!\nb!\nc!\nd|!");
t!(macro_with_counts_inside, "|abcdef", "qa2xq@a", "|ef");

#[test]
fn macro_recording_status() {
    let mut h = H::vim("|a");
    h.keys("qb");
    assert_eq!(h.en.status().recording, Some('b'));
    h.keys("xq");
    assert_eq!(h.en.status().recording, None);
    if let less_input::Engine::Vim(v) = &h.en {
        assert_eq!(v.macro_keys('b').map(|k| k.len()), Some(1));
    }
}

// ---- durum, leader, eşlenmemiş ----
#[test]
fn status_mode_labels() {
    let mut h = H::vim("|abc");
    assert_eq!(h.en.status().mode_label, "NORMAL");
    h.keys("i");
    assert_eq!(h.en.status().mode_label, "INSERT");
    h.keys("<Esc>v");
    assert_eq!(h.en.status().mode_label, "VISUAL");
    h.keys("V");
    assert_eq!(h.en.status().mode_label, "V-LINE");
    h.keys("<C-v>");
    assert_eq!(h.en.status().mode_label, "V-BLOCK");
    h.keys("<Esc>R");
    assert_eq!(h.en.status().mode_label, "REPLACE");
}

#[test]
fn status_pending_keys() {
    let mut h = H::vim("|abc");
    h.keys("\"a2d");
    assert_eq!(h.en.status().pending, "\"a2d");
    h.keys("<Esc>");
    assert_eq!(h.en.status().pending, "");
    assert_eq!(h.ed.text(), "abc");
}

#[test]
fn leader_bindings_go_to_app() {
    let mut h = H::vim("|a");
    h.keys("<Space>ga<Space>j<Space>d");
    assert_eq!(h.en.status().pending, "<Space>d");
    h.keys("u<Space>D");
    assert_eq!(
        h.apps,
        vec![
            AppCommand::Leader("ga".into()),
            AppCommand::Leader("j".into()),
            AppCommand::Leader("du".into()),
            AppCommand::Leader("D".into())
        ]
    );
}

#[test]
fn unknown_sequences_are_unmapped() {
    let mut h = H::vim("|a");
    h.keys("gdK]d<Space>x");
    assert_eq!(
        h.apps,
        vec![
            AppCommand::Unmapped("gd".into()),
            AppCommand::Unmapped("K".into()),
            AppCommand::Unmapped("]d".into()),
            AppCommand::Unmapped("<Space>x".into())
        ]
    );
}

#[test]
fn cmd_and_function_keys_are_left_to_app() {
    let mut h = H::vim("|a");
    h.keys("<D-k><D-1><F8><D-F8>");
    assert_eq!(h.unhandled, 4);
    assert!(h.apps.is_empty());
}

#[test]
fn zz_and_zq_quit() {
    let mut h = H::vim("|a");
    h.keys("ZZZQ");
    assert_eq!(
        h.apps,
        vec![
            AppCommand::WriteQuit { force: false, all: false, only_if_modified: true },
            AppCommand::Quit { force: true, all: false }
        ]
    );
}

t!(alt_j_moves_line_down_keymap, "|a\nb\nc", "<M-j>", "b\n|a\nc");
t!(alt_k_moves_line_up_keymap, "a\n|b", "<M-k>", "|b\na");
t!(cmd_slash_comments_line, "|a", "<D-/>", "# |a");
t!(esc_in_normal_clears_highlight, "|ab ab", "/ab<CR><Esc>x", "ab |b");

#[test]
fn cmd_v_pastes_clipboard() {
    let mut h = H::vim("|a");
    h.s.clipboard.set_text("XY".into());
    h.keys("<D-v>");
    assert_eq!(h.show(), "aX|Y");
}

#[test]
fn yank_goes_to_system_clipboard_like_unnamedplus() {
    let mut h = H::vim("|foo");
    h.keys("yiw");
    assert_eq!(h.s.clipboard.get_text().as_deref(), Some("foo"));
}

#[test]
fn ctrl_bracket_is_escape() {
    let mut h = H::vim("|a");
    h.keys("ix");
    h.en.feed(KeyEvent::new(Key::Char('['), Mods::CTRL));
    assert_eq!(h.en.status().mode_label, "NORMAL");
}
