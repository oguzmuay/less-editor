//! less-core birim testleri: buffer G/Ç, geçmiş, register'lar, arama çevirisi, seçim, metin yardımcıları.

use less_core::buffer::{Buffer, CommentToken, Indent, LineEnding, detect_indent};
use less_core::command::{Command, Motion, Operator, Target};
use less_core::document::LoadError;
use less_core::editor::{Editor, Session};
use less_core::registers::{MemoryClipboard, RegisterContent, RegisterKind, Registers};
use less_core::search::{compile, find, vim_replacement, vim_to_regex};
use less_core::selection::{Range, Selection};
use less_core::text;
use ropey::Rope;

fn tmp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("less-rs-core-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

#[test]
fn buffer_crlf_round_trip() {
    let p = tmp("crlf.txt");
    std::fs::write(&p, "a\r\nb\r\n").unwrap();
    let mut b = Buffer::load(&p).unwrap();
    assert_eq!(b.line_ending, LineEnding::CrLf);
    assert_eq!(b.text(), "a\nb");
    assert!(b.final_newline);
    b.save().unwrap();
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "a\r\nb\r\n");
}

#[test]
fn buffer_keeps_missing_final_newline() {
    let p = tmp("nofinal.txt");
    std::fs::write(&p, "a\nb").unwrap();
    let mut b = Buffer::load(&p).unwrap();
    assert!(!b.final_newline);
    b.save().unwrap();
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "a\nb");
}

#[test]
fn buffer_rejects_binary_and_large() {
    let p = tmp("bin.dat");
    std::fs::write(&p, [0u8, 1, 2]).unwrap();
    assert_eq!(Buffer::load(&p).unwrap_err(), LoadError::Binary);
    let big = tmp("big.txt");
    let f = std::fs::File::create(&big).unwrap();
    f.set_len(less_core::document::MAX_BYTES + 1).unwrap();
    assert!(matches!(Buffer::load(&big).unwrap_err(), LoadError::TooLarge(_)));
    let _ = std::fs::remove_file(&big);
}

#[test]
fn buffer_lossy_utf8() {
    let p = tmp("lossy.txt");
    std::fs::write(&p, [b'a', 0xff, b'b']).unwrap();
    assert_eq!(Buffer::load(&p).unwrap().text(), "a\u{fffd}b");
}

#[test]
fn indent_detection() {
    assert_eq!(detect_indent("a\n  b\n    c\n  d"), Indent::Spaces(2));
    assert_eq!(detect_indent("a\n\tb\n\tc"), Indent::Tabs);
    assert_eq!(detect_indent("a\nb"), Indent::Spaces(4));
    assert_eq!(detect_indent("x\n    y\n        z"), Indent::Spaces(4));
}

#[test]
fn comment_token_by_extension() {
    use std::path::Path;
    assert_eq!(CommentToken::for_path(Path::new("A.java")).line.as_deref(), Some("//"));
    assert_eq!(CommentToken::for_path(Path::new("x.lua")).line.as_deref(), Some("--"));
    assert_eq!(CommentToken::for_path(Path::new("Dockerfile")).line.as_deref(), Some("#"));
    assert_eq!(CommentToken::for_path(Path::new("a.css")).block, Some(("/*".into(), "*/".into())));
}

#[test]
fn block_comment_toggle() {
    let mut e = Editor::from_text("a {}");
    e.buffer.comment = CommentToken::block("/*", "*/");
    let mut s = Session::default();
    let gcc = || Command::Operate { op: Operator::Comment, target: Target::Lines(1), register: None };
    e.apply(gcc(), &mut s);
    assert_eq!(e.text(), "/* a {} */");
    e.apply(gcc(), &mut s);
    assert_eq!(e.text(), "a {}");
}

#[test]
fn line_comment_with_java_token() {
    let mut e = Editor::from_text("  int x;");
    e.buffer.comment = CommentToken::line("//");
    let mut s = Session::default();
    e.apply(Command::Operate { op: Operator::Comment, target: Target::Lines(1), register: None }, &mut s);
    assert_eq!(e.text(), "  // int x;");
}

#[test]
fn dirty_after_save_and_undo() {
    let p = tmp("dirty.txt");
    std::fs::write(&p, "abc\n").unwrap();
    let mut e = Editor::load(&p).unwrap();
    let mut s = Session::default();
    let x = || Command::Operate {
        op: Operator::Delete,
        target: Target::Motion { motion: Motion::Right, count: None },
        register: None,
    };
    e.apply(x(), &mut s);
    assert!(e.buffer.is_dirty());
    e.save().unwrap();
    assert!(!e.buffer.is_dirty());
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "bc\n");
    e.apply(Command::Undo { count: 1 }, &mut s);
    assert!(e.buffer.is_dirty());
    e.apply(Command::Redo { count: 1 }, &mut s);
    assert!(!e.buffer.is_dirty());
}

#[test]
fn save_inside_insert_session_checkpoints() {
    let p = tmp("insert-save.txt");
    std::fs::write(&p, "").unwrap();
    let mut e = Editor::load(&p).unwrap();
    let mut s = Session::default();
    e.apply(Command::BeginUndoGroup, &mut s);
    e.apply(Command::InsertText("a".into()), &mut s);
    e.save().unwrap();
    assert!(!e.buffer.is_dirty());
    e.apply(Command::InsertText("b".into()), &mut s);
    e.apply(Command::EndUndoGroup, &mut s);
    assert!(e.buffer.is_dirty());
    e.apply(Command::Undo { count: 1 }, &mut s);
    assert_eq!(e.text(), "a");
    assert!(!e.buffer.is_dirty());
}

#[test]
fn registers_numbered_shift_and_small_delete() {
    let mut r = Registers::default();
    r.use_system_clipboard = false;
    let mut cb = MemoryClipboard::default();
    r.store(None, RegisterContent::new("one\n", RegisterKind::Line), true, &mut cb);
    r.store(None, RegisterContent::new("two\n", RegisterKind::Line), true, &mut cb);
    r.store(None, RegisterContent::new("w", RegisterKind::Char), true, &mut cb);
    assert_eq!(r.get(Some('1'), &mut cb).unwrap().text, "two\n");
    assert_eq!(r.get(Some('2'), &mut cb).unwrap().text, "one\n");
    assert_eq!(r.get(Some('-'), &mut cb).unwrap().text, "w");
    assert_eq!(r.get(None, &mut cb).unwrap().text, "w");
    assert!(cb.text.is_none());
}

#[test]
fn registers_clipboard_keeps_line_kind() {
    let mut r = Registers::default();
    let mut cb = MemoryClipboard::default();
    r.store(None, RegisterContent::new("x\n", RegisterKind::Line), false, &mut cb);
    assert_eq!(cb.text.as_deref(), Some("x\n"));
    assert_eq!(r.get(None, &mut cb).unwrap().kind, RegisterKind::Line);
    cb.text = Some("external".into());
    assert_eq!(r.get(None, &mut cb).unwrap(), RegisterContent::new("external", RegisterKind::Char));
}

#[test]
fn registers_black_hole_and_append() {
    let mut r = Registers::default();
    let mut cb = MemoryClipboard::default();
    r.store(Some('_'), RegisterContent::new("gone", RegisterKind::Char), true, &mut cb);
    assert!(cb.text.is_none());
    r.store(Some('a'), RegisterContent::new("x", RegisterKind::Char), false, &mut cb);
    r.store(Some('A'), RegisterContent::new("y", RegisterKind::Char), false, &mut cb);
    assert_eq!(r.get(Some('a'), &mut cb).unwrap().text, "xy");
}

#[test]
fn vim_regex_translation() {
    assert_eq!(vim_to_regex("\\(a\\|b\\)\\+"), "(a|b)+");
    assert_eq!(vim_to_regex("(a)"), "\\(a\\)");
    assert_eq!(vim_to_regex("\\<foo\\>"), "\\bfoo\\b");
    assert_eq!(vim_to_regex("a\\{2,3}"), "a{2,3}");
    assert_eq!(vim_to_regex("\\v(a|b)+"), "(a|b)+");
    assert_eq!(vim_replacement("&-\\1\\n$"), "${0}-${1}\n$$");
}

#[test]
fn search_case_rules() {
    assert!(compile("foo", true, true).unwrap().is_match("FOO"));
    assert!(!compile("Foo", true, true).unwrap().is_match("foo"));
    assert!(compile("Foo\\c", true, true).unwrap().is_match("foo"));
    assert!(!compile("foo\\C", true, true).unwrap().is_match("FOO"));
    assert!(compile("a(", true, true).is_some());
}

#[test]
fn search_find_wraps() {
    let re = compile("ab", false, false).unwrap();
    assert_eq!(find(&re, "ab ab", 0, true, true), Some((3, 5)));
    assert_eq!(find(&re, "ab ab", 3, true, true), Some((0, 2)));
    assert_eq!(find(&re, "ab ab", 3, true, false), None);
    assert_eq!(find(&re, "ab ab", 3, false, true), Some((0, 2)));
}

#[test]
fn selection_normalize_merges_overlaps() {
    let mut s = Selection::new(vec![Range::new(5, 8), Range::new(0, 2), Range::new(6, 10)], 2);
    s.normalize();
    assert_eq!(s.ranges(), &[Range::new(0, 2), Range::new(5, 10)]);
    assert_eq!(s.primary(), Range::new(5, 10));
}

#[test]
fn text_vcol_with_tabs() {
    let r = Rope::from_str("\tab\nx");
    assert_eq!(text::vcol(&r, 1, 4), 4);
    assert_eq!(text::pos_at_vcol(&r, 0, 5, 4, false), 2);
    assert_eq!(text::pos_at_vcol(&r, 0, 99, 4, true), 3);
    assert_eq!(text::pos_at_vcol(&r, 0, 99, 4, false), 2);
    assert_eq!(text::line_last_char(&r, 1), 4);
    assert_eq!(text::first_non_blank(&r, 0), 1);
}

#[test]
fn multi_cursor_delete_word_backward() {
    let mut e = Editor::from_text("foo\nbar");
    let mut s = Session::default();
    e.apply(Command::SetCursorMode(less_core::command::CursorMode::Bar), &mut s);
    e.set_selection(Selection::new(vec![Range::point(3), Range::point(7)], 0));
    e.apply(Command::DeleteWordBackward, &mut s);
    assert_eq!(e.text(), "\n");
    assert_eq!(e.selection().len(), 2);
}

#[test]
fn marks_follow_edits() {
    let mut e = Editor::from_text("abc def");
    let mut s = Session::default();
    e.set_selection(Selection::point(4));
    e.apply(Command::SetMark('a'), &mut s);
    e.set_selection(Selection::point(0));
    e.apply(Command::InsertText("xx".into()), &mut s);
    assert_eq!(e.mark('a'), Some(6));
}

#[test]
fn unknown_set_option_goes_to_app() {
    let mut e = Editor::default();
    let mut s = Session::default();
    let out = e.apply(Command::Set { name: "wrap".into(), value: less_core::command::SetValue::Off }, &mut s);
    assert!(matches!(out, Some(less_core::command::AppCommand::Set { .. })));
}

#[test]
fn select_next_match_adds_ranges_and_wraps() {
    let mut e = Editor::from_text("foo bar foo\nfoo");
    let mut s = Session::default();
    e.apply(Command::SetCursorMode(less_core::command::CursorMode::Bar), &mut s);
    e.set_selection(Selection::single(Range::new(8, 11)));
    assert!(e.select_next_match());
    assert!(e.select_next_match(), "wraps to the first foo");
    assert_eq!(e.selection_spans(), vec![(0, 3), (8, 11), (12, 15)]);
    assert!(!e.select_next_match(), "every foo is selected");
    e.set_selection(Selection::point(4));
    assert!(!e.select_next_match(), "empty selection");
}

#[test]
fn select_next_match_in_visual_mode_keeps_inclusive_ends() {
    let mut e = Editor::from_text("ab x ab");
    let mut s = Session::default();
    e.apply(Command::SetVisual(Some(less_core::command::VisualKind::Char)), &mut s);
    e.set_selection(Selection::single(Range::new(0, 1)));
    assert!(e.select_next_match());
    assert_eq!(e.selection_spans(), vec![(0, 2), (5, 7)]);
}
