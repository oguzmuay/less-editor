//! Vim modal motoru: Normal / Insert / Replace / Visual / Command. Her tuşta bekleyen tuş dizisi baştan ayrıştırılır
//! (`"a3d2w` gibi); tamamlanınca `Command`'lar üretilir. `.` son değişikliğin tuşlarını, makrolar kayıtlı tuşları
//! yeniden besler.

use std::collections::HashMap;

use less_core::command::{
    AppCommand, Command, CursorMode, Motion, Operator, ScrollAnchor, Target, TextObject, VisualKind,
};

use crate::common;
use crate::ex;
use crate::keys::{Key, KeyEvent, notation};
use crate::{Output, Status};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VimMode {
    Normal,
    Insert,
    Replace,
    Visual(VisualKind),
    /// `:` / `/` / `?` komut satırı.
    Command,
}

/// `.` tekrarı için son değişiklik: register, sayaç ve sayaçsız tuşlar (insert oturumu `<Esc>` dahil).
#[derive(Clone, Debug, Default)]
struct DotRecord {
    register: Option<char>,
    count: Option<usize>,
    body: Vec<KeyEvent>,
}

/// Ayrıştırma sonrası motorun yapacağı mod değişikliği / özel iş.
#[derive(Clone, Debug, PartialEq)]
enum Then {
    Stay,
    Normal,
    /// Insert'e geç; sayaç > 1 ise `<Esc>`'te `prefix` + yazılanlar sayaç kadar tekrarlanır.
    Insert {
        count: usize,
        prefix: Vec<Command>,
        replace: bool,
    },
    Visual(VisualKind),
    Cmdline {
        kind: char,
        text: String,
        op: Option<(Operator, Option<char>, Option<usize>)>,
    },
    Dot,
    Record(char),
    StopRecord,
    Macro(char),
}

#[derive(Clone, Debug)]
struct Action {
    cmds: Vec<Command>,
    then: Then,
    repeat: bool,
}

impl Action {
    fn new(cmds: Vec<Command>) -> Self {
        Self { cmds, then: Then::Stay, repeat: false }
    }

    fn repeat(mut self) -> Self {
        self.repeat = true;
        self
    }

    fn then(mut self, then: Then) -> Self {
        self.then = then;
        self
    }
}

enum Parse {
    Incomplete,
    Done { action: Action, register: Option<char>, count: Option<usize>, body: usize },
}

/// Vim motoru. `leader`: `Space` sonrası tanınan LESS kısayolları (`AppCommand::Leader` olarak çıkar).
#[derive(Clone, Debug)]
pub struct VimEngine {
    mode: VimMode,
    pending: Vec<KeyEvent>,
    cmdline: String,
    cmd_kind: char,
    cmd_visual: Option<VisualKind>,
    cmd_op: Option<(Operator, Option<char>, Option<usize>)>,
    history: HashMap<char, Vec<String>>,
    history_pos: Option<usize>,
    dot: Option<DotRecord>,
    dot_pending: Option<DotRecord>,
    macros: HashMap<char, Vec<KeyEvent>>,
    recording: Option<(char, Vec<KeyEvent>)>,
    last_macro: Option<char>,
    insert_repeat: Option<(usize, Vec<Command>)>,
    insert_cmds: Vec<Command>,
    insert_ctrl_r: bool,
    last_visual: Option<VisualKind>,
    depth: usize,
    pub leader: Vec<String>,
}

impl Default for VimEngine {
    fn default() -> Self {
        Self {
            mode: VimMode::Normal,
            pending: Vec::new(),
            cmdline: String::new(),
            cmd_kind: ':',
            cmd_visual: None,
            cmd_op: None,
            history: HashMap::new(),
            history_pos: None,
            dot: None,
            dot_pending: None,
            macros: HashMap::new(),
            recording: None,
            last_macro: None,
            insert_repeat: None,
            insert_cmds: Vec::new(),
            insert_ctrl_r: false,
            last_visual: None,
            depth: 0,
            leader: ["j", "D", "du", "dr", "ga"].iter().map(|s| s.to_string()).collect(),
        }
    }
}

impl VimEngine {
    pub fn mode(&self) -> VimMode {
        self.mode
    }

    /// Makro register içeriği (tuş listesi).
    pub fn macro_keys(&self, reg: char) -> Option<&[KeyEvent]> {
        self.macros.get(&reg).map(|v| v.as_slice())
    }

    /// Normal moda döner, bekleyen tuşları atar; editörü eşlemek için komutlar döner.
    pub fn reset(&mut self) -> Vec<Command> {
        let mut cmds = Vec::new();
        if matches!(self.mode, VimMode::Insert | VimMode::Replace) {
            cmds.push(Command::EndUndoGroup);
        }
        self.mode = VimMode::Normal;
        self.pending.clear();
        self.cmdline.clear();
        self.dot_pending = None;
        cmds.extend([
            Command::SetOverwrite(false),
            Command::SetVisual(None),
            Command::SetCursorMode(CursorMode::Block),
        ]);
        cmds
    }

    pub fn status(&self) -> Status {
        let label = match self.mode {
            VimMode::Normal => "NORMAL",
            VimMode::Insert => "INSERT",
            VimMode::Replace => "REPLACE",
            VimMode::Visual(VisualKind::Char) => "VISUAL",
            VimMode::Visual(VisualKind::Line) => "V-LINE",
            VimMode::Visual(VisualKind::Block) => "V-BLOCK",
            VimMode::Command => "COMMAND",
        };
        Status {
            mode_label: label,
            pending: notation(&self.pending),
            recording: self.recording.as_ref().map(|(c, _)| *c),
            cmdline: (self.mode == VimMode::Command).then(|| format!("{}{}", self.cmd_kind, self.cmdline)),
        }
    }

    /// Kullanıcı tuşu: makro kaydına yazılır, sonra işlenir.
    pub fn feed(&mut self, key: KeyEvent) -> Output {
        if let Some((_, keys)) = self.recording.as_mut() {
            keys.push(key);
        }
        self.process(key)
    }

    fn process(&mut self, key: KeyEvent) -> Output {
        match self.mode {
            VimMode::Insert | VimMode::Replace => self.insert_key(key),
            VimMode::Command => self.cmdline_key(key),
            VimMode::Normal | VimMode::Visual(_) => self.normal_key(key),
        }
    }

    fn replay(&mut self, keys: &[KeyEvent]) -> Vec<Command> {
        if self.depth > 100 {
            return Vec::new();
        }
        self.depth += 1;
        let mut out = Vec::new();
        for &k in keys {
            out.extend(self.process(k).commands);
        }
        self.depth -= 1;
        out
    }

    // ---- Normal / Visual ----

    fn normal_key(&mut self, key: KeyEvent) -> Output {
        if self.pending.is_empty() {
            let visual = matches!(self.mode, VimMode::Visual(_));
            if let Some(cmds) = common::editor_shortcut(key, visual, false) {
                if visual && cmds.iter().any(|c| matches!(c, Command::Operate { .. })) {
                    self.leave_visual();
                }
                return Output::handled(cmds);
            }
            if key.mods.cmd || matches!(key.key, Key::F(_)) {
                return Output::unhandled();
            }
        }
        if matches!(key.key, Key::Esc) || key.ctrl_char() == Some('c') || key.ctrl_char() == Some('[') {
            let had_pending = !self.pending.is_empty();
            self.pending.clear();
            return Output::handled(match self.mode {
                VimMode::Visual(_) => {
                    self.leave_visual();
                    vec![Command::SetVisual(None)]
                }
                _ if had_pending => Vec::new(),
                _ => vec![Command::CollapseSelection, Command::ClearSearchHighlight],
            });
        }
        self.pending.push(key);
        let keys = self.pending.clone();
        match self.parse(&keys) {
            None => {
                self.pending.clear();
                let unmapped = notation(&keys);
                Output::handled(vec![Command::App(AppCommand::Unmapped(unmapped))])
            }
            Some(Parse::Incomplete) => Output::handled(Vec::new()),
            Some(Parse::Done { action, register, count, body }) => {
                self.pending.clear();
                let record = action.repeat.then(|| DotRecord { register, count, body: keys[body..].to_vec() });
                Output::handled(self.run_action(action, record, count))
            }
        }
    }

    fn leave_visual(&mut self) {
        if let VimMode::Visual(k) = self.mode {
            self.last_visual = Some(k);
        }
        self.mode = VimMode::Normal;
    }

    fn run_action(&mut self, action: Action, record: Option<DotRecord>, count: Option<usize>) -> Vec<Command> {
        let mut cmds = action.cmds;
        match action.then {
            Then::Stay => {}
            Then::Normal => self.leave_visual(),
            Then::Visual(k) => self.mode = VimMode::Visual(k),
            Then::Insert { count, prefix, replace } => {
                self.leave_visual();
                self.mode = if replace { VimMode::Replace } else { VimMode::Insert };
                self.insert_repeat = (count > 1).then_some((count, prefix));
                self.insert_cmds.clear();
                self.insert_ctrl_r = false;
                self.dot_pending = record;
                return cmds;
            }
            Then::Cmdline { kind, text, op } => {
                self.cmd_visual = match self.mode {
                    VimMode::Visual(k) if kind != ':' => Some(k),
                    _ => None,
                };
                if kind == ':' {
                    self.leave_visual();
                }
                self.mode = VimMode::Command;
                self.cmd_kind = kind;
                self.cmdline = text;
                self.cmd_op = op;
                self.history_pos = None;
            }
            Then::Dot => return self.repeat_dot(count),
            Then::Record(r) => self.recording = Some((r, Vec::new())),
            Then::StopRecord => {
                if let Some((r, mut keys)) = self.recording.take() {
                    keys.pop();
                    self.macros.insert(r.to_ascii_lowercase(), keys);
                }
            }
            Then::Macro(r) => {
                let reg = if r == '@' { self.last_macro } else { Some(r.to_ascii_lowercase()) };
                let Some(reg) = reg else { return cmds };
                self.last_macro = Some(reg);
                let keys = self.macros.get(&reg).cloned().unwrap_or_default();
                let n = count.unwrap_or(1);
                for _ in 0..n {
                    cmds.extend(self.replay(&keys));
                }
                return cmds;
            }
        }
        if let Some(r) = record {
            self.dot = Some(r);
        }
        cmds
    }

    fn repeat_dot(&mut self, count: Option<usize>) -> Vec<Command> {
        let Some(dot) = self.dot.clone() else { return Vec::new() };
        let mut keys = Vec::new();
        if let Some(r) = dot.register {
            keys.push(KeyEvent::char('"'));
            keys.push(KeyEvent::char(r));
        }
        if let Some(n) = count.or(dot.count) {
            keys.extend(n.to_string().chars().map(KeyEvent::char));
        }
        keys.extend(dot.body);
        self.replay(&keys)
    }

    /// Bekleyen tuşları ayrıştırır: `None` = geçersiz dizi.
    fn parse(&self, keys: &[KeyEvent]) -> Option<Parse> {
        let visual = match self.mode {
            VimMode::Visual(k) => Some(k),
            _ => None,
        };
        let mut i = 0;
        let mut register = None;
        if keys[0].is_char('"') {
            let Some(r) = keys.get(1) else { return Some(Parse::Incomplete) };
            register = Some(r.plain_char()?);
            i = 2;
        }
        let count = match parse_count(keys, &mut i) {
            Some(c) => c,
            None => return Some(Parse::Incomplete),
        };
        let body = i;
        let Some(&k) = keys.get(i) else { return Some(Parse::Incomplete) };
        let n = count.unwrap_or(1);
        let done = |action: Action| Some(Parse::Done { action, register, count, body });

        if k.is_char(' ') {
            let rest: String = keys[i + 1..].iter().filter_map(KeyEvent::plain_char).collect();
            if keys[i + 1..].iter().any(|k| k.plain_char().is_none()) {
                return None;
            }
            if self.leader.contains(&rest) {
                return done(Action::new(vec![Command::App(AppCommand::Leader(rest))]));
            }
            return self.leader.iter().any(|l| l.starts_with(&rest)).then_some(Parse::Incomplete);
        }
        if let Some(c) = k.ctrl_char() {
            return done(match (c, visual) {
                ('r', None) => Action::new(vec![Command::Redo { count: n }]),
                ('v', _) => self.visual_switch(VisualKind::Block, visual),
                ('e', _) => Action::new(vec![Command::ScrollLines(n as isize)]),
                ('y', _) => Action::new(vec![Command::ScrollLines(-(n as isize))]),
                _ => {
                    let m = ctrl_motion(c)?;
                    Action::new(vec![mv(m, count, visual.is_some())])
                }
            });
        }
        if k.mods.alt
            && let Some(c @ ('j' | 'k')) = (match k.key {
                Key::Char(c) => Some(c),
                _ => None,
            })
        {
            return done(Action::new(vec![Command::MoveLines { down: c == 'j' }]));
        }

        if let Some(v) = visual {
            return self.parse_visual(keys, i, v, register, count, body);
        }

        // Operatörler
        let (op, op_len) = match operator_at(keys, i) {
            Some(Some(x)) => x,
            Some(None) => return Some(Parse::Incomplete),
            None => (Operator::Delete, 0),
        };
        if op_len > 0 {
            let mut j = i + op_len;
            let count2 = match parse_count(keys, &mut j) {
                Some(c) => c,
                None => return Some(Parse::Incomplete),
            };
            let total = match (count, count2) {
                (None, None) => None,
                (a, b) => Some(a.unwrap_or(1) * b.unwrap_or(1)),
            };
            let Some(&m) = keys.get(j) else { return Some(Parse::Incomplete) };
            let doubled = if op_len == 1 {
                m == keys[i]
            } else if m == keys[i + 1] {
                true
            } else if m.is_char('g') {
                match keys.get(j + 1) {
                    None => return Some(Parse::Incomplete),
                    Some(x) => *x == keys[i + 1],
                }
            } else {
                false
            };
            let target = if doubled {
                Target::Lines(total.unwrap_or(1))
            } else if m.is_char('i') || m.is_char('a') {
                let Some(o) = keys.get(j + 1) else { return Some(Parse::Incomplete) };
                Target::TextObject {
                    object: text_object(o.plain_char()?)?,
                    around: m.is_char('a'),
                    count: total.unwrap_or(1),
                }
            } else if m.is_char('/') || m.is_char('?') {
                let kind = if m.is_char('/') { '/' } else { '?' };
                return done(Action::new(Vec::new()).then(Then::Cmdline {
                    kind,
                    text: String::new(),
                    op: Some((op, register, total)),
                }));
            } else {
                match parse_motion(keys, j)? {
                    MParse::Incomplete => return Some(Parse::Incomplete),
                    MParse::Done(motion) => Target::Motion { motion, count: total },
                }
            };
            return done(operate(op, target, register));
        }

        match parse_motion(keys, i) {
            Some(MParse::Incomplete) => return Some(Parse::Incomplete),
            Some(MParse::Done(m)) => return done(Action::new(vec![mv(m, count, false)])),
            None => {}
        }

        let ch = k.plain_char();
        let next = keys.get(i + 1).and_then(KeyEvent::plain_char);
        let need_next = || Some(Parse::Incomplete);
        let insert = |prefix: Vec<Command>, cmds: Vec<Command>| {
            let mut all = vec![Command::BeginUndoGroup, Command::SetCursorMode(CursorMode::Bar)];
            all.extend(cmds);
            Action::new(all).then(Then::Insert { count: n, prefix, replace: false }).repeat()
        };
        let tm = |m: Motion| Target::Motion { motion: m, count };
        let action = match (ch, k.key) {
            (Some('x'), _) | (_, Key::Delete) => {
                Action::new(vec![opc(Operator::Delete, tm(Motion::Right), register)]).repeat()
            }
            (Some('X'), _) => Action::new(vec![opc(Operator::Delete, tm(Motion::Left), register)]).repeat(),
            (Some('D'), _) => Action::new(vec![opc(Operator::Delete, tm(Motion::LineEnd), register)]).repeat(),
            (Some('Y'), _) => Action::new(vec![opc(Operator::Yank, tm(Motion::LineEnd), register)]),
            (Some('s'), _) => change(tm(Motion::Right), register),
            (Some('S'), _) => change(Target::Lines(n), register),
            (Some('C'), _) => change(tm(Motion::LineEnd), register),
            (Some('i'), _) => insert(Vec::new(), Vec::new()),
            (Some('a'), _) => insert(Vec::new(), vec![mv(Motion::Right, None, false)]),
            (Some('I'), _) => insert(Vec::new(), vec![mv(Motion::FirstNonBlank, None, false)]),
            (Some('A'), _) => insert(Vec::new(), vec![mv(Motion::LineEnd, None, false)]),
            (Some('o'), _) => {
                insert(vec![Command::OpenLine { above: false }], vec![Command::OpenLine { above: false }])
            }
            (Some('O'), _) => insert(vec![Command::OpenLine { above: false }], vec![Command::OpenLine { above: true }]),
            (Some('R'), _) => Action::new(vec![
                Command::BeginUndoGroup,
                Command::SetCursorMode(CursorMode::Bar),
                Command::SetOverwrite(true),
            ])
            .then(Then::Insert { count: n, prefix: Vec::new(), replace: true })
            .repeat(),
            (Some('r'), _) => {
                let Some(&t) = keys.get(i + 1) else { return need_next() };
                let c = match t.key {
                    Key::Enter => '\n',
                    _ => t.plain_char()?,
                };
                Action::new(vec![Command::ReplaceChar { ch: c, count: n }]).repeat()
            }
            (Some('J'), _) => Action::new(vec![Command::Join { count: n.max(2), spaces: true }]).repeat(),
            (Some('~'), _) => Action::new(vec![Command::ToggleCaseChar { count: n }]).repeat(),
            (Some('p'), _) | (Some('P'), _) => {
                Action::new(vec![Command::Paste { register, before: ch == Some('P'), count: n }]).repeat()
            }
            (Some('u'), _) => Action::new(vec![Command::Undo { count: n }]),
            (Some('.'), _) => Action::new(Vec::new()).then(Then::Dot),
            (Some('q'), _) => {
                if self.recording.is_some() {
                    Action::new(Vec::new()).then(Then::StopRecord)
                } else {
                    let Some(r) = next else { return need_next() };
                    if !(r.is_ascii_alphanumeric()) {
                        return None;
                    }
                    Action::new(Vec::new()).then(Then::Record(r))
                }
            }
            (Some('@'), _) => {
                let Some(r) = next else { return need_next() };
                Action::new(Vec::new()).then(Then::Macro(r))
            }
            (Some('m'), _) => {
                let Some(r) = next else { return need_next() };
                Action::new(vec![Command::SetMark(r)])
            }
            (Some('v'), _) => self.visual_switch(VisualKind::Char, None),
            (Some('V'), _) => self.visual_switch(VisualKind::Line, None),
            (Some(':'), _) => {
                let text = if n > 1 { format!(".,.+{}", n - 1) } else { String::new() };
                Action::new(Vec::new()).then(Then::Cmdline { kind: ':', text, op: None })
            }
            (Some('/'), _) => Action::new(Vec::new()).then(Then::Cmdline { kind: '/', text: String::new(), op: None }),
            (Some('?'), _) => Action::new(Vec::new()).then(Then::Cmdline { kind: '?', text: String::new(), op: None }),
            (Some('*'), _) => Action::new(vec![Command::SearchWordUnderCursor { forward: true, whole: true }]),
            (Some('#'), _) => Action::new(vec![Command::SearchWordUnderCursor { forward: false, whole: true }]),
            (Some('+'), _) | (_, Key::Enter) => Action::new(vec![mv(Motion::Down, count, false), fnb()]),
            (Some('-'), _) => Action::new(vec![mv(Motion::Up, count, false), fnb()]),
            (Some('_'), _) => {
                let mut v = Vec::new();
                if n > 1 {
                    v.push(mv(Motion::Down, Some(n - 1), false));
                }
                v.push(fnb());
                Action::new(v)
            }
            (Some('g'), _) => {
                let Some(c) = next else { return if keys.len() > i + 1 { None } else { need_next() } };
                match c {
                    'J' => Action::new(vec![Command::Join { count: n.max(2), spaces: false }]).repeat(),
                    'v' => {
                        let kv = self.last_visual?;
                        Action::new(vec![Command::ReselectVisual]).then(Then::Visual(kv))
                    }
                    '*' => Action::new(vec![Command::SearchWordUnderCursor { forward: true, whole: false }]),
                    '#' => Action::new(vec![Command::SearchWordUnderCursor { forward: false, whole: false }]),
                    'i' => insert(Vec::new(), vec![mv(Motion::Mark { name: '^', linewise: false }, None, false)]),
                    _ => return None,
                }
            }
            (Some('z'), _) => {
                let Some(&t) = keys.get(i + 1) else { return need_next() };
                let a = match (t.plain_char(), t.key) {
                    (Some('t'), _) | (_, Key::Enter) => ScrollAnchor::Top,
                    (Some('z'), _) | (Some('.'), _) => ScrollAnchor::Center,
                    (Some('b'), _) | (Some('-'), _) => ScrollAnchor::Bottom,
                    _ => return None,
                };
                let mut v = vec![Command::Scroll(a)];
                if matches!(t.key, Key::Enter) || matches!(t.plain_char(), Some('.') | Some('-')) {
                    v.push(fnb());
                }
                Action::new(v)
            }
            (Some('Z'), _) => match next {
                None => return need_next(),
                Some('Z') => Action::new(vec![Command::App(AppCommand::WriteQuit {
                    force: false,
                    all: false,
                    only_if_modified: true,
                })]),
                Some('Q') => Action::new(vec![Command::App(AppCommand::Quit { force: true, all: false })]),
                _ => return None,
            },
            (Some('[') | Some(']'), _) => {
                if keys.len() == i + 1 {
                    return need_next();
                }
                return None;
            }
            _ => return None,
        };
        done(action)
    }

    fn visual_switch(&self, kind: VisualKind, current: Option<VisualKind>) -> Action {
        if current == Some(kind) {
            Action::new(vec![Command::SetVisual(None)]).then(Then::Normal)
        } else {
            Action::new(vec![Command::SetVisual(Some(kind))]).then(Then::Visual(kind))
        }
    }

    fn parse_visual(
        &self,
        keys: &[KeyEvent],
        i: usize,
        v: VisualKind,
        register: Option<char>,
        count: Option<usize>,
        body: usize,
    ) -> Option<Parse> {
        let done = |action: Action| Some(Parse::Done { action, register, count, body });
        let k = keys[i];
        let ch = k.plain_char();
        let next = keys.get(i + 1).and_then(KeyEvent::plain_char);
        let sel = |o: Operator| Action::new(vec![opc(o, Target::Selection, register)]).then(Then::Normal);
        let lines_then = |o: Operator| {
            Action::new(vec![Command::SetVisual(Some(VisualKind::Line)), opc(o, Target::Selection, register)])
                .then(Then::Normal)
        };
        let change_sel = |linewise: bool| {
            let mut cmds = vec![Command::BeginUndoGroup, Command::SetCursorMode(CursorMode::Bar)];
            if linewise {
                cmds.insert(0, Command::SetVisual(Some(VisualKind::Line)));
            }
            cmds.push(opc(Operator::Change, Target::Selection, register));
            Action::new(cmds).then(Then::Insert { count: 1, prefix: Vec::new(), replace: false })
        };
        if ch == Some('i') || ch == Some('a') {
            let Some(o) = next else { return Some(Parse::Incomplete) };
            let object = text_object(o)?;
            return done(Action::new(vec![Command::SelectTextObject {
                object,
                around: ch == Some('a'),
                count: count.unwrap_or(1),
            }]));
        }
        match operator_at(keys, i) {
            Some(None) => return Some(Parse::Incomplete),
            Some(Some((op, _))) => {
                return done(match op {
                    Operator::Change => change_sel(false),
                    _ => sel(op),
                });
            }
            None => {}
        }
        match parse_motion(keys, i) {
            Some(MParse::Incomplete) => return Some(Parse::Incomplete),
            Some(MParse::Done(m)) => return done(Action::new(vec![mv(m, count, true)])),
            None => {}
        }
        let action = match (ch, k.key) {
            (Some('x'), _) | (_, Key::Delete) => sel(Operator::Delete),
            (Some('X'), _) | (Some('D'), _) => lines_then(Operator::Delete),
            (Some('Y'), _) => lines_then(Operator::Yank),
            (Some('s'), _) => change_sel(false),
            (Some('S'), _) | (Some('R'), _) => change_sel(true),
            (Some('C'), _) => change_sel(v != VisualKind::Block),
            (Some('~'), _) => sel(Operator::ToggleCase),
            (Some('u'), _) => sel(Operator::Lowercase),
            (Some('U'), _) => sel(Operator::Uppercase),
            (Some('J'), _) => Action::new(vec![Command::Join { count: 0, spaces: true }]).then(Then::Normal),
            (Some('r'), _) => {
                let Some(&t) = keys.get(i + 1) else { return Some(Parse::Incomplete) };
                let c = if matches!(t.key, Key::Enter) { '\n' } else { t.plain_char()? };
                Action::new(vec![Command::ReplaceChar { ch: c, count: 1 }]).then(Then::Normal)
            }
            (Some('p'), _) | (Some('P'), _) => {
                Action::new(vec![Command::Paste { register, before: true, count: 1 }]).then(Then::Normal)
            }
            (Some('o'), _) | (Some('O'), _) => Action::new(vec![Command::SwapAnchor]),
            (Some('v'), _) => self.visual_switch(VisualKind::Char, Some(v)),
            (Some('V'), _) => self.visual_switch(VisualKind::Line, Some(v)),
            (Some('I'), _) | (Some('A'), _) if v == VisualKind::Block => {
                Action::new(vec![Command::BeginUndoGroup, Command::BlockToCursors { append: ch == Some('A') }])
                    .then(Then::Insert { count: 1, prefix: Vec::new(), replace: false })
            }
            (Some(':'), _) => Action::new(vec![Command::SetVisual(None)]).then(Then::Cmdline {
                kind: ':',
                text: "'<,'>".into(),
                op: None,
            }),
            (Some('/'), _) => Action::new(Vec::new()).then(Then::Cmdline { kind: '/', text: String::new(), op: None }),
            (Some('?'), _) => Action::new(Vec::new()).then(Then::Cmdline { kind: '?', text: String::new(), op: None }),
            (Some('g'), _) => match next {
                None => return Some(Parse::Incomplete),
                Some('J') => Action::new(vec![Command::Join { count: 0, spaces: false }]).then(Then::Normal),
                _ => return None,
            },
            _ => return None,
        };
        done(action)
    }

    // ---- Insert ----

    fn insert_key(&mut self, key: KeyEvent) -> Output {
        if let Some(d) = self.dot_pending.as_mut() {
            d.body.push(key);
        }
        if self.insert_ctrl_r {
            self.insert_ctrl_r = false;
            let Some(r) = key.plain_char() else { return Output::handled(Vec::new()) };
            return self.insert_cmd(vec![Command::PasteInsert { register: r }]);
        }
        if matches!(key.key, Key::Esc) || matches!(key.ctrl_char(), Some('c') | Some('[')) {
            return Output::handled(self.finish_insert());
        }
        if let Some(cmds) = common::editor_shortcut(key, false, true) {
            return Output::handled(cmds);
        }
        if let Some(c) = key.ctrl_char() {
            let cmds = match c {
                'r' => {
                    self.insert_ctrl_r = true;
                    return Output::handled(Vec::new());
                }
                'w' => vec![Command::DeleteWordBackward],
                'u' => vec![Command::DeleteToLineStart],
                'h' => vec![Command::Backspace],
                'j' | 'm' => vec![Command::InsertNewline],
                't' => vec![opc(Operator::Indent, Target::Lines(1), None)],
                'd' => vec![opc(Operator::Outdent, Target::Lines(1), None)],
                _ => return Output::unhandled(),
            };
            return self.insert_cmd(cmds);
        }
        if key.mods.cmd || matches!(key.key, Key::F(_)) {
            return Output::unhandled();
        }
        let cmds = match key.key {
            Key::Char(c) if key.mods.alt && matches!(c, 'j' | 'k') => vec![Command::MoveLines { down: c == 'j' }],
            Key::Char(c) => vec![Command::InsertText(c.to_string())],
            Key::Enter => vec![Command::InsertNewline],
            Key::Tab if key.mods.shift => vec![Command::Outdent],
            Key::Tab => vec![Command::InsertTab],
            Key::Backspace if key.mods.alt => vec![Command::DeleteWordBackward],
            Key::Backspace => vec![Command::Backspace],
            Key::Delete => vec![Command::DeleteForward],
            Key::Left => vec![mv(Motion::LeftWrap, None, false)],
            Key::Right => vec![mv(Motion::RightWrap, None, false)],
            Key::Up => vec![mv(Motion::Up, None, false)],
            Key::Down => vec![mv(Motion::Down, None, false)],
            Key::Home => vec![mv(Motion::LineStart, None, false)],
            Key::End => vec![mv(Motion::LineEnd, None, false)],
            Key::PageUp => vec![mv(Motion::PageUp, None, false)],
            Key::PageDown => vec![mv(Motion::PageDown, None, false)],
            _ => return Output::unhandled(),
        };
        self.insert_cmd(cmds)
    }

    fn insert_cmd(&mut self, cmds: Vec<Command>) -> Output {
        self.insert_cmds.extend(cmds.iter().cloned());
        Output::handled(cmds)
    }

    fn finish_insert(&mut self) -> Vec<Command> {
        let mut out = Vec::new();
        if let Some((count, prefix)) = self.insert_repeat.take() {
            for _ in 1..count {
                out.extend(prefix.iter().cloned());
                out.extend(self.insert_cmds.iter().cloned());
            }
        }
        out.push(Command::EndUndoGroup);
        if self.mode == VimMode::Replace {
            out.push(Command::SetOverwrite(false));
        }
        out.push(Command::SetMark('^'));
        out.push(Command::SetCursorMode(CursorMode::Block));
        self.mode = VimMode::Normal;
        if let Some(d) = self.dot_pending.take() {
            self.dot = Some(d);
        }
        out
    }

    // ---- Komut satırı ----

    fn cmdline_key(&mut self, key: KeyEvent) -> Output {
        let back = |e: &mut Self| {
            e.mode = match e.cmd_visual {
                Some(k) => VimMode::Visual(k),
                None => VimMode::Normal,
            };
        };
        if matches!(key.key, Key::Esc) || matches!(key.ctrl_char(), Some('c') | Some('[')) {
            back(self);
            self.cmdline.clear();
            return Output::handled(Vec::new());
        }
        match (key.key, key.ctrl_char()) {
            (_, Some('u')) => self.cmdline.clear(),
            (_, Some('w')) => {
                let t = self.cmdline.trim_end();
                let keep = t.rfind(|c: char| !c.is_alphanumeric() && c != '_').map_or(0, |p| p + 1);
                self.cmdline.truncate(keep.min(t.len()));
            }
            (_, Some(_)) => {}
            (Key::Backspace, _) => {
                if self.cmdline.pop().is_none() {
                    back(self);
                }
            }
            (Key::Up, _) | (Key::Down, _) => {
                let hist = self.history.get(&self.cmd_kind).cloned().unwrap_or_default();
                if !hist.is_empty() {
                    let pos = match (self.history_pos, key.key) {
                        (None, Key::Up) => Some(hist.len() - 1),
                        (Some(p), Key::Up) => Some(p.saturating_sub(1)),
                        (Some(p), _) if p + 1 < hist.len() => Some(p + 1),
                        _ => None,
                    };
                    self.history_pos = pos;
                    self.cmdline = pos.map(|p| hist[p].clone()).unwrap_or_default();
                }
            }
            (Key::Enter, _) => return Output::handled(self.submit_cmdline()),
            (Key::Tab, _) => self.cmdline.push('\t'),
            (Key::Char(c), _) if !key.mods.cmd => self.cmdline.push(c),
            _ => return Output::unhandled(),
        }
        Output::handled(Vec::new())
    }

    fn submit_cmdline(&mut self) -> Vec<Command> {
        let text = std::mem::take(&mut self.cmdline);
        let kind = self.cmd_kind;
        let visual = self.cmd_visual.take();
        let op = self.cmd_op.take();
        self.mode = match visual {
            Some(k) => VimMode::Visual(k),
            None => VimMode::Normal,
        };
        if !text.is_empty() {
            let h = self.history.entry(kind).or_default();
            h.retain(|t| *t != text);
            h.push(text.clone());
        }
        if kind == ':' {
            return match ex::parse(&text) {
                Ok(cmds) => cmds,
                Err(e) => vec![Command::App(AppCommand::Message(e))],
            };
        }
        let motion = Motion::Search { pattern: text, forward: kind == '/' };
        match op {
            Some((o, register, count)) => {
                let mut cmds = Vec::new();
                if o == Operator::Change {
                    cmds.extend([Command::BeginUndoGroup, Command::SetCursorMode(CursorMode::Bar)]);
                    self.mode = VimMode::Insert;
                    self.insert_repeat = None;
                    self.insert_cmds.clear();
                }
                cmds.push(Command::Operate { op: o, target: Target::Motion { motion, count }, register });
                cmds
            }
            None => vec![Command::Move { motion, count: None, extend: visual.is_some() }],
        }
    }
}

fn mv(motion: Motion, count: Option<usize>, extend: bool) -> Command {
    Command::Move { motion, count, extend }
}

fn fnb() -> Command {
    mv(Motion::FirstNonBlank, None, false)
}

fn opc(op: Operator, target: Target, register: Option<char>) -> Command {
    Command::Operate { op, target, register }
}

fn change(target: Target, register: Option<char>) -> Action {
    Action::new(vec![
        Command::BeginUndoGroup,
        Command::SetCursorMode(CursorMode::Bar),
        opc(Operator::Change, target, register),
    ])
    .then(Then::Insert { count: 1, prefix: Vec::new(), replace: false })
    .repeat()
}

fn operate(op: Operator, target: Target, register: Option<char>) -> Action {
    match op {
        Operator::Change => change(target, register),
        Operator::Yank => Action::new(vec![opc(op, target, register)]),
        _ => Action::new(vec![opc(op, target, register)]).repeat(),
    }
}

/// `keys[i..]`'de sayaç: `Some(None)` yok, `Some(Some(n))` var; yalnız rakamlarla bitiyorsa `None` (eksik).
fn parse_count(keys: &[KeyEvent], i: &mut usize) -> Option<Option<usize>> {
    let start = *i;
    let mut n: usize = 0;
    while let Some(c) = keys.get(*i).and_then(KeyEvent::plain_char) {
        let first = *i == start;
        match c.to_digit(10) {
            Some(d) if !(first && d == 0) => {
                n = n.saturating_mul(10).saturating_add(d as usize);
                *i += 1;
            }
            _ => break,
        }
    }
    if *i == start {
        return Some(None);
    }
    if *i >= keys.len() {
        return None;
    }
    Some(Some(n))
}

/// `keys[i]`'deki operatör: `None` operatör değil, `Some(None)` eksik (`g` sonrası bekleniyor).
fn operator_at(keys: &[KeyEvent], i: usize) -> Option<Option<(Operator, usize)>> {
    let c = keys.get(i)?.plain_char()?;
    let single = match c {
        'd' => Some(Operator::Delete),
        'c' => Some(Operator::Change),
        'y' => Some(Operator::Yank),
        '>' => Some(Operator::Indent),
        '<' => Some(Operator::Outdent),
        '=' => Some(Operator::Format),
        _ => None,
    };
    if let Some(o) = single {
        return Some(Some((o, 1)));
    }
    if c != 'g' {
        return None;
    }
    let Some(n) = keys.get(i + 1) else { return Some(None) };
    let o = match n.plain_char()? {
        '~' => Operator::ToggleCase,
        'u' => Operator::Lowercase,
        'U' => Operator::Uppercase,
        'c' => Operator::Comment,
        _ => return None,
    };
    Some(Some((o, 2)))
}

fn text_object(c: char) -> Option<TextObject> {
    Some(match c {
        'w' => TextObject::Word { big: false },
        'W' => TextObject::Word { big: true },
        '"' | '\'' | '`' => TextObject::Quote(c),
        '(' | ')' | 'b' => TextObject::Pair { open: '(', close: ')' },
        '{' | '}' | 'B' => TextObject::Pair { open: '{', close: '}' },
        '[' | ']' => TextObject::Pair { open: '[', close: ']' },
        '<' | '>' => TextObject::Pair { open: '<', close: '>' },
        'p' => TextObject::Paragraph,
        't' => TextObject::Tag,
        _ => return None,
    })
}

fn ctrl_motion(c: char) -> Option<Motion> {
    Some(match c {
        'd' => Motion::HalfPageDown,
        'u' => Motion::HalfPageUp,
        'f' => Motion::PageDown,
        'b' => Motion::PageUp,
        'n' | 'j' => Motion::Down,
        'p' => Motion::Up,
        'h' => Motion::LeftWrap,
        _ => return None,
    })
}

enum MParse {
    Incomplete,
    Done(Motion),
}

/// `keys[i..]`'deki hareket; `None` = hareket değil.
fn parse_motion(keys: &[KeyEvent], i: usize) -> Option<MParse> {
    let k = keys[i];
    let next = keys.get(i + 1);
    let arg = |f: &dyn Fn(char) -> Motion| match next {
        None => Some(MParse::Incomplete),
        Some(n) => n.plain_char().map(|c| MParse::Done(f(c))),
    };
    if k.mods.ctrl || k.mods.cmd {
        return None;
    }
    if k.mods.alt {
        return match k.key {
            Key::Left => Some(MParse::Done(Motion::WordLeft)),
            Key::Right => Some(MParse::Done(Motion::WordRight)),
            _ => None,
        };
    }
    let m = match k.key {
        Key::Left => Motion::LeftWrap,
        Key::Right => Motion::RightWrap,
        Key::Up => Motion::Up,
        Key::Down => Motion::Down,
        Key::Home => Motion::LineStart,
        Key::End => Motion::LineEnd,
        Key::PageUp => Motion::PageUp,
        Key::PageDown => Motion::PageDown,
        Key::Backspace => Motion::LeftWrap,
        Key::Char(c) => match c {
            'h' => Motion::Left,
            'l' => Motion::Right,
            'j' => Motion::Down,
            'k' => Motion::Up,
            'w' => Motion::WordForward { big: false },
            'W' => Motion::WordForward { big: true },
            'b' => Motion::WordBackward { big: false },
            'B' => Motion::WordBackward { big: true },
            'e' => Motion::WordEnd { big: false },
            'E' => Motion::WordEnd { big: true },
            '0' => Motion::LineStart,
            '^' => Motion::FirstNonBlank,
            '$' => Motion::LineEnd,
            'G' => Motion::FileEnd,
            '%' => Motion::MatchPair,
            '}' => Motion::ParagraphForward,
            '{' => Motion::ParagraphBackward,
            'H' => Motion::ScreenTop,
            'M' => Motion::ScreenMiddle,
            'L' => Motion::ScreenBottom,
            'n' => Motion::SearchNext { reverse: false },
            'N' => Motion::SearchNext { reverse: true },
            ';' => Motion::RepeatFind { reverse: false },
            ',' => Motion::RepeatFind { reverse: true },
            'f' => return arg(&|c| Motion::FindChar { ch: c, forward: true, till: false }),
            'F' => return arg(&|c| Motion::FindChar { ch: c, forward: false, till: false }),
            't' => return arg(&|c| Motion::FindChar { ch: c, forward: true, till: true }),
            'T' => return arg(&|c| Motion::FindChar { ch: c, forward: false, till: true }),
            '`' => return arg(&|c| Motion::Mark { name: c, linewise: false }),
            '\'' => return arg(&|c| Motion::Mark { name: c, linewise: true }),
            'g' => {
                let Some(n) = next else { return Some(MParse::Incomplete) };
                return match n.plain_char()? {
                    'g' => Some(MParse::Done(Motion::FileStart)),
                    'e' => Some(MParse::Done(Motion::WordEndBackward { big: false })),
                    'E' => Some(MParse::Done(Motion::WordEndBackward { big: true })),
                    '_' => Some(MParse::Done(Motion::LastNonBlank)),
                    '0' => Some(MParse::Done(Motion::LineStart)),
                    'j' => Some(MParse::Done(Motion::Down)),
                    'k' => Some(MParse::Done(Motion::Up)),
                    _ => None,
                };
            }
            _ => return None,
        },
        _ => return None,
    };
    Some(MParse::Done(m))
}
