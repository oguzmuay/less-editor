//! Proje ekranı = panel ev sahibi (host). Üst çubuk, `less-workbench` yerleşimiyle bölgeler (sol / kod / sağ sütun /
//! alt şerit, sürüklenerek boyutlanır), kod alanında tab'lar ve düzenlenebilir editör, altta durum satırı.
//!
//! Tuş akışı (her tuş için): kod odaktaysa önce uygulama kısayolu (`keys::shortcut`), sonra etkin özel tab'ın
//! paneli ya da `editor::lang` + Vim / modeless motoru; panel odaktaysa önce panel, sonra uygulama kısayolu. Kimsenin
//! kullanmadığı tuş `Event::Shortcut` olarak panellere gider. Motorun `AppCommand`'ları `app_command`'da işlenir
//! (`:w`, `:q` #41, `:e`, `:set`, ex / leader / eşlenmemiş diziler panellere). Paneller `Cx` ile istek bırakır,
//! kare sonunda `process` uygular. Sözleşme: `less-rs/docs/handoff/B0.md`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::Instant;

use eframe::egui::{self, Align2, CursorIcon, Frame, Margin, Rect, Response, Sense, Stroke, Ui, pos2, vec2};
use less_core::command::{AppCommand, Command, ScrollAnchor};
use less_core::editor::Session;
use less_core::selection::{Range, Selection};
use less_input::{EditMode, Engine, Key, KeyEvent};
use less_theme::Palette;
use less_workbench::layout::{Effect, Layout, Metrics, Rects};
use less_workbench::persist::Snapshot;
use less_workspace::recent::{self, Recent};
use less_workspace::search::{self, FileEntry};
use less_workspace::session::{Session as SavedSession, TabState};

use crate::app::{Action, Target};
use crate::clipboard::SystemClipboard;
use crate::cmdline::{self, Cmdline};
use crate::dispatch::{self, Builtin, QuitOutcome, ViewSettings};
use crate::editor::find::{self, FindBar};
use crate::editor::lang::Lang;
use crate::editor::view::{self, CODE_SIZE, ViewOptions};
use crate::editor::{CloseWhich, EditorTab, Tab, TabKind, Tabs};
use crate::files::{Files, FilesOutcome, file_icon};
use crate::fonts::icon;
use crate::keys::{self, Shortcut};
use crate::notes;
use crate::panels::{self, Area, Cx, Event, LayoutOp, Panel, Region, Request, StripPart};
use crate::persistence::ProjectState;
use crate::picker::{self, Command as PickerCommand, Mode, Picker};
use crate::settings::{Keymap, Settings};
use crate::status::{self, Seg};
use crate::sysinfo::SystemInfo;
use crate::welcome::keycaps;
use crate::widgets::{DRAG_STRIP, color, drag_strip, ui_font};

const STATUS_HEIGHT: f32 = 26.0;
const TAB_HEIGHT: f32 = 36.0;
const MAX_MRU: usize = 50;
const MAX_MESSAGES: usize = 200;
const HANDLE: f32 = 6.0;

/// Arama penceresinin dosya listesi; arka planda taranır, eski liste yenisi gelene kadar kullanılır.
#[derive(Default)]
struct FileIndex {
    files: Option<Arc<Vec<FileEntry>>>,
    rx: Option<Receiver<Vec<FileEntry>>>,
}

impl FileIndex {
    fn refresh(&mut self, ctx: &egui::Context, roots: &[PathBuf]) {
        if self.rx.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let roots = roots.to_vec();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(search::list_files(&roots, search::MAX_FILES));
            ctx.request_repaint();
        });
        self.rx = Some(rx);
    }

    fn poll(&mut self) {
        if let Some(files) = self.rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.files = Some(Arc::new(files));
            self.rx = None;
        }
    }
}

/// İkinci basışta onaylanan yıkıcı iş (kaydedilmemiş tab'ı / projeyi kapatma).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Armed {
    CloseTab(usize),
    CloseProject,
}

/// `Cx` kurmak için kare boyunca sabit parçalar.
#[derive(Clone, Copy)]
struct CxParts<'a> {
    ctx: &'a egui::Context,
    palette: &'a Palette,
    roots: &'a [PathBuf],
    mode: EditMode,
}

impl<'a> CxParts<'a> {
    fn cx(
        self,
        active: Option<&'a EditorTab>,
        requests: &'a mut Vec<Request>,
        owner: &'static str,
        focused: bool,
    ) -> Cx<'a> {
        Cx {
            ctx: self.ctx,
            palette: self.palette,
            roots: self.roots,
            edit_mode: self.mode,
            focused,
            active,
            owner,
            requests,
        }
    }
}

pub struct ProjectView {
    title: String,
    roots: Vec<PathBuf>,
    is_workspace: bool,
    branch: Option<String>,
    layout: Layout,
    /// Son bilinen gövde boyutu; ilk karede `reset`, değişince `clamp_sizes`.
    size: Option<(f32, f32)>,
    rects: Rects,
    files: Files,
    tabs: Tabs,
    engine: Engine,
    session: Session,
    lang: Lang,
    panels: Vec<Box<dyn Panel>>,
    view: ViewSettings,
    /// Kod görünümüne sığan satır sayısı (`editor.view.height`).
    code_rows: usize,
    scroll_acc: f32,
    drag_anchor: Option<usize>,
    picker: Option<Picker>,
    /// Cmd+F dosya içi arama çubuğu (Vim ve modeless).
    find: FindBar,
    /// Kod alanının tab çubuğu altındaki gövdesi (arama çubuğu buraya yaslanır).
    code_rect: Rect,
    index: FileIndex,
    /// En son etkin olan dosyalar, en yenisi başta (Cmd+E).
    mru: Vec<PathBuf>,
    /// Durum satırındaki mesaj (hata mı).
    status: Option<(String, bool)>,
    /// Mesaj geçmişi (alt şeritteki Messages).
    messages: Vec<String>,
    armed: Option<Armed>,
    requests: Vec<Request>,
    effects: Vec<Effect>,
    pending_app: Vec<AppCommand>,
    /// Bu projenin kendi depoları; proje geçişinde düzenlerin karışmasını önler (#99).
    state: ProjectState,
    last_session: Option<SavedSession>,
    first_tick: bool,
    sysinfo: SystemInfo,
    /// Yüzen komut satırının öneri durumu (#87).
    cmdline: Cmdline,
}

impl ProjectView {
    /// Proje ya da workspace adı; pencere başlığında kullanılır.
    pub fn title(&self) -> &str {
        &self.title
    }

    #[cfg(test)]
    pub fn new(title: String, roots: Vec<PathBuf>, is_workspace: bool, keymap: Keymap) -> Self {
        let key = roots.first().cloned().unwrap_or_default();
        Self::new_with_key(title, roots, is_workspace, keymap, key)
    }

    /// Workspace'te sanal klasörü, projede kökü kalıcılık anahtarı yapar (#32 #99).
    pub fn new_with_key(title: String, roots: Vec<PathBuf>, is_workspace: bool, keymap: Keymap, key: PathBuf) -> Self {
        #[cfg(test)]
        let state = ProjectState::isolated(&key);
        #[cfg(not(test))]
        let state = ProjectState::new(&key, is_workspace);
        Self::with_state(title, roots, is_workspace, keymap, state)
    }

    fn with_state(title: String, roots: Vec<PathBuf>, is_workspace: bool, keymap: Keymap, state: ProjectState) -> Self {
        let branch = roots.first().and_then(|r| less_git::branch(r));
        let mut layout = Layout::new(Metrics::from_font(CODE_SIZE));
        layout.open(Area::Files);
        let mut files = Files::new(&roots);
        files.focus(None);
        let lang = Lang::new(&roots);
        Self {
            title,
            branch,
            is_workspace,
            layout,
            size: None,
            rects: Rects::default(),
            files,
            tabs: Tabs::default(),
            engine: Engine::new(edit_mode(keymap)),
            session: Session::new(clipboard()),
            lang,
            panels: panels::all(),
            view: ViewSettings::default(),
            code_rows: 40,
            scroll_acc: 0.0,
            drag_anchor: None,
            picker: None,
            find: FindBar::default(),
            code_rect: Rect::NOTHING,
            index: FileIndex::default(),
            mru: Vec::new(),
            status: None,
            messages: Vec::new(),
            armed: None,
            requests: Vec::new(),
            effects: Vec::new(),
            pending_app: Vec::new(),
            state,
            last_session: None,
            first_tick: true,
            sysinfo: SystemInfo::default(),
            cmdline: Cmdline::default(),
            roots,
        }
    }

    #[cfg(test)]
    pub fn show(&mut self, ui: &mut Ui, p: &Palette, settings: &Settings, actions: &mut Vec<Action>) {
        self.show_with_modal(ui, p, settings, &[], false, actions);
    }

    /// Ortak modal açıkken proje klavyeyi işlemez; File menüsü her proje için yeni örnek açar (#61).
    pub fn show_with_modal(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        settings: &Settings,
        recents: &[Recent],
        modal: bool,
        actions: &mut Vec<Action>,
    ) {
        self.restore_session(ui.ctx(), p);
        let ctx = ui.ctx().clone();
        self.index.poll();
        self.sync_mode(edit_mode(settings.keymap));
        self.tick(&ctx, p);
        if !modal {
            let events = ui.input(|i| i.events.clone());
            let typed = keys::translate(&events);
            if self.picker.is_some() {
                for k in &typed {
                    if let Some(Shortcut::Picker(mode)) = keys::shortcut(k) {
                        self.toggle_picker(&ctx, mode);
                    }
                }
            } else {
                let text_edit = ctx.text_edit_focused();
                for k in typed {
                    if text_edit && !k.mods.cmd {
                        continue;
                    }
                    self.route_key(k, &ctx, p, actions);
                }
            }
        }
        self.run_app_commands(&ctx, p, actions);

        let close_clicked = egui::Panel::top("project-top")
            .exact_size(DRAG_STRIP)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::NONE.fill(color(p.surface)).inner_margin(Margin { left: 84, right: 12, top: 0, bottom: 0 }))
            .show(ui, |ui| self.top_bar(ui, p, recents, actions))
            .inner;
        if close_clicked {
            self.shortcut(Shortcut::CloseProject, &ctx, p, actions);
        }
        let servers_clicked = egui::Panel::bottom("project-status")
            .exact_size(STATUS_HEIGHT)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::NONE.fill(color(p.surface)).inner_margin(Margin::symmetric(14, 0)))
            .show(ui, |ui| self.status_bar(ui, p, settings))
            .inner;
        if servers_clicked {
            self.pending_app.push(AppCommand::Ex("options".into()));
        }
        egui::CentralPanel::default().frame(Frame::NONE.fill(color(p.bg))).show(ui, |ui| self.body(ui, p));

        let full = ui.max_rect();
        let line = Stroke::new(1.0, color(p.border));
        ui.painter().hline(full.left()..=full.right(), full.top() + DRAG_STRIP, line);
        ui.painter().hline(full.left()..=full.right(), full.bottom() - STATUS_HEIGHT, line);

        self.picker_window(&ctx, p, settings, actions);
        self.find_window(&ctx, p);
        self.cmdline_window(&ctx, p, full);
        self.run_app_commands(&ctx, p, actions);
        self.process(&ctx, p, actions);
        self.track_mru();
        for t in &mut self.tabs.tabs {
            if t.preview && t.editor().is_some_and(EditorTab::is_dirty) {
                t.preview = false;
            }
        }
        if !ctx.text_edit_focused()
            && let Some(id) = ctx.memory(|m| m.focused())
        {
            ctx.memory_mut(|m| m.surrender_focus(id));
        }
        self.persist();
    }

    fn tick(&mut self, ctx: &egui::Context, p: &Palette) {
        self.sysinfo.tick(ctx, Instant::now());
        if status::any_pending(&self.lang.active_servers()) {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
        let first_tick = self.first_tick;
        let allow_ai = self.state.allow_ai_autostart();
        let parts = CxParts { ctx, palette: p, roots: &self.roots, mode: self.engine.mode() };
        let active = self.tabs.active_editor();
        for panel in &mut self.panels {
            let start = self.requests.len();
            let mut cx = parts.cx(active, &mut self.requests, panel.id(), false);
            panel.tick(&mut cx);
            if first_tick && panel.id() == "ai" && !allow_ai {
                suppress_ai_autostart(&mut self.requests, start);
            }
        }
        let mut cx = parts.cx(None, &mut self.requests, "lang", false);
        self.lang.tick(&mut cx);
        self.first_tick = false;
    }

    // ---- klavye ----

    fn route_key(&mut self, k: KeyEvent, ctx: &egui::Context, p: &Palette, actions: &mut Vec<Action>) {
        match self.layout.focus() {
            Area::Code => {
                if let Some(s) = keys::shortcut(&k) {
                    self.shortcut(s, ctx, p, actions);
                    return;
                }
                self.status = None;
                self.armed = None;
                if k == KeyEvent::plain(Key::Esc) && self.find.open {
                    self.close_find(ctx, p);
                }
                if self.code_key(k, ctx, p) {
                    return;
                }
            }
            Area::Files => match self.files.key(&k, self.tabs.active_path().as_deref()) {
                FilesOutcome::Unhandled => {}
                FilesOutcome::Handled => return,
                outcome => {
                    self.files_outcome(outcome, ctx, p);
                    return;
                }
            },
            area => {
                if self.panel_key(area, k, ctx, p) {
                    return;
                }
                if k == KeyEvent::plain(Key::Esc) && self.owner_of_area(area).is_none() {
                    self.press(Area::Code);
                    return;
                }
            }
        }
        if let Some(s) = keys::shortcut(&k) {
            self.shortcut(s, ctx, p, actions);
            return;
        }
        self.offer_shortcut(k, ctx, p);
    }

    /// Kod alanı odaktayken tuş; `false` = kimse kullanmadı.
    fn code_key(&mut self, k: KeyEvent, ctx: &egui::Context, p: &Palette) -> bool {
        if let Some(Tab { kind: TabKind::Custom { owner, key }, .. }) = self.tabs.active() {
            let (owner, key) = (*owner, key.clone());
            return self.with_panel(owner, ctx, p, true, |panel, cx| panel.tab_event(&key, &Event::Key(k), cx));
        }
        if self.tabs.active_editor().is_some() && self.engine.status().cmdline.is_some() {
            return self.cmdline_key(k, ctx, p);
        }
        let Some(tab) = self.tabs.active_editor_mut() else { return false };
        if tab.md_preview {
            // Önizlemede tuşlar görünmeyen metni değiştirmesin; Esc metne döner.
            if k == KeyEvent::plain(Key::Esc) {
                tab.md_preview = false;
            }
            return true;
        }
        let parts = CxParts { ctx, palette: p, roots: &self.roots, mode: self.engine.mode() };
        let mut cx = parts.cx(None, &mut self.requests, "lang", true);
        if self.lang.event(&Event::Key(k), tab, &mut self.session, &mut cx) {
            return true;
        }
        let out = self.engine.feed(k);
        self.apply(out.commands, ctx, p);
        out.handled
    }

    /// Komut satırı açıkken tuş: önce öneri listesi (Tab / ↑↓), sonra motor; dil servisleri bu tuşları görmez (#87).
    fn cmdline_key(&mut self, k: KeyEvent, ctx: &egui::Context, p: &Palette) -> bool {
        self.cmdline.sync(self.engine.status().cmdline.as_deref(), &self.roots[0]);
        match self.cmdline.key(&k) {
            cmdline::KeyOutcome::Consumed => true,
            cmdline::KeyOutcome::Replace(text) => {
                self.set_cmdline(&text, ctx, p);
                true
            }
            cmdline::KeyOutcome::Pass => {
                let out = self.engine.feed(k);
                self.apply(out.commands, ctx, p);
                self.cmdline.sync(self.engine.status().cmdline.as_deref(), &self.roots[0]);
                out.handled
            }
        }
    }

    /// Motorun komut satırını `text` yapar (Ctrl+U + karakterler).
    fn set_cmdline(&mut self, text: &str, ctx: &egui::Context, p: &Palette) {
        for k in cmdline::replace_keys(text) {
            let out = self.engine.feed(k);
            self.apply(out.commands, ctx, p);
        }
        self.cmdline.sync(self.engine.status().cmdline.as_deref(), &self.roots[0]);
    }

    /// `:` / `/` / `?` açıkken pencerenin üst ortasındaki kutu ve öneri listesi (#87).
    fn cmdline_window(&mut self, ctx: &egui::Context, p: &Palette, window: Rect) {
        let open = self.layout.focus() == Area::Code && self.tabs.active_editor().is_some();
        let line = if open { self.engine.status().cmdline } else { None };
        self.cmdline.sync(line.as_deref(), &self.roots[0]);
        if line.is_none() {
            return;
        }
        if let Some(text) = self.cmdline.show(ctx, p, window, window.top() + DRAG_STRIP + 48.0) {
            self.set_cmdline(&text, ctx, p);
        }
    }

    /// Odaktaki panelin tuşu.
    fn panel_key(&mut self, area: Area, k: KeyEvent, ctx: &egui::Context, p: &Palette) -> bool {
        let Some(i) = self.owner_of_area(area) else { return false };
        let id = self.panels[i].id();
        self.with_panel(id, ctx, p, true, |panel, cx| panel.event(&Event::Key(k), cx))
    }

    /// Kullanılmamış tuş: önce dil servisleri (kod odaktaysa), sonra paneller.
    fn offer_shortcut(&mut self, k: KeyEvent, ctx: &egui::Context, p: &Palette) {
        if k.key == Key::Char('d')
            && k.mods.cmd
            && !k.mods.shift
            && self.layout.focus() == Area::Code
            && let Some(tab) = self.tabs.active_editor_mut()
        {
            let at = tab.editor.cursor();
            let selected = tab.editor.selection_spans().iter().any(|(a, b)| a < b);
            if !selected {
                self.select_word(at, ctx, p);
            } else if !tab.editor.select_next_match() {
                self.message("No more matches");
            }
            return;
        }
        if self.layout.focus() == Area::Code
            && let Some(tab) = self.tabs.active_editor_mut()
        {
            let parts = CxParts { ctx, palette: p, roots: &self.roots, mode: self.engine.mode() };
            let mut cx = parts.cx(None, &mut self.requests, "lang", true);
            if self.lang.event(&Event::Shortcut(k), tab, &mut self.session, &mut cx) {
                return;
            }
        }
        self.first_consumer(&Event::Shortcut(k), ctx, p);
    }

    fn shortcut(&mut self, s: Shortcut, ctx: &egui::Context, p: &Palette, actions: &mut Vec<Action>) {
        if !matches!(s, Shortcut::CloseTab | Shortcut::CloseProject) {
            self.armed = None;
        }
        match s {
            Shortcut::Panel(area) => self.press(area),
            Shortcut::ResetLayout => {
                if let Some((w, h)) = self.size {
                    self.layout.reset(w, h);
                }
            }
            Shortcut::Find => self.start_find(ctx, p),
            Shortcut::Picker(mode) => self.toggle_picker(ctx, mode),
            Shortcut::NextTab | Shortcut::PrevTab => self.cycle_tab(s == Shortcut::NextTab),
            Shortcut::NewTab => {
                let tab = EditorTab::from_text("", None, false);
                self.push_tab("untitled".to_owned(), TabKind::Editor(Box::new(tab)));
                self.press(Area::Code);
            }
            Shortcut::CloseTab => {
                if let Some(a) = self.tabs.active {
                    self.close_tab(a, ctx, p);
                }
            }
            Shortcut::Pin => {
                if self.preview(ctx) {
                    return;
                }
                if let Some(active) = self.tabs.active {
                    match self.toggle_pin_at(active) {
                        Some(true) => self.message("Tab pinned"),
                        Some(false) => self.message("Tab unpinned"),
                        None => {}
                    }
                }
            }
            Shortcut::CloseProject => {
                let dirty = self.tabs.dirty_count();
                if dirty == 0 || self.armed == Some(Armed::CloseProject) {
                    actions.push(Action::CloseProject);
                } else {
                    self.armed = Some(Armed::CloseProject);
                    self.error(format!("{dirty} unsaved file(s). Press Cmd+Shift+W again to discard"));
                }
            }
            Shortcut::Quit => self.pending_app.push(AppCommand::Quit { force: false, all: true }),
            Shortcut::Notes => {
                if self.layout.focus() == Area::Files {
                    self.new_file(ctx, p);
                } else {
                    actions.push(Action::Notes {
                        project: self.state.key().to_path_buf(),
                        command: notes::Command::Toggle,
                    });
                }
            }
            Shortcut::NewWindow => actions.push(Action::NewWindow(None)),
        }
    }

    fn in_insert(&self) -> bool {
        self.engine.mode() == EditMode::Vim && self.engine.status().mode_label == "INSERT"
    }

    /// Ayar değişince motor modunu değiştirir (insert'teyse geri alma grubu kapanır).
    fn sync_mode(&mut self, mode: EditMode) {
        if self.engine.mode() == mode {
            return;
        }
        let cmds = self.engine.set_mode(mode);
        if let Some(tab) = self.tabs.active_editor_mut() {
            for c in cmds {
                tab.apply(c, &mut self.session);
            }
        }
    }

    /// Komutları etkin editöre uygular; değişiklikler `lang`'e, uygulama istekleri kuyruğa.
    fn apply(&mut self, cmds: Vec<Command>, ctx: &egui::Context, p: &Palette) {
        let rows = self.code_rows;
        let Some(tab) = self.tabs.active_editor_mut() else { return };
        let parts = CxParts { ctx, palette: p, roots: &self.roots, mode: self.engine.mode() };
        if !cmds.is_empty() {
            tab.mark = None;
        }
        for c in cmds {
            tab.editor.view.height = rows;
            let applied = tab.apply(c, &mut self.session);
            if let Some(edit) = applied.edit {
                let mut cx = parts.cx(None, &mut self.requests, "lang", true);
                self.lang.changed(tab, &edit, &mut cx);
            }
            self.pending_app.extend(applied.app);
        }
    }

    fn run_app_commands(&mut self, ctx: &egui::Context, p: &Palette, actions: &mut Vec<Action>) {
        for _ in 0..8 {
            let cmds = std::mem::take(&mut self.pending_app);
            if cmds.is_empty() {
                return;
            }
            for c in cmds {
                self.app_command(c, ctx, p, actions);
            }
        }
    }

    fn app_command(&mut self, cmd: AppCommand, ctx: &egui::Context, p: &Palette, actions: &mut Vec<Action>) {
        match cmd {
            AppCommand::Write { path, .. } => {
                self.write_active(path.as_deref(), ctx, p);
            }
            AppCommand::Quit { force, all } => self.quit(force, all, ctx, p),
            AppCommand::WriteQuit { force, all, only_if_modified } => {
                if all {
                    self.write_all(ctx, p);
                } else if !only_if_modified || self.tabs.active_editor().is_some_and(EditorTab::is_dirty) {
                    self.write_active(None, ctx, p);
                }
                self.quit(force, all, ctx, p);
            }
            AppCommand::Edit { path, force } => self.edit(path, force, ctx, p),
            AppCommand::Set { name, value } => {
                if let Some(m) = dispatch::set_option(&mut self.view, &name, &value) {
                    self.message(m);
                }
            }
            AppCommand::Ex(cmd) => match dispatch::builtin_ex(&cmd) {
                Some(Builtin::Toggle(area)) => {
                    let fx = self.layout.toggle(area);
                    self.effects.extend(fx);
                }
                Some(Builtin::ResetLayout) => self.shortcut(Shortcut::ResetLayout, ctx, p, actions),
                Some(Builtin::Picker(mode)) => self.toggle_picker(ctx, mode),
                Some(Builtin::Minimap) => self.view.minimap = !self.view.minimap,
                Some(Builtin::Preview) => {
                    if !self.preview(ctx) {
                        self.error("Preview works for Markdown and HTML files");
                    }
                }
                Some(Builtin::Notes(command)) => {
                    actions.push(Action::Notes { project: self.state.key().to_path_buf(), command });
                }
                Some(Builtin::ChooseTheme) => actions.push(Action::ChooseTheme),
                Some(Builtin::ChooseFont) => actions.push(Action::ChooseFont),
                Some(Builtin::NewWindow) => actions.push(Action::NewWindow(None)),
                None => {
                    if !self.first_consumer(&Event::Ex(&cmd), ctx, p) {
                        self.error(format!("Not an editor command: {cmd}"));
                    }
                }
            },
            AppCommand::Leader(keys) => {
                let ev = Event::Leader(&keys);
                if self.tabs.active_editor().is_some() && self.lang_event(&ev, ctx, p) {
                    return;
                }
                if !self.first_consumer(&ev, ctx, p) {
                    self.message(format!("No mapping for Space {keys}"));
                }
            }
            AppCommand::Unmapped(keys) => {
                let ev = Event::Unmapped(&keys);
                let parts = CxParts { ctx, palette: p, roots: &self.roots, mode: self.engine.mode() };
                if let Some(tab) = self.tabs.active_editor_mut() {
                    let mut cx = parts.cx(None, &mut self.requests, "lang", true);
                    if self.lang.event(&ev, tab, &mut self.session, &mut cx) {
                        return;
                    }
                }
                if !self.first_consumer(&ev, ctx, p) {
                    self.message(format!("No mapping for {keys}"));
                }
            }
            AppCommand::Message(m) => self.message(m),
        }
    }

    fn write_active(&mut self, path: Option<&str>, ctx: &egui::Context, p: &Palette) {
        let root = self.roots[0].clone();
        let Some(tab) = self.tabs.active_editor_mut() else {
            self.error("No file is open");
            return;
        };
        let target = path.map(|s| resolve(&root, s));
        if tab.path.is_none() && target.is_none() {
            self.error("No file name (use :w <path>)");
            return;
        }
        match tab.save(target.as_deref()) {
            Ok(()) => {
                let saved = tab.path.clone().unwrap_or_default();
                if let Some(t) = self.tabs.active_mut() {
                    t.title = file_name(&saved);
                    t.preview = false;
                }
                self.saved(&saved, ctx, p);
            }
            Err(e) => self.error(format!("Could not save: {e}")),
        }
    }

    fn write_all(&mut self, ctx: &egui::Context, p: &Palette) {
        let mut saved = Vec::new();
        let mut failed = 0;
        for tab in self.tabs.tabs.iter_mut().filter_map(Tab::editor_mut) {
            if tab.is_dirty() && tab.path.is_some() {
                match tab.save(None) {
                    Ok(()) => saved.extend(tab.path.clone()),
                    Err(_) => failed += 1,
                }
            }
        }
        for path in &saved {
            self.saved(path, ctx, p);
        }
        if failed > 0 {
            self.error(format!("Could not save {failed} file(s)"));
        }
    }

    /// Kayıt sonrası: durum mesajı, `lang.saved`, panellere `Event::Saved`.
    fn saved(&mut self, path: &Path, ctx: &egui::Context, p: &Palette) {
        self.message(format!("\"{}\" written", recent::display_path(path)));
        let parts = CxParts { ctx, palette: p, roots: &self.roots, mode: self.engine.mode() };
        if let Some(i) = self.tabs.find_path(path)
            && let Some(tab) = self.tabs.tabs[i].editor()
        {
            let mut cx = parts.cx(None, &mut self.requests, "lang", false);
            self.lang.saved(tab, &mut cx);
        }
        self.broadcast(&Event::Saved(path), ctx, p);
    }

    fn quit(&mut self, force: bool, all: bool, ctx: &egui::Context, p: &Palette) {
        match dispatch::quit(force, all, self.tabs.dirty_count()) {
            QuitOutcome::Refuse(m) => self.error(m),
            QuitOutcome::Close => self.close_window(ctx, p),
        }
    }

    /// Paneller izin verirse pencereyi kapatır (#75, #80).
    fn close_window(&mut self, ctx: &egui::Context, p: &Palette) {
        let parts = CxParts { ctx, palette: p, roots: &self.roots, mode: self.engine.mode() };
        let mut ok = true;
        for panel in &mut self.panels {
            let mut cx = parts.cx(None, &mut self.requests, panel.id(), false);
            ok &= panel.can_quit(&mut cx);
        }
        if ok {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// `:e [yol]`: yolsuz = etkin dosyayı diskten yeniden yükle.
    fn edit(&mut self, path: Option<String>, force: bool, ctx: &egui::Context, p: &Palette) {
        match path {
            Some(s) => {
                let path = resolve(&self.roots[0], &s);
                self.open_file(&path, ctx, p);
                self.focus_code_here();
            }
            None => {
                let Some(tab) = self.tabs.active_mut() else { return };
                let Some(path) = tab.path().map(Path::to_path_buf) else { return };
                if tab.editor().is_some_and(EditorTab::is_dirty) && !force {
                    self.error("No write since last change (add ! to override)");
                    return;
                }
                match EditorTab::load(&path) {
                    Ok(new) => tab.kind = TabKind::Editor(Box::new(new)),
                    Err(e) => return self.error(e),
                }
                let parts = CxParts { ctx, palette: p, roots: &self.roots, mode: self.engine.mode() };
                let mut cx = parts.cx(None, &mut self.requests, "lang", false);
                self.lang.closed(&path, &mut cx);
                if let Some(tab) = self.tabs.active_editor() {
                    let mut cx = parts.cx(None, &mut self.requests, "lang", false);
                    self.lang.opened(tab, &mut cx);
                }
            }
        }
    }

    // ---- tab'lar ----

    fn push_tab(&mut self, title: String, kind: TabKind) {
        self.reset_engine();
        self.tabs.push(title, kind);
        self.enter_tab();
    }

    fn set_active(&mut self, i: usize) {
        if self.tabs.active == Some(i) || i >= self.tabs.tabs.len() {
            return;
        }
        self.reset_engine();
        self.tabs.active = Some(i);
        self.enter_tab();
    }

    /// Tab'dan çıkarken motor Normal moda döner (insert oturumu kapanır).
    fn reset_engine(&mut self) {
        let cmds = self.engine.set_mode(self.engine.mode());
        if let Some(tab) = self.tabs.active_editor_mut() {
            for c in cmds {
                tab.apply(c, &mut self.session);
            }
        }
    }

    fn enter_tab(&mut self) {
        let cmds = self.engine.mode_commands();
        if let Some(tab) = self.tabs.active_editor_mut() {
            for c in cmds {
                tab.apply(c, &mut self.session);
            }
        }
    }

    fn cycle_tab(&mut self, forward: bool) {
        let n = self.tabs.tabs.len();
        if n == 0 {
            return;
        }
        let a = self.tabs.active.unwrap_or(0);
        self.set_active(if forward { (a + 1) % n } else { (a + n - 1) % n });
    }

    /// Dosyayı normal tab olarak açar; zaten açıksa o tab'a geçer (önizleme tab'ıysa normal tab olur, #38).
    fn open_file(&mut self, path: &Path, ctx: &egui::Context, p: &Palette) {
        self.open_file_as(path, false, ctx, p);
    }

    /// `preview` ise önizleme tab'ında açar (#38): tek önizleme tab'ı vardır, kaydedilmemiş değilse yeni dosya onun
    /// yerine (aynı sıraya) gelir. Açık dosya yeniden açılırsa yalnız o tab'a geçilir.
    fn open_file_as(&mut self, path: &Path, preview: bool, ctx: &egui::Context, p: &Palette) {
        if let Some(i) = self.tabs.find_path(path) {
            if !preview {
                self.tabs.tabs[i].preview = false;
            }
            self.set_active(i);
            return;
        }
        let tab = match EditorTab::load(path) {
            Ok(tab) => tab,
            Err(e) => {
                self.error(format!("{}: {e}", file_name(path)));
                return;
            }
        };
        let slot = self
            .tabs
            .preview_index()
            .filter(|&i| preview && !self.tabs.tabs[i].editor().is_some_and(EditorTab::is_dirty));
        let mut replaced = None;
        match slot {
            Some(i) => {
                self.reset_engine();
                let new = Tab { title: file_name(path), kind: TabKind::Editor(Box::new(tab)), pinned: false, preview };
                replaced = std::mem::replace(&mut self.tabs.tabs[i], new).path().map(Path::to_path_buf);
                self.tabs.active = Some(i);
                self.enter_tab();
            }
            None => {
                self.push_tab(file_name(path), TabKind::Editor(Box::new(tab)));
                if let Some(t) = self.tabs.active_mut() {
                    t.preview = preview;
                }
            }
        }
        let parts = CxParts { ctx, palette: p, roots: &self.roots, mode: self.engine.mode() };
        if let Some(old) = replaced {
            let mut cx = parts.cx(None, &mut self.requests, "lang", false);
            self.lang.closed(&old, &mut cx);
        }
        if let Some(tab) = self.tabs.active_editor() {
            let mut cx = parts.cx(None, &mut self.requests, "lang", false);
            self.lang.opened(tab, &mut cx);
        }
    }

    /// Files'tan gelen istek: Enter / tek tık önizleme (odak Files'ta kalır), çift tık normal tab, açık dosyada ikinci
    /// Enter önizlemeyi normal tab yapıp koda geçer (#38 #55).
    fn files_outcome(&mut self, outcome: FilesOutcome, ctx: &egui::Context, p: &Palette) {
        match outcome {
            FilesOutcome::Unhandled | FilesOutcome::Handled => {}
            FilesOutcome::Preview(path) => self.open_file_as(&path, true, ctx, p),
            FilesOutcome::Open(path) => {
                self.open_file(&path, ctx, p);
                self.focus_code_here();
            }
            FilesOutcome::FocusCode => {
                if let Some(t) = self.tabs.active_mut() {
                    t.preview = false;
                }
                self.focus_code_here();
            }
        }
    }

    /// Koda odaklanır, etkin tab değişmez (dosya açınca pinli tab'a atlanmaz, #30).
    fn focus_code_here(&mut self) {
        if self.layout.focus() != Area::Code {
            self.layout.set_focus(Area::Code);
            self.effects.push(Effect::Focused(Area::Code));
        }
    }

    /// Dosyayı açar, satır verildiyse imleci oraya koyup vurgular ve koda odaklanır.
    fn open_at(&mut self, path: &Path, line: Option<(usize, Vec<(usize, usize)>)>, ctx: &egui::Context, p: &Palette) {
        self.open_file(path, ctx, p);
        if let (Some((line, ranges)), Some(tab)) = (line, self.tabs.active_editor_mut()) {
            let rope = tab.editor.buffer.rope();
            let line = line.min(rope.len_lines().saturating_sub(1));
            let text = view::line_text(rope, line);
            let byte = ranges.first().map_or(0, |r| r.0).min(text.len());
            let col = text.char_indices().take_while(|(i, _)| *i < byte).count();
            let at = rope.line_to_char(line) + col;
            tab.editor.view.height = self.code_rows;
            tab.editor.set_selection(Selection::point(at));
            tab.apply(Command::Scroll(ScrollAnchor::Center), &mut self.session);
            tab.mark = Some((line, ranges));
        }
        self.focus_code_here();
    }

    /// Cmd+W: kaydedilmemiş dosya ikinci basışta kapanır.
    fn close_tab(&mut self, i: usize, ctx: &egui::Context, p: &Palette) {
        let dirty = self.tabs.tabs.get(i).and_then(Tab::editor).is_some_and(EditorTab::is_dirty);
        if dirty && self.armed != Some(Armed::CloseTab(i)) {
            self.armed = Some(Armed::CloseTab(i));
            self.error("No write since last change. Press Cmd+W again to discard");
            return;
        }
        self.armed = None;
        let was_active = self.tabs.active == Some(i);
        if was_active {
            self.reset_engine();
        }
        let Some(tab) = self.tabs.remove(i) else { return };
        if was_active {
            self.enter_tab();
        }
        let parts = CxParts { ctx, palette: p, roots: &self.roots, mode: self.engine.mode() };
        match &tab.kind {
            TabKind::Editor(e) => {
                if let Some(path) = &e.path {
                    let mut cx = parts.cx(None, &mut self.requests, "lang", false);
                    self.lang.closed(path, &mut cx);
                }
            }
            TabKind::Custom { owner, key } => {
                let key = key.clone();
                self.with_panel(owner, ctx, p, false, |panel, cx| {
                    panel.tab_closed(&key, cx);
                    true
                });
            }
        }
    }

    /// Pin görünümünü ve proje deposunu birlikte günceller; adsız tab'lar yalnız oturumda pinlidir (#30).
    /// #83: Markdown / SVG tab'ında önizlemeyi aç / kapat, HTML dosyasını varsayılan tarayıcıda aç; başka dosyada `false`.
    fn preview(&mut self, ctx: &egui::Context) -> bool {
        let Some(tab) = self.tabs.active_editor_mut() else { return false };
        let ext = tab.path.as_deref().and_then(Path::extension).and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
        match ext.as_deref() {
            Some("md" | "markdown" | "mdx" | "svg") => {
                tab.md_preview = !tab.md_preview;
                true
            }
            Some("html" | "htm" | "xhtml") => {
                let path = tab.path.clone().expect("html path");
                let dirty = tab.is_dirty();
                // shortcut: tarayıcı kaydedilmiş dosyayı gösterir ve kendiliğinden yenilenmez; canlı önizleme gömülü webview ister.
                ctx.open_url(egui::OpenUrl::new_tab(format!("file://{}", path.display())));
                if dirty {
                    self.message("Opened the saved file in the browser. Save to see your changes");
                } else {
                    self.message("Opened in the browser");
                }
                true
            }
            _ => false,
        }
    }

    /// Markdown önizlemesinde tıklanan bağlantı: web adresi tarayıcıda, göreli yol (dosyanın klasörüne göre) editörde.
    fn open_link(&mut self, url: &str, ctx: &egui::Context, p: &Palette) {
        if url.contains("://") || url.starts_with("mailto:") {
            ctx.open_url(egui::OpenUrl::new_tab(url));
            return;
        }
        let target = url.split('#').next().unwrap_or_default();
        if target.is_empty() {
            return;
        }
        let base = self.tabs.active_path().and_then(|f| f.parent().map(Path::to_path_buf));
        let path = base.unwrap_or_else(|| self.roots[0].clone()).join(target);
        if path.is_file() {
            self.open_file(&path, ctx, p);
        } else {
            self.error(format!("Link target not found: {target}"));
        }
    }

    fn toggle_pin_at(&mut self, index: usize) -> Option<bool> {
        let path = self.tabs.tabs.get(index)?.path().map(Path::to_path_buf);
        let pinned = self.tabs.toggle_pin_at(index)?;
        if let Some(path) = path
            && let Err(e) = self.state.toggle_pin(&path)
        {
            self.error(format!("Could not save pinned tabs: {e}"));
        }
        Some(pinned)
    }

    /// Files odaktayken Cmd+N: seçili klasörde yeni dosya; mevcut dosya ezilmez (#53).
    fn new_file(&mut self, ctx: &egui::Context, p: &Palette) {
        let base = self.files.new_file_dir();
        let Some(path) =
            rfd::FileDialog::new().set_title("New File").set_directory(&base).set_file_name("untitled").save_file()
        else {
            return;
        };
        match crate::files::create_new(&path) {
            Ok(()) => {
                self.files = Files::new(&self.roots);
                self.files.reveal(&path);
                self.open_file(&path, ctx, p);
                self.focus_code_here();
            }
            Err(e) => self.error(format!("Could not create {}: {e}", file_name(&path))),
        }
    }

    /// Tab'ın sağ tık menüsünden gelen iş. Toplu kapatma kaydedilmemiş tab'ları atlar ve bildirir.
    fn tab_menu(&mut self, i: usize, action: TabMenu, ctx: &egui::Context, p: &Palette) {
        self.armed = None;
        match action {
            TabMenu::Pin => match self.toggle_pin_at(i) {
                Some(true) => self.message("Tab pinned"),
                Some(false) => self.message("Tab unpinned"),
                None => {}
            },
            TabMenu::Close => self.close_tab(i, ctx, p),
            TabMenu::CloseMany(which) => {
                let mut kept = 0;
                for j in self.tabs.close_targets(i, which) {
                    if self.tabs.tabs[j].editor().is_some_and(EditorTab::is_dirty) {
                        kept += 1;
                    } else {
                        self.close_tab(j, ctx, p);
                    }
                }
                if kept > 0 {
                    self.error(format!("{kept} unsaved tab(s) kept open. Save them or close with Cmd+W"));
                }
            }
        }
    }

    // ---- yerleşim ----

    /// Cmd+N davranışı; koda geçişte en baştaki pinli tab gösterilir (#30).
    fn press(&mut self, area: Area) {
        let fx = self.layout.press(area);
        if area == Area::Code
            && let Some(i) = self.tabs.focus_target()
        {
            self.set_active(i);
        }
        self.effects.extend(fx);
    }

    fn layout_op(&mut self, op: LayoutOp) {
        let fx = match op {
            LayoutOp::Open(a) => self.layout.open(a),
            LayoutOp::Close(a) => self.layout.close(a),
            LayoutOp::Toggle(a) => self.layout.toggle(a),
            LayoutOp::Press(a) => {
                self.press(a);
                Vec::new()
            }
            LayoutOp::Focus(a) => {
                self.layout.set_focus(a);
                vec![Effect::Focused(a)]
            }
            LayoutOp::ShowVariables => self.layout.show_variables(),
            LayoutOp::HideVariables => {
                self.layout.hide_variables();
                Vec::new()
            }
        };
        self.effects.extend(fx);
    }

    fn owner_of(&self, region: Region) -> Option<usize> {
        self.panels.iter().position(|p| p.regions().contains(&region))
    }

    fn owner_of_area(&self, area: Area) -> Option<usize> {
        self.panels.iter().position(|p| p.regions().iter().any(|r| r.area() == area))
    }

    // ---- paneller ----

    /// `id` kimlikli paneli bağlamla çağırır; panel yoksa `false`.
    fn with_panel(
        &mut self,
        id: &str,
        ctx: &egui::Context,
        p: &Palette,
        focused: bool,
        f: impl FnOnce(&mut dyn Panel, &mut Cx) -> bool,
    ) -> bool {
        let parts = CxParts { ctx, palette: p, roots: &self.roots, mode: self.engine.mode() };
        let active = self.tabs.active_editor();
        let Some(panel) = self.panels.iter_mut().find(|x| x.id() == id) else { return false };
        let mut cx = parts.cx(active, &mut self.requests, panel.id(), focused);
        f(panel.as_mut(), &mut cx)
    }

    /// Olayı panellere sırayla verir, ilk tüketende durur.
    fn first_consumer(&mut self, ev: &Event, ctx: &egui::Context, p: &Palette) -> bool {
        let parts = CxParts { ctx, palette: p, roots: &self.roots, mode: self.engine.mode() };
        let active = self.tabs.active_editor();
        let focus = self.layout.focus();
        for panel in &mut self.panels {
            let focused = panel.regions().iter().any(|r| r.area() == focus);
            let mut cx = parts.cx(active, &mut self.requests, panel.id(), focused);
            if panel.event(ev, &mut cx) {
                return true;
            }
        }
        false
    }

    /// Olayı dil servislerine verir (B5): etkin dosya tab'ı yoksa boş geçici tab ile (ör. `run.breakpoints`).
    fn lang_event(&mut self, ev: &Event, ctx: &egui::Context, p: &Palette) -> bool {
        let parts = CxParts { ctx, palette: p, roots: &self.roots, mode: self.engine.mode() };
        let focused = self.layout.focus() == Area::Code;
        let mut scratch = None;
        let tab = match self.tabs.active_editor_mut() {
            Some(t) => t,
            None => scratch.insert(EditorTab::from_text("", None, true)),
        };
        let mut cx = parts.cx(None, &mut self.requests, "lang", focused);
        self.lang.event(ev, tab, &mut self.session, &mut cx)
    }

    /// Olayı tüm panellere verir.
    fn broadcast(&mut self, ev: &Event, ctx: &egui::Context, p: &Palette) {
        let parts = CxParts { ctx, palette: p, roots: &self.roots, mode: self.engine.mode() };
        let active = self.tabs.active_editor();
        for panel in &mut self.panels {
            let mut cx = parts.cx(active, &mut self.requests, panel.id(), false);
            panel.event(ev, &mut cx);
        }
    }

    /// Kare sonu: yerleşim etkileri ve panel istekleri.
    fn process(&mut self, ctx: &egui::Context, p: &Palette, actions: &mut Vec<Action>) {
        for _ in 0..8 {
            let reqs = std::mem::take(&mut self.requests);
            let fx = std::mem::take(&mut self.effects);
            if reqs.is_empty() && fx.is_empty() {
                return;
            }
            for e in fx {
                if e == Effect::Focused(Area::Files) {
                    let active = self.tabs.active_path();
                    self.files.focus(active.as_deref());
                }
                self.broadcast(&Event::Layout(e), ctx, p);
            }
            for r in reqs {
                self.request(r, ctx, p, actions);
            }
        }
    }

    fn request(&mut self, r: Request, ctx: &egui::Context, p: &Palette, actions: &mut Vec<Action>) {
        match r {
            Request::OpenFile { path, line } => {
                let line = line.map(|l| (l, Vec::new()));
                self.open_at(&path, line, ctx, p);
            }
            Request::OpenText { title, text, language } => {
                let tab = EditorTab::from_text(&text, language, true);
                match self.tabs.find_text(&title) {
                    Some(i) => {
                        self.tabs.tabs[i].kind = TabKind::Editor(Box::new(tab));
                        self.set_active(i);
                    }
                    None => self.push_tab(title, TabKind::Editor(Box::new(tab))),
                }
                self.press(Area::Code);
            }
            Request::OpenTab { owner, key, title } => {
                match self.tabs.find_custom(owner, &key) {
                    Some(i) => {
                        self.tabs.tabs[i].title = title;
                        self.set_active(i);
                    }
                    None => self.push_tab(title, TabKind::Custom { owner, key }),
                }
                self.layout.set_focus(Area::Code);
            }
            Request::CloseTab { owner, key } => {
                if let Some(i) = self.tabs.find_custom(owner, &key) {
                    let was_active = self.tabs.active == Some(i);
                    self.tabs.remove(i);
                    if was_active {
                        self.enter_tab();
                    }
                }
            }
            Request::Layout(op) => self.layout_op(op),
            Request::Notify { text, error } => {
                self.push_message(text.clone(), error);
                actions.push(if error { Action::Error(text) } else { Action::Info(text) });
            }
            Request::Send { topic, data } => {
                let ev = Event::Message { topic: &topic, data: &data };
                self.lang_event(&ev, ctx, p);
                self.broadcast(&ev, ctx, p);
            }
            Request::Quit => self.close_window(ctx, p),
        }
    }

    // ---- mesajlar ----

    fn message(&mut self, text: impl Into<String>) {
        self.push_message(text.into(), false);
    }

    fn error(&mut self, text: impl Into<String>) {
        self.push_message(text.into(), true);
    }

    fn push_message(&mut self, text: String, error: bool) {
        self.messages.push(text.clone());
        if self.messages.len() > MAX_MESSAGES {
            self.messages.remove(0);
        }
        self.status = Some((text, error));
    }

    // ---- arama penceresi ----

    /// Cmd+F: etkin dosyada arama çubuğunu açar (Vim insert'teyse önce Normal moda döner); seçili tek satırlık metin
    /// sorgu olur. Dosya açık değilse dosya araması açılır.
    fn start_find(&mut self, ctx: &egui::Context, p: &Palette) {
        if self.tabs.active_editor().is_none() {
            self.toggle_picker(ctx, Mode::Files);
            return;
        }
        if self.in_insert() {
            let out = self.engine.feed(KeyEvent::plain(Key::Esc));
            self.apply(out.commands, ctx, p);
        }
        self.picker = None;
        let Some(tab) = self.tabs.active_editor() else { return };
        let rope = tab.editor.buffer.rope();
        let seed =
            tab.editor.selection_spans().first().map(|&(a, b)| rope.slice(a..b.min(rope.len_chars())).to_string());
        self.find.open(tab.editor.cursor(), seed);
        self.focus_code_here();
        if !self.find.query.is_empty() {
            self.find_action(find::Outcome::Changed, ctx, p);
        }
    }

    fn close_find(&mut self, ctx: &egui::Context, p: &Palette) {
        self.find.open = false;
        self.apply(vec![Command::ClearSearchHighlight], ctx, p);
    }

    fn find_window(&mut self, ctx: &egui::Context, p: &Palette) {
        if !self.find.open {
            return;
        }
        if self.tabs.active_editor().is_none() {
            self.find.open = false;
            return;
        }
        let out = self.find.show(ctx, p, self.code_rect);
        self.find_action(out, ctx, p);
    }

    /// Arama çubuğunun isteği: yazınca başlangıç konumundan ileri arar, Enter / Shift+Enter imleçten sonraki /
    /// önceki eşleşmeye gider; sonra `n / toplam` sayacı güncellenir.
    fn find_action(&mut self, out: find::Outcome, ctx: &egui::Context, p: &Palette) {
        let forward = match out {
            find::Outcome::Keep => return,
            find::Outcome::Close => {
                self.close_find(ctx, p);
                self.focus_code_here();
                return;
            }
            find::Outcome::Changed | find::Outcome::Next => true,
            find::Outcome::Prev => false,
        };
        let query = self.find.query.clone();
        self.find.error = None;
        if query.is_empty() {
            self.find.count = None;
            self.apply(vec![Command::ClearSearchHighlight], ctx, p);
            return;
        }
        let re = match self.find.opts.compile(&query) {
            Ok(re) => re,
            Err(e) => {
                self.find.error = Some(e);
                self.find.count = None;
                return;
            }
        };
        let modeless = self.engine.mode() == EditMode::Modeless;
        let rows = self.code_rows;
        let Some(tab) = self.tabs.active_editor_mut() else { return };
        let rope = tab.editor.buffer.rope().clone();
        let text = rope.to_string();
        let (from, dir) = match out {
            find::Outcome::Changed => (self.find.origin, find::Dir::AtOrAfter),
            _ => {
                let at = tab.editor.selection().primary().from();
                (at, if forward { find::Dir::After } else { find::Dir::Before })
            }
        };
        let from = rope.char_to_byte(from.min(rope.len_chars()));
        let Some((a, b)) = find::next_match(&re, &text, from, dir) else {
            self.find.count = Some((0, 0));
            self.session.search.highlight = false;
            return;
        };
        let (a, b) = (rope.byte_to_char(a), rope.byte_to_char(b));
        tab.editor.set_selection(if modeless { Selection::single(Range::new(a, b)) } else { Selection::point(a) });
        let line = rope.char_to_line(a);
        tab.editor.view.height = rows;
        if line < tab.editor.view.top || line >= tab.editor.view.top + rows {
            tab.apply(Command::Scroll(ScrollAnchor::Center), &mut self.session);
        }
        tab.mark = None;
        let search = &mut self.session.search;
        search.pattern = Some(self.find.opts.vim_pattern(&query));
        search.forward = true;
        search.highlight = true;
        let (current, total) = find::count_matches(&re, &text, rope.char_to_byte(a));
        self.find.count = Some((current, total));
        if out != find::Outcome::Changed {
            self.find.origin = a;
        }
    }

    /// Arama penceresini açar; aynı moddaysa kapatır, başka moddaysa moda geçer.
    fn toggle_picker(&mut self, ctx: &egui::Context, mode: Mode) {
        match &mut self.picker {
            Some(p) if p.mode() == mode => self.picker = None,
            Some(p) => p.set_mode(mode),
            None => self.picker = Some(Picker::new(mode)),
        }
        if self.picker.is_some() {
            self.find.open = false;
        }
        if self.picker.is_some() && matches!(mode, Mode::Files | Mode::Text) {
            self.index.refresh(ctx, &self.roots);
        }
    }

    fn picker_window(&mut self, ctx: &egui::Context, p: &Palette, settings: &Settings, actions: &mut Vec<Action>) {
        let Some(picker) = &mut self.picker else { return };
        let active = self.tabs.active_path();
        let env = picker::Env {
            files: self.index.files.as_ref(),
            indexing: self.index.rx.is_some(),
            mru: &self.mru,
            roots: &self.roots,
            active: active.as_deref(),
            theme: &settings.theme,
            keymap: settings.keymap,
        };
        match picker.show(ctx, p, &env) {
            picker::Outcome::Keep => {}
            picker::Outcome::Close => self.picker = None,
            picker::Outcome::Open(path, line) => {
                self.picker = None;
                self.open_at(&path, line, ctx, p);
            }
            picker::Outcome::Run(cmd) => {
                if !matches!(cmd, PickerCommand::Picker(_) | PickerCommand::Theme(_) | PickerCommand::Keymap(_)) {
                    self.picker = None;
                }
                match cmd {
                    PickerCommand::Picker(mode) => {
                        if let Some(pk) = &mut self.picker {
                            pk.set_mode(mode);
                        }
                        if matches!(mode, Mode::Files | Mode::Text) {
                            self.index.refresh(ctx, &self.roots);
                        }
                    }
                    other => self.picker_command(ctx, p, other, actions),
                }
            }
        }
    }

    fn picker_command(&mut self, ctx: &egui::Context, p: &Palette, cmd: PickerCommand, actions: &mut Vec<Action>) {
        match cmd {
            PickerCommand::ToggleFiles => self.press(Area::Files),
            PickerCommand::FocusEditor => self.press(Area::Code),
            PickerCommand::Panel(area) => self.press(area),
            PickerCommand::RevealInFiles => {
                if let Some(path) = self.tabs.active_path()
                    && self.files.reveal(&path)
                {
                    let fx = self.layout.open(Area::Files);
                    self.effects.extend(fx);
                }
            }
            PickerCommand::RefreshFiles => {
                self.files = Files::new(&self.roots);
                if let Some(path) = self.tabs.active_path() {
                    self.files.reveal(&path);
                }
                self.index.refresh(ctx, &self.roots);
            }
            PickerCommand::CloseTab => self.shortcut(Shortcut::CloseTab, ctx, p, actions),
            PickerCommand::NewTab => self.shortcut(Shortcut::NewTab, ctx, p, actions),
            PickerCommand::Pin => self.shortcut(Shortcut::Pin, ctx, p, actions),
            PickerCommand::NextTab => self.cycle_tab(true),
            PickerCommand::PrevTab => self.cycle_tab(false),
            PickerCommand::ResetLayout => self.shortcut(Shortcut::ResetLayout, ctx, p, actions),
            PickerCommand::CloseProject => self.shortcut(Shortcut::CloseProject, ctx, p, actions),
            PickerCommand::Quit => self.shortcut(Shortcut::Quit, ctx, p, actions),
            PickerCommand::Picker(mode) => self.toggle_picker(ctx, mode),
            PickerCommand::CopyPath => match self.tabs.active_path() {
                Some(path) => {
                    ctx.copy_text(path.display().to_string());
                    actions.push(Action::Info("Path copied".to_owned()));
                }
                None => actions.push(Action::Info("No file is open".to_owned())),
            },
            PickerCommand::RevealInFinder => {
                let path = self.tabs.active_path().unwrap_or_else(|| self.roots[0].clone());
                if std::process::Command::new("open").arg("-R").arg(&path).spawn().is_err() {
                    actions.push(Action::Error("Could not open Finder".to_owned()));
                }
            }
            PickerCommand::FindInFile => self.start_find(ctx, p),
            PickerCommand::ToggleMinimap => self.view.minimap = !self.view.minimap,
            PickerCommand::Notes => {
                self.shortcut(Shortcut::Notes, ctx, p, actions);
            }
            PickerCommand::ChooseTheme => actions.push(Action::ChooseTheme),
            PickerCommand::ChooseFont => actions.push(Action::ChooseFont),
            PickerCommand::NewWindow => actions.push(Action::NewWindow(None)),
            PickerCommand::Theme(id) => actions.push(Action::SetTheme(id)),
            PickerCommand::Keymap(k) => actions.push(Action::SetKeymap(k)),
        }
    }

    /// Etkin dosya değiştiyse son kullanılanların başına alır.
    fn track_mru(&mut self) {
        if let Some(path) = self.tabs.active_path()
            && self.mru.first() != Some(&path)
        {
            self.mru.retain(|p| p != &path);
            self.mru.insert(0, path);
            self.mru.truncate(MAX_MRU);
        }
    }

    /// Kayıtlı oturumu ilk karede açar; pin sırası ve aktif tab korunur (#30 #32 #99).
    fn restore_session(&mut self, ctx: &egui::Context, p: &Palette) {
        let Some(saved) = self.state.take_session() else { return };
        let active_path = saved.active.and_then(|index| saved.tabs.get(index)).map(|tab| tab.path.clone());
        for saved_tab in saved.tabs {
            self.open_file(&saved_tab.path, ctx, p);
            if let Some(tab) = self.tabs.active_editor_mut() {
                let rope = tab.editor.buffer.rope().clone();
                let line = saved_tab.line.min(rope.len_lines() - 1);
                let col = saved_tab.col.min(rope.line(line).len_chars());
                tab.editor.set_selection(Selection::point(rope.line_to_char(line) + col));
            }
        }
        let pins = self.state.pins().to_vec();
        for tab in &mut self.tabs.tabs {
            tab.pinned = tab.path().is_some_and(|path| pins.iter().any(|pin| pin == path));
        }
        self.tabs.tabs.sort_by_key(|tab| {
            tab.path().and_then(|path| pins.iter().position(|pin| pin == path)).unwrap_or(usize::MAX)
        });
        self.tabs.active = active_path
            .as_deref()
            .and_then(|path| self.tabs.find_path(path))
            .or_else(|| (!self.tabs.tabs.is_empty()).then_some(0));
        if let Some(tab) = self.tabs.active_editor_mut() {
            tab.editor.view.height = self.code_rows;
            tab.apply(Command::Scroll(ScrollAnchor::Center), &mut self.session);
        }
    }

    /// Düzen ve oturum değişince kendi proje anahtarına kaydeder; kayıt hatası her karede tekrar bildirilmez.
    fn persist(&mut self) {
        if let Err(e) = self.state.save_layout(Snapshot::capture(&self.layout, recent::now())) {
            self.error(format!("Could not save panel layout: {e}"));
        }
        let active_path = self.tabs.active_path();
        let mut session = SavedSession::new(self.state.key());
        session.tabs = self
            .tabs
            .tabs
            .iter()
            .filter_map(Tab::editor)
            .filter_map(|tab| {
                let (line, col) = tab.editor.cursor_line_col();
                tab.path.clone().map(|path| TabState { path, line, col })
            })
            .collect();
        session.active = active_path.and_then(|path| session.tabs.iter().position(|tab| tab.path == path));
        if self.last_session.as_ref() == Some(&session) {
            return;
        }
        self.last_session = Some(session.clone());
        if let Err(e) = self.state.save_session(&session) {
            self.error(format!("Could not save editor session: {e}"));
        }
    }

    // ---- çizim ----

    fn body(&mut self, ui: &mut Ui, p: &Palette) {
        let body = ui.max_rect();
        let (w, h) = (body.width(), body.height());
        match self.size {
            None => {
                self.layout.reset(w, h);
                self.state.take_layout().apply(&mut self.layout);
                self.layout.clamp_sizes(w, h);
                for panel in self.layout.open_panels() {
                    self.effects.push(Effect::Opened(panel));
                }
                self.effects.push(Effect::Focused(Area::Code));
            }
            Some(s) if s != (w, h) => self.layout.clamp_sizes(w, h),
            _ => {}
        }
        self.size = Some((w, h));
        self.rects = self.layout.compute(w, h);
        let at = |r: less_workbench::layout::Rect| Rect::from_min_size(body.min + vec2(r.x, r.y), vec2(r.w, r.h));
        let rects = self.rects.clone();
        let ctx = ui.ctx().clone();

        if let Some((area, r)) = rects.left {
            self.region(ui, p, &ctx, Region::Area(area), at(r));
        }
        self.code_area(ui, p, &ctx, at(rects.code));
        for (area, r) in [(Area::Ai, rects.ai), (Area::Terminal, rects.terminal), (Area::Database, rects.database)] {
            if let Some(r) = r {
                self.region(ui, p, &ctx, Region::Area(area), at(r));
            }
        }
        for (part, r) in &rects.strip_parts {
            self.region(ui, p, &ctx, Region::Strip(*part), at(*r));
        }

        let line = Stroke::new(1.0, color(p.border));
        let painter = ui.painter();
        if let Some((_, r)) = rects.left {
            painter.vline(at(r).right(), at(r).y_range(), line);
        }
        if let Some(r) = rects.ai.or(rects.terminal).or(rects.database) {
            painter.vline(at(r).left(), body.y_range(), line);
        }
        if let (Some(_), Some(t)) = (rects.ai, rects.terminal) {
            painter.hline(at(t).x_range(), at(t).top(), line);
        }
        if let Some((_, r)) = rects.strip {
            painter.hline(at(r).x_range(), at(r).top(), line);
        }
        for (_, r) in rects.strip_parts.iter().skip(1) {
            painter.vline(at(*r).left(), at(*r).y_range(), line);
        }
        self.resize_handles(ui, body, &rects);
    }

    /// Kenar sürükleyerek boyutlandırma (sol genişlik, sağ sütun genişliği, şerit yüksekliği, AI / terminal oranı).
    fn resize_handles(&mut self, ui: &Ui, body: Rect, rects: &Rects) {
        let (w, h) = (body.width(), body.height());
        let at = |r: less_workbench::layout::Rect| Rect::from_min_size(body.min + vec2(r.x, r.y), vec2(r.w, r.h));
        let mut changed = false;
        if let Some((_, r)) = rects.left {
            let r = at(r);
            let edge = Rect::from_center_size(pos2(r.right(), r.center().y), vec2(HANDLE, r.height()));
            if let Some(d) = handle(ui, "resize-left", edge, true) {
                self.layout.set_left_width(r.width() + d);
                changed = true;
            }
        }
        if let Some(r) = rects.ai.or(rects.terminal).or(rects.database) {
            let r = at(r);
            let edge = Rect::from_center_size(pos2(r.left(), body.center().y), vec2(HANDLE, h));
            if let Some(d) = handle(ui, "resize-right", edge, true) {
                self.layout.set_right_width(r.width() - d);
                changed = true;
            }
        }
        if let Some((_, r)) = rects.strip {
            let r = at(r);
            let edge = Rect::from_center_size(pos2(r.center().x, r.top()), vec2(r.width(), HANDLE));
            if let Some(d) = handle(ui, "resize-strip", edge, false) {
                self.layout.set_strip_height(r.height() - d);
                changed = true;
            }
        }
        if let (Some(ai), Some(_)) = (rects.ai, rects.terminal) {
            let r = at(ai);
            let edge = Rect::from_center_size(pos2(r.center().x, r.bottom()), vec2(r.width(), HANDLE));
            if let Some(d) = handle(ui, "resize-ai", edge, false) {
                self.layout.set_ai_ratio((r.height() + d) / h.max(1.0));
                changed = true;
            }
        }
        if changed {
            self.layout.clamp_sizes(w, h);
        }
    }

    /// Bir panel bölgesi: sahibi çizer, yoksa yer tutucu. Tıklama odağı oraya verir.
    fn region(&mut self, ui: &mut Ui, p: &Palette, ctx: &egui::Context, region: Region, rect: Rect) {
        let area = region.area();
        if clicked_in(ui, rect) && self.layout.focus() != area {
            self.layout.set_focus(area);
            self.effects.push(Effect::Focused(area));
        }
        let focused = self.layout.focus() == area;
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).id_salt(format!("{region:?}")));
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        if region == Region::Area(Area::Files) {
            let active = self.tabs.active_path();
            if let Some(outcome) = self.files.show(&mut child, p, focused, active.as_deref()) {
                self.files_outcome(outcome, ctx, p);
            }
            return;
        }
        if region == Region::Strip(StripPart::Messages) {
            messages_view(&mut child, p, &self.messages);
            return;
        }
        match self.owner_of(region) {
            Some(i) => {
                let parts = CxParts { ctx, palette: p, roots: &self.roots, mode: self.engine.mode() };
                let active = self.tabs.active_editor();
                let panel = &mut self.panels[i];
                let mut cx = parts.cx(active, &mut self.requests, panel.id(), focused);
                panel.ui(region, &mut child, &mut cx);
            }
            None => placeholder(&mut child, p, region_name(region)),
        }
    }

    fn code_area(&mut self, ui: &mut Ui, p: &Palette, ctx: &egui::Context, full: Rect) {
        if clicked_in(ui, full) && self.layout.focus() != Area::Code {
            self.layout.set_focus(Area::Code);
            self.effects.push(Effect::Focused(Area::Code));
        }
        let focused = self.layout.focus() == Area::Code;
        let bar = Rect::from_min_size(full.left_top(), vec2(full.width(), TAB_HEIGHT));
        ui.painter().rect_filled(bar, 0.0, color(p.surface));
        ui.painter().hline(bar.left()..=bar.right(), bar.bottom() - 0.5, Stroke::new(1.0, color(p.border)));
        let mut bar_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(bar)
                .id_salt("tab-bar")
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        bar_ui.set_clip_rect(bar.intersect(ui.clip_rect()));
        self.tab_bar(&mut bar_ui, p, ctx, focused);

        let body = Rect::from_min_max(pos2(full.left() + 1.0, bar.bottom()), full.right_bottom());
        self.code_rect = body;
        let mut body_ui = ui.new_child(egui::UiBuilder::new().max_rect(body).id_salt("code-body"));
        body_ui.set_clip_rect(body.intersect(ui.clip_rect()));
        let (_, line_h) = view::cell(&body_ui);
        self.code_rows = view::rows_for(body.height(), line_h);
        match self.tabs.active() {
            None => empty_editor(&mut body_ui, p),
            Some(Tab { kind: TabKind::Custom { owner, key }, .. }) => {
                let (owner, key) = (*owner, key.clone());
                let drawn = self.with_panel(owner, ctx, p, focused, |panel, cx| {
                    panel.tab_ui(&key, &mut body_ui, cx);
                    true
                });
                if !drawn {
                    message(&mut body_ui, p, icon::WARNING, "This tab's panel is not available");
                }
            }
            Some(_) => self.editor_view(&mut body_ui, p, ctx, focused),
        }
    }

    fn editor_view(&mut self, ui: &mut Ui, p: &Palette, ctx: &egui::Context, focused: bool) {
        let opts = ViewOptions { numbers: self.view.number, relative_numbers: self.view.relative_number, focused };
        let rows = self.code_rows;
        let parts = CxParts { ctx, palette: p, roots: &self.roots, mode: self.engine.mode() };
        let modeless = self.engine.mode() == EditMode::Modeless;
        let Some(tab) = self.tabs.active_editor_mut() else { return };
        if tab.md_preview {
            let text = tab.editor.buffer.rope().to_string();
            if tab.path.as_deref().and_then(Path::extension).is_some_and(|e| e.eq_ignore_ascii_case("svg")) {
                crate::editor::markdown::show_svg(ui, &text);
                return;
            }
            let id = ("md-preview", tab.path.clone());
            if let Some(url) = crate::editor::markdown::show(ui, p, &text, id) {
                self.open_link(&url, ctx, p);
            }
            return;
        }
        tab.editor.view.height = rows;
        let (resp, geo) = view::show_with(ui, p, tab, &self.session, opts, self.view.minimap);
        let mut cx = parts.cx(None, &mut self.requests, "lang", focused);
        self.lang.paint(tab, &geo, ui, &mut cx);
        if self.lang.pointer(tab, &geo, &resp, &mut self.session, &mut cx) {
            return;
        }
        let rope = tab.editor.buffer.rope().clone();
        let pointer = resp.interact_pointer_pos();
        if let Some(mm) = geo.minimap
            && let Some(at) = pointer.filter(|q| q.x >= geo.text.right())
        {
            if resp.is_pointer_button_down_on() && at.x <= mm.rect.right() {
                let visible = geo.last_line - geo.first_line;
                let target = mm.line_at(at.y).saturating_sub(visible / 2);
                let delta = target as isize - tab.editor.view.top as isize;
                if delta != 0 {
                    tab.apply(Command::ScrollLines(delta), &mut self.session);
                }
            }
        } else if (resp.clicked() || resp.drag_started())
            && let Some(at) = pointer.and_then(|pos| geo.char_at(&rope, pos))
        {
            self.drag_anchor = Some(at);
            tab.editor.set_selection(Selection::point(at));
        } else if resp.dragged()
            && modeless
            && let (Some(anchor), Some(at)) = (self.drag_anchor, pointer.and_then(|pos| geo.char_at(&rope, pos)))
        {
            // shortcut: Vim modunda sürükleme görsel moda girmez (motor durumu bilmiyor); yalnız imleç taşınır.
            tab.editor.set_selection(Selection::single(Range::new(anchor, at)));
        } else if resp.dragged()
            && let Some(at) = pointer.and_then(|pos| geo.char_at(&rope, pos))
        {
            tab.editor.set_selection(Selection::point(at));
        }
        if resp.drag_stopped() {
            self.drag_anchor = None;
        }
        let word_click = resp.double_clicked().then(|| pointer.and_then(|pos| geo.char_at(&rope, pos))).flatten();
        if resp.hovered() {
            let delta = ui.input(|i| i.smooth_scroll_delta);
            self.scroll_acc -= delta.y / geo.line_h;
            let lines = self.scroll_acc.trunc();
            if lines != 0.0 {
                self.scroll_acc -= lines;
                tab.apply(Command::ScrollLines(lines as isize), &mut self.session);
            }
            if delta.x != 0.0 {
                let cols = (-delta.x / geo.char_w).round() as isize;
                tab.left_col = tab.left_col.saturating_add_signed(cols);
            }
        }
        if let Some(at) = word_click {
            self.select_word(at, ctx, p);
        }
    }

    /// Çift tık / Cmd+D (seçim yokken) kelimeyi seçer: Vim modunda `viw` (görsel mod), Normal modda düz seçim (imleç kelimenin
    /// hemen sonundaysa o kelime).
    fn select_word(&mut self, at: usize, ctx: &egui::Context, p: &Palette) {
        if self.engine.mode() == EditMode::Vim {
            if self.engine.status().mode_label != "NORMAL" {
                let out = self.engine.feed(KeyEvent::plain(Key::Esc));
                self.apply(out.commands, ctx, p);
            }
            if let Some(tab) = self.tabs.active_editor_mut() {
                tab.editor.set_selection(Selection::point(at));
            }
            for c in ['v', 'i', 'w'] {
                let out = self.engine.feed(KeyEvent::plain(Key::Char(c)));
                self.apply(out.commands, ctx, p);
            }
            return;
        }
        let Some(tab) = self.tabs.active_editor_mut() else { return };
        let rope = tab.editor.buffer.rope();
        let is_word = |i: usize| rope.get_char(i).is_some_and(|c| c.is_alphanumeric() || c == '_');
        let at = if !is_word(at) && at > 0 && is_word(at - 1) { at - 1 } else { at };
        if !is_word(at) {
            return;
        }
        let start = (0..at).rev().take_while(|&i| is_word(i)).last().unwrap_or(at);
        let end = (at..rope.len_chars()).find(|&i| !is_word(i)).unwrap_or(rope.len_chars());
        tab.editor.set_selection(Selection::single(Range::new(start, end)));
    }

    fn tab_bar(&mut self, ui: &mut Ui, p: &Palette, ctx: &egui::Context, focused: bool) {
        ui.spacing_mut().item_spacing.x = 0.0;
        let mut activate = None;
        let mut close = None;
        let mut menu = None;
        egui::ScrollArea::horizontal()
            .id_salt("editor-tabs")
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for (i, tab) in self.tabs.tabs.iter().enumerate() {
                        let (resp, close_clicked) = tab_widget(ui, p, tab, i, self.tabs.active == Some(i), focused);
                        if close_clicked || resp.middle_clicked() {
                            close = Some(i);
                        } else if resp.clicked() {
                            activate = Some(i);
                        }
                        resp.context_menu(|ui| {
                            if let Some(action) = tab_menu_ui(ui, tab.pinned) {
                                menu = Some((i, action));
                                ui.close();
                            }
                        });
                    }
                });
            });
        if let Some(i) = activate {
            self.set_active(i);
            self.layout.set_focus(Area::Code);
        }
        if let Some(i) = close {
            self.close_tab(i, ctx, p);
        }
        if let Some((i, action)) = menu {
            self.tab_menu(i, action, ctx, p);
        }
    }

    /// File menüsü (yeni pencerede klasör / recent), proje adı, dal ve Close Project (#61).
    fn top_bar(&mut self, ui: &mut Ui, p: &Palette, recents: &[Recent], actions: &mut Vec<Action>) -> bool {
        let rect = ui.max_rect();
        let close_w = 132.0;
        let file_w = 52.0;
        drag_strip(
            ui,
            Rect::from_min_max(
                pos2(rect.left() + file_w, rect.top()),
                pos2(rect.right() - close_w - 8.0, rect.bottom()),
            ),
        );
        let file_rect = Rect::from_min_size(rect.left_top(), vec2(file_w, rect.height()));
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(file_rect).layout(egui::Layout::left_to_right(egui::Align::Center)),
            |ui| {
                ui.menu_button("File", |ui| {
                    ui.label("Open in a new LESS window");
                    if ui.button("Open Folder…").clicked() {
                        ui.close();
                        let start = self.roots.first().cloned().unwrap_or_else(less_workspace::paths::home);
                        if let Some(dir) =
                            rfd::FileDialog::new().set_title("Open Folder").set_directory(start).pick_folder()
                        {
                            actions.push(Action::NewWindow(Some(Target::Folder(dir))));
                        }
                    }
                    ui.menu_button("Open Recent", |ui| {
                        if recents.is_empty() {
                            ui.label("No recent projects");
                        }
                        for recent in recents.iter().take(15) {
                            let label = format!("{}  —  {}", recent.name, recent::display_path(&recent.dir));
                            if ui.button(label).clicked() {
                                ui.close();
                                actions.push(Action::NewWindow(Some(recent_target(recent))));
                            }
                        }
                    });
                });
            },
        );
        let y = rect.center().y;
        let painter = ui.painter();
        let title_left = rect.left() + file_w;
        let glyph = if self.is_workspace { icon::SQUARES_FOUR } else { icon::FOLDER };
        painter.text(pos2(title_left, y), Align2::LEFT_CENTER, glyph, ui_font(16.0), color(p.accent));
        let title = painter.layout_no_wrap(self.title.clone(), ui_font(14.0), color(p.fg));
        let mut x = title_left + 26.0 + title.size().x;
        painter.galley(pos2(title_left + 26.0, y - title.size().y / 2.0), title, color(p.fg));
        if let Some(branch) = &self.branch {
            x += 18.0;
            painter.text(pos2(x, y), Align2::LEFT_CENTER, icon::GIT_BRANCH, ui_font(14.0), color(p.comment));
            painter.text(pos2(x + 20.0, y), Align2::LEFT_CENTER, branch, ui_font(13.0), color(p.fg_dim));
        }

        let btn = Rect::from_min_size(pos2(rect.right() - close_w, y - 13.0), vec2(close_w, 26.0));
        let resp = ui.interact(btn, ui.scope_id().with("close-project"), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(btn, 6.0, color(p.bg_highlight));
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        let painter = ui.painter();
        let at = btn.left_center() + vec2(10.0, 0.0);
        painter.text(at, Align2::LEFT_CENTER, icon::SIGN_OUT, ui_font(14.0), color(p.fg_dim));
        let label = btn.left_center() + vec2(32.0, 0.0);
        painter.text(label, Align2::LEFT_CENTER, "Close Project", ui_font(13.0), color(p.fg_dim));
        let clicked = resp.clicked();
        resp.on_hover_text("Cmd+Shift+W");
        clicked
    }

    /// Alt durum satırı; dil sunucusu göstergesine tıklanırsa `true` (çağıran `:options` çalıştırır).
    fn status_bar(&self, ui: &mut Ui, p: &Palette, settings: &Settings) -> bool {
        let rect = ui.max_rect();
        let y = rect.center().y;
        let font = ui_font(12.0);
        let painter = ui.painter();
        let st = self.engine.status();
        let mut x = rect.left();
        let code = self.layout.focus() == Area::Code && self.tabs.active_editor().is_some();
        if code {
            let label = painter.layout_no_wrap(st.mode_label.to_owned(), ui_font(11.0), color(p.on_accent));
            let pill = Rect::from_min_size(pos2(x, y - 9.0), vec2(label.size().x + 14.0, 18.0));
            painter.rect_filled(pill, 4.0, color(p.accent));
            painter.galley(pos2(x + 7.0, y - label.size().y / 2.0), label, color(p.on_accent));
            x = pill.right() + 10.0;
        }
        let left = status_left(&st, self.status.as_ref(), code);
        let (text, tint) = match left {
            Some((t, true)) => (t, color(p.error)),
            Some((t, false)) => (t, color(p.fg)),
            None => match self.tabs.active() {
                Some(tab) => (tab.path().map_or_else(|| tab.title.clone(), recent::display_path), color(p.comment)),
                None if self.roots.len() > 1 => (format!("{} folders", self.roots.len()), color(p.comment)),
                None => (recent::display_path(&self.roots[0]), color(p.comment)),
            },
        };
        let left_end = painter.text(pos2(x, y), Align2::LEFT_CENTER, text, font, tint).right();

        let mut segs = Vec::new();
        if let Some(tab) = self.tabs.active_editor() {
            if tab.read_only {
                segs.push(Seg::Text { prio: 0, text: format!("{} Read-only", icon::LOCK_SIMPLE) });
            } else if tab.is_dirty() {
                segs.push(Seg::Text { prio: 0, text: "Modified".to_owned() });
            }
            let (line, col) = tab.editor.cursor_line_col();
            segs.push(Seg::Text { prio: 1, text: format!("Ln {}, Col {}", line + 1, col + 1) });
            segs.push(Seg::Text { prio: 6, text: format!("{} lines", tab.editor.buffer.line_count()) });
        }
        let servers = self.lang.active_servers();
        if !servers.is_empty() {
            segs.push(Seg::Servers(servers));
        }
        segs.push(Seg::Text { prio: 8, text: settings.keymap.label().to_owned() });
        segs.push(Seg::Text { prio: 10, text: p.name.to_owned() });
        segs.push(Seg::Text { prio: 9, text: "UTF-8".to_owned() });
        let sys = self.sysinfo.parts(rect.width(), CODE_SIZE * 0.6);
        segs.extend(sys.into_iter().map(|(prio, text)| Seg::Text { prio, text }));
        status::paint_right(ui, p, rect, left_end, &segs)
    }
}

impl Drop for ProjectView {
    fn drop(&mut self) {
        self.persist();
    }
}

/// Editör panosu; testlerde kullanıcının sistem panosuna dokunulmaz.
fn clipboard() -> Box<dyn less_core::registers::Clipboard> {
    if cfg!(test) {
        Box::new(less_core::registers::MemoryClipboard::default())
    } else {
        Box::new(SystemClipboard::new())
    }
}

/// Ayar → motor modu.
fn edit_mode(k: Keymap) -> EditMode {
    match k {
        Keymap::Vim => EditMode::Vim,
        Keymap::Normal => EditMode::Modeless,
    }
}

/// Kayıtlı düzende AI yoksa B3'ün varsayılan #81 açılış isteğini yalnız ilk tick'te süzer; odak isteği kalır (#99).
fn suppress_ai_autostart(requests: &mut Vec<Request>, start: usize) {
    let requests_to_keep: Vec<_> =
        requests.drain(start..).filter(|r| *r != Request::Layout(LayoutOp::Open(Area::Ai))).collect();
    requests.extend(requests_to_keep);
}

/// Durum satırının sol metni: mesaj > bekleyen tuşlar / makro kaydı (komut satırı üstte yüzer, #87). `None` = dosya yolu göster.
fn status_left(st: &less_input::Status, msg: Option<&(String, bool)>, code: bool) -> Option<(String, bool)> {
    if let Some((m, err)) = msg {
        return Some((m.clone(), *err));
    }
    if !code {
        return None;
    }
    let mut parts = Vec::new();
    if let Some(r) = st.recording {
        parts.push(format!("recording @{r}"));
    }
    if !st.pending.is_empty() {
        parts.push(st.pending.clone());
    }
    (!parts.is_empty()).then(|| (parts.join("  "), false))
}

/// `:w yol` / `:e yol`: göreli yol ilk köke göre, `~` ev klasörüne.
fn resolve(root: &Path, s: &str) -> PathBuf {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    let p = PathBuf::from(s);
    if p.is_absolute() { p } else { root.join(p) }
}

/// File > Open Recent girdisini yeni pencerenin hedefine çevirir (#61).
fn recent_target(recent: &Recent) -> Target {
    match recent.kind {
        recent::Kind::Project => Target::Folder(recent.dir.clone()),
        recent::Kind::Workspace => Target::Workspace(less_workspace::workspaces::Workspace {
            name: recent.name.clone(),
            roots: recent.roots.clone(),
            opened: recent.time,
        }),
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

fn region_name(region: Region) -> &'static str {
    match region.area() {
        Area::Git => "Git",
        Area::Ai => "AI",
        Area::Terminal => "Terminal",
        Area::Database => "Database",
        Area::Run => "Run",
        Area::GitHistory => "Git History",
        Area::Messages => "Messages",
        Area::Files => "Files",
        Area::Code => "Code",
    }
}

/// Sürükleme tutamacı; sürükleniyorsa bu karedeki kayma (yatay ya da dikey).
fn handle(ui: &Ui, id: &str, rect: Rect, horizontal: bool) -> Option<f32> {
    let resp = ui.interact(rect, ui.scope_id().with(id), Sense::drag());
    if resp.hovered() || resp.dragged() {
        ui.ctx().set_cursor_icon(if horizontal { CursorIcon::ResizeHorizontal } else { CursorIcon::ResizeVertical });
    }
    let d = resp.drag_delta();
    let d = if horizontal { d.x } else { d.y };
    (resp.dragged() && d != 0.0).then_some(d)
}

/// Bu karede `rect` içine birincil fare tuşuyla basıldı mı (odağı o bölgeye vermek için).
fn clicked_in(ui: &Ui, rect: Rect) -> bool {
    ui.input(|i| i.pointer.primary_pressed() && i.pointer.interact_pos().is_some_and(|p| rect.contains(p)))
}

/// Sahibi henüz olmayan panel bölgesi.
fn placeholder(ui: &mut Ui, p: &Palette, name: &str) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0.0, color(p.surface));
    let c = rect.center();
    let painter = ui.painter();
    painter.text(c - vec2(0.0, 10.0), Align2::CENTER_CENTER, name, ui_font(14.0), color(p.fg_dim));
    painter.text(c + vec2(0.0, 12.0), Align2::CENTER_CENTER, "Not available yet", ui_font(12.0), color(p.comment));
}

/// Alt şeritteki mesaj geçmişi (#40), en yenisi altta.
fn messages_view(ui: &mut Ui, p: &Palette, messages: &[String]) {
    ui.painter().rect_filled(ui.max_rect(), 0.0, color(p.surface));
    egui::ScrollArea::vertical().id_salt("messages").stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
        ui.add_space(6.0);
        for m in messages {
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(egui::RichText::new(m).font(egui::FontId::monospace(12.5)).color(color(p.fg_dim)));
            });
        }
    });
}

/// Tek tab: ikon + ad + kapatma düğmesi (etkin tab'da ya da üstüne gelince görünür); pinli tab'da pin işareti,
/// kaydedilmemiş tab'da nokta. Alt çizgi editör odaktayken vurgu renginde. `(tab yanıtı, ✕ tıklandı)`.
fn tab_widget(ui: &mut Ui, p: &Palette, tab: &Tab, index: usize, active: bool, focused: bool) -> (Response, bool) {
    let mut title = tab.title.clone();
    if tab.pinned {
        title.insert_str(0, "📌 ");
    }
    let format = egui::TextFormat {
        font_id: ui_font(13.0),
        color: egui::Color32::PLACEHOLDER,
        italics: tab.preview,
        ..Default::default()
    };
    let name = ui.painter().layout_job(egui::text::LayoutJob::single_section(title, format));
    let width = name.size().x + 70.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, TAB_HEIGHT), Sense::click());
    let painter = ui.painter();
    if active {
        painter.rect_filled(rect, 0.0, color(p.bg));
        let underline = Rect::from_min_max(pos2(rect.left(), rect.bottom() - 2.0), rect.right_bottom());
        let tint = if focused { color(p.accent) } else { color(p.comment).gamma_multiply(0.6) };
        painter.rect_filled(underline, 0.0, tint);
    } else if resp.hovered() {
        painter.rect_filled(rect, 0.0, color(p.bg_highlight).gamma_multiply(0.6));
    }
    painter.vline(rect.right(), rect.top()..=rect.bottom(), Stroke::new(1.0, color(p.border)));
    let y = rect.center().y;
    let icon_tint = if active { color(p.accent) } else { color(p.comment) };
    painter.text(pos2(rect.left() + 14.0, y), Align2::LEFT_CENTER, file_icon(&tab.title), ui_font(14.0), icon_tint);
    let text_color = if active { color(p.fg) } else { color(p.fg_dim) };
    painter.galley(pos2(rect.left() + 36.0, y - name.size().y / 2.0), name, text_color);

    let x_rect = Rect::from_center_size(pos2(rect.right() - 18.0, y), vec2(20.0, 20.0));
    let x = ui.interact(x_rect, ui.scope_id().with(("tab-close", index)), Sense::click());
    let dirty = tab.editor().is_some_and(EditorTab::is_dirty);
    if active || resp.hovered() || x.hovered() {
        if x.hovered() {
            ui.painter().rect_filled(x_rect, 4.0, color(p.bg_highlight));
        }
        ui.painter().text(x_rect.center(), Align2::CENTER_CENTER, icon::X, ui_font(12.0), color(p.comment));
    } else if dirty {
        ui.painter().circle_filled(x_rect.center(), 3.5, color(p.fg_dim));
    }
    (resp, x.clicked())
}

/// Tab'ın sağ tık menüsündeki iş.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TabMenu {
    Pin,
    Close,
    CloseMany(CloseWhich),
}

/// Tab sağ tık menüsünün içeriği; seçilen iş döner.
fn tab_menu_ui(ui: &mut Ui, pinned: bool) -> Option<TabMenu> {
    let mut out = None;
    let pin = if pinned { "Unpin Tab" } else { "Pin Tab" };
    for (label, action) in [
        (pin, TabMenu::Pin),
        ("Close", TabMenu::Close),
        ("Close Others", TabMenu::CloseMany(CloseWhich::Others)),
        ("Close to the Left", TabMenu::CloseMany(CloseWhich::Left)),
        ("Close to the Right", TabMenu::CloseMany(CloseWhich::Right)),
        ("Close All", TabMenu::CloseMany(CloseWhich::All)),
    ] {
        if action == TabMenu::Close {
            ui.separator();
        }
        if ui.button(label).clicked() {
            out = Some(action);
        }
    }
    out
}

/// Açık dosya yokken: ortada sönük LESS yazısı ve bu ekranda çalışan kısayollar.
fn empty_editor(ui: &mut Ui, p: &Palette) {
    let rect = ui.max_rect();
    let c = rect.center();
    let painter = ui.painter();
    painter.text(c - vec2(0.0, 70.0), Align2::CENTER_CENTER, "LESS", ui_font(56.0), color(p.bg_highlight));
    let hints = [
        ("Find file", "Cmd O"),
        ("Find in file", "Cmd F"),
        ("Find in project", "Cmd Shift F"),
        ("Recent files", "Cmd E"),
        ("Command palette", "Cmd Shift P"),
        ("Search shortcuts", "Cmd K"),
        ("Files", "Cmd 1"),
    ];
    for (i, (what, keys)) in hints.iter().enumerate() {
        let y = c.y + i as f32 * 30.0;
        painter.text(pos2(c.x - 12.0, y), Align2::RIGHT_CENTER, what, ui_font(13.5), color(p.comment));
        let width: f32 = keys.split(' ').map(|k| k.chars().count() as f32 * 7.5 + 20.0).sum();
        keycaps(ui, p, keys, c.x + 12.0 + width, y);
    }
}

fn message(ui: &mut Ui, p: &Palette, glyph: &str, text: &str) {
    let c = ui.max_rect().center();
    ui.painter().text(c - vec2(0.0, 18.0), Align2::CENTER_CENTER, glyph, ui_font(28.0), color(p.comment));
    ui.painter().text(c + vec2(0.0, 18.0), Align2::CENTER_CENTER, text, ui_font(14.0), color(p.fg_dim));
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event as E, Key as EK, Modifiers};

    const P: Palette = less_theme::TOKYONIGHT_NIGHT;

    fn frame(view: &mut ProjectView, ctx: &egui::Context, events: Vec<E>) -> Vec<Action> {
        frame_with_settings(view, ctx, events, &Settings::default())
    }

    fn frame_with_settings(
        view: &mut ProjectView,
        ctx: &egui::Context,
        events: Vec<E>,
        settings: &Settings,
    ) -> Vec<Action> {
        let mut actions = Vec::new();
        let input = egui::RawInput {
            events,
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1400.0, 900.0))),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| view.show(ui, &P, settings, &mut actions));
        out.textures_delta.clear();
        actions
    }

    fn key(k: EK, modifiers: Modifiers) -> E {
        E::Key { key: k, physical_key: Some(k), pressed: true, repeat: false, modifiers }
    }

    fn cmd() -> Modifiers {
        Modifiers { mac_cmd: true, command: true, ..Default::default() }
    }

    fn text(s: &str) -> E {
        E::Text(s.to_owned())
    }

    /// Geçici klasörde bir dosyalık proje.
    fn project(name: &str, content: &str) -> (ProjectView, egui::Context, PathBuf) {
        project_mode(name, content, Keymap::Vim)
    }

    fn project_mode(name: &str, content: &str, keymap: Keymap) -> (ProjectView, egui::Context, PathBuf) {
        let dir = std::env::temp_dir().join(format!("less-b0-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("main.rs");
        std::fs::write(&file, content).unwrap();
        let ctx = egui::Context::default();
        let mut view = ProjectView::new("t".into(), vec![dir], false, keymap);
        let settings = Settings { keymap, ..Settings::default() };
        frame_with_settings(&mut view, &ctx, Vec::new(), &settings);
        view.open_file(&file, &ctx, &P);
        view.press(Area::Code);
        (view, ctx, file)
    }

    fn active_text(view: &ProjectView) -> String {
        view.tabs.active_editor().unwrap().editor.text()
    }

    #[test]
    fn typing_and_write_reach_disk() {
        let (mut view, ctx, file) = project("write", "fn main() {}\n");
        frame(&mut view, &ctx, vec![text("i"), text("// hi"), key(EK::Escape, Modifiers::NONE)]);
        assert_eq!(active_text(&view), "// hifn main() {}");
        assert!(view.tabs.active_editor().unwrap().is_dirty());
        frame(&mut view, &ctx, vec![text(":w"), key(EK::Enter, Modifiers::NONE)]);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "// hifn main() {}\n");
        assert!(view.status.as_ref().is_some_and(|(m, _)| m.ends_with("written")));
        std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }

    /// Cmd+F çubuğu: yazdıkça imleçten itibaren arar, Enter / Shift+Enter gezer, sayaç, Esc kapatır.
    fn find_bar_flow(keymap: Keymap, name: &str) {
        let (mut view, ctx, file) = project_mode(name, "foo bar\nFoo foo\n", keymap);
        let settings = Settings { keymap, ..Settings::default() };
        let run = |view: &mut ProjectView, evs: Vec<E>| frame_with_settings(view, &ctx, evs, &settings);
        if keymap == Keymap::Vim {
            run(&mut view, vec![text("i")]);
        }
        run(&mut view, vec![key(EK::F, cmd())]);
        assert!(view.find.open && view.picker.is_none());
        assert_ne!(view.engine.status().mode_label, "INSERT", "Cmd+F leaves insert mode");
        run(&mut view, vec![text("foo")]);
        let cursor = |v: &ProjectView| {
            let e = &v.tabs.active_editor().unwrap().editor;
            let at = e.selection().primary().from();
            let rope = e.buffer.rope();
            (rope.char_to_line(at), at - rope.line_to_char(rope.char_to_line(at)))
        };
        assert_eq!(cursor(&view), (0, 0), "first match at the cursor");
        assert_eq!(view.find.count, Some((1, 3)));
        run(&mut view, vec![key(EK::Enter, Modifiers::NONE)]);
        assert_eq!(cursor(&view), (1, 0));
        assert!(view.find.open, "Enter keeps the bar open");
        run(&mut view, vec![key(EK::Enter, Modifiers::SHIFT)]);
        assert_eq!(cursor(&view), (0, 0), "Shift+Enter goes back");
        view.find.opts.case = true;
        view.find_action(find::Outcome::Changed, &ctx, &P);
        assert_eq!(view.find.count, Some((1, 2)), "match case drops Foo");
        assert!(view.session.search.highlight);
        view.find.opts.regex = true;
        view.find.query = "fo(".into();
        view.find_action(find::Outcome::Changed, &ctx, &P);
        assert!(view.find.error.is_some(), "invalid regex is reported");
        run(&mut view, vec![key(EK::Escape, Modifiers::NONE)]);
        assert!(!view.find.open);
        assert!(!view.session.search.highlight);
        std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }

    #[test]
    fn cmdline_popup_completes_runs_and_recalls_history_87() {
        let (mut view, ctx, file) = project("cmdline", "x\n");
        frame(&mut view, &ctx, vec![text(":GitH")]);
        assert_eq!(view.engine.status().cmdline.as_deref(), Some(":GitH"));
        assert!(view.cmdline.list_open());
        frame(&mut view, &ctx, vec![key(EK::Tab, Modifiers::NONE)]);
        assert_eq!(view.engine.status().cmdline.as_deref(), Some(":GitHistory"));
        frame(&mut view, &ctx, vec![key(EK::Enter, Modifiers::NONE)]);
        assert!(view.engine.status().cmdline.is_none());
        assert!(view.layout.is_open(Area::GitHistory));
        view.press(Area::Code);
        frame(&mut view, &ctx, vec![text(":"), key(EK::ArrowUp, Modifiers::NONE)]);
        assert_eq!(view.engine.status().cmdline.as_deref(), Some(":GitHistory"), "history when the list is closed");
        frame(&mut view, &ctx, vec![key(EK::Escape, Modifiers::NONE)]);
        assert!(view.engine.status().cmdline.is_none());
        std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }

    #[test]
    fn cmd_f_find_bar_in_vim_mode_79() {
        find_bar_flow(Keymap::Vim, "find-vim");
    }

    #[test]
    fn cmd_f_find_bar_in_modeless_mode_79() {
        find_bar_flow(Keymap::Normal, "find-modeless");
    }

    #[test]
    fn files_enter_opens_single_preview_tab_and_promotes_38_55() {
        let (mut view, ctx, file) = project("preview", "x\n");
        let dir = file.parent().unwrap().to_path_buf();
        let (a, b) = (dir.join("a.rs"), dir.join("b.rs"));
        std::fs::write(&a, "a\n").unwrap();
        std::fs::write(&b, "b\n").unwrap();
        view.tabs.toggle_pin();
        view.press(Area::Files);
        frame(&mut view, &ctx, Vec::new());
        view.files_outcome(FilesOutcome::Preview(a.clone()), &ctx, &P);
        assert_eq!(view.tabs.tabs.len(), 2);
        assert!(view.tabs.tabs[1].preview);
        assert_eq!(view.layout.focus(), Area::Files, "focus stays in Files (#55)");
        view.files_outcome(FilesOutcome::Preview(b.clone()), &ctx, &P);
        assert_eq!(view.tabs.tabs.len(), 2, "preview tab is replaced, not added");
        assert_eq!(view.tabs.active_path(), Some(b.clone()));
        view.files_outcome(FilesOutcome::FocusCode, &ctx, &P);
        assert!(!view.tabs.tabs[1].preview, "second Enter promotes");
        assert_eq!(view.layout.focus(), Area::Code);
        assert_eq!(view.tabs.active_path(), Some(b.clone()), "no jump to the pinned tab (#30)");
        view.files_outcome(FilesOutcome::Preview(a.clone()), &ctx, &P);
        assert_eq!(view.tabs.tabs.len(), 3);
        frame(&mut view, &ctx, vec![key(EK::Num2, cmd())]);
        frame(&mut view, &ctx, Vec::new());
        view.tabs.active = Some(2);
        frame(&mut view, &ctx, vec![text("x")]);
        assert!(!view.tabs.tabs[2].preview, "editing promotes");
        view.files_outcome(FilesOutcome::Open(b.clone()), &ctx, &P);
        assert_eq!(view.tabs.tabs.len(), 3, "double click on an open file switches to it");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn tab_menu_closes_others_but_keeps_unsaved_and_pinned_30() {
        let (mut view, ctx, file) = project("tabmenu", "x\n");
        for i in 0..3 {
            let tab = EditorTab::from_text("", None, false);
            view.push_tab(format!("u{i}"), TabKind::Editor(Box::new(tab)));
        }
        frame(&mut view, &ctx, vec![text("ix"), key(EK::Escape, Modifiers::NONE)]);
        view.tab_menu(1, TabMenu::Pin, &ctx, &P);
        assert_eq!(view.tabs.tabs[0].title, "u0", "pinned tab moved to the front");
        view.tab_menu(0, TabMenu::CloseMany(CloseWhich::Others), &ctx, &P);
        let titles: Vec<_> = view.tabs.tabs.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["u0", "u2"], "unsaved u2 is kept");
        assert!(view.status.as_ref().is_some_and(|(m, e)| *e && m.starts_with("1 unsaved")));
        view.tab_menu(0, TabMenu::Pin, &ctx, &P);
        view.tab_menu(0, TabMenu::CloseMany(CloseWhich::Right), &ctx, &P);
        assert_eq!(view.tabs.tabs.len(), 2);
        std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }

    #[test]
    fn minimap_ex_toggles_and_click_scrolls() {
        let content: String = (0..400).map(|i| format!("line {i}\n")).collect();
        let (mut view, ctx, file) = project("minimap", &content);
        frame(&mut view, &ctx, Vec::new());
        let top = |v: &ProjectView| v.tabs.active_editor().unwrap().editor.view.top;
        assert_eq!(top(&view), 0);
        let mm = view.code_rect.right() - view::RULER_W - view::MINIMAP_W / 2.0;
        let at = pos2(mm, view.code_rect.top() + 600.0);
        let press = |pressed| E::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        frame(&mut view, &ctx, vec![E::PointerMoved(at)]);
        frame(&mut view, &ctx, vec![press(true)]);
        frame(&mut view, &ctx, vec![press(false)]);
        assert!(top(&view) > 100, "clicking the minimap scrolls (top = {})", top(&view));
        frame(&mut view, &ctx, vec![text(":Minimap"), key(EK::Enter, Modifiers::NONE)]);
        assert!(!view.view.minimap);
        std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }

    #[test]
    fn unmapped_sequence_is_not_silently_dropped() {
        let (mut view, ctx, file) = project("unmapped", "x\n");
        let mut actions = Vec::new();
        view.app_command(AppCommand::Unmapped("gZ".into()), &ctx, &P, &mut actions);
        assert_eq!(view.status, Some(("No mapping for gZ".to_owned(), false)));
        std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }

    #[test]
    fn colon_q_keeps_code_window_41() {
        let (mut view, ctx, file) = project("quit", "x\n");
        frame(&mut view, &ctx, vec![text(":q"), key(EK::Enter, Modifiers::NONE)]);
        assert_eq!(view.status, Some((dispatch::KEEP_CODE_OPEN.to_owned(), true)));
        assert_eq!(view.tabs.tabs.len(), 1);
        frame(&mut view, &ctx, vec![text("x"), text(":qa"), key(EK::Enter, Modifiers::NONE)]);
        assert!(view.status.as_ref().is_some_and(|(m, err)| *err && m.contains("unsaved")));
        std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }

    #[test]
    fn cmd_keys_route_to_layout_5_6_29() {
        let (mut view, ctx, file) = project("layout", "x\n");
        assert!(view.layout.is_open(Area::Files));
        frame(&mut view, &ctx, vec![key(EK::Num3, cmd())]);
        assert_eq!(view.layout.focus(), Area::Ai);
        frame(&mut view, &ctx, vec![key(EK::Num6, cmd())]);
        frame(&mut view, &ctx, Vec::new());
        let ai = view.rects.ai.expect("AI column");
        let (_, strip) = view.rects.strip.expect("Run strip");
        assert_eq!(ai.y, 0.0);
        assert!((ai.h - view.size.unwrap().1).abs() < 0.5, "right column is full height");
        assert!(strip.right() <= ai.x + 0.5, "strip ends at the right column");
        frame(&mut view, &ctx, vec![key(EK::Num6, cmd())]);
        assert!(!view.layout.is_open(Area::Run));
        assert_eq!(view.layout.focus(), Area::Code);
        frame(&mut view, &ctx, vec![key(EK::Escape, Modifiers::NONE)]);
        std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }

    /// Sözleşme testi için panel: Git bölgesini çizer, `:Hello` ile özel tab açar.
    struct Probe {
        seen: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
    }

    impl Panel for Probe {
        fn id(&self) -> &'static str {
            "probe"
        }

        fn regions(&self) -> &[Region] {
            &[Region::Area(Area::Git)]
        }

        fn ui(&mut self, _region: Region, _ui: &mut Ui, _cx: &mut Cx) {}

        fn event(&mut self, event: &Event, cx: &mut Cx) -> bool {
            self.seen.borrow_mut().push(format!("{event:?}"));
            match event {
                Event::Ex("Hello") => {
                    cx.open_tab("k", "Hello tab");
                    true
                }
                Event::Key(k) if k.is_char('q') => {
                    cx.layout(LayoutOp::Close(Area::Git));
                    true
                }
                _ => false,
            }
        }

        fn tab_event(&mut self, key: &str, event: &Event, _cx: &mut Cx) -> bool {
            self.seen.borrow_mut().push(format!("tab {key} {event:?}"));
            true
        }
    }

    #[test]
    fn panel_contract_ex_tabs_keys_and_layout() {
        let (mut view, ctx, file) = project("panel", "x\n");
        let seen = std::rc::Rc::default();
        view.panels.push(Box::new(Probe { seen: std::rc::Rc::clone(&seen) }));
        frame(&mut view, &ctx, vec![text(":Hello"), key(EK::Enter, Modifiers::NONE)]);
        assert_eq!(view.tabs.active().map(|t| t.title.as_str()), Some("Hello tab"));
        frame(&mut view, &ctx, vec![text("j")]);
        assert!(seen.borrow().iter().any(|s| s.starts_with("tab k Key")));
        frame(&mut view, &ctx, vec![key(EK::Num0, cmd())]);
        assert_eq!(view.layout.focus(), Area::Git);
        assert!(seen.borrow().iter().any(|s| s.contains("Layout(Opened(Git))")));
        frame(&mut view, &ctx, vec![text("q")]);
        assert!(!view.layout.is_open(Area::Git));
        frame(&mut view, &ctx, vec![text(":Nope"), key(EK::Enter, Modifiers::NONE)]);
        assert!(view.tabs.active().is_some_and(|t| t.title == "Hello tab"));
        std::fs::remove_dir_all(file.parent().unwrap()).unwrap();
    }

    #[test]
    fn status_leaves_cmdline_to_popup_then_message_87() {
        let st = less_input::Status {
            mode_label: "COMMAND",
            pending: String::new(),
            recording: Some('q'),
            cmdline: Some(":w".into()),
        };
        assert_eq!(status_left(&st, None, true), Some(("recording @q".into(), false)));
        let msg = ("E: x".to_owned(), true);
        assert_eq!(status_left(&st, Some(&msg), true), Some(msg.clone()));
        assert_eq!(status_left(&st, None, true), Some(("recording @q".into(), false)));
        assert_eq!(status_left(&st, None, false), None);
    }

    #[test]
    fn resolve_relative_to_first_root() {
        assert_eq!(resolve(Path::new("/r"), "a/b.rs"), PathBuf::from("/r/a/b.rs"));
        assert_eq!(resolve(Path::new("/r"), "/x"), PathBuf::from("/x"));
    }

    #[test]
    fn file_open_recent_targets_a_new_window_without_changing_this_project_61() {
        let project = Recent {
            kind: recent::Kind::Project,
            name: "api".into(),
            dir: PathBuf::from("/work/api"),
            roots: Vec::new(),
            time: 1,
        };
        assert!(matches!(recent_target(&project), Target::Folder(path) if path == Path::new("/work/api")));
        let workspace = Recent { kind: recent::Kind::Workspace, roots: vec![PathBuf::from("/work/web")], ..project };
        assert!(matches!(
            recent_target(&workspace),
            Target::Workspace(ws) if ws.name == "api" && ws.roots.len() == 1 && ws.roots[0] == Path::new("/work/web")
        ));
    }

    #[test]
    fn saved_layout_suppresses_only_default_ai_open_81_99() {
        let mut requests = vec![
            Request::Layout(LayoutOp::Open(Area::Ai)),
            Request::Layout(LayoutOp::Focus(Area::Code)),
            Request::Layout(LayoutOp::Open(Area::Terminal)),
        ];
        suppress_ai_autostart(&mut requests, 0);
        assert_eq!(
            requests,
            [Request::Layout(LayoutOp::Focus(Area::Code)), Request::Layout(LayoutOp::Open(Area::Terminal))]
        );
    }

    #[test]
    fn saved_layout_session_cursor_and_pins_restore_30_32_99() {
        let dir = std::env::temp_dir().join(format!("less-c2-restore-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("project");
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("main.rs");
        std::fs::write(&file, "zero\none\ntwo\n").unwrap();
        let layouts = dir.join("layouts.json");
        let pins = dir.join("pins.json");
        let sessions = dir.join("sessions");
        let snapshot = Snapshot {
            panels: vec!["files".into(), "terminal".into(), "runner".into()],
            sizes: less_workbench::persist::SavedSizes {
                left: Some(240.0),
                right: Some(333.0),
                right_split: Some(0.6),
                bottom: Some(180.0),
            },
            time: 1,
        };
        ProjectState::open(&root, layouts.clone(), pins.clone(), sessions.clone(), true).save_layout(snapshot).unwrap();
        let saved = SavedSession {
            version: 1,
            dir: root.clone(),
            tabs: vec![TabState { path: file.clone(), line: 2, col: 2 }],
            active: Some(0),
        };
        saved.save_in(&sessions).unwrap();
        less_workbench::persist::PinStore::open(&pins).toggle(&root.to_string_lossy(), &file).unwrap();
        let state = ProjectState::open(&root, layouts.clone(), pins.clone(), sessions.clone(), true);
        let mut view = ProjectView::with_state("t".into(), vec![root.clone()], false, Keymap::Vim, state);
        let ctx = egui::Context::default();
        frame(&mut view, &ctx, Vec::new());
        assert!(view.layout.is_open(Area::Files));
        assert!(view.layout.is_open(Area::Terminal));
        assert!(view.layout.is_open(Area::Run));
        assert!(!view.layout.is_open(Area::Ai));
        assert_eq!(view.layout.sizes.right_w, Some(333.0));
        assert_eq!(view.layout.sizes.strip_h, Some(180.0));
        assert_eq!(view.tabs.tabs.len(), 1);
        assert!(view.tabs.tabs[0].pinned);
        assert_eq!(view.tabs.active_editor().unwrap().editor.cursor_line_col(), (2, 2));
        drop(view);

        let mut reopened = ProjectState::open(&root, layouts, pins, sessions, true);
        assert_eq!(reopened.take_session().unwrap().tabs[0].line, 2);
        assert_eq!(reopened.take_layout().panels, ["files", "terminal", "runner"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn project_switch_cannot_write_old_layout_to_new_key_99() {
        let dir = std::env::temp_dir().join(format!("less-c2-switch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let first = dir.join("first");
        let second = dir.join("second");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        let layouts = dir.join("layouts.json");
        let pins = dir.join("pins.json");
        let sessions = dir.join("sessions");
        let state = ProjectState::open(&first, layouts.clone(), pins.clone(), sessions.clone(), true);
        let mut old = ProjectView::with_state("old".into(), vec![first.clone()], false, Keymap::Vim, state);
        old.layout.open(Area::Terminal);
        old.persist();
        drop(old);
        let state = ProjectState::open(&second, layouts.clone(), pins, sessions, true);
        let mut new = ProjectView::with_state("new".into(), vec![second.clone()], false, Keymap::Vim, state);
        let ctx = egui::Context::default();
        frame(&mut new, &ctx, Vec::new());
        assert!(!new.layout.is_open(Area::Terminal));
        let store = less_workbench::persist::LayoutStore::open(&layouts);
        assert!(store.get(&first.to_string_lossy()).unwrap().panels.contains(&"terminal".into()));
        assert!(!store.get(&second.to_string_lossy()).unwrap().panels.contains(&"terminal".into()));
        drop(new);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn keymap_setting_selects_engine() {
        assert_eq!(edit_mode(Keymap::Vim), EditMode::Vim);
        assert_eq!(edit_mode(Keymap::Normal), EditMode::Modeless);
    }
}
