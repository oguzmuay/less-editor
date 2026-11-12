//! Bir sekmenin düzenleyicisi: buffer + seçim + görünüm. `Command`'ları uygular; Vim ve modeless giriş katmanları
//! aynı yolu kullanır. Register'lar, arama ve pano `Session` içinde tüm sekmelerce paylaşılır.

use std::collections::HashMap;
use std::path::Path;

use ropey::Rope;

use crate::buffer::Buffer;
use crate::command::{
    AppCommand, Command, CursorMode, LineAddr, LineBase, LineRange, Motion, Operator, ScrollAnchor, SetValue, Target,
    VisualKind,
};
use crate::document::LoadError;
use crate::motion as mv;
use crate::registers::{Clipboard, MemoryClipboard, RegisterContent, RegisterKind, Registers};
use crate::search::{self, SearchState};
use crate::selection::{Range, Selection};
use crate::text::{self as tx, CharClass};
use crate::textobj;

/// Sekmeler arası paylaşılan durum: register'lar, son arama, son `f`/`t`, sistem panosu.
pub struct Session {
    pub registers: Registers,
    pub search: SearchState,
    /// Son `f`/`F`/`t`/`T`: (karakter, ileri mi, till mi).
    pub last_find: Option<(char, bool, bool)>,
    pub clipboard: Box<dyn Clipboard>,
}

impl Default for Session {
    fn default() -> Self {
        Self::new(Box::new(MemoryClipboard::default()))
    }
}

impl Session {
    pub fn new(clipboard: Box<dyn Clipboard>) -> Self {
        Self { registers: Registers::default(), search: SearchState::default(), last_find: None, clipboard }
    }
}

/// Görünür satır aralığı. `height` arayüzce verilir; `preferred_vcol` dikey harekette korunan sütun
/// (`usize::MAX` = satır sonu, `$` sonrası).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct View {
    pub top: usize,
    pub height: usize,
    pub scrolloff: usize,
    pub preferred_vcol: Option<usize>,
}

impl Default for View {
    fn default() -> Self {
        Self { top: 0, height: 40, scrolloff: 4, preferred_vcol: None }
    }
}

/// Vim'in hareket türü: operatörün hangi aralığa uygulanacağını belirler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionKind {
    Exclusive,
    Inclusive,
    Linewise,
}

/// Operatörün çalıştığı parça: karakter aralığı (dışlayıcı), satır aralığı (kapsayıcı) ya da blok satırı.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Span {
    Chars(usize, usize),
    Lines(usize, usize),
    Block(usize, usize),
}

impl Span {
    fn key(self) -> (usize, usize) {
        match self {
            Span::Chars(a, b) | Span::Lines(a, b) | Span::Block(a, b) => (a, b),
        }
    }
}

type Change = (usize, usize, String);

/// Düzenleyici. `apply` her komutu tek geri alma adımı olarak uygular (`BeginUndoGroup` ile birleştirilebilir).
#[derive(Debug)]
pub struct Editor {
    pub buffer: Buffer,
    selection: Selection,
    pub view: View,
    visual: Option<VisualKind>,
    cursor_mode: CursorMode,
    overwrite: bool,
    marks: HashMap<char, usize>,
    last_visual: Option<(Range, VisualKind)>,
    message: Option<String>,
    last_typing: bool,
    keep_view: bool,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new(Buffer::default())
    }
}

impl Editor {
    pub fn new(buffer: Buffer) -> Self {
        Self {
            buffer,
            selection: Selection::point(0),
            view: View::default(),
            visual: None,
            cursor_mode: CursorMode::Block,
            overwrite: false,
            marks: HashMap::new(),
            last_visual: None,
            message: None,
            last_typing: false,
            keep_view: false,
        }
    }

    pub fn from_text(text: &str) -> Self {
        Self::new(Buffer::from_str(text))
    }

    pub fn load(path: &Path) -> Result<Self, LoadError> {
        Buffer::load(path).map(Self::new)
    }

    /// Kaydeder; insert oturumu açıksa o ana kadarki değişiklikler ayrı geri alma adımı olur.
    pub fn save(&mut self) -> std::io::Result<()> {
        self.buffer.save()
    }

    pub fn text(&self) -> String {
        self.buffer.text()
    }

    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    /// Fare vb. ile seçimi doğrudan koyar (sınırlar düzeltilir).
    pub fn set_selection(&mut self, sel: Selection) {
        self.selection = sel;
        self.finish();
    }

    /// Birincil imlecin karakter konumu.
    pub fn cursor(&self) -> usize {
        self.selection.primary().head
    }

    /// Birincil imlecin (satır, sütun) değeri, ikisi de 0 tabanlı, sütun karakter cinsinden.
    pub fn cursor_line_col(&self) -> (usize, usize) {
        let r = self.buffer.rope();
        let head = self.cursor();
        let line = r.char_to_line(head);
        (line, head - tx::line_start(r, line))
    }

    pub fn visual(&self) -> Option<VisualKind> {
        self.visual
    }

    pub fn cursor_mode(&self) -> CursorMode {
        self.cursor_mode
    }

    pub fn overwrite(&self) -> bool {
        self.overwrite
    }

    pub fn mark(&self, name: char) -> Option<usize> {
        self.marks.get(&name).copied()
    }

    /// Çizim için seçili karakter aralıkları (dışlayıcı bitiş). Normal modda (blok imleç, görsel yok) boştur.
    pub fn selection_spans(&self) -> Vec<(usize, usize)> {
        let r = self.buffer.rope();
        let len = r.len_chars();
        match self.visual {
            Some(VisualKind::Char) => {
                self.selection.ranges().iter().map(|g| (g.from(), (g.to() + 1).min(len))).collect()
            }
            Some(VisualKind::Line) => self
                .selection
                .ranges()
                .iter()
                .map(|g| {
                    let (a, b) = (r.char_to_line(g.from()), r.char_to_line(g.to()));
                    (tx::line_start(r, a), (tx::line_end(r, b) + 1).min(len))
                })
                .collect(),
            Some(VisualKind::Block) => self.block_rows(),
            None => self.selection.ranges().iter().filter(|g| !g.is_empty()).map(|g| (g.from(), g.to())).collect(),
        }
    }

    /// Cmd+D: birincil seçimin metninin sonraki eşleşmesini (sona gelince baştan) yeni birincil seçim olarak ekler.
    /// Seçim boşsa, satır/blok görsel moddaysa ya da tüm eşleşmeler zaten seçiliyse `false`.
    pub fn select_next_match(&mut self) -> bool {
        let r = self.buffer.rope();
        let p = self.selection.primary();
        let (a, b) = match self.visual {
            Some(VisualKind::Char) => (p.from(), (p.to() + 1).min(r.len_chars())),
            None => (p.from(), p.to()),
            Some(_) => return false,
        };
        if a >= b {
            return false;
        }
        let (text, needle) = (r.to_string(), r.slice(a..b).to_string());
        let from = r.char_to_byte(b);
        let after = text[from..].match_indices(&needle).map(|(i, _)| from + i);
        let before = text[..from].match_indices(&needle).map(|(i, _)| i);
        let taken: Vec<usize> = self.selection_spans().iter().map(|s| s.0).collect();
        let Some(at) = after.chain(before).map(|i| r.byte_to_char(i)).find(|c| !taken.contains(c)) else {
            return false;
        };
        let end = if self.visual.is_some() { at + (b - a) - 1 } else { at + (b - a) };
        self.selection.push(Range::new(at, end));
        self.selection.set_primary(self.selection.len() - 1);
        self.finish();
        true
    }

    /// `first..=last` satırlarındaki arama eşleşmeleri (vurgu kapalıysa boş).
    pub fn search_matches(&self, session: &Session, first: usize, last: usize) -> Vec<(usize, usize)> {
        if !session.search.highlight {
            return Vec::new();
        }
        let Some(re) = session.search.regex() else { return Vec::new() };
        let r = self.buffer.rope();
        let last = last.min(tx::last_line(r));
        if first > last {
            return Vec::new();
        }
        let start = tx::line_start(r, first);
        let text = r.slice(start..tx::line_end(r, last)).to_string();
        let slice = r.slice(start..);
        re.find_iter(&text)
            .filter(|m| m.end() > m.start())
            .map(|m| (start + slice.byte_to_char(m.start()), start + slice.byte_to_char(m.end())))
            .collect()
    }

    /// Komutu uygular. Uygulamanın karar vereceği istekleri (`:w`, `:q`, mesaj ...) döndürür.
    pub fn apply(&mut self, cmd: Command, session: &mut Session) -> Option<AppCommand> {
        let typing = matches!(&cmd, Command::InsertText(t) if !t.contains('\n') && !t.trim().is_empty());
        self.buffer.history.begin(&self.selection);
        if !typing {
            self.buffer.history.mark_not_typing();
        }
        let out = self.run(cmd, session);
        self.finish();
        self.buffer.history.end(&self.selection, typing && self.last_typing);
        self.last_typing = typing;
        out.or_else(|| self.message.take().map(AppCommand::Message))
    }

    fn run(&mut self, cmd: Command, s: &mut Session) -> Option<AppCommand> {
        match cmd {
            Command::Move { motion, count, extend } => self.move_cursor(&motion, count, extend, s),
            Command::Operate { op, target, register } => self.operate(op, target, register, s),
            Command::InsertText(t) => self.insert_text(&t),
            Command::InsertNewline => self.insert_newline(),
            Command::InsertTab => self.insert_tab(s),
            Command::Outdent => self.operate(Operator::Outdent, Target::Selection, None, s),
            Command::Backspace => self.backspace(),
            Command::DeleteForward => self.delete_around(|_, p, len| (p, (p + 1).min(len))),
            Command::DeleteWordBackward => self.delete_around(|r, p, _| {
                let ls = tx::line_start(r, r.char_to_line(p));
                if p == ls { (p.saturating_sub(1), p) } else { (mv::word_left(r, p).max(ls), p) }
            }),
            Command::DeleteToLineStart => self.delete_around(|r, p, _| {
                let l = r.char_to_line(p);
                let (ls, fnb) = (tx::line_start(r, l), tx::first_non_blank(r, l));
                if p == ls {
                    (p.saturating_sub(1), p)
                } else if p > fnb {
                    (fnb, p)
                } else {
                    (ls, p)
                }
            }),
            Command::OpenLine { above } => self.open_line(above),
            Command::Join { count, spaces } => self.join(count, spaces),
            Command::ReplaceChar { ch, count } => self.replace_char(ch, count),
            Command::ToggleCaseChar { count } => self.toggle_case_char(count, s),
            Command::Paste { register, before, count } => self.paste(register, before, count.max(1), s),
            Command::PasteInsert { register } => match s.registers.get(Some(register), &mut *s.clipboard) {
                Some(c) => self.insert_text(&c.text),
                None => self.message = Some(format!("Nothing in register {register}")),
            },
            Command::Undo { count } => self.undo_redo(count.max(1), true),
            Command::Redo { count } => self.undo_redo(count.max(1), false),
            Command::BeginUndoGroup => self.buffer.history.begin(&self.selection),
            Command::EndUndoGroup => self.buffer.history.end(&self.selection, false),
            Command::SetCursorMode(m) => self.set_cursor_mode(m),
            Command::SetVisual(k) => self.set_visual(k),
            Command::SetOverwrite(on) => self.overwrite = on,
            Command::SwapAnchor => {
                for g in self.selection.ranges_mut() {
                    *g = Range::new(g.head, g.anchor);
                }
            }
            Command::SelectAll => self.select_all(),
            Command::SelectTextObject { object, around, count } => self.select_object(object, around, count),
            Command::CollapseSelection => {
                self.selection.collapse_to_primary();
            }
            Command::AddCursor { below } => self.add_cursor(below),
            Command::BlockToCursors { append } => self.block_to_cursors(append),
            Command::MoveLines { down } => self.move_lines(down),
            Command::SetMark(c) => {
                self.marks.insert(c, self.cursor());
            }
            Command::SearchWordUnderCursor { forward, whole } => self.search_word(forward, whole, s),
            Command::ClearSearchHighlight => s.search.highlight = false,
            Command::Substitute { range, pattern, replacement, global, ignore_case } => {
                self.substitute(&range, &pattern, &replacement, global, ignore_case, s)
            }
            Command::Scroll(anchor) => self.scroll(anchor),
            Command::ScrollLines(n) => self.scroll_lines(n),
            Command::GotoAddr(addr) => {
                if let Some(l) = self.resolve_addr(&addr) {
                    let p = tx::first_non_blank(self.buffer.rope(), l);
                    self.set_heads(|_| p, false);
                }
            }
            Command::ReselectVisual => {
                if let Some((g, k)) = self.last_visual {
                    self.selection = Selection::single(g);
                    self.visual = Some(k);
                    self.cursor_mode = CursorMode::Block;
                }
            }
            Command::Set { name, value } => return self.set_option(name, value, s),
            Command::App(c) => return Some(c),
        }
        None
    }

    /// Sınırları düzeltir (blok imleç satır sonuna geçemez), seçimleri birleştirir, görünümü imlece kaydırır.
    fn finish(&mut self) {
        let r = self.buffer.rope();
        let len = r.len_chars();
        let block = self.cursor_mode == CursorMode::Block;
        let visual = self.visual.is_some();
        for g in self.selection.ranges_mut() {
            g.anchor = g.anchor.min(len);
            g.head = g.head.min(len);
            if block {
                let l = r.char_to_line(g.head);
                let limit = if visual { tx::line_end(r, l) } else { tx::line_last_char(r, l) };
                g.head = g.head.min(limit);
                if !visual {
                    g.anchor = g.head;
                }
            }
        }
        self.selection.normalize();
        if std::mem::take(&mut self.keep_view) {
            return;
        }
        self.scroll_to_cursor();
    }

    fn scroll_to_cursor(&mut self) {
        let r = self.buffer.rope();
        let line = r.char_to_line(self.cursor());
        let h = self.view.height.max(1);
        let so = self.view.scrolloff.min(h.saturating_sub(1) / 2);
        if line < self.view.top + so {
            self.view.top = line.saturating_sub(so);
        } else if line + so >= self.view.top + h {
            self.view.top = line + so + 1 - h;
        }
        self.view.top = self.view.top.min(tx::last_line(r));
    }

    /// Her aralığın başını `f` ile taşır; `extend` değilse (ve görsel değilse) aralık imlece iner.
    fn set_heads(&mut self, f: impl Fn(usize) -> usize, extend: bool) {
        let keep = extend || self.visual.is_some();
        for g in self.selection.ranges_mut() {
            g.head = f(g.head);
            if !keep {
                g.anchor = g.head;
            }
        }
    }

    /// Sıralı, çakışmayan değişiklikleri (eski koordinatlarda) sondan başa uygular; seçim ve işaretler kaydırılır.
    /// Her değişikliğin yeni metindeki başlangıcını (sıralı) döndürür.
    fn edit(&mut self, mut changes: Vec<Change>) -> Vec<usize> {
        changes.sort_by_key(|c| (c.0, c.1));
        let mut kept: Vec<Change> = Vec::with_capacity(changes.len());
        for c in changes {
            if kept.last().is_none_or(|l| c.0 >= l.1) {
                kept.push(c);
            }
        }
        let mut starts = Vec::with_capacity(kept.len());
        let mut delta = 0isize;
        for (a, b, t) in &kept {
            starts.push((*a as isize + delta) as usize);
            delta += t.chars().count() as isize - (b - a) as isize;
        }
        let sel = self.selection.clone();
        for (a, b, t) in kept.iter().rev() {
            let (a, b) = (*a, *b);
            let ins = t.chars().count();
            self.buffer.replace(a, b, t, &sel);
            let shift = |p: usize| {
                if p >= b {
                    p - (b - a) + ins
                } else if p > a {
                    a
                } else {
                    p
                }
            };
            for g in self.selection.ranges_mut() {
                *g = g.map(shift);
            }
            for m in self.marks.values_mut() {
                *m = shift(*m);
            }
            if let Some((g, _)) = self.last_visual.as_mut() {
                *g = g.map(shift);
            }
        }
        starts
    }

    // ---- hareket ----

    fn move_cursor(&mut self, motion: &Motion, count: Option<usize>, extend: bool, s: &mut Session) {
        let vertical = matches!(
            motion,
            Motion::Up | Motion::Down | Motion::HalfPageDown | Motion::HalfPageUp | Motion::PageDown | Motion::PageUp
        );
        let horizontal_bar = matches!(motion, Motion::Left | Motion::Right | Motion::LeftWrap | Motion::RightWrap)
            && self.cursor_mode == CursorMode::Bar
            && !extend;
        let ranges = self.selection.ranges().to_vec();
        let mut out = Vec::with_capacity(ranges.len());
        let mut view_moved = false;
        for (i, g) in ranges.iter().enumerate() {
            if horizontal_bar && !g.is_empty() {
                let edge = if matches!(motion, Motion::Left | Motion::LeftWrap) { g.from() } else { g.to() };
                out.push(Range::point(edge));
                continue;
            }
            let pos = if i > 0 && view_moved {
                self.motion_pos_no_view(g.head, motion, count, s)
            } else {
                let top = self.view.top;
                let p = self.motion_pos(g.head, motion, count, false, s).map(|(p, _)| p);
                view_moved = self.view.top != top;
                p
            };
            let head = pos.unwrap_or(g.head);
            let keep = extend || self.visual.is_some();
            out.push(Range::new(if keep { g.anchor } else { head }, head));
        }
        let primary = self.selection.primary_index();
        self.selection = Selection::new(out, primary);
        if matches!(motion, Motion::LineEnd) {
            self.view.preferred_vcol = Some(usize::MAX);
        } else if !vertical {
            self.view.preferred_vcol = None;
        }
    }

    fn motion_pos_no_view(&mut self, head: usize, m: &Motion, count: Option<usize>, s: &mut Session) -> Option<usize> {
        let top = self.view.top;
        let p = self.motion_pos(head, m, count, false, s).map(|(p, _)| p);
        self.view.top = top;
        p
    }

    /// Hareketin hedefi ve türü; `op` iken operatör kuralları (satır sonuna gidebilir, görünüm kaymaz).
    fn motion_pos(
        &mut self,
        head: usize,
        motion: &Motion,
        count: Option<usize>,
        op: bool,
        s: &mut Session,
    ) -> Option<(usize, MotionKind)> {
        use MotionKind::*;
        let n = count.unwrap_or(1).max(1);
        let bar = self.cursor_mode == CursorMode::Bar;
        let visual = self.visual.is_some();
        let multi = self.selection.len() > 1;
        let tab = self.buffer.tab_width;
        let r = self.buffer.rope();
        let len = r.len_chars();
        let line = r.char_to_line(head);
        let last = tx::last_line(r);
        let end_of = |l: usize| if bar || op { tx::line_end(r, l) } else { tx::line_last_char(r, l) };
        let res = match motion {
            Motion::Left => (head.saturating_sub(n).max(tx::line_start(r, line)), Exclusive),
            Motion::Right => ((head + n).min(end_of(line)).max(head.min(end_of(line))), Exclusive),
            Motion::LeftWrap => {
                let mut p = head;
                for _ in 0..n {
                    let l = r.char_to_line(p);
                    if p > tx::line_start(r, l) {
                        p -= 1;
                    } else if l > 0 {
                        p = end_of(l - 1);
                    }
                }
                (p, Exclusive)
            }
            Motion::RightWrap => {
                let mut p = head;
                for _ in 0..n {
                    let l = r.char_to_line(p);
                    if p < end_of(l) {
                        p += 1;
                    } else if l < last {
                        p = tx::line_start(r, l + 1);
                    }
                }
                (p, Exclusive)
            }
            Motion::Up
            | Motion::Down
            | Motion::HalfPageDown
            | Motion::HalfPageUp
            | Motion::PageDown
            | Motion::PageUp => {
                let h = self.view.height.max(2);
                let (down, amount) = match motion {
                    Motion::Up => (false, n),
                    Motion::Down => (true, n),
                    Motion::HalfPageDown => (true, h / 2),
                    Motion::HalfPageUp => (false, h / 2),
                    Motion::PageDown => (true, (h - 2).max(1) * n),
                    _ => (false, (h - 2).max(1) * n),
                };
                let mut target = if down { (line + amount).min(last) } else { line.saturating_sub(amount) };
                if !op && !matches!(motion, Motion::Up | Motion::Down) {
                    self.view.top =
                        if down { (self.view.top + amount).min(last) } else { self.view.top.saturating_sub(amount) };
                    let so = self.view.scrolloff.min(h.saturating_sub(1) / 2);
                    let lo = if self.view.top == 0 { 0 } else { self.view.top + so };
                    let hi = (self.view.top + h - 1).saturating_sub(so).max(lo);
                    target = target.clamp(lo.min(last), hi.min(last));
                }
                if target == line && bar && !op {
                    (if down { len } else { 0 }, Exclusive)
                } else if target == line && matches!(motion, Motion::Up | Motion::Down) {
                    return None;
                } else {
                    let own = tx::vcol(r, head, tab);
                    let want = if multi { own } else { *self.view.preferred_vcol.get_or_insert(own) };
                    let allow_end = bar || (visual && want == usize::MAX);
                    (tx::pos_at_vcol(r, target, want, tab, allow_end), Linewise)
                }
            }
            Motion::WordForward { big } => {
                let mut p = head;
                for i in 0..n {
                    let prev = p;
                    p = mv::word_forward(r, p, *big);
                    let pl = r.char_to_line(prev);
                    if op && i + 1 == n && r.char_to_line(p) > pl && tx::line_len(r, pl) > 0 {
                        p = tx::line_end(r, pl).max(prev);
                    }
                }
                (p, Exclusive)
            }
            Motion::WordBackward { big } => ((0..n).fold(head, |p, _| mv::word_backward(r, p, *big)), Exclusive),
            Motion::WordEnd { big } => ((0..n).fold(head, |p, _| mv::word_end(r, p, *big)), Inclusive),
            Motion::WordEndBackward { big } => ((0..n).fold(head, |p, _| mv::word_end_backward(r, p, *big)), Inclusive),
            Motion::WordLeft => ((0..n).fold(head, |p, _| mv::word_left(r, p)), Exclusive),
            Motion::WordRight => ((0..n).fold(head, |p, _| mv::word_right(r, p)), Exclusive),
            Motion::LineStart => (tx::line_start(r, line), Exclusive),
            Motion::FirstNonBlank => (tx::first_non_blank(r, line), Exclusive),
            Motion::LineEnd => {
                let l = (line + n - 1).min(last);
                let p = if bar || (visual && !op) { tx::line_end(r, l) } else { tx::line_last_char(r, l) };
                (p, Inclusive)
            }
            Motion::LastNonBlank => {
                let l = (line + n - 1).min(last);
                let ls = tx::line_start(r, l);
                let mut p = tx::line_end(r, l);
                while p > ls && tx::is_blank(r.char(p - 1)) {
                    p -= 1;
                }
                (p.saturating_sub(1).max(ls), Inclusive)
            }
            Motion::SmartHome => {
                let fnb = tx::first_non_blank(r, line);
                (if head == fnb { tx::line_start(r, line) } else { fnb }, Exclusive)
            }
            Motion::FileStart | Motion::FileEnd | Motion::Line(_) => {
                let l = match motion {
                    Motion::FileStart => count.map_or(0, |c| c.saturating_sub(1)),
                    Motion::Line(k) => k.saturating_sub(1),
                    _ => count.map_or(last, |c| c.saturating_sub(1)),
                };
                (tx::first_non_blank(r, l.min(last)), Linewise)
            }
            Motion::DocumentStart => (0, Exclusive),
            Motion::DocumentEnd => (len, Exclusive),
            Motion::FindChar { ch, forward, till } => {
                s.last_find = Some((*ch, *forward, *till));
                let p = mv::find_char(r, head, *ch, *forward, *till, n, false)?;
                (p, if *forward { Inclusive } else { Exclusive })
            }
            Motion::RepeatFind { reverse } => {
                let (c, f, t) = s.last_find?;
                let forward = f != *reverse;
                let p = mv::find_char(r, head, c, forward, t, n, true)?;
                (p, if forward { Inclusive } else { Exclusive })
            }
            Motion::MatchPair => match count {
                Some(c) => {
                    let l = ((c * (last + 1)).div_ceil(100)).saturating_sub(1).min(last);
                    (tx::first_non_blank(r, l), Linewise)
                }
                None => (mv::match_pair(r, head)?, Inclusive),
            },
            Motion::ParagraphForward => {
                ((0..n).fold(head, |p, _| mv::paragraph_forward(r, r.char_to_line(p))), Exclusive)
            }
            Motion::ParagraphBackward => {
                ((0..n).fold(head, |p, _| mv::paragraph_backward(r, r.char_to_line(p))), Exclusive)
            }
            Motion::ScreenTop | Motion::ScreenMiddle | Motion::ScreenBottom => {
                let top = self.view.top.min(last);
                let bottom = (top + self.view.height.max(1) - 1).min(last);
                let so = self.view.scrolloff.min(self.view.height.saturating_sub(1) / 2);
                let l = match motion {
                    Motion::ScreenTop => (top + n - 1).max(if top > 0 { top + so } else { top }).min(bottom),
                    Motion::ScreenBottom => {
                        let lo = if bottom < last { bottom.saturating_sub(so) } else { bottom };
                        bottom.saturating_sub(n - 1).min(lo).max(top)
                    }
                    _ => (top + bottom) / 2,
                };
                (tx::first_non_blank(r, l), Linewise)
            }
            Motion::Search { pattern, forward } => {
                if !pattern.is_empty() {
                    s.search.pattern = Some(pattern.clone());
                }
                s.search.forward = *forward;
                s.search.highlight = true;
                match search_from(r, head, *forward, n, &s.search) {
                    Ok(p) => (p, Exclusive),
                    Err(e) => {
                        self.message = Some(e);
                        return None;
                    }
                }
            }
            Motion::SearchNext { reverse } => {
                let forward = s.search.forward != *reverse;
                s.search.highlight = true;
                match search_from(r, head, forward, n, &s.search) {
                    Ok(p) => (p, Exclusive),
                    Err(e) => {
                        self.message = Some(e);
                        return None;
                    }
                }
            }
            Motion::Mark { name, linewise } => {
                let Some(&p) = self.marks.get(name) else {
                    self.message = Some("Mark not set".into());
                    return None;
                };
                let p = p.min(len);
                if *linewise { (tx::first_non_blank(r, r.char_to_line(p)), Linewise) } else { (p, Exclusive) }
            }
        };
        Some(res)
    }

    // ---- operatörler ----

    fn operate(&mut self, op: Operator, target: Target, register: Option<char>, s: &mut Session) {
        let head = self.cursor();
        let spans = self.target_spans(op, &target, s);
        if op == Operator::Comment && self.visual.is_some() {
            let from = self.selection.primary().from();
            self.selection = Selection::point(from);
        }
        if self.visual.is_some() {
            self.exit_visual();
        }
        if spans.is_empty() {
            return;
        }
        match op {
            Operator::Yank => {
                self.store(&spans, register, false, s);
                let r = self.buffer.rope();
                let p = match spans[0] {
                    Span::Lines(a, _) if r.char_to_line(head) > a => {
                        tx::pos_at_vcol(r, a, tx::vcol(r, head, self.buffer.tab_width), self.buffer.tab_width, false)
                    }
                    Span::Lines(..) => head,
                    sp => sp.key().0,
                };
                self.selection = Selection::point(p);
            }
            Operator::Delete => {
                self.store(&spans, register, true, s);
                self.delete_spans(&spans);
            }
            Operator::Change => {
                self.store(&spans, register, true, s);
                self.change_spans(&spans);
            }
            Operator::Indent | Operator::Outdent => self.shift_lines(&spans, op == Operator::Indent),
            Operator::ToggleCase | Operator::Lowercase | Operator::Uppercase => self.case_spans(&spans, op, head),
            Operator::Format => {
                let r = self.buffer.rope();
                let p = tx::first_non_blank(r, spans[0].key().0);
                self.selection = Selection::point(p);
            }
            Operator::Comment => self.toggle_comment(&spans),
        }
    }

    fn target_spans(&mut self, op: Operator, target: &Target, s: &mut Session) -> Vec<Span> {
        let line_op = matches!(op, Operator::Indent | Operator::Outdent | Operator::Comment | Operator::Format);
        let mut out = Vec::new();
        match target {
            Target::Range(lr) => {
                if let Some(rg) = self.resolve_range(lr) {
                    out.push(Span::Lines(rg.0, rg.1));
                }
            }
            Target::Selection | Target::SelectionOrLine if self.visual.is_some() => out = self.visual_spans(),
            _ => {
                for g in self.selection.ranges().to_vec() {
                    if let Some(sp) = self.range_span(op, target, g, line_op, s) {
                        out.push(sp);
                    }
                }
            }
        }
        if line_op {
            let r = self.buffer.rope();
            out = out.into_iter().map(|sp| to_lines(r, sp)).collect();
        }
        out.sort_by_key(|sp| sp.key());
        let mut kept: Vec<Span> = Vec::with_capacity(out.len());
        for sp in out {
            let overlaps = match (kept.last(), sp) {
                (Some(Span::Lines(_, b)), Span::Lines(a, _)) => a <= *b,
                (Some(prev), _) => sp.key().0 < prev.key().1,
                (None, _) => false,
            };
            if !overlaps {
                kept.push(sp);
            }
        }
        kept
    }

    fn range_span(&mut self, op: Operator, target: &Target, g: Range, line_op: bool, s: &mut Session) -> Option<Span> {
        let head = g.head;
        match target {
            Target::Motion { motion, count } => {
                if op == Operator::Change
                    && let Motion::WordForward { big } = motion
                {
                    let r = self.buffer.rope();
                    if head < r.len_chars()
                        && !matches!(tx::class(r.char(head), *big), CharClass::Blank | CharClass::Eol)
                    {
                        let mut p = head;
                        for i in 0..count.unwrap_or(1).max(1) {
                            let at_end =
                                p + 1 >= r.len_chars() || tx::class(r.char(p + 1), *big) != tx::class(r.char(p), *big);
                            if !(i == 0 && at_end) {
                                p = mv::word_end(r, p, *big);
                            }
                        }
                        return Some(motion_span(r, head, p, MotionKind::Inclusive));
                    }
                }
                let (pos, kind) = self.motion_pos(head, motion, *count, true, s)?;
                Some(motion_span(self.buffer.rope(), head, pos, kind))
            }
            Target::TextObject { object, around, count } => {
                let r = self.buffer.rope();
                let (a, b, linewise) = textobj::resolve(r, head, *object, *around, *count)?;
                if linewise {
                    Some(Span::Lines(r.char_to_line(a), r.char_to_line(b.saturating_sub(1).max(a))))
                } else {
                    Some(Span::Chars(a, b))
                }
            }
            Target::Lines(n) => {
                let r = self.buffer.rope();
                let l = r.char_to_line(head);
                Some(Span::Lines(l, (l + n.max(&1) - 1).min(tx::last_line(r))))
            }
            Target::Selection | Target::SelectionOrLine => {
                let r = self.buffer.rope();
                if !g.is_empty() {
                    Some(Span::Chars(g.from(), g.to()))
                } else if line_op || matches!(target, Target::SelectionOrLine) {
                    let l = r.char_to_line(head);
                    Some(Span::Lines(l, l))
                } else {
                    None
                }
            }
            Target::Range(_) => None,
        }
    }

    fn visual_spans(&self) -> Vec<Span> {
        let r = self.buffer.rope();
        let len = r.len_chars();
        match self.visual {
            Some(VisualKind::Char) => {
                self.selection.ranges().iter().map(|g| Span::Chars(g.from(), (g.to() + 1).min(len))).collect()
            }
            Some(VisualKind::Line) => self
                .selection
                .ranges()
                .iter()
                .map(|g| Span::Lines(r.char_to_line(g.from()), r.char_to_line(g.to())))
                .collect(),
            Some(VisualKind::Block) => self.block_rows().into_iter().map(|(a, b)| Span::Block(a, b)).collect(),
            None => Vec::new(),
        }
    }

    /// Görsel blok sınırları: (ilk satır, son satır, sol vcol, sağ vcol kapsayıcı, `$` ile satır sonuna mı).
    fn block_bounds(&self) -> (usize, usize, usize, usize, bool) {
        let r = self.buffer.rope();
        let tab = self.buffer.tab_width;
        let g = self.selection.primary();
        let (va, vh) = (tx::vcol(r, g.anchor, tab), tx::vcol(r, g.head, tab));
        let (la, lh) = (r.char_to_line(g.anchor), r.char_to_line(g.head));
        (la.min(lh), la.max(lh), va.min(vh), va.max(vh), self.view.preferred_vcol == Some(usize::MAX))
    }

    fn block_rows(&self) -> Vec<(usize, usize)> {
        let r = self.buffer.rope();
        let tab = self.buffer.tab_width;
        let (first, last, left, right, to_end) = self.block_bounds();
        (first..=last)
            .filter_map(|l| {
                let le = tx::line_end(r, l);
                if tx::vcol(r, le, tab) <= left && !(to_end && left == 0) {
                    return None;
                }
                let a = tx::pos_at_vcol(r, l, left, tab, true);
                let b = if to_end { le } else { (tx::pos_at_vcol(r, l, right, tab, true) + 1).min(le) };
                Some((a, b.max(a)))
            })
            .collect()
    }

    fn exit_visual(&mut self) {
        let Some(kind) = self.visual.take() else { return };
        let g = self.selection.primary();
        self.last_visual = Some((g, kind));
        let r = self.buffer.rope();
        let (a, b) = (g.from(), g.to());
        let (a, b) = if kind == VisualKind::Line {
            (tx::line_start(r, r.char_to_line(a)), tx::line_last_char(r, r.char_to_line(b)))
        } else {
            (a, b)
        };
        self.marks.insert('<', a);
        self.marks.insert('>', b);
        for g in self.selection.ranges_mut() {
            g.anchor = g.head;
        }
    }

    fn content(&self, spans: &[Span]) -> RegisterContent {
        let r = self.buffer.rope();
        match spans[0] {
            Span::Lines(..) => {
                let mut t = String::new();
                for sp in spans {
                    let (a, b) = sp.key();
                    t.push_str(&r.slice(tx::line_start(r, a)..tx::line_end(r, b)).to_string());
                    t.push('\n');
                }
                RegisterContent::new(t, RegisterKind::Line)
            }
            sp => {
                let parts: Vec<String> = spans.iter().map(|sp| r.slice(sp.key().0..sp.key().1).to_string()).collect();
                let kind = if matches!(sp, Span::Block(..)) { RegisterKind::Block } else { RegisterKind::Char };
                RegisterContent::new(parts.join("\n"), kind)
            }
        }
    }

    fn store(&self, spans: &[Span], register: Option<char>, delete: bool, s: &mut Session) {
        let c = self.content(spans);
        s.registers.store(register, c, delete, &mut *s.clipboard);
    }

    fn line_delete_range(&self, a: usize, b: usize) -> (usize, usize) {
        let r = self.buffer.rope();
        let last = tx::last_line(r);
        if b < last {
            (tx::line_start(r, a), tx::line_start(r, b + 1))
        } else if a > 0 {
            (tx::line_end(r, a - 1), r.len_chars())
        } else {
            (0, r.len_chars())
        }
    }

    fn delete_spans(&mut self, spans: &[Span]) {
        let changes = spans
            .iter()
            .map(|sp| match *sp {
                Span::Lines(a, b) => {
                    let (x, y) = self.line_delete_range(a, b);
                    (x, y, String::new())
                }
                Span::Chars(a, b) | Span::Block(a, b) => (a, b, String::new()),
            })
            .collect();
        let starts = self.edit(changes);
        let r = self.buffer.rope();
        let pts: Vec<Range> = starts
            .iter()
            .zip(spans)
            .map(|(&p, sp)| match sp {
                Span::Lines(a, _) => Range::point(tx::first_non_blank(r, (*a).min(tx::last_line(r)))),
                _ => Range::point(p),
            })
            .collect();
        let pts = if matches!(spans[0], Span::Block(..)) { vec![pts[0]] } else { pts };
        self.selection = Selection::new(pts, 0);
    }

    fn change_spans(&mut self, spans: &[Span]) {
        self.cursor_mode = CursorMode::Bar;
        let r = self.buffer.rope();
        let mut indents = Vec::new();
        let changes = spans
            .iter()
            .map(|sp| match *sp {
                Span::Lines(a, b) => {
                    let ind = tx::indent_of(r, a);
                    indents.push(ind.chars().count());
                    (tx::line_start(r, a), tx::line_end(r, b), ind)
                }
                Span::Chars(a, b) | Span::Block(a, b) => {
                    indents.push(0);
                    (a, b, String::new())
                }
            })
            .collect();
        let starts = self.edit(changes);
        let pts = starts.iter().zip(indents).map(|(&p, i)| Range::point(p + i)).collect();
        self.selection = Selection::new(pts, 0);
    }

    fn shift_lines(&mut self, spans: &[Span], indent: bool) {
        let r = self.buffer.rope();
        let tab = self.buffer.tab_width;
        let unit = self.buffer.indent.unit();
        let width = self.buffer.indent.width(tab);
        let mut changes = Vec::new();
        for sp in spans {
            let (a, b) = sp.key();
            for l in a..=b {
                if tx::line_len(r, l) == 0 {
                    continue;
                }
                let ls = tx::line_start(r, l);
                if indent {
                    changes.push((ls, ls, unit.clone()));
                } else {
                    let (mut col, mut i) = (0, ls);
                    while col < width && i < tx::line_end(r, l) {
                        match r.char(i) {
                            ' ' => col += 1,
                            '\t' => col += tab,
                            _ => break,
                        }
                        i += 1;
                    }
                    if i > ls {
                        changes.push((ls, i, String::new()));
                    }
                }
            }
        }
        self.edit(changes);
        if self.cursor_mode == CursorMode::Block {
            let r = self.buffer.rope();
            self.selection = Selection::point(tx::first_non_blank(r, spans[0].key().0));
        }
    }

    fn case_spans(&mut self, spans: &[Span], op: Operator, head: usize) {
        let r = self.buffer.rope();
        let changes = spans
            .iter()
            .map(|sp| {
                let (a, b) = match *sp {
                    Span::Lines(a, b) => (tx::line_start(r, a), tx::line_end(r, b)),
                    other => other.key(),
                };
                let t: String = r.slice(a..b).chars().map(|c| convert_case(c, op)).collect();
                (a, b, t)
            })
            .collect();
        let starts = self.edit(changes);
        let p = if matches!(spans[0], Span::Lines(..)) { head } else { starts[0] };
        self.selection = Selection::point(p);
    }

    fn toggle_comment(&mut self, spans: &[Span]) {
        let r = self.buffer.rope();
        let token = self.buffer.comment.clone();
        let mut lines: Vec<usize> = spans.iter().flat_map(|sp| sp.key().0..=sp.key().1).collect();
        lines.dedup();
        let mut targets: Vec<usize> =
            lines.iter().copied().filter(|&l| !tx::line_text(r, l).trim().is_empty()).collect();
        if targets.is_empty() {
            targets = lines;
        }
        let body = |l: usize| tx::line_text(r, l).trim_start().to_string();
        let mut changes = Vec::new();
        if let Some(tok) = token.line.as_deref() {
            let all = targets.iter().all(|&l| body(l).starts_with(tok));
            let min_indent = targets.iter().map(|&l| tx::indent_of(r, l).chars().count()).min().unwrap_or(0);
            for &l in &targets {
                let ls = tx::line_start(r, l);
                if all {
                    let fnb = tx::first_non_blank(r, l);
                    let mut end = fnb + tok.chars().count();
                    if end < tx::line_end(r, l) && r.char(end) == ' ' {
                        end += 1;
                    }
                    changes.push((fnb, end, String::new()));
                } else {
                    let at = ls + min_indent;
                    changes.push((at, at, format!("{tok} ")));
                }
            }
        } else if let Some((open, close)) = token.block.as_ref() {
            let all = targets.iter().all(|&l| {
                let b = body(l);
                b.starts_with(open.as_str()) && b.trim_end().ends_with(close.as_str())
            });
            for &l in &targets {
                let fnb = tx::first_non_blank(r, l);
                let le = tx::line_end(r, l);
                if all {
                    let text = tx::line_text(r, l);
                    let trimmed = text.trim();
                    let inner = trimmed[open.len()..trimmed.len() - close.len()].trim();
                    changes.push((fnb, le, inner.to_string()));
                } else {
                    changes.push((fnb, le, format!("{open} {} {close}", body(l).trim_end())));
                }
            }
        }
        self.edit(changes);
    }

    // ---- ekleme ----

    fn insert_text(&mut self, text: &str) {
        let r = self.buffer.rope();
        let n = text.chars().count();
        let bar = self.cursor_mode == CursorMode::Bar;
        let changes: Vec<Change> = self
            .selection
            .ranges()
            .iter()
            .map(|g| {
                if bar && !g.is_empty() {
                    (g.from(), g.to(), text.to_string())
                } else if self.overwrite {
                    let le = tx::line_end(r, r.char_to_line(g.head));
                    (g.head, (g.head + n).min(le).max(g.head), text.to_string())
                } else {
                    (g.head, g.head, text.to_string())
                }
            })
            .collect();
        self.replace_with_cursors(changes, |start, len| start + len);
    }

    /// Değişiklikleri uygular ve her birinin yerine `cursor(başlangıç, eklenen uzunluk)` imlecini koyar.
    fn replace_with_cursors(&mut self, changes: Vec<Change>, cursor: impl Fn(usize, usize) -> usize) {
        let mut changes = changes;
        changes.sort_by_key(|c| (c.0, c.1));
        let lens: Vec<usize> = changes.iter().map(|c| c.2.chars().count()).collect();
        let starts = self.edit(changes);
        let pts = starts.iter().zip(lens).map(|(&s, n)| Range::point(cursor(s, n))).collect();
        let primary = self.selection.primary_index();
        self.selection = Selection::new(pts, primary);
    }

    fn insert_newline(&mut self) {
        let r = self.buffer.rope();
        let unit = self.buffer.indent.unit();
        let mut cursors = Vec::new();
        let changes: Vec<Change> = self
            .selection
            .ranges()
            .iter()
            .map(|g| {
                let (a, b) = if self.cursor_mode == CursorMode::Bar { (g.from(), g.to()) } else { (g.head, g.head) };
                let l = r.char_to_line(a);
                let ls = tx::line_start(r, l);
                let indent: String = tx::indent_of(r, l).chars().take(a - ls).collect();
                let before = r.slice(ls..a).to_string();
                let opener = before.trim_end().chars().last();
                let after = r.slice(b..tx::line_end(r, r.char_to_line(b))).to_string();
                let closer = after.trim_start().chars().next();
                let mut t = format!("\n{indent}");
                let mut at = t.chars().count();
                if let Some(o) = opener.filter(|c| matches!(c, '{' | '(' | '[')) {
                    t.push_str(&unit);
                    at = t.chars().count();
                    let close = match o {
                        '{' => '}',
                        '(' => ')',
                        _ => ']',
                    };
                    if closer == Some(close) {
                        t.push('\n');
                        t.push_str(&indent);
                    }
                }
                cursors.push(at);
                (a, b, t)
            })
            .collect();
        let mut order: Vec<(Change, usize)> = changes.into_iter().zip(cursors).collect();
        order.sort_by_key(|(c, _)| (c.0, c.1));
        let (changes, cursors): (Vec<Change>, Vec<usize>) = order.into_iter().unzip();
        let starts = self.edit(changes);
        let pts = starts.iter().zip(cursors).map(|(&s, c)| Range::point(s + c)).collect();
        self.selection = Selection::new(pts, 0);
    }

    fn insert_tab(&mut self, s: &mut Session) {
        let r = self.buffer.rope();
        let multi_line = self.selection.ranges().iter().any(|g| r.char_to_line(g.from()) != r.char_to_line(g.to()));
        if multi_line && self.cursor_mode == CursorMode::Bar {
            return self.operate(Operator::Indent, Target::Selection, None, s);
        }
        let tab = self.buffer.tab_width;
        let indent = self.buffer.indent;
        let bar = self.cursor_mode == CursorMode::Bar;
        let changes = self
            .selection
            .ranges()
            .iter()
            .map(|g| {
                let (a, b) = if bar { (g.from(), g.to()) } else { (g.head, g.head) };
                let t = match indent {
                    crate::buffer::Indent::Tabs => "\t".to_string(),
                    crate::buffer::Indent::Spaces(n) => {
                        let n = n.max(1) as usize;
                        " ".repeat(n - tx::vcol(r, a, tab) % n)
                    }
                };
                (a, b, t)
            })
            .collect();
        self.replace_with_cursors(changes, |s, n| s + n);
    }

    fn backspace(&mut self) {
        let r = self.buffer.rope();
        let indent = self.buffer.indent;
        let bar = self.cursor_mode == CursorMode::Bar;
        let changes: Vec<Change> = self
            .selection
            .ranges()
            .iter()
            .filter_map(|g| {
                if bar && !g.is_empty() {
                    return Some((g.from(), g.to(), String::new()));
                }
                let p = g.head;
                if p == 0 {
                    return None;
                }
                let ls = tx::line_start(r, r.char_to_line(p));
                if let crate::buffer::Indent::Spaces(n) = indent
                    && p > ls
                    && r.slice(ls..p).chars().all(|c| c == ' ')
                {
                    let n = n.max(1) as usize;
                    let col = p - ls;
                    let k = if col.is_multiple_of(n) { n } else { col % n };
                    return Some((p - k, p, String::new()));
                }
                Some((p - 1, p, String::new()))
            })
            .collect();
        if !changes.is_empty() {
            self.replace_with_cursors(changes, |s, _| s);
        }
    }

    /// Seçim varsa onu, yoksa `f(rope, imleç, uzunluk)` aralığını siler.
    fn delete_around(&mut self, f: impl Fn(&Rope, usize, usize) -> (usize, usize)) {
        let r = self.buffer.rope();
        let len = r.len_chars();
        let bar = self.cursor_mode == CursorMode::Bar;
        let changes: Vec<Change> = self
            .selection
            .ranges()
            .iter()
            .map(|g| {
                if bar && !g.is_empty() {
                    (g.from(), g.to(), String::new())
                } else {
                    let (a, b) = f(r, g.head, len);
                    (a, b, String::new())
                }
            })
            .filter(|c| c.1 > c.0)
            .collect();
        if !changes.is_empty() {
            self.replace_with_cursors(changes, |s, _| s);
        }
    }

    fn open_line(&mut self, above: bool) {
        let r = self.buffer.rope();
        let unit = self.buffer.indent.unit();
        let mut lines: Vec<usize> = self.selection.ranges().iter().map(|g| r.char_to_line(g.head)).collect();
        lines.dedup();
        let mut cursor_off = Vec::new();
        let changes = lines
            .iter()
            .map(|&l| {
                let ind = tx::indent_of(r, l);
                if above {
                    cursor_off.push(ind.chars().count());
                    let ls = tx::line_start(r, l);
                    (ls, ls, format!("{ind}\n"))
                } else {
                    let text = tx::line_text(r, l);
                    let extra = if text.trim_end().ends_with(['{', '(', '[']) { unit.as_str() } else { "" };
                    let t = format!("\n{ind}{extra}");
                    cursor_off.push(t.chars().count());
                    let le = tx::line_end(r, l);
                    (le, le, t)
                }
            })
            .collect();
        let starts = self.edit(changes);
        let pts = starts.iter().zip(cursor_off).map(|(&s, o)| Range::point(s + o)).collect();
        self.selection = Selection::new(pts, 0);
    }

    fn join(&mut self, count: usize, spaces: bool) {
        let r = self.buffer.rope();
        let last = tx::last_line(r);
        let (first, n) = if self.visual.is_some() {
            let g = self.selection.primary();
            let (a, b) = (r.char_to_line(g.from()), r.char_to_line(g.to()));
            (a, (b - a + 1).max(2))
        } else {
            (r.char_to_line(self.cursor()), count.max(2))
        };
        self.exit_visual();
        let r = self.buffer.rope();
        let mut changes = Vec::new();
        for l in first..(first + n - 1).min(last) {
            let nl = tx::line_end(r, l);
            if !spaces {
                changes.push((nl, nl + 1, String::new()));
                continue;
            }
            let fnb = tx::first_non_blank(r, l + 1);
            let next_empty = fnb == tx::line_end(r, l + 1);
            let ends_blank = nl > tx::line_start(r, l) && tx::is_blank(r.char(nl - 1));
            let sep = if next_empty || ends_blank || r.char(fnb) == ')' || tx::line_len(r, l) == 0 { "" } else { " " };
            changes.push((nl, fnb, sep.to_string()));
        }
        if changes.is_empty() {
            return;
        }
        let starts = self.edit(changes);
        self.selection = Selection::point(*starts.last().unwrap_or(&0));
    }

    fn replace_char(&mut self, ch: char, count: usize) {
        if self.visual.is_some() {
            let spans = self.visual_spans();
            self.exit_visual();
            let r = self.buffer.rope();
            let changes: Vec<Change> = spans
                .iter()
                .map(|sp| {
                    let (a, b) = match *sp {
                        Span::Lines(a, b) => (tx::line_start(r, a), tx::line_end(r, b)),
                        other => other.key(),
                    };
                    (a, b, r.slice(a..b).chars().map(|c| if c == '\n' { c } else { ch }).collect())
                })
                .collect();
            let starts = self.edit(changes);
            self.selection = Selection::point(starts.first().copied().unwrap_or(0));
            return;
        }
        let r = self.buffer.rope();
        let head = self.cursor();
        let count = count.max(1);
        if head + count > tx::line_end(r, r.char_to_line(head)) {
            return;
        }
        if ch == '\n' {
            let starts = self.edit(vec![(head, head + count, "\n".into())]);
            self.selection = Selection::point(starts[0] + 1);
        } else {
            let starts = self.edit(vec![(head, head + count, ch.to_string().repeat(count))]);
            self.selection = Selection::point(starts[0] + count - 1);
        }
    }

    fn toggle_case_char(&mut self, count: usize, s: &mut Session) {
        if self.visual.is_some() {
            return self.operate(Operator::ToggleCase, Target::Selection, None, s);
        }
        let r = self.buffer.rope();
        let head = self.cursor();
        let end = (head + count.max(1)).min(tx::line_end(r, r.char_to_line(head)));
        if end == head {
            return;
        }
        let t: String = r.slice(head..end).chars().map(|c| convert_case(c, Operator::ToggleCase)).collect();
        self.edit(vec![(head, end, t)]);
        self.selection = Selection::point(end);
    }

    fn paste(&mut self, register: Option<char>, before: bool, count: usize, s: &mut Session) {
        let Some(content) = s.registers.get(register, &mut *s.clipboard) else {
            self.message = Some("Nothing to paste".into());
            return;
        };
        let body = content.text.repeat(count);
        if self.visual.is_some() {
            return self.paste_over_visual(&content, &body, s);
        }
        let r = self.buffer.rope();
        if self.cursor_mode == CursorMode::Bar {
            let line_paste = content.kind == RegisterKind::Line;
            let changes = self
                .selection
                .ranges()
                .iter()
                .map(|g| {
                    if g.is_empty() && line_paste {
                        let ls = tx::line_start(r, r.char_to_line(g.head));
                        (ls, ls, body.clone())
                    } else {
                        (g.from(), g.to(), body.clone())
                    }
                })
                .collect();
            let heads: Vec<usize> = self.selection.ranges().iter().map(|g| g.head).collect();
            let starts = self.edit(changes);
            let n = body.chars().count();
            let pts = if line_paste && self.selection.ranges().iter().all(|g| g.is_empty()) {
                heads.iter().enumerate().map(|(i, &h)| Range::point(h + n * (i + 1))).collect()
            } else {
                starts.iter().map(|&st| Range::point(st + n)).collect()
            };
            self.selection = Selection::new(pts, 0);
            return;
        }
        let head = self.cursor();
        let line = r.char_to_line(head);
        let last = tx::last_line(r);
        match content.kind {
            RegisterKind::Char => {
                let at = if before || tx::line_len(r, line) == 0 { head } else { head + 1 };
                let n = body.chars().count();
                let st = self.edit(vec![(at, at, body.clone())])[0];
                let p = if body.contains('\n') { st } else { st + n.saturating_sub(1) };
                self.selection = Selection::point(p);
            }
            RegisterKind::Line => {
                let mut text = body.clone();
                if !text.ends_with('\n') {
                    text.push('\n');
                }
                let (at, t, target_line) = if before {
                    (tx::line_start(r, line), text, line)
                } else if line == last {
                    (tx::line_end(r, line), format!("\n{}", &text[..text.len() - 1]), line + 1)
                } else {
                    (tx::line_start(r, line + 1), text, line + 1)
                };
                self.edit(vec![(at, at, t)]);
                let r = self.buffer.rope();
                self.selection = Selection::point(tx::first_non_blank(r, target_line));
            }
            RegisterKind::Block => {
                let tab = self.buffer.tab_width;
                let col = tx::vcol(r, head, tab) + usize::from(!before && tx::line_len(r, line) > 0);
                let rows: Vec<&str> = content.text.split('\n').collect();
                let mut changes = Vec::new();
                for (i, row) in rows.iter().enumerate() {
                    let l = line + i;
                    if l > last {
                        let pad = " ".repeat(col);
                        let end = r.len_chars();
                        changes.push((end, end, format!("\n{pad}{row}")));
                        continue;
                    }
                    let width = tx::vcol(r, tx::line_end(r, l), tab);
                    if width < col {
                        let le = tx::line_end(r, l);
                        changes.push((le, le, format!("{}{row}", " ".repeat(col - width))));
                    } else {
                        let at = tx::pos_at_vcol(r, l, col, tab, true);
                        changes.push((at, at, row.to_string()));
                    }
                }
                let merged = merge_same_pos(changes);
                let starts = self.edit(merged);
                self.selection = Selection::point(starts[0]);
            }
        }
    }

    fn paste_over_visual(&mut self, content: &RegisterContent, body: &str, s: &mut Session) {
        let spans = self.visual_spans();
        let kind = self.visual;
        self.exit_visual();
        if spans.is_empty() {
            return;
        }
        self.store(&spans[..1], None, true, s);
        let r = self.buffer.rope();
        let (a, b, t, linewise) = match spans[0] {
            Span::Lines(a, b) => {
                let t = body.strip_suffix('\n').unwrap_or(body).to_string();
                (tx::line_start(r, a), tx::line_end(r, b), t, true)
            }
            sp => {
                let (a, b) = sp.key();
                let t = if content.kind == RegisterKind::Line {
                    format!("\n{}\n", body.strip_suffix('\n').unwrap_or(body))
                } else {
                    body.to_string()
                };
                (a, b, t, false)
            }
        };
        let n = t.chars().count();
        let st = self.edit(vec![(a, b, t)])[0];
        let r = self.buffer.rope();
        let p = if linewise || content.kind == RegisterKind::Line || kind == Some(VisualKind::Line) {
            let l = r.char_to_line((st + usize::from(!linewise)).min(r.len_chars()));
            tx::first_non_blank(r, l)
        } else {
            st + n.saturating_sub(1)
        };
        self.selection = Selection::point(p);
    }

    fn undo_redo(&mut self, count: usize, undo: bool) {
        self.buffer.history.force_close(&self.selection);
        self.visual = None;
        let mut any = false;
        for _ in 0..count {
            let res = if undo { self.buffer.undo() } else { self.buffer.redo() };
            let Some((sel, raw)) = res else { break };
            any = true;
            for (p, removed, inserted) in raw {
                let shift = |m: usize| {
                    if m >= p + removed {
                        m - removed + inserted
                    } else if m > p {
                        p
                    } else {
                        m
                    }
                };
                for m in self.marks.values_mut() {
                    *m = shift(*m);
                }
            }
            self.selection = sel;
        }
        if !any {
            self.message = Some(if undo { "Already at oldest change" } else { "Already at newest change" }.into());
        }
        if self.cursor_mode == CursorMode::Block {
            self.selection.collapse_to_primary();
        }
    }

    fn set_cursor_mode(&mut self, m: CursorMode) {
        if m == CursorMode::Block && self.cursor_mode == CursorMode::Bar {
            let r = self.buffer.rope();
            let first = self.selection.ranges()[0].head;
            let ls = tx::line_start(r, r.char_to_line(first));
            let p = if first > ls { first - 1 } else { first };
            self.selection = Selection::point(p);
            self.overwrite = false;
        }
        self.cursor_mode = m;
    }

    fn set_visual(&mut self, kind: Option<VisualKind>) {
        match kind {
            None => self.exit_visual(),
            Some(k) => {
                if self.visual.is_none() {
                    for g in self.selection.ranges_mut() {
                        g.anchor = g.head;
                    }
                }
                self.cursor_mode = CursorMode::Block;
                self.visual = Some(k);
            }
        }
    }

    fn select_all(&mut self) {
        let len = self.buffer.len_chars();
        if self.cursor_mode == CursorMode::Bar {
            self.selection = Selection::single(Range::new(0, len));
        } else {
            self.visual = Some(VisualKind::Line);
            self.selection = Selection::single(Range::new(0, len.saturating_sub(1)));
        }
    }

    fn select_object(&mut self, object: crate::command::TextObject, around: bool, count: usize) {
        let r = self.buffer.rope();
        let mut out = Vec::new();
        let mut linewise_any = false;
        for g in self.selection.ranges() {
            match textobj::resolve(r, g.head, object, around, count) {
                Some((a, b, lw)) => {
                    linewise_any |= lw;
                    let head = if self.cursor_mode == CursorMode::Bar { b } else { b.saturating_sub(1).max(a) };
                    out.push(Range::new(a, head));
                }
                None => out.push(*g),
            }
        }
        self.selection = Selection::new(out, self.selection.primary_index());
        if linewise_any && self.visual == Some(VisualKind::Char) {
            self.visual = Some(VisualKind::Line);
        }
    }

    fn add_cursor(&mut self, below: bool) {
        let r = self.buffer.rope();
        let tab = self.buffer.tab_width;
        let base = if below {
            self.selection.ranges().iter().max_by_key(|g| g.head)
        } else {
            self.selection.ranges().iter().min_by_key(|g| g.head)
        };
        let Some(base) = base.copied() else { return };
        let line = r.char_to_line(base.head);
        let target = if below { line + 1 } else { line.wrapping_sub(1) };
        if target > tx::last_line(r) {
            return;
        }
        let pos = tx::pos_at_vcol(r, target, tx::vcol(r, base.head, tab), tab, self.cursor_mode == CursorMode::Bar);
        self.selection.push(Range::point(pos));
        let idx = self.selection.len() - 1;
        self.selection.set_primary(idx);
    }

    fn block_to_cursors(&mut self, append: bool) {
        if self.visual != Some(VisualKind::Block) {
            return;
        }
        let r = self.buffer.rope();
        let tab = self.buffer.tab_width;
        let (first, last, left, right, to_end) = self.block_bounds();
        let pts: Vec<Range> = (first..=last)
            .filter_map(|l| {
                let le = tx::line_end(r, l);
                if append {
                    let p = if to_end { le } else { (tx::pos_at_vcol(r, l, right, tab, true) + 1).min(le) };
                    Some(Range::point(p))
                } else if tx::vcol(r, le, tab) >= left && (tx::vcol(r, le, tab) > left || left == 0) {
                    Some(Range::point(tx::pos_at_vcol(r, l, left, tab, true)))
                } else {
                    None
                }
            })
            .collect();
        self.exit_visual();
        if !pts.is_empty() {
            self.selection = Selection::new(pts, 0);
        }
        self.cursor_mode = CursorMode::Bar;
    }

    fn move_lines(&mut self, down: bool) {
        let r = self.buffer.rope();
        let a = self.selection.ranges().iter().map(|g| r.char_to_line(g.from())).min().unwrap_or(0);
        let b = self.selection.ranges().iter().map(|g| r.char_to_line(g.to())).max().unwrap_or(0);
        let last = tx::last_line(r);
        let lines = |x: usize, y: usize| (x..=y).map(|l| tx::line_text(r, l)).collect::<Vec<_>>().join("\n");
        let (start, end, text, delta) = if down {
            if b >= last {
                return;
            }
            let other = tx::line_text(r, b + 1);
            let d = other.chars().count() as isize + 1;
            (tx::line_start(r, a), tx::line_end(r, b + 1), format!("{other}\n{}", lines(a, b)), d)
        } else {
            if a == 0 {
                return;
            }
            let other = tx::line_text(r, a - 1);
            let d = -(other.chars().count() as isize + 1);
            (tx::line_start(r, a - 1), tx::line_end(r, b), format!("{}\n{other}", lines(a, b)), d)
        };
        let shifted: Vec<Range> =
            self.selection.ranges().iter().map(|g| g.map(|p| (p as isize + delta).max(0) as usize)).collect();
        let primary = self.selection.primary_index();
        self.edit(vec![(start, end, text)]);
        self.selection = Selection::new(shifted, primary);
    }

    fn search_word(&mut self, forward: bool, whole: bool, s: &mut Session) {
        let r = self.buffer.rope();
        let head = self.cursor();
        let line = r.char_to_line(head);
        let le = tx::line_end(r, line);
        let Some(start) = (head..le).find(|&i| tx::is_word_char(r.char(i))) else {
            self.message = Some("No string under cursor".into());
            return;
        };
        let mut a = start;
        while a > tx::line_start(r, line) && tx::is_word_char(r.char(a - 1)) {
            a -= 1;
        }
        let mut b = start;
        while b < le && tx::is_word_char(r.char(b)) {
            b += 1;
        }
        let word = r.slice(a..b).to_string();
        s.search.pattern = Some(if whole { format!("\\<{word}\\>") } else { word });
        s.search.forward = forward;
        s.search.highlight = true;
        match search_from(r, a, forward, 1, &s.search) {
            Ok(p) => self.set_heads(|_| p, false),
            Err(e) => self.message = Some(e),
        }
    }

    fn substitute(
        &mut self,
        range: &LineRange,
        pattern: &str,
        replacement: &str,
        global: bool,
        ignore_case: Option<bool>,
        s: &mut Session,
    ) {
        let pattern = if pattern.is_empty() { s.search.pattern.clone().unwrap_or_default() } else { pattern.into() };
        let re = match ignore_case {
            Some(ic) => search::compile(&pattern, ic, false),
            None => search::compile(&pattern, s.search.ignore_case, s.search.smart_case),
        };
        let Some(re) = re else {
            self.message = Some("No previous regular expression".into());
            return;
        };
        s.search.pattern = Some(pattern.clone());
        let Some((a, b)) = self.resolve_range(range) else { return };
        let rep = search::vim_replacement(replacement);
        let r = self.buffer.rope();
        let mut changes = Vec::new();
        for l in a..=b {
            let text = tx::line_text(r, l);
            let new = if global { re.replace_all(&text, rep.as_str()) } else { re.replacen(&text, 1, rep.as_str()) };
            if new != text {
                changes.push((tx::line_start(r, l), tx::line_end(r, l), new.into_owned()));
            }
        }
        if changes.is_empty() {
            self.message = Some(format!("Pattern not found: {pattern}"));
            return;
        }
        let last_len = changes.last().map_or(0, |c: &Change| c.2.chars().count());
        let starts = self.edit(changes);
        let r = self.buffer.rope();
        let p = tx::first_non_blank(r, r.char_to_line(starts.last().unwrap_or(&0) + last_len));
        self.selection = Selection::point(p);
    }

    fn scroll(&mut self, anchor: ScrollAnchor) {
        let line = self.buffer.rope().char_to_line(self.cursor());
        let h = self.view.height.max(1);
        let so = self.view.scrolloff.min(h.saturating_sub(1) / 2);
        self.view.top = match anchor {
            ScrollAnchor::Top => line.saturating_sub(so),
            ScrollAnchor::Center => line.saturating_sub(h / 2),
            ScrollAnchor::Bottom => (line + so + 1).saturating_sub(h),
        };
        self.keep_view = true;
    }

    fn scroll_lines(&mut self, n: isize) {
        let r = self.buffer.rope();
        let last = tx::last_line(r);
        self.view.top = (self.view.top as isize + n).clamp(0, last as isize) as usize;
        self.keep_view = true;
        if self.cursor_mode == CursorMode::Bar {
            return;
        }
        let h = self.view.height.max(1);
        let so = self.view.scrolloff.min(h.saturating_sub(1) / 2);
        let line = r.char_to_line(self.cursor());
        let lo = if self.view.top == 0 { 0 } else { self.view.top + so };
        let hi = (self.view.top + h - 1).saturating_sub(so).min(last).max(lo.min(last));
        let target = line.clamp(lo.min(last), hi);
        if target != line {
            let tab = self.buffer.tab_width;
            let p = tx::pos_at_vcol(r, target, tx::vcol(r, self.cursor(), tab), tab, false);
            self.set_heads(|_| p, false);
        }
    }

    fn resolve_addr(&mut self, addr: &LineAddr) -> Option<usize> {
        let r = self.buffer.rope();
        let last = tx::last_line(r);
        let base = match addr.base {
            LineBase::Current => r.char_to_line(self.cursor()) as isize,
            LineBase::Last => last as isize,
            LineBase::Number(n) => n as isize - 1,
            LineBase::Mark(c) => match self.marks.get(&c) {
                Some(&p) => r.char_to_line(p.min(r.len_chars())) as isize,
                None => {
                    self.message = Some("Mark not set".into());
                    return None;
                }
            },
        };
        Some((base + addr.offset).clamp(0, last as isize) as usize)
    }

    fn resolve_range(&mut self, range: &LineRange) -> Option<(usize, usize)> {
        let a = self.resolve_addr(&range.start)?;
        let b = self.resolve_addr(&range.end)?;
        Some((a.min(b), a.max(b)))
    }

    fn set_option(&mut self, name: String, value: SetValue, s: &mut Session) -> Option<AppCommand> {
        let flag = |cur: bool| match &value {
            SetValue::On => Some(true),
            SetValue::Off => Some(false),
            SetValue::Toggle => Some(!cur),
            _ => None,
        };
        let number = || match &value {
            SetValue::Value(v) => v.parse::<usize>().ok().filter(|n| *n > 0),
            _ => None,
        };
        let query = matches!(value, SetValue::Query);
        let report = |n: &str, v: String| Some(AppCommand::Message(format!("{n}={v}")));
        match name.as_str() {
            "tabstop" | "ts" => {
                if query {
                    return report("tabstop", self.buffer.tab_width.to_string());
                }
                self.buffer.tab_width = number()?;
            }
            "shiftwidth" | "sw" => {
                if query {
                    return report("shiftwidth", self.buffer.indent.width(self.buffer.tab_width).to_string());
                }
                let n = number()?;
                if let crate::buffer::Indent::Spaces(_) = self.buffer.indent {
                    self.buffer.indent = crate::buffer::Indent::Spaces(n.min(16) as u8);
                }
            }
            "expandtab" | "et" => {
                let cur = matches!(self.buffer.indent, crate::buffer::Indent::Spaces(_));
                if query {
                    return report("expandtab", cur.to_string());
                }
                self.buffer.indent = if flag(cur)? {
                    crate::buffer::Indent::Spaces(self.buffer.tab_width.min(16) as u8)
                } else {
                    crate::buffer::Indent::Tabs
                };
            }
            "hlsearch" | "hls" => {
                if query {
                    return report("hlsearch", s.search.highlight.to_string());
                }
                s.search.highlight = flag(s.search.highlight)?;
            }
            "ignorecase" | "ic" => {
                if query {
                    return report("ignorecase", s.search.ignore_case.to_string());
                }
                s.search.ignore_case = flag(s.search.ignore_case)?;
            }
            "smartcase" | "scs" => {
                if query {
                    return report("smartcase", s.search.smart_case.to_string());
                }
                s.search.smart_case = flag(s.search.smart_case)?;
            }
            "wrapscan" | "ws" => {
                if query {
                    return report("wrapscan", s.search.wrap.to_string());
                }
                s.search.wrap = flag(s.search.wrap)?;
            }
            "scrolloff" | "so" => {
                if query {
                    return report("scrolloff", self.view.scrolloff.to_string());
                }
                self.view.scrolloff = match &value {
                    SetValue::Value(v) => v.parse().ok()?,
                    _ => return None,
                };
            }
            _ => return Some(AppCommand::Set { name, value }),
        }
        None
    }
}

/// Hareket sonucundan operatör aralığı. Dışlayıcı hareket satır başında biterse Vim kuralı: önceki satırın sonuna
/// çekilir, başlangıç ilk dolu karakterde ya da öncesindeyse satır bazlı olur (`:h exclusive-linewise`).
fn motion_span(r: &Rope, head: usize, pos: usize, kind: MotionKind) -> Span {
    let (a, b) = (head.min(pos), head.max(pos));
    let len = r.len_chars();
    match kind {
        MotionKind::Linewise => Span::Lines(r.char_to_line(a), r.char_to_line(b)),
        MotionKind::Inclusive => Span::Chars(a, if b < len && r.char(b) != '\n' { b + 1 } else { b }),
        MotionKind::Exclusive => {
            let (la, lb) = (r.char_to_line(a), r.char_to_line(b));
            if b > a && lb > la && b == tx::line_start(r, lb) {
                if a <= tx::first_non_blank(r, la) {
                    return Span::Lines(la, lb - 1);
                }
                return Span::Chars(a, tx::line_end(r, lb - 1));
            }
            Span::Chars(a, b)
        }
    }
}

fn to_lines(r: &Rope, sp: Span) -> Span {
    match sp {
        Span::Lines(..) => sp,
        Span::Chars(a, b) | Span::Block(a, b) => {
            Span::Lines(r.char_to_line(a), r.char_to_line(if b > a { b - 1 } else { a }))
        }
    }
}

fn convert_case(c: char, op: Operator) -> char {
    let flip = |c: char| {
        if c.is_uppercase() { c.to_lowercase().next().unwrap_or(c) } else { c.to_uppercase().next().unwrap_or(c) }
    };
    match op {
        Operator::Uppercase => c.to_uppercase().next().unwrap_or(c),
        Operator::Lowercase => c.to_lowercase().next().unwrap_or(c),
        _ => flip(c),
    }
}

/// Aynı konuma düşen eklemeleri birleştirir (blok yapıştırmada metin sonuna eklenen satırlar).
fn merge_same_pos(changes: Vec<Change>) -> Vec<Change> {
    let mut out: Vec<Change> = Vec::new();
    for c in changes {
        match out.last_mut() {
            Some(l) if l.0 == c.0 && l.1 == c.1 && c.0 == c.1 => l.2.push_str(&c.2),
            _ => out.push(c),
        }
    }
    out
}

/// `head`'den itibaren `n`. eşleşmenin karakter konumu; bulunamazsa İngilizce hata metni.
fn search_from(r: &Rope, head: usize, forward: bool, n: usize, st: &SearchState) -> Result<usize, String> {
    let pat = st.pattern.clone().ok_or_else(|| "No previous search pattern".to_string())?;
    let re = st.regex().ok_or_else(|| format!("Invalid pattern: {pat}"))?;
    let text = r.to_string();
    let mut byte = r.char_to_byte(head);
    for _ in 0..n {
        match search::find(&re, &text, byte, forward, st.wrap) {
            Some((b, _)) => byte = b,
            None => return Err(format!("Pattern not found: {pat}")),
        }
    }
    Ok(r.byte_to_char(byte))
}
