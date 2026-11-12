//! Kod editörünün dil servisleri (B5 lang-ui): `less-lang::LspManager` ile belge eşitleme, problem sayacı +
//! kenar işaretleri + alt çizgi (#31), hata / dokümantasyon tooltip'i (#52, #70), `gd` / `gr` / `K` / `]d`,
//! rename / code action, oluk tıklamasıyla breakpoint (#92), Ctrl/Option+tık ve sağ tık menüsü (#13).
//!
//! Host (`project.rs`) kancaları B0 sözleşmesindeki anlarda çağırır (`less-rs/docs/handoff/B0.md`); çizim ve
//! fare `ui` alt modülünde, UI'dan bağımsız dönüşümler `text`'te.
//!
//! less-app `lsp-types` / `serde_json`'a bağımlı olmadığı için o tiplerin adı yazılmaz: konumlar
//! [`less_lang::Pos`] ile tutulur, LSP tipleri yalnız çıkarım ve alan erişimiyle kullanılır.

mod complete;
mod text;
mod ui;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use eframe::egui;
use less_core::command::{Command, CursorMode};
use less_core::editor::Session;
use less_core::registers::MemoryClipboard;
use less_core::selection::{Range, Selection};
use less_git::blame::BlameLine;
use less_input::{EditMode, Key, KeyEvent};
use less_lang::lsp::{Location, lsp_position, workspace_edit_files};
use less_lang::tooltip::assemble;
use less_lang::{ContentChange, Diagnostic, LspConfig, LspEvent, LspManager, LspResult, Pos, RequestId, Severity};
use less_lang::{TextEdit, Tooltip};

use super::EditorTab;
use super::view::{CodeGeometry, line_text};
use crate::panels::{Cx, Event};
use text::{Edit, char_of_pos, pos_of_char};

pub use less_lang::lsp::ServerState;

/// Durum satırı için bir dil sunucusunun adı ve hali ([`Lang::active_servers`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerStatus {
    /// Kullanıcıya görünen ad (`"vtsls"`, `"rust-analyzer"`).
    pub name: String,
    pub state: ServerState,
}

/// İmleç bu kadar durunca satırın diagnostikleri gösterilir (Vim `CursorHold`, LazyVim `updatetime`).
const HOLD: Duration = Duration::from_millis(200);
/// Fare aynı kelimede bu kadar bekleyince tooltip (#52, #70; Lua `MOUSE_DELAY`).
const MOUSE_WAIT: Duration = Duration::from_millis(350);

/// Breakpoint aç/kapa konusu (B2 Run paneli dinler); veri `"<yol>:<satır, 1 tabanlı>"`.
pub const TOPIC_TOGGLE_BREAKPOINT: &str = "run.toggle_breakpoint";
/// Run panelinin bir dosyadaki breakpoint'leri (B2 gönderir; host `lang.event`'e iletirse oluk güncellenir);
/// veri `"<yol>:<satır,satır…>"` (1 tabanlı, boş = hiç yok).
pub const TOPIC_BREAKPOINTS: &str = "run.breakpoints";

/// Bir karakterin LSP `Position`'ı (tip adı yazılmadan: `lsp_position` ile üretilip alanları doldurulur).
macro_rules! lsp_pos {
    ($rope:expr, $ch:expr) => {{
        let p = pos_of_char($rope, $ch);
        let mut q = lsp_position("", 0);
        q.line = p.line;
        q.character = p.character;
        q
    }};
}

/// `WorkspaceEdit` → dosya başına [`Edit`] listesi.
macro_rules! file_edits {
    ($edit:expr) => {
        workspace_edit_files($edit)
            .into_iter()
            .map(|(path, edits)| {
                let edits: Vec<Edit> = edits
                    .iter()
                    .map(|e| Edit {
                        start: Pos { line: e.range.start.line, character: e.range.start.character },
                        end: Pos { line: e.range.end.line, character: e.range.end.character },
                        text: e.new_text.clone(),
                    })
                    .collect();
                (path, edits)
            })
            .collect::<Vec<_>>()
    };
}

/// Yanıtı beklenen isteğin amacı.
#[derive(Clone, Debug, PartialEq)]
enum Pending {
    /// Tooltip bu isteği bekliyorsa doküman eklenir; beklemiyorsa (fare gitti, tuşa basıldı) atılır.
    Hover,
    Locations {
        what: &'static str,
    },
    Rename,
    CodeActions {
        path: PathBuf,
    },
}

/// Konum isteği türü.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lsp {
    Definition,
    References,
    /// Bildirim hariç referanslar (Cmd+Enter).
    Usages,
    Implementation,
    TypeDefinition,
    Hover,
    CodeActions,
}

/// Tooltip'in bağlandığı yer.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Anchor {
    /// İmlecin altında (`K`, CursorHold).
    Cursor,
    /// Farenin altında (#70).
    Mouse(egui::Pos2),
}

/// Görünen (ya da hover yanıtı beklenen) tooltip.
#[derive(Clone, Debug)]
struct Tip {
    tooltip: Tooltip,
    anchor: Anchor,
    path: PathBuf,
    line: u32,
    /// Beklenen hover isteği; başka bir yanıt eskidir.
    hover: Option<RequestId>,
    /// Konumdaki cron string'inin ne zaman tetikleneceği (hover dokümanından önce gösterilir).
    cron: Option<String>,
}

/// Menü satırının işi.
#[derive(Clone, Debug, PartialEq)]
enum Action {
    Definition,
    References,
    Rename,
    CodeActions,
    GitBlame,
    AddFileToAi,
    SendToAi,
    Open(PathBuf, Pos),
    Apply(usize),
}

/// Sağ tık menüsü, birden çok konum listesi ya da code action listesi.
#[derive(Clone, Debug)]
struct Menu {
    /// `None` = imlecin altında.
    pos: Option<egui::Pos2>,
    items: Vec<(String, Action)>,
    selected: usize,
}

/// Rename istemi (tek satırlık giriş kutusu).
#[derive(Clone, Debug)]
struct RenamePrompt {
    path: PathBuf,
    at: usize,
    text: String,
    focus: bool,
}

/// Menüdeki code action: düzenleme ve/veya komut (`command`, `arguments`).
#[derive(Clone, Debug)]
struct CodeAct {
    title: String,
    edit: Option<lsp_types::WorkspaceEdit>,
    command: Option<(String, serde_json::Value)>,
}

/// Fare bekleyişi (aynı satır + kelime).
#[derive(Clone, Debug)]
struct MouseWait {
    path: PathBuf,
    key: (usize, Option<usize>),
    pos: egui::Pos2,
    ch: usize,
    since: Instant,
    fired: bool,
}

/// `paint`'te çizilen tıklanabilir yer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hit {
    /// Sayaç: ikon = o önem derecesi, ↑/↓ = en yüksek önem.
    Jump { forward: bool, severity: Option<Severity> },
    /// Kenar işareti: o satıra git.
    Line(u32),
}

/// Kod editörünün dil servisleri.
pub struct Lang {
    pub roots: Vec<PathBuf>,
    lsp: LspManager,
    /// Açık dosya tab'larının LSP'ye bildirilen metni (artımlı `didChange` için eski metin gerekir).
    docs: HashMap<PathBuf, String>,
    pending: HashMap<RequestId, Pending>,
    /// Açık ama etkin olmayan tab'lara uygulanacak düzenlemeler (tab çizilince uygulanır).
    deferred: HashMap<PathBuf, Vec<Edit>>,
    /// Tanıma git: tab çizilince imleç buraya (`bool` = bir kare beklendi, host'un `open_at`'ı önce çalışsın).
    goto: Option<(PathBuf, Pos, bool)>,
    /// Oluktaki breakpoint'ler (0 tabanlı satır); asıl depo Run panelinde.
    breakpoints: HashSet<(PathBuf, usize)>,
    tip: Option<Tip>,
    hold: Option<(PathBuf, usize, Instant, bool)>,
    mouse: Option<MouseWait>,
    menu: Option<Menu>,
    rename: Option<RenamePrompt>,
    actions: Vec<CodeAct>,
    actions_path: Option<PathBuf>,
    hits: Vec<(egui::Rect, Hit)>,
    /// Tamamlama listesi (IntelliSense) ve imza yardımı.
    completion: Option<complete::Completion>,
    signature: Option<complete::Signature>,
    /// Bu karede yazılan karakter (düzenleme görülünce tetikleme kararı için).
    typed: Option<char>,
    /// Gecikmeli tamamlama isteği.
    debounce: Option<(PathBuf, Instant)>,
    /// Fareyle tıklanan liste öğesi (çizimden sonra uygulanır).
    clicked_item: Option<usize>,
    /// Oluktaki blame şeridi: dosya → satır başına commit (açılışta ve kayıtta arka planda yenilenir).
    blame: HashMap<PathBuf, Vec<Option<BlameLine>>>,
    blame_tx: mpsc::Sender<(PathBuf, Vec<Option<BlameLine>>)>,
    blame_rx: mpsc::Receiver<(PathBuf, Vec<Option<BlameLine>>)>,
}

impl Lang {
    pub fn new(roots: &[PathBuf]) -> Self {
        Self::with_lsp(roots, LspManager::new(LspConfig::default()))
    }

    fn with_lsp(roots: &[PathBuf], lsp: LspManager) -> Self {
        let (blame_tx, blame_rx) = mpsc::channel();
        Self {
            roots: roots.to_vec(),
            lsp,
            docs: HashMap::new(),
            pending: HashMap::new(),
            deferred: HashMap::new(),
            goto: None,
            breakpoints: HashSet::new(),
            tip: None,
            hold: None,
            mouse: None,
            menu: None,
            rename: None,
            actions: Vec::new(),
            actions_path: None,
            hits: Vec::new(),
            completion: None,
            signature: None,
            typed: None,
            debounce: None,
            clicked_item: None,
            blame: HashMap::new(),
            blame_tx,
            blame_rx,
        }
    }

    /// `git blame`'i arka planda çalıştırır; depoda olmayan dosya için şerit çizilmez.
    /// shortcut: kayıtlar arasında yapılan düzenlemeler satırları kaydırır, şerit kayıtta düzelir.
    fn refresh_blame(&self, path: &Path, text: String) {
        let (path, tx) = (path.to_path_buf(), self.blame_tx.clone());
        std::thread::spawn(move || {
            let lines = less_git::blame::blame(&path, &text).unwrap_or_default();
            let _ = tx.send((path, lines));
        });
    }

    /// Başlatılan her dil sunucusu ve hali (durum satırı); aynı sunucu birden çok kökte çalışıyorsa kök klasör
    /// adı eklenir.
    pub fn active_servers(&self) -> Vec<ServerStatus> {
        let states = self.lsp.server_states();
        let count = |id: &str| states.iter().filter(|(k, _)| k.server == id).count();
        states
            .iter()
            .map(|(key, state)| {
                let mut name = key.server.replace('_', "-");
                if count(key.server) > 1
                    && let Some(dir) = key.root.file_name()
                {
                    name = format!("{name} ({})", dir.to_string_lossy());
                }
                ServerStatus { name, state: *state }
            })
            .collect()
    }

    /// Dosya tab'ı açıldı (LSP `didOpen`; dil sunucusu yoksa yalnız metin tutulur).
    pub fn opened(&mut self, tab: &EditorTab, _cx: &mut Cx) {
        let Some(path) = tab.path.clone() else { return };
        if tab.read_only {
            return;
        }
        let text = tab.editor.text();
        if let Some(lang) = tab.language {
            self.lsp.open(&path, lang, &text);
        }
        self.refresh_blame(&path, text.clone());
        self.docs.insert(path, text);
    }

    /// Metin değişti (LSP `didChange`). Tutulan metin rope'la uyuşmazsa tam metin gönderilir.
    pub fn changed(&mut self, tab: &EditorTab, edit: &TextEdit, cx: &mut Cx) {
        let Some(path) = &tab.path else { return };
        let Some(shadow) = self.docs.get_mut(path) else { return };
        let rope = tab.editor.buffer.rope();
        let valid = edit.old_end_byte <= shadow.len()
            && edit.start_byte <= edit.old_end_byte
            && shadow.is_char_boundary(edit.start_byte)
            && shadow.is_char_boundary(edit.old_end_byte)
            && edit.new_end_byte <= rope.len_bytes();
        let mut change = ContentChange { range: None, text: String::new() };
        if valid {
            let inserted = rope.byte_slice(edit.start_byte..edit.new_end_byte).to_string();
            change = ContentChange::from_edit(shadow, edit.start_byte, edit.old_end_byte, &inserted);
            shadow.replace_range(edit.start_byte..edit.old_end_byte, &inserted);
        }
        if !valid || shadow.len() != rope.len_bytes() {
            *shadow = rope.to_string();
            change = ContentChange { range: None, text: shadow.clone() };
        }
        let full = shadow.clone();
        self.lsp.change(path, &[change], || full);
        self.completion_changed(tab, cx);
    }

    /// Kaydedildi (LSP `didSave`).
    pub fn saved(&mut self, tab: &EditorTab, _cx: &mut Cx) {
        if let Some(path) = &tab.path {
            self.lsp.save(path, self.docs.get(path).map(String::as_str));
            self.refresh_blame(path, tab.editor.text());
        }
    }

    /// Dosya tab'ı kapandı (LSP `didClose`).
    pub fn closed(&mut self, path: &Path, _cx: &mut Cx) {
        self.lsp.close(path);
        self.docs.remove(path);
        self.deferred.remove(path);
        self.blame.remove(path);
    }

    /// Her karede: LSP olaylarını toplar, yanıtları işler.
    pub fn tick(&mut self, cx: &mut Cx) {
        for (path, lines) in self.blame_rx.try_iter() {
            if self.docs.contains_key(&path) {
                self.blame.insert(path, lines);
                cx.ctx.request_repaint();
            }
        }
        let events = self.lsp.poll();
        if events.is_empty() {
            return;
        }
        cx.ctx.request_repaint();
        for ev in events {
            self.handle(ev, cx);
        }
    }

    /// Kod alanı odaktayken: `Key` (motordan önce; menü gezinmesi, gerçek tuş tooltip'i kapatır), `Unmapped`
    /// (`gd`, `gr`, `gI`, `gy`, `K`, `]d`/`[d`, `]e`/`[e`, `]w`/`[w`), `Shortcut` (Cmd+F8 breakpoint, Cmd+Enter
    /// kullanım yerleri: tekse oraya gider, çoksa liste).
    /// `Leader` `ca` code action, `cr` rename. `Message` (`run.breakpoints`) oluğu günceller. `true` = tüketildi.
    pub fn event(&mut self, event: &Event, tab: &mut EditorTab, session: &mut Session, cx: &mut Cx) -> bool {
        match *event {
            Event::Key(k) => self.key(k, tab, session, cx),
            Event::Unmapped(keys) => self.unmapped(keys, tab, cx),
            Event::Shortcut(k) if k.key == Key::F(8) && k.mods.cmd && !k.mods.shift => {
                let Some(path) = tab.path.clone() else { return false };
                self.toggle_breakpoint(&path, tab.editor.cursor_line_col().0, cx);
                true
            }
            Event::Shortcut(k) if k.key == Key::Enter && k.mods.cmd && !k.mods.shift => {
                self.request(Lsp::Usages, tab, cx);
                true
            }
            Event::Leader("ca") => {
                self.run_action(Action::CodeActions, tab, cx);
                true
            }
            Event::Leader("cr") => {
                self.run_action(Action::Rename, tab, cx);
                true
            }
            Event::Message { topic, .. } if topic == crate::panels::options::TOPIC_SERVERS_CHANGED => {
                self.lsp.retry_missing(&self.docs);
                false
            }
            Event::Message { topic, data } if topic == TOPIC_BREAKPOINTS => {
                if let Some((path, lines)) = data.rsplit_once(':') {
                    let path = PathBuf::from(path);
                    self.breakpoints.retain(|(p, _)| *p != path);
                    let lines = lines.split(',').filter_map(|l| l.trim().parse::<usize>().ok()?.checked_sub(1));
                    self.breakpoints.extend(lines.map(|l| (path.clone(), l)));
                }
                false
            }
            _ => false,
        }
    }

    /// Metin çizildikten sonra: bekleyen düzenlemeler / imleç, problem işaretleri ve sayaç, breakpoint'ler,
    /// tooltip zamanlaması, tooltip / menü / rename istemi.
    pub fn paint(&mut self, tab: &mut EditorTab, geo: &CodeGeometry, ui: &mut egui::Ui, cx: &mut Cx) {
        self.sync_tab(tab, cx);
        self.hits.clear();
        ui::marks(self, tab, geo, ui, cx);
        self.completion_frame(tab, cx);
        self.timers(tab, geo, ui, cx);
        ui::popups(self, tab, geo, ui, cx);
        if let Some(i) = self.clicked_item.take() {
            // shortcut: paint'te Session yok; metin ekleme register'a dokunmadığı için geçici Session yeter.
            let mut session = Session::new(Box::new(MemoryClipboard::default()));
            self.accept(i, tab, &mut session, cx);
        }
    }

    /// Fare (host'un tıklama işlemesinden önce): sayaç / kenar işareti, oluğa tık breakpoint (#92),
    /// Ctrl/Option+tık tanım, sağ tık menüsü (#13). `true` = tüketildi.
    pub fn pointer(
        &mut self,
        tab: &mut EditorTab,
        geo: &CodeGeometry,
        resp: &egui::Response,
        _session: &mut Session,
        cx: &mut Cx,
    ) -> bool {
        let Some(pos) = resp.interact_pointer_pos() else { return false };
        let rope = tab.editor.buffer.rope().clone();
        if resp.clicked() {
            if let Some(&(_, hit)) = self.hits.iter().find(|(r, _)| r.contains(pos)) {
                self.hit(hit, tab, cx);
                return true;
            }
            if geo.gutter.contains(pos) {
                let row = ((pos.y - geo.text.top()) / geo.line_h).floor().max(0.0) as usize;
                let line = geo.first_line + row;
                if let Some(path) = tab.path.clone()
                    && line < rope.len_lines()
                {
                    self.toggle_breakpoint(&path, line, cx);
                }
                return true;
            }
            let mods = resp.ctx.input(|i| i.modifiers);
            if (mods.ctrl || mods.alt)
                && let Some(ch) = geo.char_at(&rope, pos)
            {
                tab.editor.set_selection(Selection::point(ch));
                self.request(Lsp::Definition, tab, cx);
                return true;
            }
        }
        if resp.secondary_clicked() {
            if selected(tab).is_none()
                && let Some(ch) = geo.char_at(&rope, pos)
            {
                tab.editor.set_selection(Selection::point(ch));
            }
            self.context_menu(Some(pos), tab);
            return true;
        }
        false
    }

    fn key(&mut self, k: KeyEvent, tab: &mut EditorTab, session: &mut Session, cx: &mut Cx) -> bool {
        self.tip = None;
        let Some(menu) = &mut self.menu else { return self.completion_key(k, tab, session, cx) };
        let n = menu.items.len().max(1);
        match k.key {
            Key::Up => menu.selected = (menu.selected + n - 1) % n,
            Key::Down | Key::Tab => menu.selected = (menu.selected + 1) % n,
            Key::Enter => {
                let action = menu.items.get(menu.selected).map(|i| i.1.clone());
                self.menu = None;
                if let Some(a) = action {
                    self.run_action(a, tab, cx);
                }
            }
            Key::Esc => self.menu = None,
            _ => {
                self.menu = None;
                return false;
            }
        }
        true
    }

    fn unmapped(&mut self, keys: &str, tab: &mut EditorTab, cx: &mut Cx) -> bool {
        let jump = |forward, severity| Hit::Jump { forward, severity };
        match keys {
            "gd" => self.request(Lsp::Definition, tab, cx),
            "gr" => self.request(Lsp::References, tab, cx),
            "gI" => self.request(Lsp::Implementation, tab, cx),
            "gy" => self.request(Lsp::TypeDefinition, tab, cx),
            "K" => self.request(Lsp::Hover, tab, cx),
            "]d" => self.hit(jump(true, None), tab, cx),
            "[d" => self.hit(jump(false, None), tab, cx),
            "]e" => self.hit(jump(true, Some(Severity::Error)), tab, cx),
            "[e" => self.hit(jump(false, Some(Severity::Error)), tab, cx),
            "]w" => self.hit(jump(true, Some(Severity::Warning)), tab, cx),
            "[w" => self.hit(jump(false, Some(Severity::Warning)), tab, cx),
            _ => return false,
        }
        true
    }

    /// Sayaç / kenar işareti / `]d`: imleci problemin başına taşır (#31).
    fn hit(&mut self, hit: Hit, tab: &mut EditorTab, cx: &mut Cx) {
        let Some(path) = tab.path.clone() else { return };
        let rope = tab.editor.buffer.rope().clone();
        let target = match hit {
            Hit::Line(line) => Some(Pos { line, character: 0 }),
            Hit::Jump { forward, severity } => {
                let from = pos_of_char(&rope, tab.editor.cursor());
                self.lsp.diagnostics().jump(&path, from, forward, severity).map(|d| d.start)
            }
        };
        match target {
            Some(p) => tab.editor.set_selection(Selection::point(char_of_pos(&rope, p))),
            None => cx.info("No problems in this file"),
        }
    }

    /// LSP isteği imleç konumunda; yanıt `tick`'te işlenir.
    fn request(&mut self, what: Lsp, tab: &EditorTab, cx: &mut Cx) {
        let Some(path) = tab.path.clone() else { return };
        if self.lsp.server_of(&path).is_none() {
            cx.info("No language server for this file");
            return;
        }
        let rope = tab.editor.buffer.rope();
        let at = lsp_pos!(rope, tab.editor.cursor());
        let line = tab.editor.cursor_line_col().0 as u32;
        let (id, pending) = match what {
            Lsp::Definition => (self.lsp.definition(&path, at), Pending::Locations { what: "definition" }),
            Lsp::References => (self.lsp.references(&path, at, true), Pending::Locations { what: "references" }),
            Lsp::Usages => (self.lsp.references(&path, at, false), Pending::Locations { what: "usages" }),
            Lsp::Implementation => (self.lsp.implementation(&path, at), Pending::Locations { what: "implementation" }),
            Lsp::TypeDefinition => {
                (self.lsp.type_definition(&path, at), Pending::Locations { what: "type definition" })
            }
            Lsp::Hover => (self.lsp.hover(&path, at), Pending::Hover),
            Lsp::CodeActions => {
                let (a, b) = selected(tab).unwrap_or((tab.editor.cursor(), tab.editor.cursor()));
                let (s, e) = (pos_of_char(rope, a), pos_of_char(rope, b));
                let mut range = ContentChange::from_edit("", 0, 0, "").range.expect("from_edit sets a range");
                (range.start.line, range.start.character) = (s.line, s.character);
                (range.end.line, range.end.character) = (e.line, e.character);
                let diags: Vec<Diagnostic> = self.lsp.diagnostics().line(&path, line).into_iter().cloned().collect();
                let refs: Vec<&Diagnostic> = diags.iter().collect();
                (self.lsp.code_actions(&path, range, &refs), Pending::CodeActions { path: path.clone() })
            }
        };
        let Some(id) = id else {
            cx.info("Not supported by the language server");
            return;
        };
        self.pending.insert(id, pending);
        if what == Lsp::Hover {
            let diags = self.lsp.diagnostics().line(&path, line);
            let cron = cron_at(rope, tab.editor.cursor());
            let tooltip = assemble(&diags, &docs(cron.as_deref(), None));
            self.tip = Some(Tip { tooltip, anchor: Anchor::Cursor, path, line, hover: Some(id), cron });
        }
    }

    fn handle(&mut self, ev: LspEvent, cx: &mut Cx) {
        match ev {
            LspEvent::ServerNotFound { server, hint, .. } => cx.info(format!("{server} is not installed. {hint}")),
            LspEvent::ServerWarning { message, .. } => cx.info(message),
            LspEvent::ServerExited { key, code, stderr } if code != Some(0) => {
                let code = code.map_or_else(|| "signal".to_owned(), |c| c.to_string());
                let why = stderr.last().map(|l| format!(": {l}")).unwrap_or_default();
                cx.error(format!("{} exited ({code}){why}", key.server));
            }
            LspEvent::ShowMessage { level: 1, message, .. } => cx.error(message),
            LspEvent::ShowMessage { level: 2 | 3, message, .. } => cx.info(message),
            LspEvent::ApplyEdit { edit: Some(edit), .. } => {
                let files = file_edits!(&edit);
                self.apply_files(files, cx);
            }
            LspEvent::Response { id, key, result } if !self.completion_response(id, &key, &result, cx) => {
                self.response(id, result, cx);
            }
            _ => {}
        }
    }

    fn response(&mut self, id: RequestId, result: LspResult, cx: &mut Cx) {
        let Some(pending) = self.pending.remove(&id) else { return };
        match (pending, result) {
            (Pending::Hover, result) => {
                let raw = match result {
                    LspResult::Hover { raw, .. } => Some(raw),
                    _ => None,
                };
                let Some(tip) = self.tip.as_mut().filter(|t| t.hover == Some(id)) else { return };
                let diags = self.lsp.diagnostics().line(&tip.path, tip.line);
                tip.tooltip = assemble(&diags, &docs(tip.cron.as_deref(), raw));
                tip.hover = None;
                if tip.tooltip.is_empty() && tip.anchor == Anchor::Cursor {
                    cx.info("No information available");
                }
            }
            (_, LspResult::Error { message, .. }) => cx.error(message),
            (Pending::Locations { what }, LspResult::Locations(locs)) => self.locations(what, locs, cx),
            (Pending::Rename, LspResult::Rename(Some(edit))) => {
                let files = file_edits!(&edit);
                self.apply_files(files, cx);
            }
            (Pending::Rename, _) => cx.info("Nothing to rename"),
            (Pending::CodeActions { path }, LspResult::CodeActions(items)) => {
                self.actions = items.iter().filter_map(code_act).collect();
                self.actions_path = Some(path);
                if self.actions.is_empty() {
                    cx.info("No code actions available");
                    return;
                }
                let items = self.actions.iter().enumerate().map(|(i, a)| (a.title.clone(), Action::Apply(i)));
                self.menu = Some(Menu { pos: None, items: items.collect(), selected: 0 });
            }
            _ => {}
        }
    }

    fn locations(&mut self, what: &'static str, locs: Vec<Location>, cx: &mut Cx) {
        let targets: Vec<(PathBuf, Pos)> = locs
            .into_iter()
            .filter(|l| !l.path.as_os_str().is_empty())
            .map(|l| (l.path, Pos { line: l.range.start.line, character: l.range.start.character }))
            .collect();
        match targets.as_slice() {
            [] => cx.info(format!("No {what} found")),
            [(path, pos)] => self.go(path.clone(), *pos, cx),
            _ => {
                let mut files: HashMap<PathBuf, Vec<String>> = HashMap::new();
                let items = targets
                    .into_iter()
                    .map(|(path, pos)| {
                        let lines = files.entry(path.clone()).or_insert_with(|| self.file_lines(&path));
                        let text = lines.get(pos.line as usize).map_or("", |l| l.trim());
                        let label = format!("{}:{}  {text}", self.display(&path), pos.line + 1);
                        (label, Action::Open(path, pos))
                    })
                    .collect();
                self.menu = Some(Menu { pos: None, items, selected: 0 });
            }
        }
    }

    /// Dosyayı açtırır, imleci tab çizilince konuma koyar.
    fn go(&mut self, path: PathBuf, pos: Pos, cx: &mut Cx) {
        cx.open_file(path.clone(), Some(pos.line as usize));
        self.goto = Some((path, pos, false));
    }

    fn file_lines(&self, path: &Path) -> Vec<String> {
        let text = self.docs.get(path).cloned().or_else(|| std::fs::read_to_string(path).ok()).unwrap_or_default();
        text.lines().map(str::to_owned).collect()
    }

    /// Proje köküne göreli yol (değilse tam yol).
    fn display(&self, path: &Path) -> String {
        let rel = self.roots.iter().find_map(|r| path.strip_prefix(r).ok());
        rel.unwrap_or(path).display().to_string()
    }

    /// Workspace düzenlemesi: açık tab'lara (çizilince), açık olmayan dosyalara diskte.
    fn apply_files(&mut self, files: Vec<(PathBuf, Vec<Edit>)>, cx: &mut Cx) {
        let n = files.len();
        for (path, edits) in files {
            if self.docs.contains_key(&path) {
                self.deferred.entry(path).or_default().extend(edits);
            } else if let Err(e) = text::apply_to_file(&path, &edits) {
                cx.error(format!("Could not update {}: {e}", self.display(&path)));
            }
        }
        if n > 1 {
            cx.info(format!("Updated {n} files"));
        }
        cx.ctx.request_repaint();
    }

    /// Tab çizilirken: bekleyen düzenlemeleri uygular, tanıma gidişte imleci sütuna koyar.
    fn sync_tab(&mut self, tab: &mut EditorTab, cx: &mut Cx) {
        let Some(path) = tab.path.clone() else { return };
        if let Some(edits) = self.deferred.remove(&path) {
            // shortcut: paint'te Session yok; metin ekleme register'a dokunmadığı için geçici Session yeter.
            let mut session = Session::new(Box::new(MemoryClipboard::default()));
            self.apply_edits(tab, &edits, &mut session, cx);
        }
        match &mut self.goto {
            Some((p, _, armed @ false)) if *p == path => *armed = true,
            Some((p, pos, true)) if *p == path => {
                let at = char_of_pos(tab.editor.buffer.rope(), *pos);
                tab.editor.set_selection(Selection::point(at));
                self.goto = None;
            }
            _ => {}
        }
    }

    /// Düzenlemeleri tek geri alma adımında editöre uygular; her değişiklik LSP'ye de bildirilir.
    fn apply_edits(&mut self, tab: &mut EditorTab, edits: &[Edit], session: &mut Session, cx: &mut Cx) {
        let rope = tab.editor.buffer.rope().clone();
        let keep = pos_of_char(&rope, tab.editor.cursor());
        let mode = tab.editor.cursor_mode();
        self.run(tab, Command::BeginUndoGroup, session, cx);
        self.run(tab, Command::SetCursorMode(CursorMode::Bar), session, cx);
        for (a, b, t) in text::char_edits(&rope, edits) {
            tab.editor.set_selection(Selection::single(Range::new(a, b)));
            self.run(tab, Command::InsertText(t), session, cx);
        }
        self.run(tab, Command::SetCursorMode(mode), session, cx);
        self.run(tab, Command::EndUndoGroup, session, cx);
        let at = char_of_pos(tab.editor.buffer.rope(), keep);
        tab.editor.set_selection(Selection::point(at));
    }

    fn run(&mut self, tab: &mut EditorTab, cmd: Command, session: &mut Session, cx: &mut Cx) {
        if let Some(edit) = tab.apply(cmd, session).edit {
            self.changed(tab, &edit, cx);
        }
    }

    fn toggle_breakpoint(&mut self, path: &Path, line: usize, cx: &mut Cx) {
        let key = (path.to_path_buf(), line);
        if !self.breakpoints.remove(&key) {
            self.breakpoints.insert(key);
        }
        cx.send(TOPIC_TOGGLE_BREAKPOINT, format!("{}:{}", path.display(), line + 1));
    }

    /// Sağ tık menüsü (Lua `PopUp` menüsü, #13).
    fn context_menu(&mut self, pos: Option<egui::Pos2>, tab: &EditorTab) {
        let mut items = vec![
            ("Go to Definition".to_owned(), Action::Definition),
            ("Find References".to_owned(), Action::References),
            ("Rename Symbol".to_owned(), Action::Rename),
            ("Code Action".to_owned(), Action::CodeActions),
            ("Annotate with Git Blame".to_owned(), Action::GitBlame),
            ("Add File to AI".to_owned(), Action::AddFileToAi),
        ];
        if selected(tab).is_some() {
            items.push(("Send to AI".to_owned(), Action::SendToAi));
        }
        self.tip = None;
        self.menu = Some(Menu { pos, items, selected: 0 });
    }

    fn run_action(&mut self, action: Action, tab: &mut EditorTab, cx: &mut Cx) {
        let path = tab.path.clone();
        match action {
            Action::Definition => self.request(Lsp::Definition, tab, cx),
            Action::References => self.request(Lsp::References, tab, cx),
            Action::CodeActions => self.request(Lsp::CodeActions, tab, cx),
            Action::Rename => match path {
                Some(path) if self.lsp.server_of(&path).is_some() => {
                    let at = tab.editor.cursor();
                    let text = text::word_at(tab.editor.buffer.rope(), at);
                    self.rename = Some(RenamePrompt { path, at, text, focus: true });
                }
                _ => cx.info("No language server for this file"),
            },
            Action::GitBlame => cx.send("git.blame", path_text(path.as_deref())),
            Action::AddFileToAi => match path {
                Some(path) => cx.send("ai.send", format!("@{} ", self.display(&path))),
                None => cx.info("This tab has no file"),
            },
            Action::SendToAi => {
                if let Some((a, b)) = selected(tab) {
                    cx.send("ai.send", tab.editor.buffer.rope().slice(a..b).to_string());
                }
            }
            Action::Open(path, pos) => self.go(path, pos, cx),
            Action::Apply(i) => self.code_action(i, cx),
        }
    }

    /// Code action'ı uygular: önce düzenleme, sonra komut (LSP sırası).
    fn code_action(&mut self, i: usize, cx: &mut Cx) {
        let (Some(act), Some(path)) = (self.actions.get(i).cloned(), self.actions_path.clone()) else { return };
        if act.edit.is_none() && act.command.is_none() {
            cx.info("This code action is not supported yet");
        }
        if let Some(edit) = act.edit {
            let files = file_edits!(&edit);
            self.apply_files(files, cx);
        }
        if let Some((command, args)) = act.command
            && self.lsp.execute_command(&path, &command, args).is_none()
        {
            cx.info("Not supported by the language server");
        }
    }

    /// Rename isteminden: LSP `rename`.
    fn submit_rename(&mut self, prompt: RenamePrompt, tab: &EditorTab, cx: &mut Cx) {
        let name = prompt.text.trim();
        if name.is_empty() || tab.path.as_deref() != Some(prompt.path.as_path()) {
            return;
        }
        let at = lsp_pos!(tab.editor.buffer.rope(), prompt.at);
        match self.lsp.rename(&prompt.path, at, name) {
            Some(id) => {
                self.pending.insert(id, Pending::Rename);
            }
            None => cx.info("Rename is not supported by the language server"),
        }
    }

    /// CursorHold (imleç durunca satırın diagnostikleri) ve fare bekleyişi (diagnostik + hover, #52 #70).
    fn timers(&mut self, tab: &EditorTab, geo: &CodeGeometry, ui: &egui::Ui, cx: &mut Cx) {
        let Some(path) = tab.path.clone() else { return };
        let now = Instant::now();
        let busy = self.menu.is_some()
            || self.rename.is_some()
            || self.completion.as_ref().is_some_and(|c| !c.shown.is_empty());
        let cursor = tab.editor.cursor();
        if !matches!(&self.hold, Some((p, c, ..)) if *p == path && *c == cursor) {
            self.hold = Some((path.clone(), cursor, now, false));
        }
        if let Some((_, _, since, fired)) = &mut self.hold
            && !*fired
            && cx.focused
            && !busy
        {
            let left = HOLD.saturating_sub(now - *since);
            if left.is_zero() {
                *fired = true;
                let normal = cx.edit_mode == EditMode::Modeless || tab.editor.cursor_mode() == CursorMode::Block;
                let mouse_tip = matches!(&self.tip, Some(Tip { anchor: Anchor::Mouse(_), .. }));
                let line = tab.editor.cursor_line_col().0 as u32;
                let diags = self.lsp.diagnostics().line(&path, line);
                let cron = cron_at(tab.editor.buffer.rope(), cursor);
                if normal && !mouse_tip && (!diags.is_empty() || cron.is_some()) {
                    let tooltip = assemble(&diags, &docs(cron.as_deref(), None));
                    let path = path.clone();
                    self.tip = Some(Tip { tooltip, anchor: Anchor::Cursor, path, line, hover: None, cron });
                }
            } else {
                cx.ctx.request_repaint_after(left);
            }
        }

        let rope = tab.editor.buffer.rope();
        let pointer = ui.input(|i| (i.pointer.hover_pos(), i.pointer.any_down()));
        let over = match pointer {
            (Some(pos), false) if geo.text.contains(pos) && !busy => {
                geo.char_at(rope, pos - egui::vec2(geo.char_w / 2.0, 0.0)).map(|ch| (pos, ch))
            }
            _ => None,
        };
        let Some((pos, ch)) = over else {
            self.mouse = None;
            return;
        };
        let line = rope.char_to_line(ch);
        let text = line_text(rope, line);
        let col = ch - rope.line_to_char(line);
        let byte = text.char_indices().nth(col).map(|(b, _)| b);
        let key = (line, byte.and_then(|b| less_lang::tooltip::word_start(&text, b)));
        if !matches!(&self.mouse, Some(m) if m.path == path && m.key == key) {
            self.mouse = Some(MouseWait { path: path.clone(), key, pos, ch, since: now, fired: false });
        }
        let Some(m) = &mut self.mouse else { return };
        (m.pos, m.ch) = (pos, ch);
        if m.fired {
            return;
        }
        let left = MOUSE_WAIT.saturating_sub(now - m.since);
        if !left.is_zero() {
            cx.ctx.request_repaint_after(left);
            return;
        }
        m.fired = true;
        let (pos, ch, word) = (m.pos, m.ch, m.key.1.is_some());
        let line = line as u32;
        let hover = if word && self.lsp.server_of(&path).is_some() {
            let at = lsp_pos!(rope, ch);
            self.lsp.hover(&path, at)
        } else {
            None
        };
        if let Some(id) = hover {
            self.pending.insert(id, Pending::Hover);
        }
        let diags = self.lsp.diagnostics().line(&path, line);
        let cron = cron_at(rope, ch);
        if hover.is_some() || !diags.is_empty() || cron.is_some() {
            let tooltip = assemble(&diags, &docs(cron.as_deref(), None));
            self.tip = Some(Tip { tooltip, anchor: Anchor::Mouse(pos), path, line, hover, cron });
        }
    }
}

impl Drop for Lang {
    /// Dil sunucularını kapatır (pencere kapanınca süreç kalmasın).
    fn drop(&mut self) {
        self.lsp.shutdown_all(Duration::from_millis(300));
    }
}

/// İlk seçili aralık (Vim görsel modun kapsayıcı ucu dahil); seçim yoksa `None`.
fn selected(tab: &EditorTab) -> Option<(usize, usize)> {
    tab.editor.selection_spans().first().copied().filter(|(a, b)| a < b)
}

/// `ch` karakterinin içinde durduğu cron string'inin açıklaması.
fn cron_at(rope: &ropey::Rope, ch: usize) -> Option<String> {
    let line = rope.char_to_line(ch.min(rope.len_chars()));
    let text = line_text(rope, line);
    let byte = text.char_indices().nth(ch - rope.line_to_char(line)).map_or(text.len(), |(b, _)| b);
    less_lang::cron::describe_at(&text, byte)
}

/// Tooltip dokümanları: önce cron açıklaması, sonra LSP hover yanıtı.
fn docs(cron: Option<&str>, hover: Option<serde_json::Value>) -> Vec<serde_json::Value> {
    let cron =
        cron.map(|d| serde_json::json!({ "contents": { "kind": "markdown", "value": format!("**Cron:** {d}") } }));
    cron.into_iter().chain(hover).collect()
}

fn path_text(path: Option<&Path>) -> String {
    path.map(|p| p.display().to_string()).unwrap_or_default()
}

/// `CodeActionOrCommand` → [`CodeAct`] (devre dışı olanlar atlanır).
fn code_act(item: &lsp_types::CodeActionOrCommand) -> Option<CodeAct> {
    use lsp_types::CodeActionOrCommand as C;
    let args =
        |c: &lsp_types::Command| (c.command.clone(), serde_json::Value::Array(c.arguments.clone().unwrap_or_default()));
    Some(match item {
        C::Command(c) => CodeAct { title: c.title.clone(), edit: None, command: Some(args(c)) },
        C::CodeAction(a) if a.disabled.is_none() => {
            CodeAct { title: a.title.clone(), edit: a.edit.clone(), command: a.command.as_ref().map(args) }
        }
        C::CodeAction(_) => return None,
    })
}
