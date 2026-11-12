//! Ortak arama penceresi (proje ekranı): Cmd+O dosya bul, Cmd+Shift+F projede metin ara, Cmd+E son dosyalar,
//! Cmd+K kısayol ara, Cmd+Shift+P komut paleti. Üstte mod çipleri ve giriş kutusu, altında süzülmüş liste;
//! ↑↓ / PageUp / PageDown seç, Enter aç ya da çalıştır, Esc ya da pencere dışına tık kapatır.
//! Metin araması (`crate::grep`, rg ya da Rust) arka plan iş parçacığında çalışır ve yazma 150 ms durunca başlar;
//! sonuçlar geldikçe listelenir. Büyük/küçük harf, tam kelime ve regex düğmeleri girişin yanında; sağda seçili
//! sonucun dosyası eşleşme satırı ortada, vurgulu önizlenir (Snacks grep önizlemesi gibi).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{self, Align2, Color32, FontId, Frame, Key, Margin, Modifiers, Rect, Sense, Stroke, Ui, pos2, vec2};
use less_theme::Palette;
use less_workspace::recent;
use less_workspace::search::{self, FileEntry, Hit};

use crate::editor::EditorTab;
use crate::editor::find::{FindOptions, toggle};
use crate::editor::view::{CODE_SIZE, line_job, line_text};
use crate::fonts::icon;
use crate::settings::Keymap;
use crate::welcome::keycaps;
use crate::widgets::{DRAG_STRIP, color, text_input, ui_font};

const WIDTH: f32 = 720.0;
/// Metin modunda pencere genişliği (liste + önizleme).
const TEXT_WIDTH: f32 = 1240.0;
const ROW: f32 = 30.0;
const VISIBLE_ROWS: usize = 14;
const MAX_FILE_ITEMS: usize = 300;
const MAX_HITS: usize = 2000;
const DEBOUNCE: Duration = Duration::from_millis(150);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Files,
    Text,
    Recent,
    Shortcuts,
    Commands,
}

impl Mode {
    const ALL: [Mode; 5] = [Mode::Files, Mode::Text, Mode::Recent, Mode::Shortcuts, Mode::Commands];

    fn label(self) -> &'static str {
        match self {
            Mode::Files => "Files",
            Mode::Text => "Text",
            Mode::Recent => "Recent",
            Mode::Shortcuts => "Shortcuts",
            Mode::Commands => "Commands",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Mode::Files => "Find file by name",
            Mode::Text => "Find in project",
            Mode::Recent => "Recent files",
            Mode::Shortcuts => "Search shortcuts",
            Mode::Commands => "Run a command",
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            Mode::Files => icon::FILE,
            Mode::Text => icon::MAGNIFYING_GLASS,
            Mode::Recent => icon::CLOCK,
            Mode::Shortcuts => icon::KEYBOARD,
            Mode::Commands => icon::TERMINAL,
        }
    }
}

/// Komut paletinden ve kısayol listesinden çalıştırılabilen işler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    ToggleFiles,
    FocusEditor,
    RevealInFiles,
    RefreshFiles,
    CloseTab,
    NewTab,
    Pin,
    NextTab,
    PrevTab,
    ResetLayout,
    CloseProject,
    Quit,
    Picker(Mode),
    CopyPath,
    RevealInFinder,
    Theme(&'static str),
    Keymap(Keymap),
    /// Cmd+F: dosya içi arama çubuğu.
    FindInFile,
    /// `:Minimap`.
    ToggleMinimap,
    Notes,
    ChooseTheme,
    ChooseFont,
    NewWindow,
    /// Cmd+N panel tuşu (#5).
    Panel(less_workbench::layout::Panel),
}

/// Cmd+K listesi: `(tuşlar, açıklama, Enter'da çalışan komut)`.
pub const SHORTCUTS: [(&str, &str, Option<Command>); 28] = [
    ("Cmd 1", "Files", Some(Command::ToggleFiles)),
    ("Cmd 2", "Code editor", Some(Command::FocusEditor)),
    ("Cmd 3", "AI", Some(Command::Panel(less_workbench::layout::Panel::Ai))),
    ("Cmd 4", "Terminal", Some(Command::Panel(less_workbench::layout::Panel::Terminal))),
    ("Cmd 5", "Database", Some(Command::Panel(less_workbench::layout::Panel::Database))),
    ("Cmd M", "Maven", Some(Command::Panel(less_workbench::layout::Panel::Maven))),
    ("Cmd 6", "Run", Some(Command::Panel(less_workbench::layout::Panel::Run))),
    ("Cmd 7", "Git History", Some(Command::Panel(less_workbench::layout::Panel::GitHistory))),
    ("Cmd 0", "Git", Some(Command::Panel(less_workbench::layout::Panel::Git))),
    ("Cmd Shift 0", "Reset panel sizes", Some(Command::ResetLayout)),
    ("Cmd F", "Find in file", Some(Command::FindInFile)),
    ("Cmd D", "Select word, then next match", None),
    ("Cmd Enter", "Find usages", None),
    ("Cmd O", "Find file by name", Some(Command::Picker(Mode::Files))),
    ("Cmd Shift F", "Find in project", Some(Command::Picker(Mode::Text))),
    ("Cmd E", "Recent files", Some(Command::Picker(Mode::Recent))),
    ("Cmd K", "Search shortcuts", None),
    ("Cmd Shift P", "Command palette", Some(Command::Picker(Mode::Commands))),
    ("Cmd T", "New tab", Some(Command::NewTab)),
    ("Cmd W", "Close tab", Some(Command::CloseTab)),
    ("Cmd P", "Pin or unpin tab", Some(Command::Pin)),
    ("Cmd Q", "Quit this window", Some(Command::Quit)),
    ("Cmd N", "Notes", Some(Command::Notes)),
    ("Cmd Shift N", "New window", Some(Command::NewWindow)),
    ("Cmd Shift →", "Next tab", Some(Command::NextTab)),
    ("Cmd Shift ←", "Previous tab", Some(Command::PrevTab)),
    ("Cmd Shift W", "Close project", Some(Command::CloseProject)),
    ("Esc", "Close this window", None),
];

/// Komut paletinin sabit girdileri: `(ad, tuşlar, komut)`; temalar ve keymap'ler bunlara eklenir.
const COMMANDS: [(&str, Option<&str>, Command); 18] = [
    ("Toggle Files", Some("Cmd 1"), Command::ToggleFiles),
    ("Focus Code Editor", Some("Cmd 2"), Command::FocusEditor),
    ("Reveal Current File in Files", None, Command::RevealInFiles),
    ("Refresh Files", None, Command::RefreshFiles),
    ("Find File", Some("Cmd O"), Command::Picker(Mode::Files)),
    ("Find in File", Some("Cmd F"), Command::FindInFile),
    ("Toggle Minimap", None, Command::ToggleMinimap),
    ("Find in Project", Some("Cmd Shift F"), Command::Picker(Mode::Text)),
    ("Recent Files", Some("Cmd E"), Command::Picker(Mode::Recent)),
    ("Search Shortcuts", Some("Cmd K"), Command::Picker(Mode::Shortcuts)),
    ("Close Tab", Some("Cmd W"), Command::CloseTab),
    ("Next Tab", Some("Cmd Shift →"), Command::NextTab),
    ("Previous Tab", Some("Cmd Shift ←"), Command::PrevTab),
    ("Copy Path of Current File", None, Command::CopyPath),
    ("Reveal Current File in Finder", None, Command::RevealInFinder),
    ("Notes", Some("Cmd N"), Command::Notes),
    ("Choose Theme", None, Command::ChooseTheme),
    ("Choose Font", None, Command::ChooseFont),
];

pub enum Outcome {
    Keep,
    Close,
    /// Dosyayı aç; metin aramasından geldiyse satır (0 tabanlı) ve o satırdaki eşleşmeler.
    Open(PathBuf, Option<(usize, Vec<(usize, usize)>)>),
    Run(Command),
}

/// Pencerenin okuduğu proje durumu.
pub struct Env<'a> {
    /// Dosya listesi; ilk tarama bitmediyse `None`.
    pub files: Option<&'a Arc<Vec<FileEntry>>>,
    pub indexing: bool,
    /// En son kullanılan dosyalar, en yenisi başta.
    pub mru: &'a [PathBuf],
    pub roots: &'a [PathBuf],
    pub active: Option<&'a Path>,
    pub theme: &'a str,
    pub keymap: Keymap,
}

enum Target {
    File(PathBuf),
    Line(PathBuf, usize, Vec<(usize, usize)>),
    Run(Option<Command>),
}

struct Item {
    glyph: &'static str,
    title: String,
    title_hl: Vec<usize>,
    detail: String,
    detail_hl: Vec<usize>,
    keys: Option<&'static str>,
    current: bool,
    target: Target,
}

enum Msg {
    Hits(Vec<Hit>),
    Done(bool),
}

/// Süren ya da biten metin araması.
#[derive(Default)]
struct TextSearch {
    query: String,
    opts: FindOptions,
    cancel: Option<Arc<AtomicBool>>,
    rx: Option<Receiver<Msg>>,
    hits: Vec<Hit>,
    done: bool,
    truncated: bool,
}

impl TextSearch {
    fn stop(&mut self) {
        if let Some(c) = self.cancel.take() {
            c.store(true, Ordering::Relaxed);
        }
        self.rx = None;
    }

    /// Yeni aramayı başlatır; sonuçlar 60 ms'de bir toplu gönderilir.
    fn start(
        &mut self,
        ctx: &egui::Context,
        files: Arc<Vec<FileEntry>>,
        roots: Vec<PathBuf>,
        query: String,
        opts: FindOptions,
    ) {
        self.stop();
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        let flag = cancel.clone();
        let q = query.clone();
        std::thread::spawn(move || {
            let mut batch = Vec::new();
            let mut last = Instant::now();
            let mut count = 0;
            let finished = crate::grep::run(&files, &roots, &q, opts, &flag, |hit| {
                batch.push(hit);
                count += 1;
                if last.elapsed() > Duration::from_millis(60) {
                    let _ = tx.send(Msg::Hits(std::mem::take(&mut batch)));
                    ctx.request_repaint();
                    last = Instant::now();
                }
                count < MAX_HITS
            });
            let _ = tx.send(Msg::Hits(batch));
            let _ = tx.send(Msg::Done(!finished && !flag.load(Ordering::Relaxed)));
            ctx.request_repaint();
        });
        *self =
            Self { query, opts, cancel: Some(cancel), rx: Some(rx), hits: Vec::new(), done: false, truncated: false };
    }

    /// Gelen sonuçları alır; yeni bir şey geldiyse `true`.
    fn poll(&mut self) -> bool {
        let Some(rx) = &self.rx else { return false };
        let mut changed = false;
        while let Ok(msg) = rx.try_recv() {
            changed = true;
            match msg {
                Msg::Hits(h) => self.hits.extend(h),
                Msg::Done(truncated) => {
                    self.done = true;
                    self.truncated = truncated;
                }
            }
        }
        if self.done {
            self.rx = None;
            self.cancel = None;
        }
        changed
    }
}

pub struct Picker {
    mode: Mode,
    query: String,
    selected: usize,
    items: Vec<Item>,
    /// Sınır yüzünden kesilmeden önceki eşleşme sayısı.
    total: usize,
    dirty: bool,
    /// Son yapılan listenin dosya listesi (değişince liste yeniden kurulur).
    files_ptr: usize,
    first_build: bool,
    reveal_selected: bool,
    list_scroll: f32,
    edited_at: Instant,
    text: TextSearch,
    /// Metin aramasının seçenekleri (pencere kapanınca da kalmaz; her açılışta varsayılan).
    opts: FindOptions,
    /// Metin modunda seçili sonucun dosyası (yol değişince yeniden yüklenir).
    preview: Option<(PathBuf, Option<EditorTab>)>,
}

impl Picker {
    pub fn new(mode: Mode) -> Self {
        Self {
            mode,
            query: String::new(),
            selected: 0,
            items: Vec::new(),
            total: 0,
            dirty: true,
            files_ptr: 0,
            first_build: true,
            reveal_selected: true,
            list_scroll: 0.0,
            edited_at: Instant::now(),
            text: TextSearch::default(),
            opts: FindOptions::default(),
            preview: None,
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Modu değiştirir; yazılmış sorgu korunur (Cmd+F'te yazılanı Cmd+Shift+F'te aramak için).
    pub fn set_mode(&mut self, mode: Mode) {
        if self.mode != mode {
            self.mode = mode;
            self.selected = 0;
            self.dirty = true;
            self.first_build = true;
            self.reveal_selected = true;
            self.edited_at = Instant::now() - DEBOUNCE;
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, p: &Palette, env: &Env) -> Outcome {
        if self.text.poll() && self.mode == Mode::Text {
            self.dirty = true;
        }
        let (up, down, page_up, page_down, enter, esc) = ctx.input_mut(|i| {
            (
                i.consume_key(Modifiers::NONE, Key::ArrowUp),
                i.consume_key(Modifiers::NONE, Key::ArrowDown),
                i.consume_key(Modifiers::NONE, Key::PageUp),
                i.consume_key(Modifiers::NONE, Key::PageDown),
                i.consume_key(Modifiers::NONE, Key::Enter),
                i.consume_key(Modifiers::NONE, Key::Escape),
            )
        });
        if esc {
            self.text.stop();
            return Outcome::Close;
        }
        self.text_search(ctx, env);
        let files_ptr = env.files.map(|f| Arc::as_ptr(f) as usize).unwrap_or(0);
        if files_ptr != self.files_ptr && matches!(self.mode, Mode::Files) {
            self.dirty = true;
        }
        if self.dirty {
            self.files_ptr = files_ptr;
            self.rebuild(env);
        }
        let n = self.items.len();
        if n > 0 {
            let step = VISIBLE_ROWS - 1;
            let sel = self.selected as isize
                + if up { -1 } else { 0 }
                + if down { 1 } else { 0 }
                + if page_up { -(step as isize) } else { 0 }
                + if page_down { step as isize } else { 0 };
            if up || down || page_up || page_down {
                self.selected = if up && self.selected == 0 {
                    n - 1
                } else if down && self.selected + 1 == n {
                    0
                } else {
                    sel.clamp(0, n as isize - 1) as usize
                };
                self.reveal_selected = true;
            }
        }
        if enter && let Some(out) = self.activate(self.selected) {
            return out;
        }

        let screen = ctx.content_rect();
        let backdrop = egui::Area::new(egui::Id::unique("picker-backdrop"))
            .order(egui::Order::Middle)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let resp = ui.allocate_rect(screen, Sense::click());
                ui.painter().rect_filled(screen, 0.0, Color32::from_black_alpha(if p.dark { 110 } else { 50 }));
                resp.clicked()
            });
        if backdrop.inner {
            self.text.stop();
            return Outcome::Close;
        }
        let width = if self.mode == Mode::Text { TEXT_WIDTH } else { WIDTH }.min(screen.width() - 32.0);
        let at = pos2(screen.center().x - width / 2.0, screen.top() + DRAG_STRIP + 36.0);
        let frame = Frame::NONE
            .fill(color(p.surface))
            .stroke(Stroke::new(1.0, color(p.border)))
            .corner_radius(12.0)
            .inner_margin(Margin::same(10))
            .shadow(egui::Shadow { offset: [0, 10], blur: 28, spread: 0, color: Color32::from_black_alpha(80) });
        let inner = egui::Area::new(egui::Id::unique("picker"))
            .order(egui::Order::Foreground)
            .fixed_pos(at)
            .show(ctx, |ui| frame.show(ui, |ui| self.contents(ui, p, env, width - 20.0)).inner);
        if let Some(mode) = inner.inner.0 {
            self.set_mode(mode);
        }
        match inner.inner.1 {
            Some(i) => self.activate(i).unwrap_or(Outcome::Keep),
            None => Outcome::Keep,
        }
    }

    /// Metin modunda yazma durunca aramayı başlatır; dosya listesi hazır değilse bekler.
    fn text_search(&mut self, ctx: &egui::Context, env: &Env) {
        if self.mode != Mode::Text || (self.query == self.text.query && self.opts == self.text.opts) {
            return;
        }
        if self.query.is_empty() {
            self.text.stop();
            self.text = TextSearch::default();
            self.dirty = true;
            return;
        }
        let wait = DEBOUNCE.saturating_sub(self.edited_at.elapsed());
        match env.files {
            Some(files) if wait.is_zero() => {
                self.text.start(ctx, files.clone(), env.roots.to_vec(), self.query.clone(), self.opts);
                self.selected = 0;
                self.dirty = true;
            }
            _ => ctx.request_repaint_after(wait.max(Duration::from_millis(30))),
        }
    }

    /// Seçili satırı çalıştırır; bir şey olmayacaksa (`Cmd K` gibi) `None`.
    fn activate(&mut self, i: usize) -> Option<Outcome> {
        let out = match &self.items.get(i)?.target {
            Target::File(p) => Outcome::Open(p.clone(), None),
            Target::Line(p, line, ranges) => Outcome::Open(p.clone(), Some((*line, ranges.clone()))),
            Target::Run(Some(c)) => Outcome::Run(*c),
            Target::Run(None) => return None,
        };
        self.text.stop();
        Some(out)
    }

    /// Listeyi moda ve sorguya göre yeniden kurar.
    fn rebuild(&mut self, env: &Env) {
        self.dirty = false;
        let (items, total) = match self.mode {
            Mode::Files => files_items(&self.query, env),
            Mode::Text => text_items(&self.text.hits),
            Mode::Recent => recent_items(&self.query, env),
            Mode::Shortcuts => shortcut_items(&self.query),
            Mode::Commands => command_items(&self.query, env),
        };
        self.items = items;
        self.total = total;
        if std::mem::take(&mut self.first_build) && self.mode == Mode::Recent && self.query.is_empty() {
            let current = self
                .items
                .iter()
                .position(|it| matches!(&it.target, Target::File(p) if Some(p.as_path()) == env.active));
            self.selected = if current == Some(0) && self.items.len() > 1 { 1 } else { 0 };
        }
        self.selected = self.selected.min(self.items.len().saturating_sub(1));
        self.reveal_selected = true;
    }

    /// Pencere içi: mod çipleri, giriş, liste, alt bilgi. `(tıklanan mod, tıklanan satır)`.
    fn contents(&mut self, ui: &mut Ui, p: &Palette, env: &Env, width: f32) -> (Option<Mode>, Option<usize>) {
        ui.set_width(width);
        ui.spacing_mut().item_spacing.y = 8.0;
        let mut clicked_mode = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for mode in Mode::ALL {
                if chip(ui, p, mode, mode == self.mode).clicked() {
                    clicked_mode = Some(mode);
                }
            }
        });
        let id = ui.scope_id().with("picker-query");
        let text_mode = self.mode == Mode::Text;
        let input_w = if text_mode { width - 3.0 * 30.0 } else { width };
        let resp = ui
            .horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let resp =
                    text_input(ui, p, id, &mut self.query, self.mode.hint(), Some(self.mode.glyph()), input_w, true);
                if text_mode {
                    for (label, tip, on) in [
                        ("Aa", "Match case", &mut self.opts.case),
                        ("ab", "Match whole word", &mut self.opts.word),
                        (".*", "Use regular expression", &mut self.opts.regex),
                    ] {
                        if toggle(ui, p, label, *on).on_hover_text(tip).clicked() {
                            *on = !*on;
                            self.edited_at = Instant::now() - DEBOUNCE;
                        }
                    }
                }
                resp
            })
            .inner;
        if resp.changed() {
            self.dirty = true;
            self.selected = 0;
            self.edited_at = Instant::now();
        }
        let invalid =
            text_mode.then(|| self.opts.compile(&self.query).err()).flatten().filter(|_| !self.query.is_empty());

        let mut clicked = None;
        if let Some(err) = invalid {
            let (rect, _) = ui.allocate_exact_size(vec2(width, 64.0), Sense::hover());
            let text = format!("Invalid regular expression: {err}");
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, ui_font(13.0), color(p.error));
        } else if self.items.is_empty() {
            let (rect, _) = ui.allocate_exact_size(vec2(width, 64.0), Sense::hover());
            let text = self.empty_text(env);
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, ui_font(13.0), color(p.comment));
        } else if text_mode {
            let view_h = ROW * VISIBLE_ROWS as f32;
            let list_w = (width * 0.42).max(280.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                // `allocate_ui` yatay düzeni miras alır; satırlar yan yana dizilip tek satır görünüyordu.
                ui.allocate_ui_with_layout(vec2(list_w, view_h), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(list_w);
                    clicked = self.list(ui, p, view_h);
                });
                let (rect, _) = ui.allocate_exact_size(vec2(width - list_w - 8.0, view_h), Sense::hover());
                self.preview(ui, p, rect);
            });
        } else {
            let view_h = ROW * self.items.len().min(VISIBLE_ROWS) as f32;
            clicked = ui.scope(|ui| self.list(ui, p, view_h)).inner;
        }
        footer(ui, p, &self.status(env), width);
        (clicked_mode, clicked)
    }

    /// Sanal kaydırmalı sonuç listesi; tıklanan satır döner. Satırlar arasında boşluk yok (`show_rows` hesabı doğru
    /// kalsın, seçili satır hep görünür).
    fn list(&mut self, ui: &mut Ui, p: &Palette, view_h: f32) -> Option<usize> {
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut clicked = None;
        let mut area =
            egui::ScrollArea::vertical().id_salt("picker-list").max_height(view_h).auto_shrink([false, true]);
        if std::mem::take(&mut self.reveal_selected) {
            let top = self.selected as f32 * ROW;
            let off = self.list_scroll.min(top).max(top + ROW - view_h);
            area = area.vertical_scroll_offset(off.max(0.0));
        }
        let out = area.show_rows(ui, ROW, self.items.len(), |ui, range| {
            for i in range {
                if item_row(ui, p, &self.items[i], i == self.selected).clicked() {
                    clicked = Some(i);
                }
            }
        });
        self.list_scroll = out.state.clamped_offset().y;
        clicked
    }

    /// Seçili metin sonucunun dosyası: eşleşme satırı ortada ve vurgulu, sözdizimi renkli, satır numaralı.
    fn preview(&mut self, ui: &Ui, p: &Palette, rect: Rect) {
        let painter = ui.painter_at(rect);
        painter.rect(rect, 8.0, color(p.bg), Stroke::new(1.0, color(p.border)), egui::StrokeKind::Inside);
        let Some(Target::Line(path, line, ranges)) = self.items.get(self.selected).map(|it| &it.target) else { return };
        let (path, line, ranges) = (path.clone(), *line, ranges.clone());
        if self.preview.as_ref().is_none_or(|(p, _)| *p != path) {
            let tab = (std::fs::metadata(&path).is_ok_and(|m| m.len() <= search::MAX_GREP_BYTES))
                .then(|| EditorTab::load(&path).ok())
                .flatten();
            self.preview = Some((path.clone(), tab));
        }
        let Some((_, Some(tab))) = &mut self.preview else {
            painter.text(rect.center(), Align2::CENTER_CENTER, "No preview", ui_font(13.0), color(p.comment));
            return;
        };
        let font = FontId::monospace(CODE_SIZE - 1.0);
        let line_h = (font.size * 1.5).round();
        let rope = tab.editor.buffer.rope().clone();
        let total = rope.len_lines().max(1);
        let rows = ((rect.height() - 12.0) / line_h).floor().max(1.0) as usize;
        let first = line.saturating_sub(rows / 2).min(total.saturating_sub(rows));
        let last = (first + rows).min(total);
        let highlights = tab.highlights(first..last).to_vec();
        let digits = total.to_string().len().max(3) as f32;
        let char_w = ui.fonts_mut(|f| f.glyph_width(&font, 'M'));
        let gutter = rect.left() + 10.0 + digits * char_w;
        let x0 = gutter + 12.0;
        let tab_width = tab.editor.buffer.tab_width;
        for (row, n) in (first..last).enumerate() {
            let y = rect.top() + 6.0 + row as f32 * line_h;
            let text = line_text(&rope, n);
            if n == line {
                let band = Rect::from_min_max(pos2(rect.left() + 1.0, y), pos2(rect.right() - 1.0, y + line_h));
                painter.rect_filled(band, 0.0, color(p.bg_highlight));
                let cols = crate::editor::syntax::display_cols(&text, tab_width);
                let col_of = |b: usize| cols[text[..b.min(text.len())].chars().count().min(cols.len() - 1)];
                for &(a, b) in &ranges {
                    let r = Rect::from_min_max(
                        pos2(x0 + col_of(a) as f32 * char_w, y + 1.0),
                        pos2(x0 + col_of(b) as f32 * char_w, y + line_h - 1.0),
                    );
                    painter.rect_filled(r, 3.0, color(p.accent).gamma_multiply(0.4));
                }
            }
            let num_tint = if n == line { color(p.fg) } else { color(p.comment) };
            painter.text(
                pos2(gutter, y + line_h / 2.0),
                Align2::RIGHT_CENTER,
                (n + 1).to_string(),
                font.clone(),
                num_tint,
            );
            let spans = highlights.get(row).map(Vec::as_slice).unwrap_or_default();
            let galley = painter.layout_job(line_job(&text, spans, tab_width, &font, p));
            painter.galley(pos2(x0, y + (line_h - galley.size().y) / 2.0), galley, color(p.fg));
        }
    }

    fn empty_text(&self, env: &Env) -> &'static str {
        match self.mode {
            Mode::Files if env.files.is_none() => "Indexing files…",
            Mode::Text if self.query.is_empty() => "Type to search in all files",
            Mode::Text if !self.text.done => "Searching…",
            Mode::Recent if self.query.is_empty() => "No recent files yet",
            _ => "No matches",
        }
    }

    fn status(&self, env: &Env) -> String {
        match self.mode {
            Mode::Files => match env.files {
                None => "Indexing files…".to_owned(),
                Some(f) => {
                    let more = if self.total > self.items.len() {
                        format!(" (showing {})", self.items.len())
                    } else {
                        String::new()
                    };
                    let refreshing = if env.indexing { " · refreshing…" } else { "" };
                    if self.query.is_empty() {
                        format!("{} files{refreshing}", f.len())
                    } else {
                        format!("{} of {} files{more}{refreshing}", self.total, f.len())
                    }
                }
            },
            Mode::Text if self.query.is_empty() => String::new(),
            Mode::Text => {
                let files = {
                    let mut paths: Vec<&Path> = self.text.hits.iter().map(|h| h.path.as_path()).collect();
                    paths.dedup();
                    paths.len()
                };
                let state = if !self.text.done {
                    " · searching…"
                } else if self.text.truncated {
                    " · limit reached"
                } else {
                    ""
                };
                format!("{} results in {files} files{state}", self.text.hits.len())
            }
            _ => format!("{} items", self.items.len()),
        }
    }
}

/// Dosya modu: sorgu boşken önce son kullanılanlar, sonra liste sırası; sorguyla bulanık eşleşme puanına göre.
fn files_items(query: &str, env: &Env) -> (Vec<Item>, usize) {
    let Some(files) = env.files else { return (Vec::new(), 0) };
    if query.trim().is_empty() {
        let recent: Vec<&FileEntry> = env.mru.iter().filter_map(|m| files.iter().find(|f| &f.path == m)).collect();
        let rest = files.iter().filter(|f| !env.mru.contains(&f.path));
        let items = recent.into_iter().chain(rest).take(MAX_FILE_ITEMS).map(|f| file_item(f, &[])).collect::<Vec<_>>();
        return (items, files.len());
    }
    let mut scored: Vec<(i32, usize, Vec<usize>)> = files
        .iter()
        .enumerate()
        .filter_map(|(i, f)| search::fuzzy(query, &f.rel).map(|m| (m.score, i, m.positions)))
        .collect();
    let total = scored.len();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| files[a.1].rel.len().cmp(&files[b.1].rel.len())));
    let items = scored.into_iter().take(MAX_FILE_ITEMS).map(|(_, i, pos)| file_item(&files[i], &pos)).collect();
    (items, total)
}

/// `rel` içindeki konumları dosya adı (başlık) ve klasör (ayrıntı) olarak böler.
fn file_item(f: &FileEntry, positions: &[usize]) -> Item {
    let start = f.name_start();
    let title: String = f.rel.chars().skip(start).collect();
    let detail: String = f.rel.chars().take(start.saturating_sub(1)).collect();
    Item {
        glyph: file_glyph(&title),
        title_hl: positions.iter().filter(|&&i| i >= start).map(|i| i - start).collect(),
        detail_hl: positions.iter().copied().filter(|&i| i + 1 < start).collect(),
        title,
        detail,
        keys: None,
        current: false,
        target: Target::File(f.path.clone()),
    }
}

fn text_items(hits: &[Hit]) -> (Vec<Item>, usize) {
    let items = hits
        .iter()
        .map(|h| Item {
            glyph: file_glyph(&h.rel),
            title: h.snippet.clone(),
            title_hl: h.snippet_ranges.iter().flat_map(|&(s, e)| s..e).collect(),
            detail: format!("{}:{}", h.rel, h.line + 1),
            detail_hl: Vec::new(),
            keys: None,
            current: false,
            target: Target::Line(h.path.clone(), h.line, h.ranges.clone()),
        })
        .collect();
    (items, hits.len())
}

/// Son kullanılan dosyalar (hâlâ var olanlar), en yenisi başta.
fn recent_items(query: &str, env: &Env) -> (Vec<Item>, usize) {
    let mut out = Vec::new();
    for path in env.mru.iter().filter(|p| p.is_file()) {
        let rel = relative(path, env.roots);
        let entry = FileEntry { path: path.clone(), rel };
        if let Some(m) = search::fuzzy(query, &entry.rel) {
            out.push((m.score, file_item(&entry, &m.positions)));
        }
    }
    if !query.trim().is_empty() {
        out.sort_by_key(|a| std::cmp::Reverse(a.0));
    }
    let items: Vec<Item> = out.into_iter().map(|(_, it)| it).collect();
    let n = items.len();
    (items, n)
}

fn shortcut_items(query: &str) -> (Vec<Item>, usize) {
    let mut out: Vec<(i32, Item)> = SHORTCUTS
        .iter()
        .filter_map(|&(keys, what, cmd)| {
            let (score, hl) = match search::fuzzy(query, what) {
                Some(m) => (m.score, m.positions),
                None => (search::fuzzy(query, keys)?.score - 20, Vec::new()),
            };
            let item = Item {
                glyph: icon::KEYBOARD,
                title: what.to_owned(),
                title_hl: hl,
                detail: String::new(),
                detail_hl: Vec::new(),
                keys: Some(keys),
                current: false,
                target: Target::Run(cmd),
            };
            Some((score, item))
        })
        .collect();
    if !query.trim().is_empty() {
        out.sort_by_key(|a| std::cmp::Reverse(a.0));
    }
    let items: Vec<Item> = out.into_iter().map(|(_, it)| it).collect();
    let n = items.len();
    (items, n)
}

fn command_items(query: &str, env: &Env) -> (Vec<Item>, usize) {
    let mut all: Vec<(String, Option<&'static str>, Command, &'static str, bool)> =
        COMMANDS.iter().map(|&(t, k, c)| (t.to_owned(), k, c, icon::TERMINAL, false)).collect();
    for p in less_theme::ALL {
        all.push((format!("Theme: {}", p.name), None, Command::Theme(p.id), icon::PALETTE, p.id == env.theme));
    }
    for k in [Keymap::Vim, Keymap::Normal] {
        all.push((format!("Keymap: {}", k.label()), None, Command::Keymap(k), icon::KEYBOARD, k == env.keymap));
    }
    all.push(("Close Project".to_owned(), Some("Cmd Shift W"), Command::CloseProject, icon::SIGN_OUT, false));
    let mut out: Vec<(i32, Item)> = all
        .into_iter()
        .filter_map(|(title, keys, cmd, glyph, current)| {
            let m = search::fuzzy(query, &title)?;
            let item = Item {
                glyph,
                title,
                title_hl: m.positions,
                detail: String::new(),
                detail_hl: Vec::new(),
                keys,
                current,
                target: Target::Run(Some(cmd)),
            };
            Some((m.score, item))
        })
        .collect();
    if !query.trim().is_empty() {
        out.sort_by_key(|a| std::cmp::Reverse(a.0));
    }
    let items: Vec<Item> = out.into_iter().map(|(_, it)| it).collect();
    let n = items.len();
    (items, n)
}

/// Köklerden birine göre göreli yol (çok kökte kök adıyla), değilse `~` kısaltmalı tam yol.
pub fn relative(path: &Path, roots: &[PathBuf]) -> String {
    for root in roots {
        if let Ok(rel) = path.strip_prefix(root) {
            let rel = rel.to_string_lossy();
            return match (roots.len() > 1, root.file_name()) {
                (true, Some(name)) => format!("{}/{rel}", name.to_string_lossy()),
                _ => rel.into_owned(),
            };
        }
    }
    recent::display_path(path)
}

fn file_glyph(name: &str) -> &'static str {
    crate::files::file_icon(name)
}

fn chip(ui: &mut Ui, p: &Palette, mode: Mode, active: bool) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(mode.label().to_owned(), ui_font(12.0), color(p.fg));
    let (rect, resp) = ui.allocate_exact_size(vec2(galley.size().x + 20.0, 24.0), Sense::click());
    let fill = if active {
        color(p.accent).gamma_multiply(0.25)
    } else if resp.hovered() {
        color(p.bg_highlight)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 6.0, fill);
    let tint = if active { color(p.fg) } else { color(p.comment) };
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, tint);
    resp
}

/// Vurgulu harfleri vurgu renginde yazan metin düzeni.
fn highlighted(text: &str, hl: &[usize], font: FontId, base: Color32, accent: Color32) -> LayoutJob {
    let mut job = LayoutJob::default();
    let mut run = String::new();
    let mut run_hl = false;
    for (i, c) in text.chars().enumerate() {
        let h = hl.binary_search(&i).is_ok();
        if h != run_hl && !run.is_empty() {
            let tint = if run_hl { accent } else { base };
            job.append(&std::mem::take(&mut run), 0.0, TextFormat::simple(font.clone(), tint));
        }
        run_hl = h;
        run.push(c);
    }
    if !run.is_empty() {
        job.append(&run, 0.0, TextFormat::simple(font, if run_hl { accent } else { base }));
    }
    job
}

fn item_row(ui: &mut Ui, p: &Palette, item: &Item, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, 6.0, color(p.accent).gamma_multiply(0.28));
    } else if resp.hovered() {
        painter.rect_filled(rect, 6.0, color(p.bg_highlight).gamma_multiply(0.7));
    }
    let y = rect.center().y;
    painter.text(pos2(rect.left() + 12.0, y), Align2::LEFT_CENTER, item.glyph, ui_font(14.0), color(p.comment));
    let mut right = rect.right() - 10.0;
    if item.current {
        painter.text(pos2(right, y), Align2::RIGHT_CENTER, icon::CHECK, ui_font(14.0), color(p.accent));
        right -= 24.0;
    }
    if let Some(keys) = item.keys {
        keycaps(ui, p, keys, right, y);
        right -= keys.split(' ').map(|k| k.chars().count() as f32 * 7.5 + 20.0).sum::<f32>() + 8.0;
    }
    let clip = Rect::from_min_max(pos2(rect.left() + 36.0, rect.top()), pos2(right, rect.bottom()));
    let painter = ui.painter().with_clip_rect(clip.intersect(ui.clip_rect()));
    let accent = color(p.accent);
    let title = painter.layout_job(highlighted(&item.title, &item.title_hl, ui_font(13.5), color(p.fg), accent));
    let title_w = title.size().x;
    painter.galley(pos2(clip.left(), y - title.size().y / 2.0), title, color(p.fg));
    if !item.detail.is_empty() {
        let detail =
            painter.layout_job(highlighted(&item.detail, &item.detail_hl, ui_font(12.0), color(p.comment), accent));
        painter.galley(pos2(clip.left() + title_w + 12.0, y - detail.size().y / 2.0), detail, color(p.comment));
    }
    resp
}

fn footer(ui: &mut Ui, p: &Palette, status: &str, width: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 18.0), Sense::hover());
    let painter = ui.painter();
    let font = ui_font(11.5);
    painter.text(rect.left_center() + vec2(4.0, 0.0), Align2::LEFT_CENTER, status, font.clone(), color(p.comment));
    let hint = "↑↓ select   Enter open   Esc close";
    painter.text(rect.right_center() - vec2(4.0, 0.0), Align2::RIGHT_CENTER, hint, font, color(p.comment));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(rel: &str) -> FileEntry {
        FileEntry { path: PathBuf::from("/p").join(rel), rel: rel.to_owned() }
    }

    fn env<'a>(files: Option<&'a Arc<Vec<FileEntry>>>, mru: &'a [PathBuf]) -> Env<'a> {
        Env {
            files,
            indexing: false,
            mru,
            roots: &[],
            active: None,
            theme: less_theme::DARCULA.id,
            keymap: Keymap::Normal,
        }
    }

    #[test]
    fn file_item_splits_highlights_between_name_and_folder() {
        let it = file_item(&entry("src/app/Main.java"), &[0, 8, 9]);
        assert_eq!((it.title.as_str(), it.detail.as_str()), ("Main.java", "src/app"));
        assert_eq!(it.title_hl, [0, 1]);
        assert_eq!(it.detail_hl, [0]);
    }

    #[test]
    fn files_mode_ranks_and_lists_recent_first() {
        let files = Arc::new(vec![entry("README.md"), entry("src/usecase.ts"), entry("src/UserController.ts")]);
        let mru = [PathBuf::from("/p/src/usecase.ts")];
        let (items, total) = files_items("", &env(Some(&files), &mru));
        assert_eq!(total, 3);
        assert_eq!(items[0].title, "usecase.ts");
        let (items, total) = files_items("uc", &env(Some(&files), &mru));
        assert_eq!(total, 2);
        assert_eq!(items[0].title, "UserController.ts");
        assert!(files_items("x", &env(None, &[])).0.is_empty());
    }

    #[test]
    fn shortcut_list_contains_host_commands_30_42() {
        let (items, _) = shortcut_items("reset panel");
        assert!(matches!(items[0].target, Target::Run(Some(Command::ResetLayout))));
        let (items, _) = shortcut_items("pin or");
        assert!(matches!(items[0].target, Target::Run(Some(Command::Pin))));
        let (items, _) = shortcut_items("quit this");
        assert!(matches!(items[0].target, Target::Run(Some(Command::Quit))));
    }

    #[test]
    fn commands_include_themes_and_mark_current() {
        let (items, _) = command_items("darcula", &env(None, &[]));
        assert_eq!(items[0].title, "Theme: Darcula");
        assert!(items[0].current);
        let (items, _) = shortcut_items("recent");
        assert_eq!(items[0].keys, Some("Cmd E"));
        assert!(matches!(items[0].target, Target::Run(Some(Command::Picker(Mode::Recent)))));
    }
}
