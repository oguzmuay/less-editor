//! Vim modunda da geçerli LESS editör kısayolları (KEYMAP.md "hareket" ve "tab ve dosya"): Cmd+oklar, Option+oklar,
//! Cmd+/, Cmd+S, Cmd+Z, Cmd+V. Diğer Cmd kısayolları uygulamaya bırakılır.

use less_core::command::{AppCommand, Command, Motion, Operator, Target};

use crate::keys::{Key, KeyEvent};

/// Tuş bir editör kısayoluysa komutları; `visual` iken hareketler seçimi genişletir, `insert` iken yapıştırma imlece.
pub fn editor_shortcut(key: KeyEvent, visual: bool, insert: bool) -> Option<Vec<Command>> {
    let m = key.mods;
    let mv = |motion: Motion| Some(vec![Command::Move { motion, count: None, extend: visual }]);
    if m.alt && !m.cmd && !m.ctrl {
        return match key.key {
            Key::Left => mv(Motion::WordLeft),
            Key::Right => mv(Motion::WordRight),
            _ => None,
        };
    }
    if !m.cmd || m.ctrl || m.alt {
        return None;
    }
    match key.key {
        Key::Left => mv(Motion::SmartHome),
        Key::Right => mv(Motion::LineEnd),
        Key::Up => mv(Motion::DocumentStart),
        Key::Down => mv(Motion::DocumentEnd),
        Key::Char('/') => {
            let target = if visual { Target::Selection } else { Target::Lines(1) };
            Some(vec![Command::Operate { op: Operator::Comment, target, register: None }])
        }
        Key::Char('s') | Key::Char('S') if !m.shift => {
            Some(vec![Command::App(AppCommand::Write { path: None, force: false })])
        }
        Key::Char('z') if !m.shift => Some(vec![Command::Undo { count: 1 }]),
        Key::Char('Z') | Key::Char('z') => Some(vec![Command::Redo { count: 1 }]),
        Key::Char('v') if insert => Some(vec![Command::PasteInsert { register: '+' }]),
        Key::Char('v') => Some(vec![Command::Paste { register: Some('+'), before: visual, count: 1 }]),
        Key::Char('c') if visual => {
            Some(vec![Command::Operate { op: Operator::Yank, target: Target::Selection, register: Some('+') }])
        }
        _ => None,
    }
}
