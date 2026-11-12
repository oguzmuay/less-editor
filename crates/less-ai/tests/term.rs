use std::time::{Duration, Instant};

use less_ai::term::{
    Key, KeyOptions, ModeFlags, Modifiers, MouseAction, MouseButton, MouseEvent, SelectionKind, SpawnOptions,
    TermColor, TermSize, Terminal, encode_key, encode_mouse, encode_paste,
};

fn wait_for(term: &Terminal, pred: impl Fn(&Terminal) -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        if pred(term) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

fn sh(script: &str, size: TermSize) -> Terminal {
    Terminal::spawn(SpawnOptions {
        argv: vec!["sh".into(), "-c".into(), script.into()],
        size: Some(size),
        ..Default::default()
    })
    .expect("spawn")
}

#[test]
fn pty_spawn_renders_text_colors_and_exit_27() {
    let t = sh("printf '\\033[1;31mRED\\033[0m plain \\033[38;2;1;2;3mrgb\\033[0m'; exit 3", TermSize::new(40, 5));
    assert!(wait_for(&t, |t| t.exit_status().is_some()), "process did not exit");
    let snap = t.snapshot();
    assert_eq!(snap.row_text(0), "RED plain rgb");
    let r = &snap.cells[0][0];
    assert!(r.style.bold);
    assert_eq!(r.fg, TermColor::Indexed(1));
    assert_eq!(snap.cells[0][4].fg, TermColor::DefaultFg);
    assert!(!snap.cells[0][4].style.bold);
    assert_eq!(snap.cells[0][10].fg, TermColor::Rgb(1, 2, 3));
    assert_eq!(t.exit_status().unwrap().code, 3);
    assert!(!t.is_alive());
    assert!(t.pid().is_some());
}

#[test]
fn pty_title_and_env_27() {
    let mut opts = SpawnOptions {
        argv: vec!["sh".into(), "-c".into(), "printf '\\033]0;my title\\007'; printf \"$FOO\"".into()],
        size: Some(TermSize::new(30, 3)),
        ..Default::default()
    };
    opts.env.insert("FOO".into(), "bar-value".into());
    let t = Terminal::spawn(opts).unwrap();
    assert!(wait_for(&t, |t| t.exit_status().is_some()));
    assert_eq!(t.title().as_deref(), Some("my title"));
    assert!(t.snapshot().text().contains("bar-value"));
}

#[test]
fn pty_input_echo_and_resize_27() {
    let t = sh("stty -echo; read line; printf \"got:%s\" \"$line\"; sleep 0.3", TermSize::new(20, 4));
    std::thread::sleep(Duration::from_millis(200));
    t.write(b"hello\r").unwrap();
    assert!(wait_for(&t, |t| t.snapshot().text().contains("got:hello")));
    t.resize(TermSize::new(50, 10));
    let s = t.snapshot();
    assert_eq!((s.cols, s.rows), (50, 10));
    t.kill();
    assert!(wait_for(&t, |t| t.exit_status().is_some()));
}

#[test]
fn feed_only_grid_wide_chars_and_generation_16() {
    let t = Terminal::feed_only(TermSize::new(10, 3));
    let g0 = t.generation();
    t.feed_log(b"a\nb\x1b[4mU\x1b[0m\n\xe7\x95\x8cx");
    assert!(t.generation() > g0);
    let s = t.snapshot();
    assert_eq!(s.row_text(0), "a");
    assert_eq!(s.row_text(1), "bU");
    assert!(s.cells[1][1].style.underline);
    assert!(s.cells[2][0].wide);
    assert!(s.cells[2][1].wide_spacer);
    assert_eq!(s.row_text(2), "界x");
    assert!(!t.is_alive());
}

#[test]
fn scrollback_scroll_and_selection_27() {
    let t = Terminal::feed_only(TermSize::new(10, 3));
    for i in 0..10 {
        t.feed_log(format!("line{i}\n").as_bytes());
    }
    let s = t.snapshot();
    assert_eq!(s.display_offset, 0);
    assert!(s.history_size >= 7);
    assert_eq!(s.row_text(1), "line9");
    t.scroll(2);
    let s = t.snapshot();
    assert_eq!(s.display_offset, 2);
    assert_eq!(s.row_text(0), "line6");
    assert!(!s.cursor.visible || s.cursor.row >= 2);
    t.select_start(0, 0, SelectionKind::Simple);
    t.select_update(4, 0);
    assert_eq!(t.selection_text().as_deref(), Some("line6"));
    assert!(t.snapshot().cells[0][2].selected);
    t.select_clear();
    assert!(t.selection_text().is_none());
    t.scroll_to_bottom();
    assert_eq!(t.snapshot().display_offset, 0);
}

#[test]
fn modes_bracketed_paste_and_mouse_flags_27() {
    let t = Terminal::feed_only(TermSize::new(10, 3));
    t.feed(b"\x1b[?2004h\x1b[?1000h\x1b[?1006h\x1b[?1h\x1b[?25l");
    let m = t.snapshot().mode;
    assert!(m.bracketed_paste && m.mouse_click && m.sgr_mouse && m.app_cursor);
    assert!(!t.snapshot().cursor.visible);
}

#[test]
fn key_encoding_table_12_27() {
    let o = KeyOptions { app_cursor: false, shift_enter_newline: false, mac_line_editing: true };
    let one = |k, m, o| encode_key(k, m, o).concat();
    assert_eq!(one(Key::Char('a'), Modifiers::NONE, o), b"a");
    assert_eq!(one(Key::Char('ç'), Modifiers::NONE, o), "ç".as_bytes());
    assert_eq!(one(Key::Char('c'), Modifiers::CTRL, o), [3]);
    assert_eq!(one(Key::Char('['), Modifiers::CTRL, o), [0x1b]);
    assert_eq!(one(Key::Char('x'), Modifiers::ALT, o), b"\x1bx");
    assert_eq!(one(Key::Enter, Modifiers::NONE, o), b"\r");
    assert_eq!(one(Key::Tab, Modifiers::SHIFT, o), b"\x1b[Z");
    assert_eq!(one(Key::Backspace, Modifiers::NONE, o), [0x7f]);
    assert_eq!(one(Key::Escape, Modifiers::NONE, o), [0x1b]);
    assert_eq!(one(Key::Up, Modifiers::NONE, o), b"\x1b[A");
    let app = KeyOptions { app_cursor: true, ..o };
    assert_eq!(one(Key::Up, Modifiers::NONE, app), b"\x1bOA");
    assert_eq!(one(Key::Right, Modifiers::CTRL, o), b"\x1b[1;5C");
    assert_eq!(one(Key::Left, Modifiers::SHIFT, o), b"\x1b[1;2D");
    assert_eq!(one(Key::Left, Modifiers::ALT, o), b"\x1bb");
    assert_eq!(one(Key::Right, Modifiers::ALT, o), b"\x1bf");
    assert_eq!(one(Key::Left, Modifiers::CMD, o), [0x01]);
    assert_eq!(one(Key::Right, Modifiers::CMD, o), [0x05]);
    assert!(encode_key(Key::Char('k'), Modifiers::CMD, o).is_empty());
    assert_eq!(one(Key::Home, Modifiers::NONE, o), b"\x1b[H");
    assert_eq!(one(Key::Delete, Modifiers::NONE, o), b"\x1b[3~");
    assert_eq!(one(Key::PageUp, Modifiers::SHIFT, o), b"\x1b[5;2~");
    assert_eq!(one(Key::F(1), Modifiers::NONE, o), b"\x1bOP");
    assert_eq!(one(Key::F(5), Modifiers::NONE, o), b"\x1b[15~");
    assert_eq!(one(Key::F(8), Modifiers::SHIFT, o), b"\x1b[19;2~");
    assert_eq!(one(Key::F(12), Modifiers::NONE, o), b"\x1b[24~");
}

#[test]
fn shift_enter_newline_in_claude_tabs_27() {
    let claude = KeyOptions { shift_enter_newline: true, ..KeyOptions::default() };
    assert_eq!(encode_key(Key::Enter, Modifiers::SHIFT, claude), vec![b"\\".to_vec(), b"\r".to_vec()]);
    assert_eq!(encode_key(Key::Enter, Modifiers::SHIFT, KeyOptions::default()), vec![b"\r".to_vec()]);
}

#[test]
fn paste_and_mouse_encoding_27() {
    assert_eq!(encode_paste("a\nb", false), b"a\rb");
    assert_eq!(encode_paste("a\x1b\nb", true), b"\x1b[200~a\nb\x1b[201~");
    let ev =
        MouseEvent { button: MouseButton::Left, action: MouseAction::Press, col: 4, row: 2, mods: Modifiers::NONE };
    assert_eq!(encode_mouse(ev, &ModeFlags::default()), None);
    let sgr = ModeFlags { mouse_click: true, sgr_mouse: true, ..Default::default() };
    assert_eq!(encode_mouse(ev, &sgr).unwrap(), b"\x1b[<0;5;3M");
    let rel = MouseEvent { action: MouseAction::Release, ..ev };
    assert_eq!(encode_mouse(rel, &sgr).unwrap(), b"\x1b[<0;5;3m");
    let normal = ModeFlags { mouse_click: true, ..Default::default() };
    assert_eq!(encode_mouse(ev, &normal).unwrap(), vec![0x1b, b'[', b'M', 32, 37, 35]);
    let motion = MouseEvent { action: MouseAction::Motion, ..ev };
    assert_eq!(encode_mouse(motion, &normal), None);
    let wheel = MouseEvent { button: MouseButton::WheelUp, ..ev };
    assert_eq!(encode_mouse(wheel, &sgr).unwrap(), b"\x1b[<64;5;3M");
}
