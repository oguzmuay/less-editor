//! Debug oturumları: Run öğesinin attach hedeflerine DAP oturumu açar, olayları `poll()` ile UI'a verir.
//!
//! Her oturum kendi iş parçacığında kurulur (initialize → attach → `initialized` → setBreakpoints →
//! configurationDone) ve sonra olaylarını dinler. js-debug hedefleri `startDebugging` ters isteğiyle çocuk oturum
//! açar; çocuk aynı sunucuya yeni bağlantıyla kurulur (`parent` dolu). Duran oturumun frame'leri `Stopped` olayıyla
//! gelir; değişkenler `client(session)` üzerinden (`variables::VariablesModel`).

use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::adapter::{JavaDebugStarter, JsDebugServer, java_attach_config, node_attach_config};
use super::client::{DapClient, Incoming, StepCommand};
use super::types::{Capabilities, SourceBreakpoint, StackFrame};
use crate::runner::AttachTarget;

/// Oturum kimliği.
pub type SessionId = u64;

/// Attach sürecindeki öğenin durumu (rozet: "waiting for jdtls…" / "attaching…").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DapState {
    Waiting,
    Attaching,
}

/// Oturum dili (değişken ifadeleri için).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Js,
    Java,
    Python,
}

impl Lang {
    /// Adaptör türünden (`pwa-node`, `java`, `debugpy`…), olmazsa kaynak dosya uzantısından.
    pub fn detect(adapter_type: &str, source_path: Option<&str>) -> Option<Lang> {
        let t = adapter_type;
        if t.contains("node") || t.contains("chrome") || t.contains("msedge") {
            return Some(Lang::Js);
        }
        match t {
            "java" => return Some(Lang::Java),
            "python" | "debugpy" => return Some(Lang::Python),
            _ => {}
        }
        let ext = source_path.and_then(|p| Path::new(p).extension()).and_then(|e| e.to_str())?;
        match ext {
            "js" | "mjs" | "cjs" | "ts" | "tsx" | "jsx" => Some(Lang::Js),
            "java" => Some(Lang::Java),
            "py" => Some(Lang::Python),
            _ => None,
        }
    }
}

/// `poll()` olayları.
#[derive(Debug, Clone, PartialEq)]
pub enum DebugEvent {
    State {
        runner_id: String,
        state: Option<DapState>,
    },
    SessionStarted {
        session: SessionId,
        runner_id: String,
        name: String,
        parent: Option<SessionId>,
    },
    /// Duraklama; `frames[0]` güncel frame.
    Stopped {
        session: SessionId,
        thread_id: i64,
        reason: String,
        frames: Vec<StackFrame>,
    },
    Continued {
        session: SessionId,
    },
    Output {
        session: SessionId,
        category: String,
        text: String,
    },
    SessionEnded {
        session: SessionId,
        runner_id: String,
    },
    /// Attach başarısız (kullanıcıya gösterilecek İngilizce metin).
    Failed {
        runner_id: String,
        message: String,
    },
}

/// Açık oturum bilgisi.
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub id: SessionId,
    pub runner_id: String,
    pub name: String,
    pub parent: Option<SessionId>,
    pub adapter_type: String,
    pub client: DapClient,
    pub capabilities: Capabilities,
    /// Duraklamadaysa thread ve frame'ler.
    pub stopped: Option<(i64, Vec<StackFrame>)>,
}

impl SessionInfo {
    /// Oturum dili.
    pub fn lang(&self) -> Option<Lang> {
        let path = self.stopped.as_ref().and_then(|(_, f)| f.first()?.source.as_ref()?.path.clone());
        Lang::detect(&self.adapter_type, path.as_deref())
    }

    /// Güncel frame id'si.
    pub fn frame_id(&self) -> Option<i64> {
        self.stopped.as_ref().and_then(|(_, f)| f.first()).map(|f| f.id)
    }
}

enum Msg {
    State(String, Option<DapState>),
    Ready(SessionInfo),
    Stopped(SessionId, i64, String, Vec<StackFrame>),
    Continued(SessionId),
    Output(SessionId, String, String),
    Ended(SessionId),
    Failed(String, String),
}

type Breakpoints = Arc<Mutex<BTreeMap<PathBuf, Vec<SourceBreakpoint>>>>;
type Waker = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone)]
struct Ctx {
    tx: Sender<Msg>,
    waker: Arc<Mutex<Option<Waker>>>,
    next_id: Arc<AtomicU64>,
    breakpoints: Breakpoints,
}

impl Ctx {
    fn send(&self, m: Msg) {
        let _ = self.tx.send(m);
        if let Some(w) = self.waker.lock().ok().and_then(|w| w.clone()) {
            w();
        }
    }
}

/// Debug oturum yöneticisi.
pub struct DebugManager {
    ctx: Ctx,
    rx: Receiver<Msg>,
    sessions: BTreeMap<SessionId, SessionInfo>,
    states: HashMap<String, DapState>,
    java: Option<Arc<dyn JavaDebugStarter>>,
    js_server: Arc<Mutex<Option<JsDebugServer>>>,
    /// Attach el sıkışmasının süresi (varsayılan 20 sn).
    pub handshake_timeout: Duration,
}

impl Default for DebugManager {
    fn default() -> Self {
        DebugManager::new()
    }
}

impl DebugManager {
    pub fn new() -> DebugManager {
        let (tx, rx) = mpsc::channel();
        DebugManager {
            ctx: Ctx {
                tx,
                waker: Arc::new(Mutex::new(None)),
                next_id: Arc::new(AtomicU64::new(1)),
                breakpoints: Arc::new(Mutex::new(BTreeMap::new())),
            },
            rx,
            sessions: BTreeMap::new(),
            states: HashMap::new(),
            java: None,
            js_server: Arc::new(Mutex::new(None)),
            handshake_timeout: Duration::from_secs(20),
        }
    }

    /// Arka planda olay gelince çağrılır.
    pub fn set_waker(&mut self, waker: impl Fn() + Send + Sync + 'static) {
        if let Ok(mut w) = self.ctx.waker.lock() {
            *w = Some(Arc::new(waker));
        }
    }

    /// Java attach kancası (less-lang / UI).
    pub fn set_java_starter(&mut self, starter: Arc<dyn JavaDebugStarter>) {
        self.java = Some(starter);
    }

    /// Run öğesinin hedefine oturum açar (arka planda).
    pub fn attach(&mut self, runner_id: &str, target: AttachTarget) {
        let ctx = self.ctx.clone();
        let id = runner_id.to_string();
        let timeout = self.handshake_timeout;
        match target {
            AttachTarget::Java { port, file, main_class, label, cwd } => {
                let Some(java) = self.java.clone() else {
                    let msg = format!("Java debugging needs jdtls. JDWP is listening on 127.0.0.1:{port}");
                    self.ctx.send(Msg::Failed(id, msg));
                    return;
                };
                self.states.insert(id.clone(), DapState::Waiting);
                let _ = thread::Builder::new().name("dap-java".into()).spawn(move || {
                    ctx.send(Msg::State(id.clone(), Some(DapState::Waiting)));
                    let dap_port = match java.start_debug_session(&file) {
                        Ok(p) => p,
                        Err(e) => {
                            let msg = format!(
                                "Debugger not attached: {e}. JDWP is listening on 127.0.0.1:{port}; press d to retry."
                            );
                            return ctx.send(Msg::Failed(id, msg));
                        }
                    };
                    ctx.send(Msg::State(id.clone(), Some(DapState::Attaching)));
                    let project = java.resolve_project_name(&file, &main_class).unwrap_or_default();
                    let root = java.root_dir(&file).unwrap_or(cwd);
                    let config = java_attach_config(&label, port, &root, &main_class, &project);
                    let addr: SocketAddr = ([127, 0, 0, 1], dap_port).into();
                    run_session(&ctx, &id, addr, "java", config, None, timeout);
                });
            }
            AttachTarget::Node { port, pid, label, cwd } => {
                self.states.insert(id.clone(), DapState::Attaching);
                let server = self.js_server.clone();
                let _ = thread::Builder::new().name("dap-node".into()).spawn(move || {
                    ctx.send(Msg::State(id.clone(), Some(DapState::Attaching)));
                    let addr = {
                        let Ok(mut guard) = server.lock() else { return };
                        if !guard.as_mut().is_some_and(JsDebugServer::alive) {
                            match JsDebugServer::start() {
                                Ok(s) => *guard = Some(s),
                                Err(e) => return ctx.send(Msg::Failed(id, e)),
                            }
                        }
                        guard.as_ref().map(|s| s.addr)
                    };
                    let Some(addr) = addr else { return };
                    let config = node_attach_config(&label, pid, port, &cwd);
                    run_session(&ctx, &id, addr, "pwa-node", config, None, timeout);
                });
            }
        }
    }

    /// Hazır bir DAP sunucusuna (adres) verilen yapılandırmayla attach (test ya da başka adaptörler).
    pub fn attach_addr(&mut self, runner_id: &str, addr: SocketAddr, adapter_type: &str, config: Value) {
        let ctx = self.ctx.clone();
        let (id, kind, timeout) = (runner_id.to_string(), adapter_type.to_string(), self.handshake_timeout);
        self.states.insert(id.clone(), DapState::Attaching);
        let _ = thread::Builder::new().name("dap-attach".into()).spawn(move || {
            run_session(&ctx, &id, addr, &kind, config, None, timeout);
        });
    }

    /// Arka plan mesajlarını işler, olayları döndürür.
    pub fn poll(&mut self) -> Vec<DebugEvent> {
        let mut out = Vec::new();
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::State(runner_id, state) => {
                    match state {
                        Some(s) => self.states.insert(runner_id.clone(), s),
                        None => self.states.remove(&runner_id),
                    };
                    out.push(DebugEvent::State { runner_id, state });
                }
                Msg::Ready(info) => {
                    if self.states.remove(&info.runner_id).is_some() {
                        out.push(DebugEvent::State { runner_id: info.runner_id.clone(), state: None });
                    }
                    out.push(DebugEvent::SessionStarted {
                        session: info.id,
                        runner_id: info.runner_id.clone(),
                        name: info.name.clone(),
                        parent: info.parent,
                    });
                    self.sessions.insert(info.id, info);
                }
                Msg::Stopped(session, thread_id, reason, frames) => {
                    if let Some(s) = self.sessions.get_mut(&session) {
                        s.stopped = Some((thread_id, frames.clone()));
                    }
                    out.push(DebugEvent::Stopped { session, thread_id, reason, frames });
                }
                Msg::Continued(session) => {
                    if let Some(s) = self.sessions.get_mut(&session) {
                        s.stopped = None;
                    }
                    out.push(DebugEvent::Continued { session });
                }
                Msg::Output(session, category, text) => out.push(DebugEvent::Output { session, category, text }),
                Msg::Ended(session) => {
                    if let Some(s) = self.sessions.remove(&session) {
                        out.push(DebugEvent::SessionEnded { session, runner_id: s.runner_id });
                    }
                }
                Msg::Failed(runner_id, message) => {
                    if self.states.remove(&runner_id).is_some() {
                        out.push(DebugEvent::State { runner_id: runner_id.clone(), state: None });
                    }
                    out.push(DebugEvent::Failed { runner_id, message });
                }
            }
        }
        out
    }

    /// Öğenin attach durumu.
    pub fn state(&self, runner_id: &str) -> Option<DapState> {
        self.states.get(runner_id).copied()
    }

    /// Öğenin açık oturumları.
    pub fn sessions_for(&self, runner_id: &str) -> Vec<SessionId> {
        self.sessions.values().filter(|s| s.runner_id == runner_id).map(|s| s.id).collect()
    }

    /// Öğenin kök (ebeveynsiz) oturum sayısı (rozet `×N`). js-debug'da her hedef bir kök + çocuktur.
    pub fn root_session_count(&self, runner_id: &str) -> usize {
        self.sessions.values().filter(|s| s.runner_id == runner_id && s.parent.is_none()).count()
    }

    /// Run'daki `debug(id, attached)` için: oturum var ya da bağlanıyor.
    pub fn attached_or_attaching(&self, runner_id: &str) -> bool {
        self.states.contains_key(runner_id) || !self.sessions_for(runner_id).is_empty()
    }

    /// Oturum.
    pub fn session(&self, id: SessionId) -> Option<&SessionInfo> {
        self.sessions.get(&id)
    }

    /// Tüm oturumlar.
    pub fn sessions(&self) -> impl Iterator<Item = &SessionInfo> {
        self.sessions.values()
    }

    /// Duraklamadaki son oturum (değişkenler bundan gösterilir; js-debug'da çocuk oturum).
    pub fn stopped_session(&self) -> Option<&SessionInfo> {
        self.sessions.values().rev().find(|s| s.stopped.is_some())
    }

    /// Yürütme komutu (duran thread'e; yoksa thread 0).
    pub fn step(&self, session: SessionId, command: StepCommand) {
        if let Some(s) = self.sessions.get(&session) {
            let thread = s.stopped.as_ref().map(|(t, _)| *t).unwrap_or(0);
            drop(s.client.step(command, thread));
        }
    }

    /// Öğenin oturumlarını debuggee'yi öldürmeden kapatır.
    pub fn detach(&mut self, runner_id: &str) {
        self.states.remove(runner_id);
        for s in self.sessions.values().filter(|s| s.runner_id == runner_id) {
            drop(s.client.disconnect());
        }
    }

    /// Dosyanın breakpoint'lerini kaydeder ve açık tüm oturumlara gönderir (yanıt beklenmez).
    pub fn set_breakpoints(&self, file: &Path, bps: Vec<SourceBreakpoint>) {
        if let Ok(mut all) = self.ctx.breakpoints.lock() {
            all.insert(file.to_path_buf(), bps.clone());
        }
        let path = file.display().to_string();
        for s in self.sessions.values() {
            let args = json!({ "source": { "path": path }, "breakpoints": bps });
            drop(s.client.request("setBreakpoints", args));
        }
    }

    /// Tüm breakpoint'leri değiştirir (açılışta `BreakpointStore`'dan). Yalnız sonraki oturumlara uygulanır.
    pub fn replace_breakpoints(&self, all: BTreeMap<PathBuf, Vec<SourceBreakpoint>>) {
        if let Ok(mut b) = self.ctx.breakpoints.lock() {
            *b = all;
        }
    }
}

/// Oturumu kurar ve olaylarını dinler (bu iş parçacığında, bağlantı kapanana kadar).
fn run_session(
    ctx: &Ctx,
    runner_id: &str,
    addr: SocketAddr,
    adapter_type: &str,
    config: Value,
    parent: Option<SessionId>,
    timeout: Duration,
) {
    let fail = |msg: String| ctx.send(Msg::Failed(runner_id.to_string(), msg));
    let (client, rx) = match DapClient::connect(addr, Duration::from_secs(5)) {
        Ok(c) => c,
        Err(e) => return fail(format!("Could not connect to the debug adapter: {e}")),
    };
    let caps = match client.initialize(adapter_type) {
        Ok(c) => c,
        Err(e) => return fail(format!("Debug adapter initialize failed: {e}")),
    };
    let request = config.get("request").and_then(Value::as_str).unwrap_or("attach").to_string();
    let attach = client.request(&request, config.clone());
    let mut early = Vec::new();
    let deadline = Instant::now() + timeout;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok(Incoming::Event { event, .. }) if event == "initialized" => break,
            Ok(Incoming::Closed) | Err(_) => return fail("Debug adapter did not initialize".into()),
            Ok(other) => early.push(other),
        }
    }
    let bps: Vec<(PathBuf, Vec<SourceBreakpoint>)> =
        ctx.breakpoints.lock().map(|b| b.iter().map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default();
    for (file, list) in bps {
        let _ = client.set_breakpoints(&file.display().to_string(), &list);
    }
    let _ = client.request_wait("setExceptionBreakpoints", json!({ "filters": [] }));
    if caps.supports_configuration_done_request {
        let _ = client.configuration_done();
    }
    if let Err(e) = attach.wait(timeout) {
        return fail(format!("Debugger not attached: {e}"));
    }
    let id = ctx.next_id.fetch_add(1, Ordering::SeqCst);
    let name = config.get("name").and_then(Value::as_str).unwrap_or(adapter_type).to_string();
    ctx.send(Msg::Ready(SessionInfo {
        id,
        runner_id: runner_id.to_string(),
        name,
        parent,
        adapter_type: config.get("type").and_then(Value::as_str).unwrap_or(adapter_type).to_string(),
        client: client.clone(),
        capabilities: caps,
        stopped: None,
    }));
    let handle = |msg: Incoming| -> bool {
        match msg {
            Incoming::Closed => return false,
            Incoming::Event { event, body } => match event.as_str() {
                "stopped" => {
                    let thread = body.get("threadId").and_then(Value::as_i64).unwrap_or(0);
                    let reason = body.get("reason").and_then(Value::as_str).unwrap_or("pause").to_string();
                    let frames = client.stack_trace(thread, 50).unwrap_or_default();
                    ctx.send(Msg::Stopped(id, thread, reason, frames));
                }
                "continued" => ctx.send(Msg::Continued(id)),
                "output" => {
                    let cat = body.get("category").and_then(Value::as_str).unwrap_or("console").to_string();
                    let text = body.get("output").and_then(Value::as_str).unwrap_or_default().to_string();
                    ctx.send(Msg::Output(id, cat, text));
                }
                "terminated" | "exited" => {
                    drop(client.disconnect());
                    return false;
                }
                _ => {}
            },
            Incoming::Request { seq, command, arguments } => match command.as_str() {
                "startDebugging" => {
                    client.respond(seq, &command, true, Value::Null);
                    let child_cfg = arguments.get("configuration").cloned().unwrap_or(Value::Null);
                    let mut child_cfg = child_cfg;
                    if let Some(r) = arguments.get("request") {
                        child_cfg["request"] = r.clone();
                    }
                    let (ctx, rid, kind) = (ctx.clone(), runner_id.to_string(), adapter_type.to_string());
                    let _ = thread::Builder::new().name("dap-child".into()).spawn(move || {
                        run_session(&ctx, &rid, addr, &kind, child_cfg, Some(id), timeout);
                    });
                }
                _ => client.respond(seq, &command, false, Value::Null),
            },
        }
        true
    };
    for msg in early {
        if !handle(msg) {
            return ctx.send(Msg::Ended(id));
        }
    }
    while let Ok(msg) = rx.recv() {
        if !handle(msg) {
            break;
        }
    }
    ctx.send(Msg::Ended(id));
}
