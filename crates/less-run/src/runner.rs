//! Run paneli durum makinesi: ağaç, öğe durumları, başlat / durdur / yeniden çalıştır / debug, log ve olaylar.
//!
//! **Olay modeli:** süreç çıktısı ve çıkışı arka plan iş parçacıklarından bir kanala düşer; UI her karede (ya da
//! `set_waker` ile verilen uyandırma çağrısından sonra) `poll()` çağırır. `poll` kanalı boşaltıp durumu günceller ve
//! UI'ın tepki vereceği `RunEvent` listesini döndürür (ham bayt, durum, port, soru, attach hedefi, bildirim). Eylem
//! metotları (`start`, `stop`, …) bildirimlerini de aynı kuyruğa koyar.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::command::{self, BuildContext, CommandSpec, JDWP_BASE_PORT, Mode};
use crate::config::{ConfigStore, RunConfig};
use crate::model::{Group, ItemKind, ItemSpec};
use crate::output::{self, LineSplitter, PromptTracker};
use crate::ports;
use crate::process::{self, ExitInfo, ProcMsg, PtyProcess, Sig};
use crate::providers::{self, Provider, main_class};

/// Ham log üst sınırı (bayt); aşılınca baştan kırpılır.
const MAX_RAW: usize = 4 * 1024 * 1024;
/// Temizlenmiş log satırı üst sınırı.
const MAX_LINES: usize = 20_000;
/// Rerun'da eski uygulamanın portunun boşalmasını bekleme süresi.
const RERUN_PORT_WAIT: Duration = Duration::from_secs(10);

/// Öğenin durumu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Status {
    /// Hiç çalışmadı.
    #[default]
    Idle,
    /// Başladı, hazır deseni (Spring `Started … in … seconds`) henüz görülmedi.
    Starting,
    /// Çalışıyor (hazır).
    Running,
    /// 0 koduyla bitti.
    Done,
    /// Hatayla bitti ya da başlatılamadı (`ItemState::exit`).
    Failed,
    /// Kullanıcı durdurdu.
    Stopped,
}

/// DAP attach hedefi: UI bunu `dap::DebugManager::attach`'e verir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachTarget {
    /// JDWP portu açılmış JVM (java-debug, jdtls üzerinden).
    Java { port: u16, file: PathBuf, main_class: String, label: String, cwd: PathBuf },
    /// Inspector'ı açılmış node süreci (js-debug `pwa-node` attach).
    Node { port: u16, pid: u32, label: String, cwd: PathBuf },
}

impl AttachTarget {
    /// Hedef port.
    pub fn port(&self) -> u16 {
        match self {
            AttachTarget::Java { port, .. } | AttachTarget::Node { port, .. } => *port,
        }
    }
}

/// Bildirim seviyesi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Warn,
    Error,
}

/// `poll()` ile UI'a verilen olay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunEvent {
    /// Yeni ham çıktı (ANSI dahil); terminal ızgarasına beslenir. `started` = yeni çalıştırma, ızgara sıfırlanmalı.
    Output { id: String, bytes: Vec<u8> },
    /// Yeni çalıştırma başladı (log sıfırlandı).
    Started { id: String, mode: Mode },
    /// Durum değişti.
    Status { id: String, status: Status },
    /// Uygulama portu yakalandı.
    Port { id: String, port: String },
    /// Süreç `(Y/n)` sorusunda bekliyor; UI Yes/No sorup `answer` çağırır (#77).
    Prompt { id: String, question: String },
    /// Debug hedefi hazır; UI DAP oturumu açar.
    Attach { id: String, target: AttachTarget },
    /// Süreç bitti. UI bu öğenin DAP oturumlarını kapatmalı (`DebugManager::detach`).
    Exited { id: String, exit: ExitInfo, status: Status },
    /// Kullanıcıya gösterilecek bildirim.
    Notice { id: Option<String>, level: Level, message: String },
}

/// Bir öğenin log'u: ham bayt (terminal ızgarası) ve ANSI'siz satırlar.
#[derive(Debug, Default, Clone)]
pub struct RunLog {
    pub raw: Vec<u8>,
    pub lines: Vec<String>,
}

/// Öğe ve çalışma durumu.
#[derive(Debug)]
pub struct ItemState {
    pub spec: ItemSpec,
    pub config: RunConfig,
    pub status: Status,
    /// Son başlatma modu (Rerun bunu kullanır).
    pub mode: Option<Mode>,
    /// Yakalanan uygulama portu.
    pub port: Option<String>,
    /// Debug modunda JDWP portu (Spring).
    pub debug_port: Option<u16>,
    /// Çıktıda yakalanan debug portları (node: süreç başına bir tane).
    pub debug_targets: Vec<u16>,
    /// Son çıkış.
    pub exit: Option<ExitInfo>,
    /// İlk başlatılma sırası (log tab'larının sırası, #101).
    pub seq: Option<u64>,
    /// Cevap bekleyen onay sorusu.
    pub prompt: Option<String>,
    pub log: RunLog,
    proc: Option<PtyProcess>,
    run_gen: u64,
    splitter: LineSplitter,
    tracker: PromptTracker,
    stopping: bool,
    pending_rerun: Option<Mode>,
}

impl ItemState {
    fn new(spec: ItemSpec, config: RunConfig) -> ItemState {
        ItemState {
            spec,
            config,
            status: Status::Idle,
            mode: None,
            port: None,
            debug_port: None,
            debug_targets: Vec::new(),
            exit: None,
            seq: None,
            prompt: None,
            log: RunLog::default(),
            proc: None,
            run_gen: 0,
            splitter: LineSplitter::default(),
            tracker: PromptTracker::default(),
            stopping: false,
            pending_rerun: None,
        }
    }

    /// Süreci var mı.
    pub fn is_running(&self) -> bool {
        self.proc.is_some()
    }

    /// Debug modunda çalışıyor mu.
    pub fn is_debugging(&self) -> bool {
        self.proc.is_some() && self.mode == Some(Mode::Debug)
    }

    /// Sürecin pid'i (süreç grubu).
    pub fn pid(&self) -> Option<u32> {
        self.proc.as_ref().map(PtyProcess::pid)
    }

    /// Öğe profil dışında ayar taşıyor mu (⚙).
    pub fn has_config(&self) -> bool {
        self.config.has_extra()
    }

    /// Profil (ayarlardan).
    pub fn profile(&self) -> Option<&str> {
        self.config.profile.as_deref()
    }
}

enum Msg {
    Proc { id: String, run_gen: u64, msg: ProcMsg },
    NodeTarget { id: String, run_gen: u64, port: u16, pid: Option<u32> },
    RerunReady { id: String, run_gen: u64, mode: Mode },
}

type Waker = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone)]
struct Notifier {
    tx: Sender<Msg>,
    waker: Arc<Mutex<Option<Waker>>>,
}

impl Notifier {
    fn send(&self, msg: Msg) {
        let _ = self.tx.send(msg);
        let waker = self.waker.lock().ok().and_then(|w| w.clone());
        if let Some(w) = waker {
            w();
        }
    }
}

/// Run paneli çekirdeği.
pub struct Runner {
    providers: Vec<Box<dyn Provider>>,
    roots: Vec<PathBuf>,
    tree: Vec<Group>,
    items: HashMap<String, ItemState>,
    configs: ConfigStore,
    notifier: Notifier,
    rx: Receiver<Msg>,
    events: Vec<RunEvent>,
    next_seq: u64,
    next_gen: u64,
    gradle_dir: PathBuf,
    size: (u16, u16),
    /// JVM debugger'ı beklesin mi (`suspend=y`); varsayılan false.
    pub suspend: bool,
}

impl Runner {
    /// Varsayılan sağlayıcılar ve `run_configs.json` ile.
    pub fn new() -> Runner {
        Runner::with(providers::default_providers(), ConfigStore::open_default(), crate::paths::data_dir())
    }

    /// Sağlayıcılar, ayar deposu ve `jdwp.gradle`'ın yazılacağı klasörle (test).
    pub fn with(providers: Vec<Box<dyn Provider>>, configs: ConfigStore, data_dir: PathBuf) -> Runner {
        let (tx, rx) = mpsc::channel();
        Runner {
            providers,
            roots: Vec::new(),
            tree: Vec::new(),
            items: HashMap::new(),
            configs,
            notifier: Notifier { tx, waker: Arc::new(Mutex::new(None)) },
            rx,
            events: Vec::new(),
            next_seq: 0,
            next_gen: 0,
            gradle_dir: data_dir.join("run"),
            size: (24, 120),
            suspend: false,
        }
    }

    /// Arka planda olay gelince çağrılır (egui `request_repaint`).
    pub fn set_waker(&mut self, waker: impl Fn() + Send + Sync + 'static) {
        if let Ok(mut w) = self.notifier.waker.lock() {
            *w = Some(Arc::new(waker));
        }
    }

    /// Son taranan kökler.
    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// Ağaç (sağlayıcı grupları; öğe durumu `item(id)` ile).
    pub fn tree(&self) -> &[Group] {
        &self.tree
    }

    /// Öğe durumu.
    pub fn item(&self, id: &str) -> Option<&ItemState> {
        self.items.get(id)
    }

    /// Tüm öğeler id sırasıyla.
    pub fn items(&self) -> Vec<&ItemState> {
        let mut list: Vec<_> = self.items.values().collect();
        list.sort_by(|a, b| a.spec.id.cmp(&b.spec.id));
        list
    }

    /// Kökler değiştiyse ya da ağaç boşsa tarar.
    pub fn ensure_scanned(&mut self, roots: &[PathBuf]) {
        if self.roots != roots || self.tree.is_empty() {
            self.scan(roots);
        }
    }

    /// Tüm sağlayıcıları çalıştırıp ağacı yeniden kurar. Önceden bilinen öğelerin durumu korunur; kaybolan ama hâlâ
    /// çalışan öğeler saklanır.
    pub fn scan(&mut self, roots: &[PathBuf]) {
        self.roots = roots.to_vec();
        self.tree = providers::discover(&self.providers, roots);
        let mut seen = HashSet::new();
        for group in &self.tree {
            for spec in group.items() {
                seen.insert(spec.id.clone());
                match self.items.get_mut(&spec.id) {
                    Some(state) => state.spec = spec.clone(),
                    None => {
                        let cfg = self.configs.get(&spec.id).cleaned(spec.supports_profiles());
                        self.items.insert(spec.id.clone(), ItemState::new(spec.clone(), cfg));
                    }
                }
            }
        }
        self.items.retain(|id, s| seen.contains(id) || s.is_running());
    }

    /// Öğenin kalıcı ayarları.
    pub fn config(&self, id: &str) -> RunConfig {
        self.items.get(id).map(|s| s.config.clone()).unwrap_or_else(|| self.configs.get(id))
    }

    /// Ayarları kaydeder (boş değerler silinir). Çalışan süreç kendiliğinden yeniden başlamaz; `restart` true ise
    /// çalışıyorsa yeniden başlatılır (form kaydı).
    pub fn set_config(&mut self, id: &str, cfg: &RunConfig, restart: bool) -> std::io::Result<RunConfig> {
        let profiles = self.items.get(id).is_some_and(|s| s.spec.supports_profiles());
        let clean = self.configs.set(id, cfg, profiles)?;
        let running = match self.items.get_mut(id) {
            Some(s) => {
                s.config = clean.clone();
                s.is_running()
            }
            None => false,
        };
        if restart && running {
            self.rerun(id, None);
        }
        Ok(clean)
    }

    /// Profili değiştirir (boş = yok); çalışıyorsa aynı modda yeniden başlatır.
    pub fn set_profile(&mut self, id: &str, profile: Option<&str>) -> std::io::Result<()> {
        let mut cfg = self.config(id);
        cfg.profile = profile.map(String::from);
        self.set_config(id, &cfg, true).map(|_| ())
    }

    /// Log tab'ları: logu olan öğeler, ilk başlatılma sırasıyla (#101).
    pub fn log_items(&self) -> Vec<&ItemState> {
        let mut list: Vec<_> = self.items.values().filter(|s| s.seq.is_some()).collect();
        list.sort_by_key(|s| s.seq);
        list
    }

    /// Log tab'ları arasında gezinme (uçlarda döner): `current`'tan `step` kadar ilerideki öğe.
    pub fn cycle_log(&self, current: Option<&str>, step: i32) -> Option<String> {
        let list = self.log_items();
        if list.is_empty() {
            return None;
        }
        let n = list.len() as i64;
        let idx = match current.and_then(|c| list.iter().position(|s| s.spec.id == c)) {
            Some(i) => (i as i64 + step as i64).rem_euclid(n),
            None if step > 0 => 0,
            None => n - 1,
        };
        Some(list[idx as usize].spec.id.clone())
    }

    /// Çalışan öğeler (etiket sırasıyla).
    pub fn running(&self) -> Vec<&ItemState> {
        let mut list: Vec<_> = self.items.values().filter(|s| s.is_running()).collect();
        list.sort_by(|a, b| a.spec.label.cmp(&b.spec.label));
        list
    }

    fn notice(&mut self, id: Option<&str>, level: Level, message: String) {
        self.events.push(RunEvent::Notice { id: id.map(String::from), level, message });
    }

    fn taken_ports(&self, except: &str) -> HashSet<u16> {
        self.items.values().filter(|s| s.spec.id != except && s.is_running()).filter_map(|s| s.debug_port).collect()
    }

    /// Öğenin o anki komutu (gösterim / test için; dosyaya `jdwp.gradle` yazabilir).
    pub fn command(&self, id: &str, mode: Mode, debug_port: Option<u16>) -> Option<CommandSpec> {
        let s = self.items.get(id)?;
        let init =
            command::ensure_gradle_init(&self.gradle_dir).unwrap_or_else(|_| self.gradle_dir.join("jdwp.gradle"));
        let ctx = BuildContext {
            mode,
            debug_port,
            gradle_init: &init,
            suspend: self.suspend,
            inherited_node_options: std::env::var("NODE_OPTIONS").ok(),
        };
        Some(command::build(&s.spec, &s.config, &ctx))
    }

    /// Öğeyi başlatır (çalışıyorsa bir şey yapmaz). Debug modunda Spring için boş JDWP portu (5005'ten) seçilir,
    /// Node'da inspector ortamı eklenir; debug hedefi çıktıda görününce `RunEvent::Attach` gelir.
    pub fn start(&mut self, id: &str, mode: Mode) -> bool {
        let Some(state) = self.items.get(id) else { return false };
        if state.is_running() {
            return false;
        }
        let label = state.spec.label.clone();
        let is_spring = matches!(state.spec.kind, ItemKind::Spring { .. });
        let debug_port = if mode == Mode::Debug && is_spring {
            match ports::free_port(JDWP_BASE_PORT, &self.taken_ports(id)) {
                Some(p) => Some(p),
                None => {
                    self.notice(Some(id), Level::Error, format!("No free debug port found for {label}"));
                    return false;
                }
            }
        } else {
            None
        };
        let Some(spec) = self.command(id, mode, debug_port) else { return false };
        self.next_gen += 1;
        let run_gen = self.next_gen;
        let seq = self.next_seq + 1;
        let (rows, cols) = self.size;
        let notifier = self.notifier.clone();
        let msg_id = id.to_string();
        let spawned =
            process::spawn(&spec, rows, cols, move |msg| notifier.send(Msg::Proc { id: msg_id.clone(), run_gen, msg }));
        let Some(state) = self.items.get_mut(id) else { return false };
        if state.seq.is_none() {
            state.seq = Some(seq);
            self.next_seq = seq;
        }
        state.run_gen = run_gen;
        state.log = RunLog::default();
        state.splitter = LineSplitter::default();
        state.tracker.reset();
        state.port = None;
        state.stopping = false;
        state.pending_rerun = None;
        state.mode = Some(mode);
        state.debug_port = debug_port;
        state.debug_targets.clear();
        state.prompt = None;
        state.exit = None;
        self.events.push(RunEvent::Started { id: id.to_string(), mode });
        match spawned {
            Ok(p) => {
                state.proc = Some(p);
                state.status = if is_spring { Status::Starting } else { Status::Running };
                self.events.push(RunEvent::Status { id: id.to_string(), status: state.status });
                true
            }
            Err(e) => {
                state.status = Status::Failed;
                self.events.push(RunEvent::Status { id: id.to_string(), status: Status::Failed });
                let cmd = spec.argv.join(" ");
                self.notice(Some(id), Level::Error, format!("Could not start {label}: {cmd} ({e})"));
                false
            }
        }
    }

    fn kill(&mut self, id: &str) {
        if let Some(s) = self.items.get_mut(id)
            && let Some(p) = &s.proc
        {
            s.stopping = true;
            p.stop();
        }
    }

    /// Süreci durdurur (SIGHUP → SIGTERM → SIGKILL); bekleyen yeniden başlatmayı iptal eder.
    pub fn stop(&mut self, id: &str) {
        if let Some(s) = self.items.get_mut(id) {
            s.pending_rerun = None;
        }
        self.kill(id);
    }

    /// ⏎: çalışıyorsa durdurur, duruyorsa normal modda başlatır.
    pub fn toggle(&mut self, id: &str) {
        if self.items.get(id).is_some_and(ItemState::is_running) {
            self.stop(id);
        } else {
            self.start(id, Mode::Run);
        }
    }

    /// Yeniden çalıştırır: çalışıyorsa durdurur, kapanınca ve eski uygulama portu boşalınca (≤10 sn) aynı modla (ya da
    /// `mode` ile) başlatır; çalışmıyorsa son modla başlatır.
    pub fn rerun(&mut self, id: &str, mode: Option<Mode>) {
        let Some(s) = self.items.get_mut(id) else { return };
        let mode = mode.or(s.mode).unwrap_or_default();
        if !s.is_running() {
            self.start(id, mode);
            return;
        }
        s.pending_rerun = Some(mode);
        self.kill(id);
    }

    /// Debug eylemi: duruyorsa debug modunda başlatır; normal modda çalışıyorsa debug modunda yeniden başlatır; debug
    /// modunda çalışıp debugger bağlı değilse (`attached` false) hâlâ dinleyen hedeflere yeniden attach olayı üretir.
    /// `attached` = UI'daki DAP oturumu var ya da bağlanıyor.
    pub fn debug(&mut self, id: &str, attached: bool) {
        let Some(s) = self.items.get(id) else { return };
        let label = s.spec.label.clone();
        if !s.is_running() {
            self.start(id, Mode::Debug);
        } else if s.mode != Some(Mode::Debug) {
            self.rerun(id, Some(Mode::Debug));
        } else if attached {
            self.notice(Some(id), Level::Info, format!("Debugger is already attached to {label}"));
        } else {
            let listening = ports::listening_ports();
            let targets: Vec<u16> = s.debug_targets.iter().copied().filter(|p| listening.contains(p)).collect();
            if targets.is_empty() {
                self.notice(Some(id), Level::Warn, format!("No debug target is listening for {label}"));
            }
            if let Some(s) = self.items.get_mut(id) {
                s.debug_targets = targets.clone();
            }
            for port in targets {
                self.emit_target(id, port);
            }
        }
    }

    /// Bekleyen onay sorusuna cevap verir (`y\r` / `n\r`). Soru yoksa ya da süreç kapandıysa false.
    pub fn answer(&mut self, id: &str, yes: bool) -> bool {
        let Some(s) = self.items.get_mut(id) else { return false };
        if !s.tracker.pending() {
            return false;
        }
        s.tracker.answered();
        s.prompt = None;
        match &mut s.proc {
            Some(p) => p.write(if yes { b"y\r" } else { b"n\r" }).is_ok(),
            None => false,
        }
    }

    /// Terminale girdi yazar (log görünümünde kullanıcı yazınca).
    pub fn write(&mut self, id: &str, bytes: &[u8]) -> bool {
        self.items.get_mut(id).and_then(|s| s.proc.as_mut()).is_some_and(|p| p.write(bytes).is_ok())
    }

    /// Log görünümünün boyutu; sonraki başlatmalar ve çalışan pty'ler bu boyutu alır.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.size = (rows.max(2), cols.max(10));
        for s in self.items.values() {
            if let Some(p) = &s.proc {
                let _ = p.resize(self.size.0, self.size.1);
            }
        }
    }

    /// Süreç grubuna SIGINT (Ctrl+C) gönderir, durduruluyor işaretler (#80).
    pub fn interrupt(&mut self, id: &str) {
        if let Some(s) = self.items.get_mut(id)
            && let Some(p) = &s.proc
        {
            s.stopping = true;
            s.pending_rerun = None;
            p.signal(Sig::Int);
        }
    }

    /// Süreç grubunu SIGKILL ile öldürür (#80 `k`).
    pub fn force_kill(&mut self, id: &str) {
        if let Some(s) = self.items.get_mut(id)
            && let Some(p) = &s.proc
        {
            s.stopping = true;
            s.pending_rerun = None;
            p.force_kill();
        }
    }

    fn emit_target(&mut self, id: &str, port: u16) {
        let Some(s) = self.items.get(id) else { return };
        let label = s.spec.label.clone();
        let cwd = s.spec.cwd.clone();
        match &s.spec.kind {
            ItemKind::Spring { file, .. } => {
                if s.spec.can_attach() {
                    let target =
                        AttachTarget::Java { port, file: file.clone(), main_class: main_class(file), label, cwd };
                    self.events.push(RunEvent::Attach { id: id.to_string(), target });
                } else {
                    let msg = format!("{label}: debug port {port} is open; attach a debugger manually");
                    self.notice(Some(id), Level::Info, msg);
                }
            }
            ItemKind::Node { .. } => {
                let (run_gen, notifier, id) = (s.run_gen, self.notifier.clone(), id.to_string());
                let _ = thread::Builder::new().name("run-lsof".into()).spawn(move || {
                    let pid = ports::listening_pid(port);
                    notifier.send(Msg::NodeTarget { id, run_gen, port, pid });
                });
            }
        }
    }

    fn on_line(&mut self, id: &str, line: &str) {
        let Some(s) = self.items.get_mut(id) else { return };
        let text = output::strip_ansi(line);
        s.tracker.on_line(&text);
        s.log.lines.push(text.clone());
        if s.log.lines.len() > MAX_LINES {
            let extra = s.log.lines.len() - MAX_LINES;
            s.log.lines.drain(..extra);
        }
        if s.mode == Some(Mode::Debug) {
            let target = match s.spec.kind {
                ItemKind::Spring { .. } => output::jdwp_port(&text),
                ItemKind::Node { .. } => output::inspector_port(&text),
            };
            if let Some(port) = target {
                if !s.debug_targets.contains(&port) {
                    s.debug_targets.push(port);
                    self.emit_target(id, port);
                }
                return;
            }
        }
        if let Some(port) = output::app_port(&text)
            && s.port.as_deref() != Some(port.as_str())
        {
            s.port = Some(port.clone());
            self.events.push(RunEvent::Port { id: id.to_string(), port });
        }
        if s.status == Status::Starting && output::spring_ready(&text) {
            s.status = Status::Running;
            self.events.push(RunEvent::Status { id: id.to_string(), status: Status::Running });
        }
    }

    fn on_output(&mut self, id: &str, bytes: Vec<u8>) {
        let Some(s) = self.items.get_mut(id) else { return };
        s.log.raw.extend_from_slice(&bytes);
        if s.log.raw.len() > MAX_RAW {
            let extra = s.log.raw.len() - MAX_RAW;
            s.log.raw.drain(..extra);
        }
        let lines = s.splitter.push(&bytes);
        self.events.push(RunEvent::Output { id: id.to_string(), bytes });
        for line in lines {
            self.on_line(id, &line);
        }
        let Some(s) = self.items.get_mut(id) else { return };
        let partial = s.splitter.partial();
        if let Some(question) = s.tracker.check(&partial) {
            s.prompt = Some(question.clone());
            let label = s.spec.label.clone();
            self.events.push(RunEvent::Prompt { id: id.to_string(), question: format!("{label}: {question}") });
        }
    }

    fn on_exit(&mut self, id: &str, exit: ExitInfo) {
        let Some(s) = self.items.get_mut(id) else { return };
        if let Some(rest) = s.splitter.take_partial() {
            s.log.lines.push(output::strip_ansi(&rest));
        }
        s.proc = None;
        s.prompt = None;
        s.status = if s.stopping {
            Status::Stopped
        } else if exit.success() {
            Status::Done
        } else {
            Status::Failed
        };
        s.exit = Some(exit.clone());
        let status = s.status;
        self.events.push(RunEvent::Exited { id: id.to_string(), exit, status });
        if let Some(mode) = s.pending_rerun.take() {
            let port = s.port.as_deref().and_then(|p| p.parse::<u16>().ok());
            let (run_gen, notifier, id) = (s.run_gen, self.notifier.clone(), id.to_string());
            let _ = thread::Builder::new().name("run-rerun".into()).spawn(move || {
                if let Some(port) = port {
                    let deadline = Instant::now() + RERUN_PORT_WAIT;
                    while Instant::now() < deadline && ports::is_listening(port) {
                        thread::sleep(Duration::from_millis(250));
                    }
                }
                notifier.send(Msg::RerunReady { id, run_gen, mode });
            });
        }
    }

    /// Arka plan mesajlarını işler ve birikmiş olayları döndürür. UI her karede çağırır.
    pub fn poll(&mut self) -> Vec<RunEvent> {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Proc { id, run_gen, msg } => {
                    if self.items.get(&id).is_none_or(|s| s.run_gen != run_gen) {
                        continue;
                    }
                    match msg {
                        ProcMsg::Output(bytes) => self.on_output(&id, bytes),
                        ProcMsg::Exit(exit) => self.on_exit(&id, exit),
                    }
                }
                Msg::NodeTarget { id, run_gen, port, pid } => {
                    let Some(s) = self.items.get(&id).filter(|s| s.run_gen == run_gen) else { continue };
                    let Some(pid) = pid.filter(|p| Some(*p) != s.pid()) else { continue };
                    let target = AttachTarget::Node { port, pid, label: s.spec.label.clone(), cwd: s.spec.cwd.clone() };
                    self.events.push(RunEvent::Attach { id, target });
                }
                Msg::RerunReady { id, run_gen, mode } => {
                    if self.items.get(&id).is_some_and(|s| s.run_gen == run_gen && !s.is_running()) {
                        self.start(&id, mode);
                    }
                }
            }
        }
        std::mem::take(&mut self.events)
    }

    /// `poll`'u en çok `timeout` boyunca, `until` doğru dönene kadar tekrarlar; toplanan olayları döndürür (test ve
    /// headless kullanım).
    pub fn poll_until(&mut self, timeout: Duration, mut until: impl FnMut(&Runner) -> bool) -> Vec<RunEvent> {
        let deadline = Instant::now() + timeout;
        let mut all = Vec::new();
        loop {
            all.extend(self.poll());
            if until(self) || Instant::now() >= deadline {
                return all;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    /// `jdwp.gradle`'ın yazıldığı klasör.
    pub fn gradle_dir(&self) -> &Path {
        &self.gradle_dir
    }
}

impl Default for Runner {
    fn default() -> Self {
        Runner::new()
    }
}

impl Drop for Runner {
    fn drop(&mut self) {
        for s in self.items.values() {
            if let Some(p) = &s.proc {
                p.signal(Sig::Hup);
            }
        }
    }
}
