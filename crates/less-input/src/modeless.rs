//! Normal (modeless) mod: klasik editör. Yazılan karakter eklenir, Shift+ok seçer, Cmd+C/V/X/Z/A (PLAN.md §2).

use less_core::command::{AppCommand, Command, Motion, Operator, Target};

use crate::keys::{Key, KeyEvent};
use crate::{Output, Status};

#[derive(Clone, Debug, Default)]
pub struct ModelessEngine;

impl ModelessEngine {
    pub fn status(&self) -> Status {
        Status { mode_label: "EDIT", pending: String::new(), recording: None, cmdline: None }
    }

    pub fn feed(&mut self, key: KeyEvent) -> Output {
        match map(key) {
            Some(cmds) => Output::handled(cmds),
            None => Output::unhandled(),
        }
    }
}

fn map(key: KeyEvent) -> Option<Vec<Command>> {
    let m = key.mods;
    let extend = m.shift;
    let mv = |motion: Motion| Some(vec![Command::Move { motion, count: None, extend }]);
    let op =
        |op: Operator, target: Target, register: Option<char>| Some(vec![Command::Operate { op, target, register }]);
    if m.ctrl {
        return None;
    }
    if m.cmd && m.alt {
        return match key.key {
            Key::Up => Some(vec![Command::AddCursor { below: false }]),
            Key::Down => Some(vec![Command::AddCursor { below: true }]),
            _ => None,
        };
    }
    if m.cmd {
        return match key.key {
            Key::Left => mv(Motion::SmartHome),
            Key::Right => mv(Motion::LineEnd),
            Key::Up => mv(Motion::DocumentStart),
            Key::Down => mv(Motion::DocumentEnd),
            Key::Backspace => Some(vec![Command::DeleteToLineStart]),
            Key::Char(c) => match (c.to_ascii_lowercase(), m.shift || c.is_ascii_uppercase()) {
                ('a', false) => Some(vec![Command::SelectAll]),
                ('c', false) => op(Operator::Yank, Target::SelectionOrLine, Some('+')),
                ('x', false) => op(Operator::Delete, Target::SelectionOrLine, Some('+')),
                ('v', false) => Some(vec![Command::Paste { register: Some('+'), before: true, count: 1 }]),
                ('z', false) => Some(vec![Command::Undo { count: 1 }]),
                ('z', true) => Some(vec![Command::Redo { count: 1 }]),
                ('/', _) => op(Operator::Comment, Target::Selection, None),
                ('s', false) => Some(vec![Command::App(AppCommand::Write { path: None, force: false })]),
                _ => None,
            },
            _ => None,
        };
    }
    if m.alt {
        return match key.key {
            Key::Left => mv(Motion::WordLeft),
            Key::Right => mv(Motion::WordRight),
            Key::Up => Some(vec![Command::MoveLines { down: false }]),
            Key::Down => Some(vec![Command::MoveLines { down: true }]),
            Key::Backspace => Some(vec![Command::DeleteWordBackward]),
            Key::Char(c) => Some(vec![Command::InsertText(c.to_string())]),
            _ => None,
        };
    }
    match key.key {
        Key::Char(c) => Some(vec![Command::InsertText(c.to_string())]),
        Key::Enter => Some(vec![Command::InsertNewline]),
        Key::Tab if m.shift => Some(vec![Command::Outdent]),
        Key::Tab => Some(vec![Command::InsertTab]),
        Key::Backspace => Some(vec![Command::Backspace]),
        Key::Delete => Some(vec![Command::DeleteForward]),
        Key::Esc => Some(vec![Command::CollapseSelection]),
        Key::Left => mv(Motion::LeftWrap),
        Key::Right => mv(Motion::RightWrap),
        Key::Up => mv(Motion::Up),
        Key::Down => mv(Motion::Down),
        Key::Home => mv(Motion::LineStart),
        Key::End => mv(Motion::LineEnd),
        Key::PageUp => mv(Motion::PageUp),
        Key::PageDown => mv(Motion::PageDown),
        Key::F(_) => None,
    }
}
