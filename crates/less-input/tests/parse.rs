//! Tuş gösterimi ve ex ayrıştırıcısı birim testleri.

use less_core::command::{AppCommand, Command, LineAddr, LineBase, LineRange, Operator, SetValue, Target};
use less_input::ex::parse;
use less_input::{Key, KeyEvent, Mods, notation, parse_keys};

#[test]
fn parse_keys_plain_and_special() {
    let k = parse_keys("d2w<Esc><CR><lt><Space>");
    assert_eq!(k[0], KeyEvent::char('d'));
    assert_eq!(k[3], KeyEvent::plain(Key::Esc));
    assert_eq!(k[4], KeyEvent::plain(Key::Enter));
    assert_eq!(k[5], KeyEvent::char('<'));
    assert_eq!(k[6], KeyEvent::char(' '));
    assert_eq!(k.len(), 7);
}

#[test]
fn parse_keys_modifiers() {
    assert_eq!(parse_keys("<C-r>")[0], KeyEvent::new(Key::Char('r'), Mods::CTRL));
    assert_eq!(parse_keys("<C-R>")[0], KeyEvent::new(Key::Char('r'), Mods::CTRL));
    assert_eq!(parse_keys("<D-s>")[0], KeyEvent::new(Key::Char('s'), Mods::CMD));
    assert_eq!(parse_keys("<M-j>")[0], KeyEvent::new(Key::Char('j'), Mods::ALT));
    assert_eq!(parse_keys("<A-j>")[0], KeyEvent::new(Key::Char('j'), Mods::ALT));
    assert_eq!(parse_keys("<S-Left>")[0], KeyEvent::new(Key::Left, Mods::SHIFT));
    assert_eq!(parse_keys("<F8>")[0], KeyEvent::plain(Key::F(8)));
    let cmd_shift = parse_keys("<D-S-Left>")[0];
    assert!(cmd_shift.mods.cmd && cmd_shift.mods.shift);
}

#[test]
fn parse_keys_unknown_bracket_is_literal() {
    assert_eq!(parse_keys("<foo>").len(), 5);
    assert_eq!(parse_keys("<").len(), 1);
}

#[test]
fn notation_round_trip() {
    for s in ["d2w", "<C-r>", "<D-s>", "<M-j>", "<S-Left>", "<CR>", "<Esc>", "<Space>", "<lt>", "<F8>", "<D-F8>"] {
        assert_eq!(notation(&parse_keys(s)), s);
    }
}

fn one(line: &str) -> Command {
    let mut v = parse(line).unwrap();
    assert_eq!(v.len(), 1, "{line}");
    v.remove(0)
}

#[test]
fn ex_write_variants() {
    assert_eq!(one("w"), Command::App(AppCommand::Write { path: None, force: false }));
    assert_eq!(one("write foo.txt"), Command::App(AppCommand::Write { path: Some("foo.txt".into()), force: false }));
    assert_eq!(one(":w!"), Command::App(AppCommand::Write { path: None, force: true }));
}

#[test]
fn ex_quit_variants_41() {
    assert_eq!(one("q"), Command::App(AppCommand::Quit { force: false, all: false }));
    assert_eq!(one("quit!"), Command::App(AppCommand::Quit { force: true, all: false }));
    assert_eq!(one("qa!"), Command::App(AppCommand::Quit { force: true, all: true }));
    assert_eq!(one("wqa"), Command::App(AppCommand::WriteQuit { force: false, all: true, only_if_modified: false }));
    assert_eq!(one("xa"), Command::App(AppCommand::WriteQuit { force: false, all: true, only_if_modified: true }));
    assert_eq!(one("exit"), Command::App(AppCommand::WriteQuit { force: false, all: false, only_if_modified: true }));
}

#[test]
fn ex_edit() {
    assert_eq!(one("e! x.rs"), Command::App(AppCommand::Edit { path: Some("x.rs".into()), force: true }));
    assert_eq!(one("edit"), Command::App(AppCommand::Edit { path: None, force: false }));
}

#[test]
fn ex_ranges() {
    let addr = |base, offset| LineAddr { base, offset };
    match one("%d") {
        Command::Operate { op: Operator::Delete, target: Target::Range(r), register: None } => {
            assert_eq!(r, LineRange::all())
        }
        c => panic!("{c:?}"),
    }
    match one("'<,'>y a") {
        Command::Operate { op: Operator::Yank, target: Target::Range(r), register: Some('a') } => {
            assert_eq!(r, LineRange::visual())
        }
        c => panic!("{c:?}"),
    }
    match one(".,$-1d") {
        Command::Operate { target: Target::Range(r), .. } => {
            assert_eq!(r.start, addr(LineBase::Current, 0));
            assert_eq!(r.end, addr(LineBase::Last, -1));
        }
        c => panic!("{c:?}"),
    }
    assert_eq!(one("12"), Command::GotoAddr(addr(LineBase::Number(12), 0)));
    assert_eq!(one("-"), Command::GotoAddr(addr(LineBase::Current, -1)));
    assert_eq!(one("'a+2"), Command::GotoAddr(addr(LineBase::Mark('a'), 2)));
}

#[test]
fn ex_substitute_parsing() {
    match one("%s/a\\/b/c/gi") {
        Command::Substitute { range, pattern, replacement, global, ignore_case } => {
            assert_eq!(range, LineRange::all());
            assert_eq!(pattern, "a/b");
            assert_eq!(replacement, "c");
            assert!(global);
            assert_eq!(ignore_case, Some(true));
        }
        c => panic!("{c:?}"),
    }
    match one("s/\\(x\\)/\\1/") {
        Command::Substitute { pattern, replacement, global, .. } => {
            assert_eq!(pattern, "\\(x\\)");
            assert_eq!(replacement, "\\1");
            assert!(!global);
        }
        c => panic!("{c:?}"),
    }
    assert!(parse("s").is_err());
}

#[test]
fn ex_set_values() {
    let cmds = parse("set ts=2 noic hls! sw? number").unwrap();
    let want = [
        ("ts", SetValue::Value("2".into())),
        ("ic", SetValue::Off),
        ("hls", SetValue::Toggle),
        ("sw", SetValue::Query),
        ("number", SetValue::On),
    ];
    for (c, (n, v)) in cmds.iter().zip(want) {
        assert_eq!(*c, Command::Set { name: n.into(), value: v });
    }
}

#[test]
fn ex_passthrough_keeps_text_24() {
    assert_eq!(one("GitHistory"), Command::App(AppCommand::Ex("GitHistory".into())));
    assert_eq!(one("Theme intellij-dark"), Command::App(AppCommand::Ex("Theme intellij-dark".into())));
    assert_eq!(one("UIReset"), Command::App(AppCommand::Ex("UIReset".into())));
    assert_eq!(one("wa"), Command::App(AppCommand::Ex("wa".into())));
}

#[test]
fn ex_misc() {
    assert_eq!(one("noh"), Command::ClearSearchHighlight);
    assert_eq!(one("nohlsearch"), Command::ClearSearchHighlight);
    assert_eq!(one("u"), Command::Undo { count: 1 });
    assert_eq!(one("redo"), Command::Redo { count: 1 });
    assert_eq!(parse(">>").unwrap().len(), 2);
    assert!(parse("").unwrap().is_empty());
    assert_eq!(parse("j!").unwrap()[1], Command::Join { count: 2, spaces: false });
}
