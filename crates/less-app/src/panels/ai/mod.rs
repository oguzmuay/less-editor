//! AI + Terminal sağ sütun paneli (B3): AI paneli üstte, Terminal altta (#6, #20); her ikisinde tab çubuğu,
//! Cmd+T (AI'da tür seçici, Terminal'de yeni shell), Cmd+Shift+←/→ tab gezme, `:R` yeniden adlandırma (#27),
//! AI türleri ve son seçilen türün varsayılan olması (#64), Claude hesapları (`:Claude account`, #63), birden çok AI
//! sekmesi varken en solda maskotlu özet tab'ı (#60). Süreçler, oturum okuma ve posta kutusu (#65) `less-ai`'da;
//! bu modül yalnız arayüz ve yönlendirme. Sözleşme: `less-rs/docs/handoff/B0.md`.
//!
//! Panel kapanınca ya da Database onu gizleyince (#45) süreçler ölmez; panel nesnesi düşerken (pencere kapanışı)
//! tüm süreçler sonlandırılır. Açılış düzeninin AI kısmı (#81) ilk `tick`'te istenir.

mod summary;
#[cfg(test)]
mod tests;
mod view;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use eframe::egui::{self, Align2, Rect, Ui, UiBuilder, pos2, vec2};
use less_ai::accounts::Accounts;
use less_ai::agents::{self, AgentKind, AiState};
use less_ai::summary::{FRAME_EVERY, MascotClock, unix_now};
use less_ai::tabs::{Group, Item, NewTab, REFRESH_EVERY, TabId, TermTabs, default_shell};
use less_ai::term::{self as aterm, KeyOptions, SpawnOptions, encode_key};
use less_input::{Key, KeyEvent};

use super::{Area, Cx, Effect, Event, LayoutOp, Panel, Region};
use crate::widgets::{color, ui_font};
use view::TermView;

/// Bu klasörün panelleri; host `panels::all()` ile toplar.
pub fn panels() -> Vec<Box<dyn Panel>> {
    vec![Box::new(AiPanel::new())]
}

const REGIONS: [Region; 2] = [Region::Area(Area::Ai), Region::Area(Area::Terminal)];
const BAR_H: f32 = 26.0;

/// Sekme süreçlerinin komutu: gerçek komutlar ya da (test derlemesi) her sekmede aynı sabit komut. Test derlemesinde
/// gerçek `claude` / `codex` hiç başlatılmaz, `ai.json` / hesap kaydına yazılmaz.
#[derive(Clone, Debug)]
enum Launch {
    Real,
    Fake(Vec<String>),
}

/// Metin sorusunun amacı.
#[derive(Clone, Debug, PartialEq)]
enum Purpose {
    Ollama,
    Rename(TabId),
    AccountName,
    AccountDir(String),
}

/// Panelin gövdesinin üstünde açılan küçük soru penceresi.
#[derive(Clone, Debug, PartialEq)]
enum Prompt {
    /// Cmd+T / `+`: AI türü seçici.
    Kinds {
        sel: usize,
    },
    /// `:Claude account`.
    Accounts {
        sel: usize,
    },
    /// "Close session '<ad>'?" (`yes` = seçili düğme).
    Confirm {
        tab: TabId,
        yes: bool,
    },
    Text {
        purpose: Purpose,
        value: String,
        focused: bool,
    },
}

/// Çizim sırasında toplanan, çizimden sonra uygulanan işler.
enum Act {
    Select(Group, Item),
    Close(TabId),
    New(Group),
    Rename(TabId),
    Pick(usize),
    Confirm(bool),
    Submit(String),
    Cancel,
    Summary(summary::Picked),
}

fn group_of(area: Area) -> Option<Group> {
    match area {
        Area::Ai => Some(Group::Ai),
        Area::Terminal => Some(Group::Shell),
        _ => None,
    }
}

fn area_of(g: Group) -> Area {
    match g {
        Group::Ai => Area::Ai,
        Group::Shell => Area::Terminal,
    }
}

fn gi(g: Group) -> usize {
    match g {
        Group::Ai => 0,
        Group::Shell => 1,
    }
}

/// `less-input` tuşu → terminal tuşu.
pub fn term_key(k: Key) -> aterm::Key {
    match k {
        Key::Char(c) => aterm::Key::Char(c),
        Key::Enter => aterm::Key::Enter,
        Key::Esc => aterm::Key::Escape,
        Key::Tab => aterm::Key::Tab,
        Key::Backspace => aterm::Key::Backspace,
        Key::Delete => aterm::Key::Delete,
        Key::Left => aterm::Key::Left,
        Key::Right => aterm::Key::Right,
        Key::Up => aterm::Key::Up,
        Key::Down => aterm::Key::Down,
        Key::Home => aterm::Key::Home,
        Key::End => aterm::Key::End,
        Key::PageUp => aterm::Key::PageUp,
        Key::PageDown => aterm::Key::PageDown,
        Key::F(n) => aterm::Key::F(n),
    }
}

fn term_mods(k: &KeyEvent) -> aterm::Modifiers {
    aterm::Modifiers { shift: k.mods.shift, ctrl: k.mods.ctrl, alt: k.mods.alt, cmd: k.mods.cmd }
}

/// Tab çubuğu etiketini `max` karaktere kısaltır (`…`).
pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// `~/x` → ev klasörü altında.
fn expand_home(s: &str) -> PathBuf {
    match s.strip_prefix("~") {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => {
            less_ai::paths::home().join(rest.trim_start_matches('/'))
        }
        _ => PathBuf::from(s),
    }
}

/// AI + Terminal paneli.
pub struct AiPanel {
    tabs: Option<TermTabs>,
    launch: Launch,
    /// İlk `tick`'te açılış düzeninin AI kısmını iste (#81).
    autostart: bool,
    started: bool,
    /// `[AI, Terminal]` görünüyor mu (yerleşim olaylarından).
    open: [bool; 2],
    focus: Group,
    views: HashMap<TabId, TermView>,
    prompt: Option<(Group, Prompt)>,
    clock: MascotClock,
    summary_sel: usize,
}

impl AiPanel {
    pub fn new() -> Self {
        if cfg!(test) {
            Self::with_launch(Launch::Fake(vec!["/bin/sh".into()]), false)
        } else {
            Self::with_launch(Launch::Real, true)
        }
    }

    fn with_launch(launch: Launch, autostart: bool) -> Self {
        Self {
            tabs: None,
            launch,
            autostart,
            started: false,
            open: [false; 2],
            focus: Group::Ai,
            views: HashMap::new(),
            prompt: None,
            clock: MascotClock::default(),
            summary_sel: 0,
        }
    }

    /// Sekme modeli + süreçler; ilk kullanımda kurulur (kayıtlar o anda okunur).
    fn tabs(&mut self, ctx: &egui::Context) -> &mut TermTabs {
        let launch = &self.launch;
        self.tabs.get_or_insert_with(|| {
            let mut t = match launch {
                Launch::Real => TermTabs::new(),
                Launch::Fake(_) => {
                    let nowhere = Path::new("/nonexistent/less-rs-test");
                    TermTabs::with_parts(
                        Accounts::load_from(&nowhere.join("accounts.json"), None, nowhere),
                        AiState::load_from(&nowhere.join("ai.json"), None),
                    )
                }
            };
            let ctx = ctx.clone();
            t.set_on_change(move || ctx.request_repaint());
            t
        })
    }

    /// Tek kökte o kök, çok köklü workspace'te köklerin ortak üst klasörü; ortak klasör home ya da kök dizinse ilk kök.
    fn cwd(cx: &Cx) -> PathBuf {
        common_dir(cx.roots, &less_ai::paths::home()).unwrap_or_else(less_ai::paths::home)
    }

    fn current(&mut self, g: Group) -> Option<Item> {
        self.tabs.as_mut()?.model.current(g)
    }

    fn current_tab(&mut self, g: Group) -> Option<TabId> {
        match self.current(g)? {
            Item::Tab(id) => Some(id),
            Item::Summary => None,
        }
    }

    fn new_shell(&mut self, cx: &mut Cx) {
        let cwd = Self::cwd(cx);
        let argv = match &self.launch {
            Launch::Real => vec![default_shell()],
            Launch::Fake(a) => a.clone(),
        };
        let opts = SpawnOptions { argv, cwd: Some(cwd.clone()), ..Default::default() };
        if let Err(e) = self.tabs(cx.ctx).spawn(Group::Shell, opts, NewTab { cwd, ..Default::default() }) {
            cx.error(format!("Failed to start terminal: {e}"));
        }
    }

    /// AI sekmesi açar; gerçek kipte tür varsayılan olur, hesap / posta kutusu `less-ai`'da hazırlanır.
    fn new_agent(&mut self, kind: AgentKind, model: Option<&str>, cx: &mut Cx) {
        let cwd = Self::cwd(cx);
        let launch = self.launch.clone();
        let t = self.tabs(cx.ctx);
        let res = match launch {
            Launch::Real => t.open_agent(kind, &cwd, model, Vec::new()),
            Launch::Fake(argv) => {
                let used: Vec<String> = t.model.tabs(Group::Ai).iter().filter_map(|t| t.ai_name.clone()).collect();
                let name = agents::next_name(kind, used.iter().map(String::as_str));
                let opts = SpawnOptions { argv, cwd: Some(cwd.clone()), ..Default::default() };
                let init = NewTab { cwd, agent: Some(kind), ai_name: Some(name), ..Default::default() };
                t.spawn(Group::Ai, opts, init)
            }
        };
        if let Err(e) = res {
            cx.error(format!("Failed to start {}: {e}", kind.label()));
        }
    }

    /// Türü başlatır; Ollama'da önce model sorulur (son model önerilir).
    fn start_kind(&mut self, kind: AgentKind, cx: &mut Cx) {
        if kind == AgentKind::Ollama {
            let value = self.tabs(cx.ctx).ai_state.ollama_model().to_string();
            self.prompt = Some((Group::Ai, Prompt::Text { purpose: Purpose::Ollama, value, focused: false }));
        } else {
            self.new_agent(kind, None, cx);
        }
    }

    fn new_tab(&mut self, g: Group, cx: &mut Cx) {
        match g {
            Group::Ai => {
                let default = self.tabs(cx.ctx).ai_state.default_kind();
                let sel = AgentKind::ORDER.iter().position(|k| *k == default).unwrap_or(0);
                self.prompt = Some((Group::Ai, Prompt::Kinds { sel }));
            }
            Group::Shell => self.new_shell(cx),
        }
    }

    /// Boş panel açılınca: AI'da son seçilen tür, Terminal'de shell (#64).
    fn on_open(&mut self, g: Group, cx: &mut Cx) {
        if self.prompt.as_ref().is_some_and(|(pg, _)| *pg == g) || !self.tabs(cx.ctx).model.tabs(g).is_empty() {
            return;
        }
        match g {
            Group::Ai => {
                let kind = self.tabs(cx.ctx).ai_state.default_kind();
                self.start_kind(kind, cx);
            }
            Group::Shell => self.new_shell(cx),
        }
    }

    fn close_tab(&mut self, id: TabId, cx: &mut Cx) {
        let t = self.tabs(cx.ctx);
        if t.needs_confirm(id) {
            let g = t.model.group_of(id).unwrap_or(Group::Ai);
            self.prompt = Some((g, Prompt::Confirm { tab: id, yes: false }));
        } else {
            t.close(id);
        }
    }

    fn cycle(&mut self, g: Group, delta: i32) {
        if let Some(t) = self.tabs.as_mut() {
            t.model.cycle(g, delta);
        }
    }

    /// `:R` hedefi: odaktaki grubun görünen sekmesi, sonra AI, sonra Terminal.
    fn rename_target(&mut self) -> Option<TabId> {
        let first = self.focus;
        let other = if first == Group::Ai { Group::Shell } else { Group::Ai };
        [first, other].into_iter().find_map(|g| {
            let t = self.tabs.as_mut()?;
            match t.model.current(g)? {
                Item::Tab(id) => Some(id),
                Item::Summary => t.model.active(g),
            }
        })
    }

    fn rename(&mut self, id: TabId, name: &str, cx: &mut Cx) {
        let t = self.tabs(cx.ctx);
        t.model.rename(id, name);
        if let Some(tab) = t.model.tab(id) {
            cx.info(format!("Tab name: {}", tab.label()));
        }
    }

    fn ex(&mut self, cmd: &str, cx: &mut Cx) -> bool {
        let (name, arg) = cmd.trim().split_once(' ').map(|(a, b)| (a, b.trim())).unwrap_or((cmd.trim(), ""));
        match name {
            "R" | "TerminalRename" => match self.rename_target() {
                None => cx.error("No terminal tab found to rename"),
                Some(id) if !arg.is_empty() => self.rename(id, arg, cx),
                Some(id) => {
                    let t = self.tabs(cx.ctx);
                    let g = t.model.group_of(id).unwrap_or(Group::Ai);
                    let value = t.model.tab(id).and_then(|t| t.name.clone()).unwrap_or_default();
                    self.prompt = Some((g, Prompt::Text { purpose: Purpose::Rename(id), value, focused: false }));
                    cx.focus(area_of(g));
                }
            },
            "TerminalNew" if arg.is_empty() => {
                self.new_shell(cx);
                cx.focus(Area::Terminal);
            }
            "ClaudeNew" if arg.is_empty() => {
                self.new_agent(AgentKind::Claude, None, cx);
                cx.focus(Area::Ai);
            }
            "TermTabClose" if arg.is_empty() => match self.current_tab(self.focus) {
                Some(id) => self.close_tab(id, cx),
                None => cx.error("No terminal/Claude tab to close"),
            },
            "Claude" if arg == "account" => {
                let current = self.tabs(cx.ctx).accounts.current();
                let sel = self.tabs(cx.ctx).accounts.list().iter().position(|a| *a == current).unwrap_or(0);
                self.prompt = Some((Group::Ai, Prompt::Accounts { sel }));
                cx.focus(Area::Ai);
            }
            _ => return false,
        }
        true
    }

    fn layout(&mut self, e: Effect) {
        let (area, open) = match e {
            Effect::Opened(a) | Effect::Restored(a) => (a, Some(true)),
            Effect::Closed(a) | Effect::Hidden(a) => (a, Some(false)),
            Effect::Focused(a) => (a, None),
        };
        let Some(g) = group_of(area) else { return };
        match open {
            Some(o) => self.open[gi(g)] = o,
            None => self.focus = g,
        }
        if matches!(e, Effect::Opened(_)) {
            self.focus = g;
        }
        if open == Some(false) && self.prompt.as_ref().is_some_and(|(pg, _)| *pg == g) {
            self.prompt = None;
        }
    }

    fn copy(&mut self, g: Group, cx: &mut Cx) {
        let Some(id) = self.current_tab(g) else { return };
        if let Some(text) = self.tabs.as_ref().and_then(|t| t.terminal(id)).and_then(|t| t.selection_text()) {
            cx.ctx.copy_text(text);
        }
    }

    fn paste(&mut self, g: Group, cx: &mut Cx) {
        let text = cx.ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Paste(t) => Some(t.clone()),
                _ => None,
            })
        });
        let (Some(text), Some(id)) = (text, self.current_tab(g)) else { return };
        if let Some(term) = self.tabs.as_ref().and_then(|t| t.terminal(id)) {
            term.paste(&text);
        }
    }

    /// Sağ tık "Send to AI" (`ai.send`): metin etkin AI sekmesine yapıştırılır, AI paneli odaklanır.
    fn send_to_ai(&mut self, text: &str, cx: &mut Cx) {
        let t = self.tabs(cx.ctx);
        let Some(id) = t.model.active(Group::Ai) else {
            cx.error("No AI session is open");
            return;
        };
        t.model.select(Group::Ai, Item::Tab(id));
        if let Some(term) = t.terminal(id) {
            term.paste(text);
        }
        cx.focus(Area::Ai);
    }

    fn key(&mut self, k: KeyEvent, cx: &mut Cx) -> bool {
        let g = self.focus;
        if self.prompt.as_ref().is_some_and(|(pg, _)| *pg == g) && !k.mods.cmd {
            return self.prompt_key(k, cx);
        }
        let m = k.mods;
        if m.cmd && !m.ctrl {
            match k.key {
                Key::Char('t') if !m.shift && !m.alt => {
                    self.new_tab(g, cx);
                    return true;
                }
                Key::Char('w') if !m.shift && !m.alt => {
                    if let Some(id) = self.current_tab(g) {
                        self.close_tab(id, cx);
                    }
                    return true;
                }
                Key::Left | Key::Right if m.shift != m.alt => {
                    self.cycle(g, if k.key == Key::Right { 1 } else { -1 });
                    return true;
                }
                Key::Char('c') if !m.shift && !m.alt => {
                    self.copy(g, cx);
                    return true;
                }
                Key::Char('v') if !m.shift && !m.alt => {
                    self.paste(g, cx);
                    return true;
                }
                _ => {}
            }
            if crate::keys::shortcut(&k).is_some()
                || encode_key(term_key(k.key), term_mods(&k), KeyOptions::default()).is_empty()
            {
                return false;
            }
        }
        match self.current(g) {
            Some(Item::Summary) => self.summary_key(k, cx),
            Some(Item::Tab(id)) => {
                if let Some(t) = self.tabs.as_ref() {
                    t.send_key(id, term_key(k.key), term_mods(&k));
                }
                true
            }
            None => false,
        }
    }

    fn summary_key(&mut self, k: KeyEvent, cx: &mut Cx) -> bool {
        let n = self.tabs(cx.ctx).model.tabs(Group::Ai).len();
        match k.key {
            Key::Up => self.summary_sel = self.summary_sel.saturating_sub(1),
            Key::Down => self.summary_sel = (self.summary_sel + 1).min(n.saturating_sub(1)),
            Key::Enter => self.summary_go(self.summary_sel, cx),
            _ => return false,
        }
        true
    }

    fn summary_go(&mut self, index: usize, cx: &mut Cx) {
        let t = self.tabs(cx.ctx);
        if let Some(id) = t.model.tabs(Group::Ai).get(index).map(|t| t.id) {
            t.model.select(Group::Ai, Item::Tab(id));
        }
    }

    fn prompt_len(&mut self, p: &Prompt, cx: &mut Cx) -> usize {
        match p {
            Prompt::Kinds { .. } => AgentKind::ORDER.len(),
            Prompt::Accounts { .. } => self.tabs(cx.ctx).accounts.list().len() + 1,
            _ => 0,
        }
    }

    fn prompt_key(&mut self, k: KeyEvent, cx: &mut Cx) -> bool {
        let Some((g, mut p)) = self.prompt.clone() else { return false };
        let len = self.prompt_len(&p, cx);
        match (&mut p, k.key) {
            (_, Key::Esc) => return self.apply(Act::Cancel, cx),
            (Prompt::Kinds { sel } | Prompt::Accounts { sel }, Key::Up) => *sel = sel.saturating_sub(1),
            (Prompt::Kinds { sel } | Prompt::Accounts { sel }, Key::Down) => *sel = (*sel + 1).min(len - 1),
            (Prompt::Kinds { sel } | Prompt::Accounts { sel }, Key::Enter) => return self.apply(Act::Pick(*sel), cx),
            (Prompt::Kinds { .. }, Key::Char(c @ '1'..='4')) => {
                return self.apply(Act::Pick(c as usize - '1' as usize), cx);
            }
            (Prompt::Confirm { .. }, Key::Char('y' | 'Y')) => return self.apply(Act::Confirm(true), cx),
            (Prompt::Confirm { .. }, Key::Char('n' | 'N')) => return self.apply(Act::Confirm(false), cx),
            (Prompt::Confirm { yes, .. }, Key::Left | Key::Right | Key::Tab) => *yes = !*yes,
            (Prompt::Confirm { yes, .. }, Key::Enter) => return self.apply(Act::Confirm(*yes), cx),
            (Prompt::Text { value, .. }, Key::Enter) => return self.apply(Act::Submit(value.clone()), cx),
            _ => {}
        }
        self.prompt = Some((g, p));
        true
    }

    fn apply(&mut self, act: Act, cx: &mut Cx) -> bool {
        match act {
            Act::Select(g, item) => self.tabs(cx.ctx).model.select(g, item),
            Act::Close(id) => self.close_tab(id, cx),
            Act::New(g) => self.new_tab(g, cx),
            Act::Rename(id) => {
                let t = self.tabs(cx.ctx);
                let g = t.model.group_of(id).unwrap_or(Group::Ai);
                let value = t.model.tab(id).and_then(|t| t.name.clone()).unwrap_or_default();
                self.prompt = Some((g, Prompt::Text { purpose: Purpose::Rename(id), value, focused: false }));
            }
            Act::Cancel => self.prompt = None,
            Act::Summary(p) => {
                self.summary_sel = p.index;
                if p.go {
                    self.summary_go(p.index, cx);
                }
            }
            Act::Pick(i) => match self.prompt.take() {
                Some((_, Prompt::Kinds { .. })) => {
                    if let Some(kind) = AgentKind::ORDER.get(i) {
                        self.start_kind(*kind, cx);
                    }
                }
                Some((g, Prompt::Accounts { .. })) => {
                    let t = self.tabs(cx.ctx);
                    match t.accounts.list().get(i).map(|a| a.name.clone()) {
                        Some(name) => match t.accounts.select(&name) {
                            Ok(()) => cx.info(format!("New Claude sessions use account: {name}")),
                            Err(e) => cx.error(e.to_string()),
                        },
                        None => {
                            let p =
                                Prompt::Text { purpose: Purpose::AccountName, value: String::new(), focused: false };
                            self.prompt = Some((g, p));
                        }
                    }
                }
                other => self.prompt = other,
            },
            Act::Confirm(yes) => {
                if let Some((_, Prompt::Confirm { tab, .. })) = self.prompt.take()
                    && yes
                {
                    self.tabs(cx.ctx).close(tab);
                }
            }
            Act::Submit(value) => {
                let value = value.trim().to_string();
                let Some((g, Prompt::Text { purpose, .. })) = self.prompt.take() else { return true };
                match purpose {
                    Purpose::Ollama => {
                        let model = if value.is_empty() { None } else { Some(value.as_str()) };
                        self.new_agent(AgentKind::Ollama, model, cx);
                    }
                    Purpose::Rename(id) => self.rename(id, &value, cx),
                    Purpose::AccountName if value.is_empty() => {}
                    Purpose::AccountName => {
                        if self.tabs(cx.ctx).accounts.list().iter().any(|a| a.name == value) {
                            cx.error(format!("Account already exists: {value}"));
                        } else {
                            let p = Prompt::Text {
                                purpose: Purpose::AccountDir(value.clone()),
                                value: format!("~/.{value}"),
                                focused: false,
                            };
                            self.prompt = Some((g, p));
                        }
                    }
                    Purpose::AccountDir(_) if value.is_empty() => {}
                    Purpose::AccountDir(name) => match self.tabs(cx.ctx).accounts.add(&name, Some(expand_home(&value)))
                    {
                        Ok(()) => cx.info(format!("New Claude sessions use account: {name}")),
                        Err(e) => cx.error(e.to_string()),
                    },
                }
            }
        }
        true
    }

    fn tab_bar(&mut self, ui: &mut Ui, rect: Rect, g: Group, cx: &mut Cx, acts: &mut Vec<Act>) {
        let p = cx.palette;
        ui.painter().rect_filled(rect, 0.0, color(p.surface));
        let t = self.tabs(cx.ctx);
        let items = t.model.items(g);
        let shown = t.model.current(g);
        let per = ((rect.width() / 8.0 - 4.0) / items.len().max(1) as f32 - 6.0).max(6.0) as usize;
        let mut bar = ui.new_child(
            UiBuilder::new()
                .max_rect(rect.shrink2(vec2(6.0, 2.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        bar.spacing_mut().item_spacing.x = 2.0;
        for item in items {
            let selected = Some(item) == shown;
            let label = match item {
                Item::Summary => "≡ Summary".to_string(),
                Item::Tab(id) => {
                    let tab = t.model.tab(id);
                    let name = truncate(&tab.map(|t| t.label()).unwrap_or_default(), per);
                    match g {
                        Group::Ai => format!("{} {name}", tab.map(|t| t.status_icon()).unwrap_or("◌")),
                        Group::Shell => name,
                    }
                }
            };
            let fg = if selected { color(p.fg) } else { color(p.comment) };
            let text = egui::RichText::new(label).font(ui_font(12.0)).color(fg);
            let resp = bar.add(egui::Button::new(text).selected(selected).frame_when_inactive(false));
            if resp.clicked() {
                acts.push(Act::Select(g, item));
            }
            if let Item::Tab(id) = item {
                if resp.middle_clicked() {
                    acts.push(Act::Close(id));
                }
                resp.context_menu(|ui| {
                    if ui.button("Rename…").clicked() {
                        acts.push(Act::Rename(id));
                        ui.close();
                    }
                    if ui.button("Close").clicked() {
                        acts.push(Act::Close(id));
                        ui.close();
                    }
                });
                if selected {
                    let x = egui::RichText::new("✕").font(ui_font(11.0)).color(color(p.comment));
                    if bar.add(egui::Button::new(x).frame(false)).on_hover_text("Close tab").clicked() {
                        acts.push(Act::Close(id));
                    }
                }
            }
        }
        let plus = egui::RichText::new("+").font(ui_font(14.0)).color(color(p.comment));
        let hint = if g == Group::Ai { "New AI session (Cmd+T)" } else { "New terminal (Cmd+T)" };
        if bar.add(egui::Button::new(plus).frame(false)).on_hover_text(hint).clicked() {
            acts.push(Act::New(g));
        }
    }

    fn prompt_ui(&mut self, ui: &mut Ui, body: Rect, cx: &mut Cx, acts: &mut Vec<Act>) {
        let Some((_, prompt)) = self.prompt.as_ref() else { return };
        let p = cx.palette;
        let width = body.width().clamp(120.0, 360.0);
        let area = Rect::from_min_size(pos2(body.center().x - width / 2.0, body.top() + 12.0), vec2(width, 10.0));
        let mut child = ui.new_child(UiBuilder::new().max_rect(area));
        let row = |ui: &mut Ui, text: String, selected: bool| {
            let fg = if selected { color(p.fg) } else { color(p.fg_dim) };
            ui.add(
                egui::Button::selectable(selected, egui::RichText::new(text).color(fg))
                    .min_size(vec2(width - 20.0, 22.0)),
            )
        };
        let accounts: Vec<(String, String, bool)> = match prompt {
            Prompt::Accounts { .. } => {
                let t = self.tabs.as_ref().map(|t| &t.accounts);
                let current = t.map(|a| a.current());
                t.map(|a| a.list())
                    .unwrap_or_default()
                    .iter()
                    .map(|a| {
                        let home = less_ai::paths::home();
                        let dir = a.dir.strip_prefix(&home).map(|r| format!("~/{}", r.display()));
                        (
                            a.name.clone(),
                            dir.unwrap_or_else(|_| a.dir.display().to_string()),
                            Some(a) == current.as_ref(),
                        )
                    })
                    .collect()
            }
            _ => Vec::new(),
        };
        let default_kind = self.tabs.as_ref().map(|t| t.ai_state.default_kind());
        let label = match prompt {
            Prompt::Confirm { tab, .. } => {
                self.tabs.as_ref().and_then(|t| t.model.tab(*tab)).map(|t| t.label()).unwrap_or_default()
            }
            _ => String::new(),
        };
        let Some((_, prompt)) = self.prompt.as_mut() else { return };
        egui::Frame::popup(child.style()).fill(color(p.surface)).show(&mut child, |ui| {
            ui.set_width(width - 16.0);
            let title = |ui: &mut Ui, t: &str| {
                ui.label(egui::RichText::new(t).font(ui_font(12.0)).color(color(p.comment)));
            };
            match prompt {
                Prompt::Kinds { sel } => {
                    title(ui, "New AI session");
                    for (i, kind) in AgentKind::ORDER.iter().enumerate() {
                        let mark = if Some(*kind) == default_kind { "  (default)" } else { "" };
                        if row(ui, format!("{}  {}{mark}", i + 1, kind.label()), i == *sel).clicked() {
                            acts.push(Act::Pick(i));
                        }
                    }
                }
                Prompt::Accounts { sel } => {
                    title(ui, "Claude account for new sessions");
                    for (i, (name, dir, current)) in accounts.iter().enumerate() {
                        let dot = if *current { "●" } else { " " };
                        if row(ui, format!("{dot} {name}  {dir}"), i == *sel).clicked() {
                            acts.push(Act::Pick(i));
                        }
                    }
                    if row(ui, "+ Add account…".into(), *sel == accounts.len()).clicked() {
                        acts.push(Act::Pick(accounts.len()));
                    }
                }
                Prompt::Confirm { yes, .. } => {
                    ui.label(egui::RichText::new(format!("Close session '{label}'?")).color(color(p.fg)));
                    ui.horizontal(|ui| {
                        if ui.add(egui::Button::selectable(*yes, "Yes")).clicked() {
                            acts.push(Act::Confirm(true));
                        }
                        if ui.add(egui::Button::selectable(!*yes, "No")).clicked() {
                            acts.push(Act::Confirm(false));
                        }
                    });
                }
                Prompt::Text { purpose, value, focused } => {
                    title(
                        ui,
                        match purpose {
                            Purpose::Ollama => "Ollama model",
                            Purpose::Rename(_) => "Tab name (empty = automatic)",
                            Purpose::AccountName => "Account name",
                            Purpose::AccountDir(_) => "Config folder",
                        },
                    );
                    let resp = ui.add(egui::TextEdit::singleline(value).desired_width(f32::INFINITY));
                    if !*focused {
                        resp.request_focus();
                        *focused = true;
                    }
                    if resp.lost_focus() {
                        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                        acts.push(if enter { Act::Submit(value.clone()) } else { Act::Cancel });
                    }
                }
            }
        });
    }

    fn empty(ui: &Ui, rect: Rect, g: Group, cx: &Cx) {
        let p = cx.palette;
        ui.painter().rect_filled(rect, 0.0, color(p.bg));
        let text = match g {
            Group::Ai => "No AI session. Press Cmd+T to start one.",
            Group::Shell => "No terminal. Press Cmd+T to open one.",
        };
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, ui_font(13.0), color(p.comment));
    }
}

impl Default for AiPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for AiPanel {
    fn drop(&mut self) {
        if let Some(t) = self.tabs.as_mut() {
            let ids: Vec<TabId> = t.model.all_tabs().map(|t| t.id).collect();
            for id in ids {
                t.close(id);
            }
        }
    }
}

impl Panel for AiPanel {
    fn id(&self) -> &'static str {
        "ai"
    }

    fn regions(&self) -> &[Region] {
        &REGIONS
    }

    fn tick(&mut self, cx: &mut Cx) {
        if self.autostart && !self.started {
            self.started = true;
            cx.layout(LayoutOp::Open(Area::Ai));
            cx.layout(LayoutOp::Focus(Area::Code));
        }
        let [ai, shell] = self.open;
        let Some(t) = self.tabs.as_mut() else { return };
        let groups: HashMap<TabId, Group> = t.model.all_tabs().map(|t| (t.id, t.group)).collect();
        let res = t.poll(ai, shell);
        let mut emptied = Vec::new();
        for (id, _) in &res.removed {
            self.views.remove(id);
            if let Some(g) = groups.get(id).copied()
                && self.open[gi(g)]
                && t.model.tabs(g).is_empty()
                && !emptied.contains(&g)
            {
                emptied.push(g);
            }
        }
        let busy = t.model.all_tabs().next().is_some();
        let summary = ai && t.model.current(Group::Ai) == Some(Item::Summary);
        for n in res.notices {
            cx.error(n);
        }
        for g in emptied {
            cx.layout(LayoutOp::Close(area_of(g)));
        }
        if res.changed {
            cx.ctx.request_repaint();
        }
        if busy {
            cx.ctx.request_repaint_after(REFRESH_EVERY);
        }
        if summary {
            if self.clock.tick() {
                cx.ctx.request_repaint();
            }
            cx.ctx.request_repaint_after(FRAME_EVERY);
        }
    }

    fn ui(&mut self, region: Region, ui: &mut Ui, cx: &mut Cx) {
        let Some(g) = group_of(region.area()) else { return };
        let rect = ui.max_rect();
        let bar = Rect::from_min_size(rect.min, vec2(rect.width(), BAR_H));
        let body = Rect::from_min_max(pos2(rect.left(), bar.bottom()), rect.max);
        let mut acts = Vec::new();
        self.tab_bar(ui, bar, g, cx, &mut acts);
        let mut child = ui.new_child(UiBuilder::new().max_rect(body).id_salt(("ai-body", gi(g))));
        child.set_clip_rect(body.intersect(ui.clip_rect()));
        match self.current(g) {
            Some(Item::Summary) => {
                let now = unix_now();
                let view = self.tabs(cx.ctx).summary(now);
                if let Some(p) = summary::show(&mut child, &view, self.clock.frame, self.summary_sel, cx.palette) {
                    acts.push(Act::Summary(p));
                }
            }
            Some(Item::Tab(id)) => {
                let term = self.tabs.as_ref().and_then(|t| t.terminal(id)).cloned();
                match term {
                    Some(term) => {
                        let (files, at) = cx.ctx.input(|i| {
                            let files: Vec<PathBuf> =
                                i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect();
                            (files, i.pointer.latest_pos())
                        });
                        if !files.is_empty() && at.map_or(g == Group::Ai, |p| body.contains(p)) {
                            term.paste(&drop_text(&files));
                        }
                        let size = self.views.entry(id).or_default().show(&mut child, &term, cx.palette, cx.focused);
                        self.tabs(cx.ctx).default_size = size;
                    }
                    None => Self::empty(&child, body, g, cx),
                }
            }
            None => Self::empty(&child, body, g, cx),
        }
        if self.prompt.as_ref().is_some_and(|(pg, _)| *pg == g) {
            self.prompt_ui(&mut child, body, cx, &mut acts);
        }
        for a in acts {
            self.apply(a, cx);
        }
    }

    fn event(&mut self, event: &Event, cx: &mut Cx) -> bool {
        match *event {
            Event::Layout(e) => {
                self.layout(e);
                if let Effect::Opened(a) = e
                    && let Some(g) = group_of(a)
                {
                    self.on_open(g, cx);
                }
                false
            }
            Event::Key(k) => self.key(k, cx),
            Event::Ex(cmd) => self.ex(cmd, cx),
            Event::Message { topic: "ai.send", data } => {
                self.send_to_ai(data, cx);
                true
            }
            _ => false,
        }
    }
}

/// Köklerin ortak üst klasörü; tek kök kendisi, ortak klasör `home` ya da `/` ise ilk kök.
fn common_dir(roots: &[PathBuf], home: &Path) -> Option<PathBuf> {
    let first = roots.first()?;
    let mut common = first.clone();
    for root in &roots[1..] {
        while !root.starts_with(&common) {
            common.pop();
        }
    }
    Some(if common == home || common.parent().is_none() { first.clone() } else { common })
}

/// Bırakılan dosya yollarını kabuk/ajan istemine yapıştırılacak metne çevirir (boşluklar `\` ile kaçırılır).
fn drop_text(files: &[PathBuf]) -> String {
    files.iter().map(|f| f.to_string_lossy().replace(' ', "\\ ")).collect::<Vec<_>>().join(" ") + " "
}

#[cfg(test)]
mod drop_tests {
    use super::*;

    #[test]
    fn common_dir_picks_shared_parent() {
        let home = Path::new("/Users/u");
        let roots: Vec<PathBuf> = ["/Users/u/p/iflow/ssr", "/Users/u/p/iflow/be"].iter().map(PathBuf::from).collect();
        assert_eq!(common_dir(&roots, home), Some(PathBuf::from("/Users/u/p/iflow")));
        assert_eq!(common_dir(&roots[..1], home), Some(roots[0].clone()));
        let far: Vec<PathBuf> = ["/Users/u/a", "/Users/u/b"].iter().map(PathBuf::from).collect();
        assert_eq!(common_dir(&far, home), Some(PathBuf::from("/Users/u/a")));
    }

    #[test]
    fn drop_text_escapes_spaces() {
        assert_eq!(drop_text(&[PathBuf::from("/a b/c.png")]), "/a\\ b/c.png ");
    }
}
