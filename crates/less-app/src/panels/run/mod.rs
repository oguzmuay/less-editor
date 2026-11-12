//! Run paneli (B2): alt şeritte Run ağacı + log tab'ları + debug Variables; `less-run` (`Runner`, `DebugManager`,
//! `VariablesModel`, `Shutdown`) ile `less-ai::term` ızgarasını bağlar.
//!
//! - Ağaç (#16 #34): ⏎ çalıştır / durdur, `d r s l p e m R q`, satır sonu butonları, sağ tık menüsü.
//! - Loglar (#101): çalıştırılmış her öğe bir tab; Cmd+Shift+←/→ gezer; log odaktayken tuşlar sürece gider.
//! - Debug (#91–#97): `RunEvent::Attach` → `DebugManager::attach`; breakpoint / exception'da Variables açılır, son
//!   oturum bitince kapanır; F7/F8/F9/Shift+F8 adım, Cmd+F8 breakpoint.
//! - `(Y/n)` sorusu (#77), ayarlar formu (#69), çıkışta süreç listesi (#80) pencere olarak (`popup`).
//!
//! Paneller arası mesajlar: gelen `run.toggle_breakpoint` (`"<yol>:<satır 1 tabanlı>"`, oluk tıklaması), giden
//! `run.breakpoints` (`"<yol>:<satır,satır…>"`, dosyanın güncel breakpoint satırları; boş = hiç yok).

mod log;
mod popup;
mod tree;
mod vars;

#[cfg(test)]
mod tests;

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Duration;

use eframe::egui::{self, Align2, Color32, FontId, Rect, Sense, Ui, pos2, vec2};
use less_input::{Key, KeyEvent};
use less_run::breakpoints::BreakpointStore;
use less_run::dap::{DebugEvent, DebugManager, StepCommand};
use less_run::shutdown::Shutdown;
use less_run::{Child, Group, GroupKind, Level, Mode, RunEvent, Runner};
use less_theme::Palette;

use super::{Area, Cx, Effect, Event, LayoutOp, Panel, Region, StripPart};
use crate::fonts::icon;
use crate::widgets::{color, ui_font};
use popup::{ConfigForm, Entry, InputFor, Outcome, Popup, ShutdownKey, ShutdownView, Then};
use tree::{Action, Row, Tone};
use vars::{Op, VarsView};

/// Bu klasörün panelleri; host `panels::all()` ile toplar.
pub fn panels() -> Vec<Box<dyn Panel>> {
    vec![Box::new(RunPanel::new())]
}

/// Bu panelde kullanılan Phosphor ikonları (kod noktaları gömülü fontta var; görsel olarak denetlenmedi).
mod icons {
    pub const PLAY: &str = "\u{E3D0}";
    pub const STOP: &str = "\u{E46C}";
    pub const BUG: &str = "\u{E5F4}";
    pub const RERUN: &str = "\u{E036}";
    pub const LOG: &str = "\u{E23A}";
    pub const GEAR: &str = "\u{E270}";
    pub const LINK: &str = "\u{E2E2}";
    pub const LINK_BREAK: &str = "\u{E2E4}";
    pub const SPINNER: &str = "\u{E66A}";
    pub const CHECK_CIRCLE: &str = "\u{E184}";
    pub const X_CIRCLE: &str = "\u{E4F8}";
    pub const CIRCLE: &str = "\u{E18A}";
}

const REGIONS: [Region; 3] =
    [Region::Strip(StripPart::RunTree), Region::Strip(StripPart::RunLog), Region::Strip(StripPart::Variables)];

const ROW_H: f32 = 22.0;
const HEAD_H: f32 = 26.0;
const BUTTON_W: f32 = 22.0;
/// Satır sonu butonları bu genişlikten itibaren çizilir (LESS-nvim: metin ≥ 37 sütun).
const BUTTONS_MIN_W: f32 = 280.0;
/// Ağacın altındaki tuş ipucu satırı.
const HINT_H: f32 = 22.0;
const TREE_HINT: &str = "⏎ run/stop · d debug · r rerun · s stop · l log · e config · p profile · m menu · R rescan";

/// Run paneli.
pub struct RunPanel {
    runner: Runner,
    dbg: DebugManager,
    logs: log::Logs,
    vars: VarsView,
    store: Option<BreakpointStore>,
    /// `None` = varsayılan `breakpoints.json` (test geçici dosya verir).
    bp_file: Option<PathBuf>,
    collapsed: HashSet<String>,
    /// Ağaç imleci (satır kimliği).
    cursor: Option<String>,
    /// Görünen log tab'ı (öğe kimliği); kapanıp açılınca aynı kalır (#94).
    log: Option<String>,
    /// Panel içi odak: ağaç, log ya da Variables.
    part: StripPart,
    popup: Option<Popup>,
    prompts: VecDeque<(String, String)>,
    shutdown: Option<ShutdownView>,
    /// Variables'ı biz gösterdik mi (yerleşim Run kapanınca hatırlar, #94).
    vars_shown: bool,
    wired: bool,
    scanned: bool,
    /// Ağaç ve Variables kaydırması: son karedeki (ofset, görünür yükseklik) ve son görünür kılınan imleç.
    tree_scroll: Scroll,
    vars_scroll: Scroll,
}

/// Liste kaydırma durumu; imleç değişince satırı görünür tutar (#16).
#[derive(Default)]
struct Scroll {
    offset: f32,
    height: f32,
    seen: Option<String>,
}

impl Scroll {
    /// İmleç değiştiyse `index` satırını (bir satır payla) görünür kılan ofseti verir.
    fn want(&mut self, cursor: &Option<String>, index: usize, stride: f32) -> Option<f32> {
        if self.height <= 0.0 || self.seen == *cursor {
            return None;
        }
        self.seen = cursor.clone();
        let top = index as f32 * stride;
        let margin = stride.min((self.height - ROW_H).max(0.0) / 2.0);
        let off = if top - margin < self.offset {
            top - margin
        } else if top + ROW_H + margin > self.offset + self.height {
            top + ROW_H + margin - self.height
        } else {
            return None;
        };
        Some(off.max(0.0))
    }

    fn update<R>(&mut self, out: &egui::scroll_area::ScrollAreaOutput<R>) {
        self.offset = out.state.clamped_offset().y;
        self.height = out.inner_rect.height();
    }
}

impl RunPanel {
    pub fn new() -> Self {
        Self::with(Runner::new(), None)
    }

    /// Verilen çekirdekle (test: geçici `run_configs.json`, breakpoint dosyası).
    pub fn with(runner: Runner, bp_file: Option<PathBuf>) -> Self {
        Self {
            runner,
            dbg: DebugManager::new(),
            logs: log::Logs::default(),
            vars: VarsView::default(),
            store: None,
            bp_file,
            collapsed: HashSet::new(),
            cursor: None,
            log: None,
            part: StripPart::RunTree,
            popup: None,
            prompts: VecDeque::new(),
            shutdown: None,
            vars_shown: false,
            wired: false,
            scanned: false,
            tree_scroll: Scroll::default(),
            vars_scroll: Scroll::default(),
        }
    }

    fn scan(&mut self, roots: &[PathBuf]) {
        self.runner.scan(roots);
        self.scanned = true;
    }

    /// Açılışta: kökler değiştiyse ya da ağaç boşsa tarar (LESS-nvim `open_panel`).
    fn ensure_scanned(&mut self, roots: &[PathBuf]) {
        if !self.scanned || self.runner.roots() != roots || self.runner.tree().is_empty() {
            self.scan(roots);
        }
    }

    fn rows(&self) -> Vec<Row> {
        tree::rows(self.runner.tree(), &self.collapsed)
    }

    fn cursor_index(&self, rows: &[Row]) -> usize {
        self.cursor.as_ref().and_then(|c| rows.iter().position(|r| &r.id == c)).unwrap_or(0)
    }

    fn attached(&self, id: &str) -> bool {
        self.dbg.attached_or_attaching(id)
    }

    // ---- olaylar ----

    fn run_events(&mut self, cx: &mut Cx) {
        for ev in self.runner.poll() {
            match ev {
                RunEvent::Output { id, bytes } => self.logs.feed(&id, &bytes),
                RunEvent::Started { id, .. } => {
                    self.logs.reset(&id);
                    self.log = Some(id);
                }
                RunEvent::Prompt { id, question } => self.prompts.push_back((id, question)),
                RunEvent::Attach { id, target } => self.dbg.attach(&id, target),
                RunEvent::Exited { id, .. } => self.dbg.detach(&id),
                RunEvent::Notice { level: Level::Error, message, .. } => cx.error(message),
                RunEvent::Notice { message, .. } => cx.info(message),
                RunEvent::Status { .. } | RunEvent::Port { .. } => {}
            }
        }
    }

    fn debug_events(&mut self, cx: &mut Cx) {
        for ev in self.dbg.poll() {
            match ev {
                DebugEvent::Stopped { session, reason, frames, .. } => {
                    self.vars.on_stopped(&self.dbg, session);
                    if let Some(f) = frames.first()
                        && let Some(path) = f.source.as_ref().and_then(|s| s.path.clone())
                    {
                        cx.open_file(PathBuf::from(path), Some((f.line.max(1) - 1) as usize));
                    }
                    if reason.contains("breakpoint") || reason == "exception" {
                        cx.layout(LayoutOp::ShowVariables);
                        self.vars_shown = true;
                    }
                }
                DebugEvent::Continued { session } => {
                    if self.vars.session == Some(session) {
                        self.vars.on_continued();
                    }
                }
                DebugEvent::SessionEnded { session, .. } => {
                    if self.dbg.sessions().next().is_none() {
                        self.vars.on_ended();
                        if std::mem::take(&mut self.vars_shown) {
                            cx.layout(LayoutOp::HideVariables);
                        }
                    } else if self.vars.session == Some(session) {
                        self.vars.session = None;
                    }
                }
                DebugEvent::Failed { message, .. } => cx.error(message),
                DebugEvent::State { .. } | DebugEvent::SessionStarted { .. } | DebugEvent::Output { .. } => {}
            }
        }
        for e in self.vars.poll() {
            cx.error(e);
        }
    }

    // ---- pencereler ----

    /// Pencereleri çizer ve tuşlarını egui girdisinden alır (host bunları görmesin).
    fn overlays(&mut self, cx: &mut Cx) {
        let ctx = cx.ctx.clone();
        if let Some(sd) = &mut self.shutdown {
            let mut cancel = sd.show(&ctx, cx.palette, &mut self.runner) == ShutdownKey::Cancel;
            for k in take_keys(&ctx) {
                cancel |= sd.key(&k, &mut self.runner) == ShutdownKey::Cancel;
            }
            self.shutdown_step(cancel, cx);
            return;
        }
        self.next_prompt();
        let Some(p) = &mut self.popup else { return };
        let mut out = p.show(&ctx, cx.palette);
        for k in take_keys(&ctx) {
            if out != Outcome::Keep {
                break;
            }
            if let Some(p) = &mut self.popup {
                out = p.key(&k);
            }
        }
        self.apply(out, cx);
    }

    fn shutdown_step(&mut self, cancel: bool, cx: &mut Cx) {
        let Some(sd) = &self.shutdown else { return };
        if cancel {
            self.shutdown = None;
            cx.info("Quit cancelled");
        } else if sd.shutdown.finished(&self.runner) {
            self.shutdown = None;
            cx.quit();
        } else {
            cx.ctx.request_repaint_after(Duration::from_millis(500));
        }
    }

    /// Sıradaki `(Y/n)` sorusu (hâlâ bekliyorsa) pencereye.
    fn next_prompt(&mut self) {
        while self.popup.is_none()
            && let Some((id, question)) = self.prompts.pop_front()
        {
            if self.runner.item(&id).is_some_and(|s| s.prompt.is_some()) {
                self.popup = Some(Popup::Prompt { id, question, yes: true });
            }
        }
    }

    /// Pencere sonucunu uygular.
    fn apply(&mut self, out: Outcome, cx: &mut Cx) {
        match out {
            Outcome::Keep => return,
            Outcome::Close => {}
            Outcome::Answer { id, yes } => {
                self.runner.answer(&id, yes);
            }
            Outcome::Then(Then::Do(id, action)) => {
                self.popup = None;
                self.act(&id, action, cx);
                return;
            }
            Outcome::Then(Then::Pick(id)) => {
                self.popup = self.menu(&id);
                return;
            }
            Outcome::Input(InputFor::Profile(id), v) => {
                let v = v.trim();
                if let Err(e) = self.runner.set_profile(&id, (!v.is_empty()).then_some(v)) {
                    cx.error(format!("Could not save the profile: {e}"));
                }
            }
            Outcome::Input(InputFor::VarSet(path), value) => {
                self.vars.run(&self.dbg, Op::Set { path, value });
            }
            Outcome::Input(InputFor::VarAppend(path), value) => {
                self.vars.run(&self.dbg, Op::Append { path, value });
            }
            Outcome::Save(id, cfg) => match self.runner.set_config(&id, &cfg, true) {
                Ok(_) => cx.info("Run configuration saved"),
                Err(e) => cx.error(format!("Could not save the run configuration: {e}")),
            },
        }
        self.popup = None;
    }

    /// Öğenin eylem menüsü (`m`, `Space j`).
    fn menu(&self, id: &str) -> Option<Popup> {
        let s = self.runner.item(id)?;
        let attached = self.attached(id);
        let entries = tree::actions(s, attached)
            .into_iter()
            .map(|a| Entry {
                icon: tree::action_icon(a),
                label: tree::action_label(a, s, attached),
                then: Then::Do(id.to_string(), a),
            })
            .collect();
        Some(Popup::Menu { title: s.spec.label.clone(), entries, sel: 0 })
    }

    /// `Space j`: önce öğe, sonra eylem.
    fn picker(&self) -> Option<Popup> {
        /// Grubun öğelerini `Grup › …` yoluyla toplar.
        fn collect(g: &Group, path: &str, out: &mut Vec<(String, String)>) {
            for c in &g.children {
                match c {
                    Child::Group(sub) => collect(sub, &format!("{path}{} › ", sub.label), out),
                    Child::Item(i) => out.push((i.id.clone(), format!("{path}{}", i.label))),
                }
            }
        }
        let mut flat = Vec::new();
        for g in self.runner.tree() {
            collect(g, &format!("{} › ", g.label), &mut flat);
        }
        let entries = flat
            .into_iter()
            .filter_map(|(id, label)| {
                let s = self.runner.item(&id)?;
                Some(Entry { icon: tree::status_icon(s).0, label, then: Then::Pick(id) })
            })
            .collect::<Vec<_>>();
        (!entries.is_empty()).then(|| Popup::Menu { title: "Run".into(), entries, sel: 0 })
    }

    /// Öğe eylemi.
    fn act(&mut self, id: &str, action: Action, cx: &mut Cx) {
        let Some(s) = self.runner.item(id) else { return };
        match action {
            Action::Run => {
                self.runner.start(id, Mode::Run);
            }
            Action::Stop => self.runner.stop(id),
            Action::Debug => {
                let attached = self.attached(id);
                self.runner.debug(id, attached);
            }
            Action::Rerun => self.runner.rerun(id, None),
            Action::ShowLog => {
                if s.seq.is_some() {
                    self.log = Some(id.to_string());
                    self.part = StripPart::RunLog;
                } else {
                    cx.info(format!("{} has no log yet", s.spec.label));
                }
            }
            Action::EditConfig => self.popup = Some(Popup::Form(Box::new(ConfigForm::new(s)))),
            Action::SetProfile => {
                if s.spec.supports_profiles() {
                    let title = format!("{} profile (empty = none)", s.spec.label);
                    let value = s.profile().unwrap_or_default().to_string();
                    self.popup = Some(Popup::Input { title, value, purpose: InputFor::Profile(id.to_string()) });
                }
            }
        }
    }

    // ---- breakpoint ----

    /// Projenin breakpoint deposu (ilk çağrıda açılır, kayıtlı satırlar `run.breakpoints` ile yayınlanır).
    fn store(&mut self, cx: &mut Cx) -> Option<&mut BreakpointStore> {
        if self.store.is_none() {
            let root = cx.roots.first()?;
            let store = match &self.bp_file {
                Some(f) => BreakpointStore::open(f.clone(), root),
                None => BreakpointStore::open_default(root),
            };
            for (file, bps) in store.by_file() {
                let lines: Vec<String> = bps.iter().map(|b| b.line.to_string()).collect();
                cx.send("run.breakpoints", format!("{}:{}", file.display(), lines.join(",")));
            }
            self.dbg.replace_breakpoints(store.by_file());
            self.store = Some(store);
        }
        self.store.as_mut()
    }

    /// Satırda breakpoint aç/kapa (`line` 1 tabanlı); açık oturumlara gönderir, `run.breakpoints` yayınlar.
    fn toggle_breakpoint(&mut self, file: &Path, line: u32, cx: &mut Cx) {
        let Some(store) = self.store(cx) else { return };
        let added = match store.toggle(file, line) {
            Ok(a) => a,
            Err(e) => return cx.error(format!("Could not save breakpoints: {e}")),
        };
        let bps = store.for_file(file);
        let lines: Vec<String> = bps.iter().map(|b| b.line.to_string()).collect();
        self.dbg.set_breakpoints(file, bps);
        let name = file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        cx.info(format!("Breakpoint {} at {name}:{line}", if added { "added" } else { "removed" }));
        cx.send("run.breakpoints", format!("{}:{}", file.display(), lines.join(",")));
    }

    /// Etkin dosyanın imleç satırında breakpoint (Cmd+F8, `Space d b`).
    fn toggle_at_cursor(&mut self, cx: &mut Cx) -> bool {
        let Some(tab) = cx.active else { return false };
        let Some(path) = tab.path.clone() else { return false };
        let line = tab.editor.cursor_line_col().0 as u32 + 1;
        self.toggle_breakpoint(&path, line, cx);
        true
    }

    /// F7/F8/F9/Shift+F8 (#92); oturum yoksa `false`.
    fn step(&mut self, cmd: StepCommand, cx: &mut Cx) -> bool {
        let session = self.dbg.stopped_session().or_else(|| self.dbg.sessions().next()).map(|s| s.id);
        let Some(id) = session else { return false };
        if cmd != StepCommand::Pause && self.dbg.session(id).is_some_and(|s| s.stopped.is_none()) {
            cx.info("The program is running");
            return true;
        }
        self.dbg.step(id, cmd);
        true
    }

    fn toggle_vars(&mut self, cx: &mut Cx) {
        if self.vars_shown {
            cx.layout(LayoutOp::HideVariables);
            self.vars_shown = false;
            self.part = StripPart::RunTree;
        } else {
            cx.layout(LayoutOp::ShowVariables);
            self.vars_shown = true;
        }
    }

    // ---- tuşlar ----

    fn next_part(&mut self) {
        self.part = match self.part {
            StripPart::RunTree => StripPart::RunLog,
            StripPart::RunLog if self.vars_shown => StripPart::Variables,
            _ => StripPart::RunTree,
        };
    }

    fn cycle_log(&mut self, step: i32) -> bool {
        match self.runner.cycle_log(self.log.as_deref(), step) {
            Some(id) => {
                self.log = Some(id);
                true
            }
            None => false,
        }
    }

    /// Panel odaktayken tuş.
    fn key(&mut self, k: KeyEvent, cx: &mut Cx) -> bool {
        let tab_nav = k.mods.cmd && (k.mods.shift || k.mods.alt) && matches!(k.key, Key::Left | Key::Right);
        if tab_nav {
            return self.cycle_log(if k.key == Key::Right { 1 } else { -1 });
        }
        if k.key == Key::Tab && !k.mods.cmd {
            self.next_part();
            return true;
        }
        match self.part {
            StripPart::RunLog => self.log_key(k, cx),
            StripPart::Variables => self.vars_key(k, cx),
            _ => self.tree_key(k, cx),
        }
    }

    fn tree_key(&mut self, k: KeyEvent, cx: &mut Cx) -> bool {
        let rows = self.rows();
        let i = self.cursor_index(&rows);
        let row = rows.get(i).cloned();
        let plain = !k.mods.ctrl && !k.mods.alt && !k.mods.cmd;
        let go = |me: &mut Self, j: usize| {
            if let Some(r) = rows.get(j) {
                me.cursor = Some(r.id.clone());
            }
        };
        match k.key {
            Key::Up => go(self, i.saturating_sub(1)),
            Key::Down => go(self, (i + 1).min(rows.len().saturating_sub(1))),
            Key::Home | Key::PageUp => go(self, 0),
            Key::End | Key::PageDown => go(self, rows.len().saturating_sub(1)),
            Key::Esc => cx.layout(LayoutOp::Focus(Area::Code)),
            Key::Enter => match row {
                Some(r) if r.is_group() => self.toggle_group(&r.id, None),
                Some(r) => self.runner.toggle(&r.id),
                None => {}
            },
            Key::Char(' ') if plain => {
                if let Some(r) = row.filter(Row::is_group) {
                    self.toggle_group(&r.id, None);
                }
            }
            Key::Right => {
                if let Some(r) = row.filter(Row::is_group) {
                    self.toggle_group(&r.id, Some(true));
                }
            }
            Key::Left => match row {
                Some(r) if r.group.is_some_and(|(_, open)| open) => self.toggle_group(&r.id, Some(false)),
                Some(r) => {
                    if let Some(p) = r.parent {
                        go(self, p);
                    }
                }
                None => {}
            },
            Key::Char('R') if plain => {
                let roots = cx.roots.to_vec();
                self.scan(&roots);
            }
            Key::Char('q') if plain => cx.layout(LayoutOp::Close(Area::Run)),
            Key::Char(c @ ('d' | 'r' | 's' | 'l' | 'p' | 'e' | 'm')) if plain => {
                let Some(r) = row.filter(|r| !r.is_group()) else { return true };
                let action = match c {
                    'd' => Action::Debug,
                    'r' => Action::Rerun,
                    's' => Action::Stop,
                    'l' => Action::ShowLog,
                    'p' => Action::SetProfile,
                    'e' => Action::EditConfig,
                    _ => {
                        self.popup = self.menu(&r.id);
                        return true;
                    }
                };
                self.act(&r.id, action, cx);
            }
            _ => return false,
        }
        true
    }

    fn toggle_group(&mut self, id: &str, open: Option<bool>) {
        let open = open.unwrap_or_else(|| self.collapsed.contains(id));
        if open {
            self.collapsed.remove(id);
        } else {
            self.collapsed.insert(id.to_string());
        }
    }

    /// Log odaktayken: Esc ağaca döner, Shift+PageUp/Down kaydırır, kalan tuşlar çalışan sürece yazılır.
    fn log_key(&mut self, k: KeyEvent, _cx: &mut Cx) -> bool {
        let Some(id) = self.log.clone() else {
            self.part = StripPart::RunTree;
            return k.key == Key::Esc;
        };
        if k.key == Key::Esc {
            self.part = StripPart::RunTree;
            return true;
        }
        let term = self.logs.get(&id);
        if k.mods.shift && matches!(k.key, Key::PageUp | Key::PageDown) {
            if let Some(t) = term {
                t.scroll_page(k.key == Key::PageUp);
            }
            return true;
        }
        let app_cursor = term.is_some_and(|t| t.mode().app_cursor);
        let Some(bytes) = log::encode(&k, app_cursor) else { return false };
        if let Some(t) = term {
            t.scroll_to_bottom();
        }
        self.runner.write(&id, &bytes)
    }

    fn vars_key(&mut self, k: KeyEvent, cx: &mut Cx) -> bool {
        let plain = !k.mods.ctrl && !k.mods.alt && !k.mods.cmd;
        let row = self.vars.current();
        match k.key {
            Key::Up => self.vars.move_cursor(-1),
            Key::Down => self.vars.move_cursor(1),
            Key::Home | Key::PageUp => self.vars.move_cursor(isize::MIN / 2),
            Key::End | Key::PageDown => self.vars.move_cursor(isize::MAX / 2),
            Key::Esc => self.part = StripPart::RunTree,
            Key::Char('q') if plain => self.toggle_vars(cx),
            Key::Right => {
                if let Some(r) = row.filter(|r| r.expandable && !r.expanded) {
                    self.vars.toggle(&self.dbg, &r.path);
                }
            }
            Key::Left => {
                if let Some(r) = row {
                    if r.expandable && r.expanded {
                        self.vars.toggle(&self.dbg, &r.path);
                    } else if let Some(p) = self.vars.model.parent(&r.path) {
                        self.vars.cursor = Some(p.path.clone());
                    }
                }
            }
            Key::Enter => match row {
                Some(r) if r.expandable => self.vars.toggle(&self.dbg, &r.path),
                Some(r) => self.edit_var(&r.path, None),
                None => {}
            },
            Key::F(2) => self.edit_var_current(None),
            Key::Char('e') if plain => self.edit_var_current(None),
            Key::Char('n') | Key::Delete if plain => {
                let null = less_run::variables::VariablesModel::null_literal(self.vars.lang(&self.dbg));
                self.edit_var_current(Some(null));
            }
            Key::Char('d') if plain => {
                if let Some(r) = row {
                    self.vars.run(&self.dbg, Op::Delete { path: r.path });
                }
            }
            Key::Char('a') if plain => {
                if let Some(r) = row {
                    match self.vars.model.append_target(&r.path) {
                        Ok((list, _)) => {
                            let title = "New element (expression)".to_string();
                            self.popup =
                                Some(Popup::Input { title, value: String::new(), purpose: InputFor::VarAppend(list) });
                        }
                        Err(e) => cx.error(e),
                    }
                }
            }
            Key::Char('K') if plain => {
                if let Some(n) = row.and_then(|r| self.vars.model.node(&r.path).cloned()) {
                    cx.open_text(format!("{} (value)", n.name), n.value.unwrap_or_default(), None);
                }
            }
            _ => return false,
        }
        true
    }

    fn edit_var_current(&mut self, value: Option<&str>) {
        if let Some(r) = self.vars.current() {
            self.edit_var(&r.path, value);
        }
    }

    /// `e` / F2 / ⏎: dolu giriş penceresi; `value` verilirse (null) doğrudan atar.
    fn edit_var(&mut self, path: &str, value: Option<&str>) {
        let Some(n) = self.vars.model.node(path).filter(|n| !n.is_scope).cloned() else { return };
        match value {
            Some(v) => {
                self.vars.run(&self.dbg, Op::Set { path: path.to_string(), value: v.to_string() });
            }
            None => {
                let title = format!("Set value of {}", n.name);
                let value = n.value.unwrap_or_default();
                self.popup = Some(Popup::Input { title, value, purpose: InputFor::VarSet(path.to_string()) });
            }
        }
    }

    // ---- çizim ----

    fn tree_ui(&mut self, ui: &mut Ui, cx: &mut Cx) {
        let p = cx.palette;
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0.0, color(p.surface));
        header(ui, p, "RUN", cx.focused && self.part == StripPart::RunTree);
        let rows = self.rows();
        if rows.is_empty() {
            let at = rect.left_top() + vec2(14.0, HEAD_H + 16.0);
            let msg = "Nothing runnable found (Spring Boot / package.json)";
            ui.painter().text(at, Align2::LEFT_TOP, msg, ui_font(12.5), color(p.comment));
            return;
        }
        let cur = self.cursor_index(&rows);
        let buttons = rect.width() >= BUTTONS_MIN_W;
        let mut clicked: Option<(usize, bool)> = None;
        let mut act: Option<(String, Action)> = None;
        let foot = Rect::from_min_max(pos2(rect.left(), rect.bottom() - HINT_H), rect.right_bottom());
        ui.painter().hline(foot.x_range(), foot.top() + 0.5, egui::Stroke::new(1.0, color(p.border)));
        let hint_font = ui_font(11.5);
        text_upto(
            &ui.painter_at(foot),
            pos2(foot.left() + 12.0, foot.center().y),
            TREE_HINT,
            hint_font,
            color(p.comment),
            foot.right() - 8.0,
        );
        let list = Rect::from_min_max(ui.cursor().min, pos2(rect.right(), foot.top()));
        let mut list_ui = ui.new_child(egui::UiBuilder::new().max_rect(list));
        let mut area = egui::ScrollArea::vertical().id_salt("run-tree").auto_shrink([false, false]);
        let stride = ROW_H + list_ui.spacing().item_spacing.y;
        if let Some(off) = self.tree_scroll.want(&self.cursor, cur, stride) {
            area = area.vertical_scroll_offset(off);
        }
        let out = area.show_rows(&mut list_ui, ROW_H, rows.len(), |ui, range| {
            for (i, row) in rows.iter().enumerate().take(range.end).skip(range.start) {
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                let focused = cx.focused && self.part == StripPart::RunTree;
                if i == cur {
                    let fill = if focused { color(p.accent).gamma_multiply(0.35) } else { color(p.bg_highlight) };
                    ui.painter().rect_filled(r.shrink2(vec2(4.0, 0.0)), 5.0, fill);
                } else if resp.hovered() {
                    ui.painter().rect_filled(r.shrink2(vec2(4.0, 0.0)), 5.0, color(p.bg_highlight).gamma_multiply(0.6));
                }
                let hit = self.row_ui(ui, p, row, r, buttons);
                if let Some(a) = hit {
                    act = Some((row.id.clone(), a));
                } else if resp.double_clicked() {
                    clicked = Some((i, true));
                } else if resp.clicked() {
                    clicked = Some((i, false));
                }
                if !row.is_group()
                    && let Some(s) = self.runner.item(&row.id)
                {
                    let attached = self.attached(&row.id);
                    resp.context_menu(|ui| {
                        for a in tree::actions(s, attached) {
                            let label = format!("{}  {}", tree::action_icon(a), tree::action_label(a, s, attached));
                            if ui.button(label).clicked() {
                                act = Some((row.id.clone(), a));
                                ui.close();
                            }
                        }
                    });
                }
            }
        });
        self.tree_scroll.update(&out);
        if let Some((id, a)) = act {
            self.cursor = Some(id.clone());
            self.act(&id, a, cx);
        }
        if let Some((i, double)) = clicked {
            let row = &rows[i];
            self.cursor = Some(row.id.clone());
            self.part = StripPart::RunTree;
            if row.is_group() {
                self.toggle_group(&row.id, None);
            } else if double {
                self.runner.toggle(&row.id);
            }
        }
    }

    /// Satırın içeriği; tıklanan satır sonu butonunun eylemi. Metin butonların soluna kadar `…` ile kısaltılır,
    /// hiçbir şey butonların üstüne çizilmez (#34).
    fn row_ui(&self, ui: &mut Ui, p: &Palette, row: &Row, r: Rect, buttons: bool) -> Option<Action> {
        let painter = ui.painter_at(r);
        let y = r.center().y;
        let mut x = r.left() + 10.0 + row.depth as f32 * 14.0;
        let limit = if buttons { buttons_left(r) - 6.0 } else { r.right() - 8.0 };
        if let Some((kind, open)) = row.group {
            let caret = if open { icon::CARET_DOWN } else { icon::CARET_RIGHT };
            text_upto(&painter, pos2(x, y), caret, ui_font(11.0), color(p.comment), r.right() - 8.0);
            let glyph = if kind == GroupKind::Package { icon::FOLDER } else { icon::FOLDER_OPEN };
            text_upto(&painter, pos2(x + 16.0, y), glyph, ui_font(14.0), color(p.icon), r.right() - 8.0);
            let end = text_upto(&painter, pos2(x + 38.0, y), &row.label, ui_font(13.0), color(p.fg), r.right() - 8.0);
            if let Some(d) = &row.detail {
                text_upto(&painter, pos2(end + 8.0, y), d, ui_font(12.0), color(p.comment), r.right() - 8.0);
            }
            return None;
        }
        let s = self.runner.item(&row.id)?;
        let (glyph, tone) = tree::status_icon(s);
        x += 16.0;
        text_upto(&painter, pos2(x, y), glyph, ui_font(14.0), tone_color(p, tone), limit);
        let mut end = text_upto(&painter, pos2(x + 22.0, y), &row.label, ui_font(13.0), color(p.fg), limit);
        let detail = tree::item_detail(s);
        if !detail.is_empty() {
            end = text_upto(&painter, pos2(end + 8.0, y), &detail, ui_font(12.0), color(p.comment), limit);
        }
        if s.has_config() {
            end = text_upto(&painter, pos2(end + 8.0, y), icons::GEAR, ui_font(12.0), color(p.comment), limit);
        }
        let sessions = self.dbg.root_session_count(&row.id);
        if let Some((badge, tone)) = tree::debug_badge(s, self.dbg.state(&row.id), sessions) {
            text_upto(&painter, pos2(end + 8.0, y), &badge, ui_font(12.0), tone_color(p, tone), limit);
        }
        if !buttons {
            return None;
        }
        let mut hit = None;
        let list = tree::buttons(s);
        for (n, a) in list.iter().enumerate() {
            let bx = buttons_left(r) + n as f32 * BUTTON_W;
            let br = Rect::from_min_size(pos2(bx, r.top()), vec2(BUTTON_W, ROW_H));
            let resp = ui.interact(br, ui.scope_id().with(("run-btn", &row.id, n)), Sense::click());
            let enabled = *a != Action::ShowLog || s.seq.is_some();
            let tint = match a {
                Action::Stop => p.error,
                Action::Run => p.extra.diff_add,
                Action::Debug => p.extra.warn,
                _ => p.accent,
            };
            let fg = if !enabled {
                color(p.comment).gamma_multiply(0.5)
            } else if resp.hovered() {
                color(tint)
            } else {
                color(tint).gamma_multiply(0.8)
            };
            if resp.hovered() && enabled {
                ui.painter().rect_filled(br.shrink(2.0), 4.0, color(p.bg_highlight));
            }
            ui.painter().text(br.center(), Align2::CENTER_CENTER, tree::action_icon(*a), ui_font(13.0), fg);
            if resp.clicked() && enabled {
                hit = Some(*a);
            }
        }
        hit
    }

    fn log_ui(&mut self, ui: &mut Ui, cx: &mut Cx) {
        let p = cx.palette;
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0.0, color(p.surface));
        let bar = Rect::from_min_size(rect.left_top(), vec2(rect.width(), HEAD_H));
        ui.painter().hline(bar.left()..=bar.right(), bar.bottom() - 0.5, egui::Stroke::new(1.0, color(p.border)));
        let mut x = bar.left() + 8.0;
        let mut pick = None;
        for s in self.runner.log_items() {
            let (glyph, tone) = tree::status_icon(s);
            let text = format!("{glyph} {}", s.spec.label);
            let galley = ui.painter().layout_no_wrap(text.clone(), ui_font(12.5), color(p.fg));
            let tab = Rect::from_min_size(pos2(x, bar.top() + 3.0), vec2(galley.size().x + 16.0, HEAD_H - 6.0));
            let resp = ui.interact(tab, ui.scope_id().with(("run-log-tab", &s.spec.id)), Sense::click());
            let current = self.log.as_deref() == Some(s.spec.id.as_str());
            if current {
                ui.painter().rect_filled(tab, 5.0, color(p.bg_highlight));
            }
            let fg = if current { color(p.fg) } else { color(p.fg_dim) };
            ui.painter().text(
                tab.left_center() + vec2(8.0, 0.0),
                Align2::LEFT_CENTER,
                glyph,
                ui_font(12.5),
                tone_color(p, tone),
            );
            let label_x =
                tab.left() + 8.0 + ui.painter().layout_no_wrap(format!("{glyph} "), ui_font(12.5), fg).size().x;
            ui.painter().text(pos2(label_x, tab.center().y), Align2::LEFT_CENTER, &s.spec.label, ui_font(12.5), fg);
            if resp.clicked() {
                pick = Some(s.spec.id.clone());
            }
            x = tab.right() + 4.0;
        }
        if let Some(id) = pick {
            self.log = Some(id);
            self.part = StripPart::RunLog;
        }
        let body = Rect::from_min_max(pos2(rect.left() + 6.0, bar.bottom() + 4.0), rect.right_bottom());
        let cell = log::cell_size(ui);
        if self.logs.resize(log::fit(body, cell)) {
            let size = self.logs.size();
            self.runner.resize(size.rows, size.cols);
        }
        let Some(term) = self.log.as_deref().and_then(|id| self.logs.get(id)) else {
            let msg = "Run an item to see its log here";
            ui.painter().text(
                body.left_top() + vec2(8.0, 10.0),
                Align2::LEFT_TOP,
                msg,
                ui_font(12.5),
                color(p.comment),
            );
            return;
        };
        let resp = ui.interact(body, ui.scope_id().with("run-log-body"), Sense::click());
        if resp.clicked() {
            self.part = StripPart::RunLog;
        }
        if resp.hovered() {
            let dy = ui.input(|i| i.smooth_scroll_delta.y);
            let lines = (dy / cell.1).round() as i32;
            if lines != 0 {
                term.scroll(lines);
            }
        }
        let snap = term.snapshot();
        log::paint(ui, body, &snap, p, cell);
        let Some(pos) = resp.hover_pos() else { return };
        let (col, row) = (((pos.x - body.left()) / cell.0) as usize, ((pos.y - body.top()) / cell.1) as usize);
        let Some(link) = log::links(&snap).into_iter().find(|l| l.contains(row, col)) else { return };
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        let stroke = egui::Stroke::new(1.0, color(p.accent));
        for &(r, a, b) in &link.spans {
            let y = body.top() + (r + 1) as f32 * cell.1 - 2.0;
            let xs = body.left() + a as f32 * cell.0..=body.left() + b as f32 * cell.0;
            ui.painter_at(body).hline(xs, y, stroke);
        }
        if resp.clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(link.url));
        }
    }

    fn vars_ui(&mut self, ui: &mut Ui, cx: &mut Cx) {
        let p = cx.palette;
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0.0, color(p.surface));
        ui.painter().vline(rect.left() + 0.5, rect.y_range(), egui::Stroke::new(1.0, color(p.border)));
        let focused = cx.focused && self.part == StripPart::Variables;
        let state = if self.vars.is_running() { "  running" } else { "" };
        header(ui, p, &format!("VARIABLES{state}"), focused);
        let rows = self.vars.rows();
        if rows.is_empty() {
            let msg = "Variables appear when the debugger stops";
            ui.painter().text(
                rect.left_top() + vec2(14.0, HEAD_H + 16.0),
                Align2::LEFT_TOP,
                msg,
                ui_font(12.5),
                color(p.comment),
            );
            return;
        }
        let cur = self.vars.cursor_index(&rows);
        let cell = log::cell_size(ui);
        let name_w =
            (rows.iter().map(|r| r.name_cell.chars().count()).max().unwrap_or(4).clamp(4, 30) as f32 + 2.0) * cell.0;
        let type_w =
            (rows.iter().map(|r| r.type_.chars().count()).max().unwrap_or(4).clamp(4, 20) as f32 + 2.0) * cell.0;
        let dim = if self.vars.is_running() { 0.5 } else { 1.0 };
        let font = egui::FontId::monospace(log::LOG_FONT);
        let mut clicked: Option<(String, bool)> = None;
        let mut area = egui::ScrollArea::vertical().id_salt("run-vars").auto_shrink([false, false]);
        let stride = ROW_H + ui.spacing().item_spacing.y;
        if let Some(off) = self.vars_scroll.want(&self.vars.cursor, cur, stride) {
            area = area.vertical_scroll_offset(off);
        }
        let out = area.show_rows(ui, ROW_H, rows.len(), |ui, range| {
            for (i, row) in rows.iter().enumerate().take(range.end).skip(range.start) {
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                if i == cur {
                    let fill = if focused { color(p.accent).gamma_multiply(0.35) } else { color(p.bg_highlight) };
                    ui.painter().rect_filled(r, 0.0, fill);
                }
                let painter = ui.painter_at(r);
                let y = r.center().y;
                let x0 = r.left() + 10.0;
                let name_c = if row.is_scope { color(p.accent) } else { color(p.fg) };
                painter.text(
                    pos2(x0, y),
                    Align2::LEFT_CENTER,
                    &row.name_cell,
                    font.clone(),
                    name_c.gamma_multiply(dim),
                );
                let vc = if row.changed { color(p.extra.warn) } else { color(p.fg_dim) };
                let max_chars = ((r.width() - name_w - type_w - 20.0) / cell.0).max(4.0) as usize;
                let value = clip(&row.value, max_chars);
                painter.text(pos2(x0 + name_w, y), Align2::LEFT_CENTER, value, font.clone(), vc.gamma_multiply(dim));
                painter.text(
                    pos2(r.right() - type_w, y),
                    Align2::LEFT_CENTER,
                    &row.type_,
                    font.clone(),
                    color(p.comment).gamma_multiply(dim),
                );
                if resp.double_clicked() {
                    clicked = Some((row.path.clone(), true));
                } else if resp.clicked() {
                    clicked = Some((row.path.clone(), false));
                }
            }
        });
        self.vars_scroll.update(&out);
        if let Some((path, double)) = clicked {
            self.vars.cursor = Some(path);
            self.part = StripPart::Variables;
            if double {
                self.vars_key(KeyEvent::plain(Key::Enter), cx);
            }
        }
    }
}

impl Default for RunPanel {
    fn default() -> Self {
        Self::new()
    }
}

/// Pencere açıkken klavye olaylarını egui girdisinden alır (host görmesin) ve uygulama tuşlarına çevirir.
fn take_keys(ctx: &egui::Context) -> Vec<KeyEvent> {
    let events = ctx.input(|i| i.events.clone());
    ctx.input_mut(|i| {
        i.events.retain(|e| {
            !matches!(
                e,
                egui::Event::Key { .. }
                    | egui::Event::Text(_)
                    | egui::Event::Paste(_)
                    | egui::Event::Copy
                    | egui::Event::Cut
            )
        })
    });
    crate::keys::translate(&events)
}

fn header(ui: &mut Ui, p: &Palette, title: &str, focused: bool) {
    let rect = ui.max_rect();
    let (head, _) = ui.allocate_exact_size(vec2(rect.width(), HEAD_H), Sense::hover());
    let c = if focused { color(p.accent) } else { color(p.comment) };
    ui.painter().text(head.left_center() + vec2(14.0, 0.0), Align2::LEFT_CENTER, title, ui_font(11.0), c);
}

fn tone_color(p: &Palette, tone: Tone) -> Color32 {
    match tone {
        Tone::Dim => color(p.comment),
        Tone::Ok => color(p.extra.diff_add),
        Tone::Busy => color(p.extra.warn),
        Tone::Error => color(p.error),
        Tone::Debug => color(p.extra.warn),
    }
}

/// Satır sonu butonlarının sol kenarı.
fn buttons_left(r: Rect) -> f32 {
    r.right() - 8.0 - 4.0 * BUTTON_W
}

/// `text`'i `at`'ten (sol, dikey orta) `limit` x'ine kadar çizer, sığmazsa `…` ile kısaltır; yer yoksa çizmez.
/// Bitişin x'ini döner.
fn text_upto(painter: &egui::Painter, at: egui::Pos2, text: &str, font: FontId, c: Color32, limit: f32) -> f32 {
    let w = limit - at.x;
    if w < 6.0 || text.is_empty() {
        return at.x;
    }
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_string(), font, c);
    job.wrap = egui::text::TextWrapping::truncate_at_width(w);
    let galley = painter.layout_job(job);
    let rect = Align2::LEFT_CENTER.anchor_size(at, galley.size());
    painter.galley(rect.min, galley, c);
    rect.right()
}

/// En çok `n` karakter; fazlası `…`.
fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_string() } else { s.chars().take(n.saturating_sub(1)).chain(['…']).collect() }
}

impl Panel for RunPanel {
    fn id(&self) -> &'static str {
        "run"
    }

    fn regions(&self) -> &[Region] {
        &REGIONS
    }

    fn tick(&mut self, cx: &mut Cx) {
        if !self.wired {
            self.wired = true;
            let ctx = cx.ctx.clone();
            self.runner.set_waker(move || ctx.request_repaint());
            let ctx = cx.ctx.clone();
            self.dbg.set_waker(move || ctx.request_repaint());
            self.vars.set_waker(cx.ctx.clone());
        }
        let _ = self.store(cx);
        self.run_events(cx);
        self.debug_events(cx);
        self.overlays(cx);
    }

    fn ui(&mut self, region: Region, ui: &mut Ui, cx: &mut Cx) {
        if !self.scanned {
            let roots = cx.roots.to_vec();
            self.scan(&roots);
        }
        match region {
            Region::Strip(StripPart::RunTree) => self.tree_ui(ui, cx),
            Region::Strip(StripPart::RunLog) => self.log_ui(ui, cx),
            Region::Strip(StripPart::Variables) => {
                self.vars_shown = true;
                self.vars_ui(ui, cx);
            }
            _ => {}
        }
    }

    fn event(&mut self, event: &Event, cx: &mut Cx) -> bool {
        match *event {
            Event::Key(k) => self.key(k, cx),
            Event::Shortcut(k) => {
                let m = k.mods;
                let bare = !m.shift && !m.ctrl && !m.alt && !m.cmd;
                match k.key {
                    Key::F(8) if m.cmd && !m.shift => self.toggle_at_cursor(cx),
                    Key::F(9) if bare => self.step(StepCommand::Continue, cx),
                    Key::F(8) if bare => self.step(StepCommand::Next, cx),
                    Key::F(7) if bare => self.step(StepCommand::StepIn, cx),
                    Key::F(8) if m.shift && !m.cmd && !m.ctrl && !m.alt => self.step(StepCommand::StepOut, cx),
                    _ => false,
                }
            }
            Event::Leader("j") => {
                let roots = cx.roots.to_vec();
                self.ensure_scanned(&roots);
                match self.picker() {
                    Some(p) => self.popup = Some(p),
                    None => cx.info("Nothing runnable found (Spring Boot / package.json)"),
                }
                true
            }
            Event::Leader("du") => {
                self.toggle_vars(cx);
                true
            }
            Event::Leader("db") => self.toggle_at_cursor(cx),
            Event::Layout(Effect::Opened(Area::Run)) => {
                let roots = cx.roots.to_vec();
                self.ensure_scanned(&roots);
                self.part = StripPart::RunTree;
                if self.vars_shown && self.dbg.sessions().next().is_none() {
                    self.vars_shown = false;
                    cx.layout(LayoutOp::HideVariables);
                }
                false
            }
            Event::Message { topic: "run.toggle_breakpoint", data } => {
                let Some((file, line)) = data.rsplit_once(':') else { return false };
                let Ok(line) = line.trim().parse::<u32>() else { return false };
                self.toggle_breakpoint(Path::new(file), line.max(1), cx);
                true
            }
            _ => false,
        }
    }

    /// #80: çalışan süreç varsa hepsine SIGINT, liste penceresi; hepsi kapanınca `cx.quit()`.
    fn can_quit(&mut self, cx: &mut Cx) -> bool {
        if self.shutdown.is_some() {
            return false;
        }
        match Shutdown::begin(&mut self.runner) {
            None => true,
            Some(shutdown) => {
                self.shutdown = Some(ShutdownView { shutdown, sel: 0 });
                cx.ctx.request_repaint();
                false
            }
        }
    }
}
