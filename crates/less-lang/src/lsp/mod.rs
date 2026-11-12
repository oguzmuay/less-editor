//! LSP istemcisi.
//!
//! UI akışı:
//! - Dosya açılınca [`LspManager::open`] (sunucu yoksa (dil, kök) için başlatılır; ikili bulunamazsa bir kez
//!   [`LspEvent::ServerNotFound`]).
//! - Her düzenlemede [`ContentChange::from_edit`] + [`LspManager::change`]; kaydedince [`LspManager::save`];
//!   tab kapanınca [`LspManager::close`].
//! - İstekler (`hover`, `definition`…) hemen bir [`RequestId`] döner; yanıt daha sonra [`LspEvent::Response`] olarak gelir.
//! - Her karede [`LspManager::poll`] (bloklamaz) çağrılır, olaylar işlenir. Diagnostikler [`LspManager::diagnostics`]
//!   deposunda tutulur; [`LspEvent::Diagnostics`] yalnız "bu dosya değişti" bildirimidir.
//!
//! İş parçacığı tasarımı için bkz. `connection` modülü. Konumlar LSP biçimindedir (satır + UTF-16 sütun);
//! dönüşüm için [`uri::byte_to_utf16`] / [`uri::utf16_to_byte`].

mod connection;
pub mod jsonrpc;
pub mod registry;
pub mod uri;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use lsp_types::{
    CodeActionOrCommand, CompletionItem, DocumentSymbolResponse, Hover, Position, Range, SemanticTokens, TextEdit,
    WorkspaceEdit,
};
use serde_json::{Value, json};

use crate::diagnostics::{Diagnostic, DiagnosticStore};
use crate::language::Language;
use connection::{Connection, Incoming};
use jsonrpc::Message;
pub use registry::{LspConfig, ServerSpec};

/// Çalışan bir sunucunun kimliği: sunucu türü + proje kökü.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ServerKey {
    pub server: &'static str,
    pub root: PathBuf,
}

impl ServerKey {
    fn store_id(&self) -> String {
        format!("{}@{}", self.server, self.root.display())
    }
}

/// UI'ye dönen istek kimliği (yönetici genelinde tekil).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RequestId(pub u64);

/// Bir LSP konumu (dosya yolu + aralık).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub path: PathBuf,
    /// Dosya dışı URI'ler (`jdt://` sınıf dosyaları) için orijinal URI.
    pub uri: String,
    pub range: Range,
}

/// Yanıtın türlenmiş içeriği.
#[derive(Debug, Clone, PartialEq)]
pub enum LspResult {
    /// Ham hover (tooltip için [`crate::tooltip::assemble`]'a `raw` verilebilir).
    Hover {
        hover: Option<Hover>,
        raw: Value,
    },
    Locations(Vec<Location>),
    Rename(Option<WorkspaceEdit>),
    CodeActions(Vec<CodeActionOrCommand>),
    Completion {
        items: Vec<CompletionItem>,
        incomplete: bool,
    },
    Symbols(Option<DocumentSymbolResponse>),
    Formatting(Vec<TextEdit>),
    SemanticTokens(Option<SemanticTokens>),
    /// `workspace/executeCommand` ve diğer türlenmemiş sonuçlar.
    Raw(Value),
    Error {
        code: i64,
        message: String,
    },
}

/// `poll()` ile alınan olaylar.
#[derive(Debug, Clone, PartialEq)]
pub enum LspEvent {
    ServerStarted {
        key: ServerKey,
    },
    /// İkili bulunamadı (sunucu türü başına bir kez). `hint` kurulum önerisi.
    ServerNotFound {
        server: &'static str,
        language: Language,
        hint: String,
    },
    /// Başlatma hatası ya da kullanıcı uyarısı (ör. jdtls için Java 25+ yok).
    ServerWarning {
        server: &'static str,
        message: String,
    },
    ServerExited {
        key: ServerKey,
        code: Option<i32>,
        stderr: Vec<String>,
    },
    /// Dosyanın diagnostikleri değişti.
    Diagnostics {
        path: PathBuf,
    },
    Response {
        id: RequestId,
        key: ServerKey,
        result: LspResult,
    },
    /// `$/progress` ya da jdtls `language/status`.
    Progress {
        key: ServerKey,
        /// İlerleme belirteci (`$/progress` `token`; jdtls durum mesajında `"language/status"`).
        token: String,
        title: Option<String>,
        message: Option<String>,
        percentage: Option<u32>,
        done: bool,
    },
    /// `window/showMessage` (+ `showMessageRequest`): 1 hata, 2 uyarı, 3 bilgi, 4 log.
    ShowMessage {
        key: ServerKey,
        level: u8,
        message: String,
    },
    LogMessage {
        key: ServerKey,
        level: u8,
        message: String,
    },
    /// Sunucu `workspace/applyEdit` istedi; okuyucu "uygulandı" diye yanıtladı, UI düzenlemeyi uygulamalı.
    ApplyEdit {
        key: ServerKey,
        label: Option<String>,
        edit: Option<WorkspaceEdit>,
        raw: Value,
    },
}

/// Metin değişikliği (LSP `TextDocumentContentChangeEvent`). `range` yoksa tam metin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentChange {
    pub range: Option<Range>,
    pub text: String,
}

impl ContentChange {
    /// Değişiklik **uygulanmadan önceki** metinden, bayt aralığını UTF-16 konumlara çevirerek.
    pub fn from_edit(old_text: &str, start_byte: usize, old_end_byte: usize, inserted: &str) -> ContentChange {
        ContentChange {
            range: Some(Range { start: lsp_position(old_text, start_byte), end: lsp_position(old_text, old_end_byte) }),
            text: inserted.to_string(),
        }
    }
}

/// Metindeki bayt ofsetinin LSP konumu.
pub fn lsp_position(text: &str, byte: usize) -> Position {
    let p = crate::highlight::point_at(text, byte);
    let line_start = byte.min(text.len()) - p.column;
    let line_end = text[line_start..].find('\n').map_or(text.len(), |i| line_start + i);
    Position { line: p.row as u32, character: uri::byte_to_utf16(&text[line_start..line_end], p.column) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Initialize,
    Shutdown,
    /// `textDocument/diagnostic` (pull; ESLint yalnız böyle verir).
    Pull,
    Hover,
    Locations,
    Rename,
    CodeAction,
    Completion,
    Symbols,
    Formatting,
    SemanticTokens,
    Raw,
}

struct Pending {
    kind: Kind,
    id: RequestId,
}

struct Server {
    conn: Connection,
    ready: bool,
    /// Başlatma bitene kadar bekleyen mesajlar.
    queue: Vec<Value>,
    capabilities: Value,
    settings: Value,
    next_id: i64,
    pending: HashMap<i64, Pending>,
    /// Bekleyen pull diagnostik isteklerinin belgesi.
    pulls: HashMap<i64, PathBuf>,
}

impl Server {
    fn send(&mut self, msg: Value) {
        if self.ready {
            self.conn.send(msg);
        } else {
            self.queue.push(msg);
        }
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.send(json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    fn request(&mut self, method: &str, params: Value, kind: Kind, id: RequestId) {
        self.next_id += 1;
        let n = self.next_id;
        self.pending.insert(n, Pending { kind, id });
        let msg = json!({ "jsonrpc": "2.0", "id": n, "method": method, "params": params });
        if kind == Kind::Initialize { self.conn.send(msg) } else { self.send(msg) }
    }

    /// `textDocumentSync` değişiklik türü: 0 yok, 1 tam, 2 artımlı.
    fn sync_kind(&self) -> u64 {
        let sync = &self.capabilities["textDocumentSync"];
        sync.as_u64().or_else(|| sync["change"].as_u64()).unwrap_or(0)
    }

    fn save_includes_text(&self) -> bool {
        let save = &self.capabilities["textDocumentSync"]["save"];
        save["includeText"].as_bool().unwrap_or(false)
    }

    /// Sunucu pull diagnostik (`diagnosticProvider`) veriyorsa belgenin diagnostiklerini ister. Push yapan sunucularda
    /// (vtsls, jdtls) hiçbir şey yapmaz. Hazır değilse başlatma bitince [`LspManager::on_response`] ister.
    fn pull(&mut self, path: &Path) {
        if self.ready && self.supports("diagnosticProvider") {
            let params = json!({ "textDocument": { "uri": uri::path_to_uri(path) } });
            self.request("textDocument/diagnostic", params, Kind::Pull, RequestId(0));
            self.pulls.insert(self.next_id, path.to_path_buf());
        }
    }

    fn supports(&self, capability: &str) -> bool {
        match &self.capabilities[capability] {
            Value::Null => false,
            Value::Bool(b) => *b,
            _ => true,
        }
    }
}

struct Doc {
    lang: Language,
    version: i32,
    /// İstekleri (hover, tanım…) yanıtlayan ana sunucu.
    server: Option<ServerKey>,
    /// Yalnız diagnostik veren ek sunucular (ESLint); belge eşitlemesi alırlar, istek almazlar.
    linters: Vec<ServerKey>,
}

impl Doc {
    fn keys(&self) -> impl Iterator<Item = &ServerKey> {
        self.server.iter().chain(&self.linters)
    }
}

/// Dil sunucularını (dil, kök) başına başlatır, belgeleri eşitler, olayları toplar.
pub struct LspManager {
    config: LspConfig,
    tx: Sender<Incoming>,
    rx: Receiver<Incoming>,
    servers: HashMap<ServerKey, Server>,
    not_found: HashSet<&'static str>,
    docs: HashMap<PathBuf, Doc>,
    diagnostics: DiagnosticStore,
    next_request: u64,
    outbox: Vec<LspEvent>,
    /// Sunucu başına süren `workDone` ilerlemeleri (belirteçler).
    progress: HashMap<ServerKey, HashSet<String>>,
    /// Kapanmış sunucular (yeniden başlatılınca listeden çıkar).
    exited: Vec<ServerKey>,
}

/// Bir sunucunun durum satırındaki hali.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerState {
    /// Süreç açıldı, `initialize` yanıtı bekleniyor.
    Starting,
    Running,
    /// `workDone` ilerlemesi sürüyor (indeksleme, derleme).
    Busy,
    Exited,
}

impl LspManager {
    pub fn new(config: LspConfig) -> LspManager {
        let (tx, rx) = channel();
        LspManager {
            config,
            tx,
            rx,
            servers: HashMap::new(),
            not_found: HashSet::new(),
            docs: HashMap::new(),
            diagnostics: DiagnosticStore::default(),
            next_request: 0,
            outbox: Vec::new(),
            progress: HashMap::new(),
            exited: Vec::new(),
        }
    }

    pub fn diagnostics(&self) -> &DiagnosticStore {
        &self.diagnostics
    }

    /// Çalışan sunucular.
    pub fn servers(&self) -> impl Iterator<Item = &ServerKey> {
        self.servers.keys()
    }

    /// Bilinen her sunucunun durumu: çalışanlar (başlatılıyor / hazır / meşgul), sonra kapanmış olanlar.
    pub fn server_states(&self) -> Vec<(ServerKey, ServerState)> {
        let mut out: Vec<(ServerKey, ServerState)> = self
            .servers
            .iter()
            .map(|(key, s)| {
                let busy = self.progress.get(key).is_some_and(|p| !p.is_empty());
                let state = match (s.ready, busy) {
                    (false, _) => ServerState::Starting,
                    (true, true) => ServerState::Busy,
                    (true, false) => ServerState::Running,
                };
                (key.clone(), state)
            })
            .collect();
        out.sort_by(|a, b| (a.0.server, &a.0.root).cmp(&(b.0.server, &b.0.root)));
        out.extend(self.exited.iter().map(|k| (k.clone(), ServerState::Exited)));
        out
    }

    fn track_progress(&mut self, key: &ServerKey, token: &str, done: bool) {
        let set = self.progress.entry(key.clone()).or_default();
        if done {
            set.remove(token);
        } else {
            set.insert(token.to_string());
        }
    }

    /// Yeni sunucu kuruldu: "bulunamadı" kayıtlarını unutur ve sunucusu eksik açık belgeleri yeniden açar
    /// (`texts` belgelerin güncel metni). Yeniden açılan belgelerin yolları döner.
    pub fn retry_missing(&mut self, texts: &HashMap<PathBuf, String>) -> Vec<PathBuf> {
        self.not_found.clear();
        let missing: Vec<(PathBuf, Language)> = self
            .docs
            .iter()
            .filter(|(path, d)| {
                let wanted = registry::server_for(d.lang).is_some() as usize
                    + registry::linters_for(d.lang).iter().filter(|s| registry::extra_root(s, path).is_some()).count();
                d.keys().count() < wanted
            })
            .map(|(p, d)| (p.clone(), d.lang))
            .collect();
        let mut out = Vec::new();
        for (path, lang) in missing {
            if let Some(text) = texts.get(&path) {
                self.open(&path, lang, text);
                out.push(path);
            }
        }
        out
    }

    /// Belgenin bağlı olduğu sunucu.
    pub fn server_of(&self, path: &Path) -> Option<&ServerKey> {
        self.docs.get(path)?.server.as_ref()
    }

    /// Sunucuyu süreç yerine verilen akışlara bağlar (testler; ileride soket). `server` dil kaydındaki kimlik olmalı.
    pub fn connect_streams(
        &mut self,
        server: &'static str,
        root: &Path,
        reader: impl std::io::Read + Send + 'static,
        writer: impl std::io::Write + Send + 'static,
        settings: Value,
    ) -> ServerKey {
        let key = ServerKey { server, root: root.to_path_buf() };
        let conn = Connection::from_streams(reader, writer, key.clone(), settings.clone(), self.tx.clone());
        self.install(key.clone(), conn, settings, Value::Null);
        key
    }

    fn install(&mut self, key: ServerKey, conn: Connection, settings: Value, init_options: Value) {
        let mut server = Server {
            conn,
            ready: false,
            queue: Vec::new(),
            capabilities: Value::Null,
            settings,
            next_id: 0,
            pending: HashMap::new(),
            pulls: HashMap::new(),
        };
        let params = initialize_params(&key.root, init_options);
        server.request("initialize", params, Kind::Initialize, RequestId(0));
        self.exited.retain(|k| *k != key);
        self.progress.remove(&key);
        self.servers.insert(key, server);
    }

    /// `spec` sunucusunu `root` için başlatır (çalışıyorsa var olanı döndürür); ikili yoksa bir kez `ServerNotFound`.
    fn ensure_server(&mut self, lang: Language, spec: ServerSpec, root: PathBuf) -> Option<ServerKey> {
        let key = ServerKey { server: spec.id, root: root.clone() };
        if self.servers.contains_key(&key) {
            return Some(key);
        }
        let Some(launch) = registry::launch_for(&spec, &root, &self.config) else {
            if self.not_found.insert(spec.id) {
                self.outbox.push(LspEvent::ServerNotFound {
                    server: spec.id,
                    language: lang,
                    hint: spec.install_hint.to_string(),
                });
            }
            return None;
        };
        if let Some(message) = launch.warning.clone() {
            self.outbox.push(LspEvent::ServerWarning { server: spec.id, message });
        }
        let path = self.config.child_path();
        match Connection::spawn(
            &launch.program,
            &launch.args,
            &path,
            key.clone(),
            launch.settings.clone(),
            self.tx.clone(),
        ) {
            Ok(conn) => {
                self.install(key.clone(), conn, launch.settings, launch.init_options);
                Some(key)
            }
            Err(err) => {
                self.outbox.push(LspEvent::ServerWarning {
                    server: spec.id,
                    message: format!("Could not start {}: {err}", launch.program.display()),
                });
                None
            }
        }
    }

    /// Belgeyi açar (`didOpen`). Aynı yol zaten açıksa önce kapatılır.
    pub fn open(&mut self, path: &Path, lang: Language, text: &str) {
        if self.docs.contains_key(path) {
            self.close(path);
        }
        let server = registry::server_for(lang).and_then(|spec| {
            let root = registry::root_for(&spec, path);
            self.ensure_server(lang, spec, root)
        });
        let linters: Vec<ServerKey> = registry::linters_for(lang)
            .into_iter()
            .filter_map(|spec| {
                let root = registry::extra_root(&spec, path)?;
                self.ensure_server(lang, spec, root)
            })
            .collect();
        let doc = Doc { lang, version: 0, server, linters };
        let open = json!({ "textDocument": {
            "uri": uri::path_to_uri(path), "languageId": lang.lsp_id(), "version": 0, "text": text
        }});
        for key in doc.keys() {
            if let Some(s) = self.servers.get_mut(key) {
                s.notify("textDocument/didOpen", open.clone());
                s.pull(path);
            }
        }
        self.docs.insert(path.to_path_buf(), doc);
    }

    /// Düzenlemeleri bildirir (`didChange`). Sunucu artımlı eşitleme istemiyorsa ya da henüz başlatılıyorsa
    /// `full_text` çağrılır (rope'u yalnız gerektiğinde dizeye çevirmek için kapanış).
    pub fn change(&mut self, path: &Path, changes: &[ContentChange], full_text: impl FnOnce() -> String) {
        let Some(doc) = self.docs.get_mut(path) else { return };
        doc.version += 1;
        let version = doc.version;
        let keys: Vec<ServerKey> = doc.keys().cloned().collect();
        let uri = uri::path_to_uri(path);
        let (mut make, mut text) = (Some(full_text), None::<String>);
        let mut full = || text.get_or_insert_with(|| make.take().map(|f| f()).unwrap_or_default()).clone();
        for key in &keys {
            let Some(server) = self.servers.get_mut(key) else { continue };
            if !server.ready {
                let text = full();
                for msg in server.queue.iter_mut() {
                    if msg["method"] == "textDocument/didOpen" && msg["params"]["textDocument"]["uri"] == uri.as_str() {
                        msg["params"]["textDocument"]["text"] = json!(text);
                        msg["params"]["textDocument"]["version"] = json!(version);
                    }
                }
                continue;
            }
            let content: Vec<Value> = match server.sync_kind() {
                0 => continue,
                2 => changes
                    .iter()
                    .map(|c| match &c.range {
                        Some(r) => json!({ "range": r, "text": c.text }),
                        None => json!({ "text": c.text }),
                    })
                    .collect(),
                _ => vec![json!({ "text": full() })],
            };
            server.notify(
                "textDocument/didChange",
                json!({ "textDocument": { "uri": uri, "version": version }, "contentChanges": content }),
            );
            server.pull(path);
        }
    }

    /// `didSave`; sunucu metin istiyorsa `text` gönderilir.
    pub fn save(&mut self, path: &Path, text: Option<&str>) {
        let Some(doc) = self.docs.get(path) else { return };
        for key in doc.keys() {
            let Some(server) = self.servers.get_mut(key) else { continue };
            let mut params = json!({ "textDocument": { "uri": uri::path_to_uri(path) } });
            if let (true, Some(text)) = (server.save_includes_text(), text) {
                params["text"] = json!(text);
            }
            server.notify("textDocument/didSave", params);
            server.pull(path);
        }
    }

    /// `didClose` ve belge kaydını siler (diagnostikler sunucu temizleyene kadar kalır).
    pub fn close(&mut self, path: &Path) {
        let Some(doc) = self.docs.remove(path) else { return };
        for key in doc.keys() {
            let Some(server) = self.servers.get_mut(key) else { continue };
            server.notify("textDocument/didClose", json!({ "textDocument": { "uri": uri::path_to_uri(path) } }));
        }
    }

    /// Belge açık mı ve dili.
    pub fn document_language(&self, path: &Path) -> Option<Language> {
        self.docs.get(path).map(|d| d.lang)
    }

    fn doc_server(&mut self, path: &Path) -> Option<&mut Server> {
        let key = self.docs.get(path)?.server.clone()?;
        self.servers.get_mut(&key)
    }

    /// Belgenin sunucusu `capability`'yi destekliyorsa isteği gönderir. Sunucu başlatılıyorsa yetenek henüz
    /// bilinmediği için istek kuyruğa girer.
    fn doc_request(
        &mut self,
        path: &Path,
        capability: &str,
        method: &str,
        params: Value,
        kind: Kind,
    ) -> Option<RequestId> {
        let next = RequestId(self.next_request + 1);
        let server = self.doc_server(path)?;
        if server.ready && !server.supports(capability) {
            return None;
        }
        server.request(method, params, kind, next);
        self.next_request += 1;
        Some(next)
    }

    fn text_position(path: &Path, pos: Position) -> Value {
        json!({ "textDocument": { "uri": uri::path_to_uri(path) }, "position": pos })
    }

    pub fn hover(&mut self, path: &Path, pos: Position) -> Option<RequestId> {
        self.doc_request(path, "hoverProvider", "textDocument/hover", Self::text_position(path, pos), Kind::Hover)
    }

    pub fn definition(&mut self, path: &Path, pos: Position) -> Option<RequestId> {
        let params = Self::text_position(path, pos);
        self.doc_request(path, "definitionProvider", "textDocument/definition", params, Kind::Locations)
    }

    pub fn implementation(&mut self, path: &Path, pos: Position) -> Option<RequestId> {
        let params = Self::text_position(path, pos);
        self.doc_request(path, "implementationProvider", "textDocument/implementation", params, Kind::Locations)
    }

    pub fn type_definition(&mut self, path: &Path, pos: Position) -> Option<RequestId> {
        let params = Self::text_position(path, pos);
        self.doc_request(path, "typeDefinitionProvider", "textDocument/typeDefinition", params, Kind::Locations)
    }

    pub fn references(&mut self, path: &Path, pos: Position, include_declaration: bool) -> Option<RequestId> {
        let mut params = Self::text_position(path, pos);
        params["context"] = json!({ "includeDeclaration": include_declaration });
        self.doc_request(path, "referencesProvider", "textDocument/references", params, Kind::Locations)
    }

    pub fn rename(&mut self, path: &Path, pos: Position, new_name: &str) -> Option<RequestId> {
        let mut params = Self::text_position(path, pos);
        params["newName"] = json!(new_name);
        self.doc_request(path, "renameProvider", "textDocument/rename", params, Kind::Rename)
    }

    /// Aralık için code action'lar; `diagnostics` o aralıktaki diagnostikler (hızlı düzeltmeler için).
    pub fn code_actions(&mut self, path: &Path, range: Range, diagnostics: &[&Diagnostic]) -> Option<RequestId> {
        let diags: Vec<Value> = diagnostics.iter().map(|d| diagnostic_json(d)).collect();
        let params = json!({
            "textDocument": { "uri": uri::path_to_uri(path) }, "range": range, "context": { "diagnostics": diags }
        });
        self.doc_request(path, "codeActionProvider", "textDocument/codeAction", params, Kind::CodeAction)
    }

    pub fn completion(&mut self, path: &Path, pos: Position) -> Option<RequestId> {
        let params = Self::text_position(path, pos);
        self.doc_request(path, "completionProvider", "textDocument/completion", params, Kind::Completion)
    }

    /// Tamamlama isteği ana sunucuya ve tamamlama veren hazır ek sunuculara (Tailwind); her biri ayrı
    /// [`LspResult::Completion`] yanıtı verir. `trigger` = yazılan tetikleyici karakter (yoksa elle / yazarken).
    pub fn complete(&mut self, path: &Path, pos: Position, trigger: Option<&str>) -> Vec<RequestId> {
        let mut params = Self::text_position(path, pos);
        params["context"] = match trigger {
            Some(c) => json!({ "triggerKind": 2, "triggerCharacter": c }),
            None => json!({ "triggerKind": 1 }),
        };
        let mut ids: Vec<RequestId> = self
            .doc_request(path, "completionProvider", "textDocument/completion", params.clone(), Kind::Completion)
            .into_iter()
            .collect();
        let extras: Vec<ServerKey> = self.docs.get(path).map(|d| d.linters.clone()).unwrap_or_default();
        for key in extras {
            let Some(server) = self.servers.get_mut(&key).filter(|s| s.ready && s.supports("completionProvider"))
            else {
                continue;
            };
            if trigger
                .is_some_and(|c| !trigger_chars(&server.capabilities["completionProvider"]).iter().any(|t| t == c))
            {
                continue;
            }
            self.next_request += 1;
            let id = RequestId(self.next_request);
            server.request("textDocument/completion", params.clone(), Kind::Completion, id);
            ids.push(id);
        }
        ids
    }

    /// `completionItem/resolve` (öğeyi veren sunucuya, `resolveProvider` varsa); yanıt [`LspResult::Raw`]
    /// (çözülmüş `CompletionItem`).
    pub fn resolve_completion(&mut self, key: &ServerKey, item: Value) -> Option<RequestId> {
        let server = self.servers.get_mut(key).filter(|s| s.ready)?;
        if server.capabilities["completionProvider"]["resolveProvider"] != json!(true) {
            return None;
        }
        self.next_request += 1;
        let id = RequestId(self.next_request);
        server.request("completionItem/resolve", item, Kind::Raw, id);
        Some(id)
    }

    /// `textDocument/signatureHelp`; yanıt [`LspResult::Raw`] (`SignatureHelp` ya da `null`).
    pub fn signature_help(&mut self, path: &Path, pos: Position, trigger: Option<&str>) -> Option<RequestId> {
        let mut params = Self::text_position(path, pos);
        params["context"] = match trigger {
            Some(c) => json!({ "triggerKind": 2, "triggerCharacter": c, "isRetrigger": false }),
            None => json!({ "triggerKind": 1, "isRetrigger": false }),
        };
        self.doc_request(path, "signatureHelpProvider", "textDocument/signatureHelp", params, Kind::Raw)
    }

    /// Belgenin sunucularının (ana + ek) tamamlama tetikleyici karakterleri.
    pub fn completion_triggers(&self, path: &Path) -> Vec<String> {
        let Some(doc) = self.docs.get(path) else { return Vec::new() };
        let caps = doc.keys().filter_map(|k| self.servers.get(k)).map(|s| &s.capabilities["completionProvider"]);
        caps.flat_map(trigger_chars).collect()
    }

    /// Ana sunucunun imza yardımı tetikleyicileri (`(`, `,`…); sunucu desteklemiyorsa boş.
    pub fn signature_triggers(&self, path: &Path) -> Vec<String> {
        let Some(server) = self.docs.get(path).and_then(|d| d.server.as_ref()).and_then(|k| self.servers.get(k)) else {
            return Vec::new();
        };
        let sig = &server.capabilities["signatureHelpProvider"];
        let mut out = trigger_chars(sig);
        out.extend(
            sig["retriggerCharacters"].as_array().into_iter().flatten().filter_map(|c| c.as_str().map(String::from)),
        );
        out
    }

    pub fn document_symbols(&mut self, path: &Path) -> Option<RequestId> {
        let params = json!({ "textDocument": { "uri": uri::path_to_uri(path) } });
        self.doc_request(path, "documentSymbolProvider", "textDocument/documentSymbol", params, Kind::Symbols)
    }

    pub fn formatting(&mut self, path: &Path, tab_size: u32, insert_spaces: bool) -> Option<RequestId> {
        let params = json!({
            "textDocument": { "uri": uri::path_to_uri(path) },
            "options": { "tabSize": tab_size, "insertSpaces": insert_spaces }
        });
        self.doc_request(path, "documentFormattingProvider", "textDocument/formatting", params, Kind::Formatting)
    }

    pub fn semantic_tokens(&mut self, path: &Path) -> Option<RequestId> {
        let params = json!({ "textDocument": { "uri": uri::path_to_uri(path) } });
        self.doc_request(
            path,
            "semanticTokensProvider",
            "textDocument/semanticTokens/full",
            params,
            Kind::SemanticTokens,
        )
    }

    /// Semantic token göstergesi (`tokenTypes`, `tokenModifiers`) — sunucu başladıktan sonra.
    pub fn semantic_legend(&self, key: &ServerKey) -> Option<&Value> {
        let legend = &self.servers.get(key)?.capabilities["semanticTokensProvider"]["legend"];
        (!legend.is_null()).then_some(legend)
    }

    /// Code action komutu çalıştırma (`workspace/executeCommand`), belgenin sunucusunda.
    pub fn execute_command(&mut self, path: &Path, command: &str, arguments: Value) -> Option<RequestId> {
        let params = json!({ "command": command, "arguments": arguments });
        self.doc_request(path, "executeCommandProvider", "workspace/executeCommand", params, Kind::Raw)
    }

    /// Olayları bloklamadan toplar.
    pub fn poll(&mut self) -> Vec<LspEvent> {
        let mut events = std::mem::take(&mut self.outbox);
        while let Ok(incoming) = self.rx.try_recv() {
            self.handle(incoming, &mut events);
        }
        events
    }

    /// İlk olay gelene kadar en fazla `timeout` bekler, sonra [`LspManager::poll`] gibi toplar (testler, kapanış).
    pub fn poll_timeout(&mut self, timeout: Duration) -> Vec<LspEvent> {
        let mut events = std::mem::take(&mut self.outbox);
        if events.is_empty()
            && let Ok(incoming) = self.rx.recv_timeout(timeout)
        {
            self.handle(incoming, &mut events);
        }
        while let Ok(incoming) = self.rx.try_recv() {
            self.handle(incoming, &mut events);
        }
        events
    }

    /// Tüm sunuculara `shutdown` + `exit` gönderir, en fazla `timeout` bekler, kalanları öldürür.
    pub fn shutdown_all(&mut self, timeout: Duration) {
        for server in self.servers.values_mut().filter(|s| s.ready) {
            server.request("shutdown", Value::Null, Kind::Shutdown, RequestId(0));
        }
        let deadline = Instant::now() + timeout;
        while self.servers.values().any(|s| s.ready && s.pending.values().any(|p| p.kind == Kind::Shutdown)) {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            let _ = self.poll_timeout(left);
        }
        for server in self.servers.values_mut() {
            server.conn.send(json!({ "jsonrpc": "2.0", "method": "exit" }));
        }
        let deadline = Instant::now() + timeout;
        while self.servers.values_mut().any(|s| s.conn.is_running()) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        self.servers.clear();
    }

    fn handle(&mut self, incoming: Incoming, events: &mut Vec<LspEvent>) {
        match incoming {
            Incoming::Message(key, Message::Response { id, result }) => self.on_response(key, id, result, events),
            Incoming::Message(key, Message::Notification { method, params }) => {
                self.on_notification(key, &method, params, events)
            }
            Incoming::Message(_, Message::Request { .. }) => {}
            Incoming::Answered(key, method, params) => match method.as_str() {
                "workspace/applyEdit" => events.push(LspEvent::ApplyEdit {
                    key,
                    label: params["label"].as_str().map(str::to_string),
                    edit: serde_json::from_value(params["edit"].clone()).ok(),
                    raw: params,
                }),
                "window/showMessageRequest" => events.push(LspEvent::ShowMessage {
                    key,
                    level: params["type"].as_u64().unwrap_or(3) as u8,
                    message: params["message"].as_str().unwrap_or_default().to_string(),
                }),
                _ => {}
            },
            Incoming::Closed(key) => {
                let Some(mut server) = self.servers.remove(&key) else { return };
                let code = server.conn.exit_code();
                let stderr = server.conn.stderr_tail();
                let paths: Vec<PathBuf> = self.diagnostics.files().map(Path::to_path_buf).collect();
                self.diagnostics.clear_server(&key.store_id());
                for path in paths {
                    if self.diagnostics.get(&path).is_empty() {
                        events.push(LspEvent::Diagnostics { path });
                    }
                }
                for doc in self.docs.values_mut() {
                    if doc.server.as_ref() == Some(&key) {
                        doc.server = None;
                    }
                    doc.linters.retain(|k| *k != key);
                }
                self.progress.remove(&key);
                if !self.exited.contains(&key) {
                    self.exited.push(key.clone());
                }
                events.push(LspEvent::ServerExited { key, code, stderr });
            }
        }
    }

    fn on_response(
        &mut self,
        key: ServerKey,
        id: Value,
        result: Result<Value, jsonrpc::ResponseError>,
        events: &mut Vec<LspEvent>,
    ) {
        let Some(server) = self.servers.get_mut(&key) else { return };
        let Some(pending) = id.as_i64().and_then(|n| server.pending.remove(&n)) else { return };
        if pending.kind == Kind::Initialize {
            server.capabilities = result.as_ref().map(|r| r["capabilities"].clone()).unwrap_or(Value::Null);
            server.ready = true;
            server.conn.send(json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }));
            let settings = server.settings.clone();
            server.conn.send(json!({
                "jsonrpc": "2.0", "method": "workspace/didChangeConfiguration", "params": { "settings": settings }
            }));
            for msg in std::mem::take(&mut server.queue) {
                server.conn.send(msg);
            }
            for (path, _) in self.docs.iter().filter(|(_, d)| d.keys().any(|k| *k == key)) {
                server.pull(path);
            }
            if let Err(err) = result {
                events.push(LspEvent::ServerWarning { server: key.server, message: err.message });
            }
            events.push(LspEvent::ServerStarted { key });
            return;
        }
        if pending.kind == Kind::Shutdown {
            return;
        }
        if pending.kind == Kind::Pull {
            let Some(path) = id.as_i64().and_then(|n| server.pulls.remove(&n)) else { return };
            // shortcut: sürüm/`resultId` takibi yok; sıra dışı gelen eski yanıt bir sonraki düzenlemeye kadar kalır.
            if let Ok(r) = result
                && r["kind"] == "full"
            {
                let items = r["items"].as_array().map(|a| a.iter().filter_map(Diagnostic::from_lsp).collect());
                self.diagnostics.set(&path, &key.store_id(), items.unwrap_or_default());
                events.push(LspEvent::Diagnostics { path });
            }
            return;
        }
        let result = match result {
            Err(err) => LspResult::Error { code: err.code, message: err.message },
            Ok(value) => convert(pending.kind, value),
        };
        events.push(LspEvent::Response { id: pending.id, key, result });
    }

    fn on_notification(&mut self, key: ServerKey, method: &str, params: Value, events: &mut Vec<LspEvent>) {
        match method {
            "textDocument/publishDiagnostics" => {
                let Some(path) = params["uri"].as_str().and_then(uri::uri_to_path) else { return };
                let diags =
                    params["diagnostics"].as_array().map(|a| a.iter().filter_map(Diagnostic::from_lsp).collect());
                self.diagnostics.set(&path, &key.store_id(), diags.unwrap_or_default());
                events.push(LspEvent::Diagnostics { path });
            }
            "$/progress" => {
                let v = &params["value"];
                let token = match &params["token"] {
                    Value::String(t) => t.clone(),
                    t => t.to_string(),
                };
                let done = v["kind"] == "end";
                self.track_progress(&key, &token, done);
                events.push(LspEvent::Progress {
                    key,
                    token,
                    title: v["title"].as_str().map(str::to_string),
                    message: v["message"].as_str().map(str::to_string),
                    percentage: v["percentage"].as_u64().map(|p| p as u32),
                    done,
                });
            }
            "language/status" => {
                let done = params["type"] == "ServiceReady" || params["type"] == "Started";
                self.track_progress(&key, "language/status", done);
                events.push(LspEvent::Progress {
                    key,
                    token: "language/status".to_string(),
                    title: params["type"].as_str().map(str::to_string),
                    message: params["message"].as_str().map(str::to_string),
                    percentage: None,
                    done,
                })
            }
            "window/showMessage" | "window/logMessage" => {
                let level = params["type"].as_u64().unwrap_or(4) as u8;
                let message = params["message"].as_str().unwrap_or_default().to_string();
                events.push(if method == "window/showMessage" {
                    LspEvent::ShowMessage { key, level, message }
                } else {
                    LspEvent::LogMessage { key, level, message }
                });
            }
            _ => {}
        }
    }
}

impl Drop for LspManager {
    fn drop(&mut self) {
        for server in self.servers.values() {
            server.conn.send(json!({ "jsonrpc": "2.0", "method": "exit" }));
        }
    }
}

/// `completionProvider` / `signatureHelpProvider` içindeki `triggerCharacters`.
fn trigger_chars(provider: &Value) -> Vec<String> {
    provider["triggerCharacters"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c.as_str().map(String::from))
        .collect()
}

fn convert(kind: Kind, value: Value) -> LspResult {
    match kind {
        Kind::Hover => LspResult::Hover { hover: serde_json::from_value(value.clone()).ok(), raw: value },
        Kind::Locations => LspResult::Locations(locations(&value)),
        Kind::Rename => LspResult::Rename(serde_json::from_value(value).ok()),
        Kind::CodeAction => LspResult::CodeActions(serde_json::from_value(value).unwrap_or_default()),
        Kind::Completion => {
            let incomplete = value["isIncomplete"].as_bool().unwrap_or(false);
            let items = if value.is_array() { value } else { value["items"].clone() };
            LspResult::Completion { items: serde_json::from_value(items).unwrap_or_default(), incomplete }
        }
        Kind::Symbols => LspResult::Symbols(serde_json::from_value(value).ok()),
        Kind::Formatting => LspResult::Formatting(serde_json::from_value(value).unwrap_or_default()),
        Kind::SemanticTokens => LspResult::SemanticTokens(serde_json::from_value(value).ok()),
        Kind::Raw | Kind::Initialize | Kind::Shutdown | Kind::Pull => LspResult::Raw(value),
    }
}

/// `Location | Location[] | LocationLink[] | null` → konum listesi.
fn locations(value: &Value) -> Vec<Location> {
    let items: Vec<&Value> = match value {
        Value::Array(a) => a.iter().collect(),
        Value::Null => Vec::new(),
        v => vec![v],
    };
    items
        .into_iter()
        .filter_map(|v| {
            let uri = v.get("uri").or_else(|| v.get("targetUri"))?.as_str()?.to_string();
            let range = v.get("range").or_else(|| v.get("targetSelectionRange"))?;
            Some(Location {
                path: uri::uri_to_path(&uri).unwrap_or_default(),
                range: serde_json::from_value(range.clone()).ok()?,
                uri,
            })
        })
        .collect()
}

/// `WorkspaceEdit`'i dosya başına düzenleme listesine çevirir (`changes` ve `documentChanges` birlikte).
/// Dosya oluşturma/silme/yeniden adlandırma işlemleri atlanır.
pub fn workspace_edit_files(edit: &WorkspaceEdit) -> Vec<(PathBuf, Vec<TextEdit>)> {
    let mut out: Vec<(PathBuf, Vec<TextEdit>)> = Vec::new();
    if let Some(changes) = &edit.changes {
        for (u, edits) in changes {
            if let Some(path) = uri::uri_to_path(u.as_str()) {
                out.push((path, edits.clone()));
            }
        }
    }
    if let Some(doc_changes) = &edit.document_changes {
        let value = serde_json::to_value(doc_changes).unwrap_or(Value::Null);
        for change in value.as_array().into_iter().flatten() {
            let (Some(u), Some(edits)) = (change["textDocument"]["uri"].as_str(), change.get("edits")) else {
                continue;
            };
            let (Some(path), Ok(edits)) = (uri::uri_to_path(u), serde_json::from_value::<Vec<TextEdit>>(edits.clone()))
            else {
                continue;
            };
            out.push((path, edits));
        }
    }
    out
}

fn diagnostic_json(d: &Diagnostic) -> Value {
    let severity = match d.severity {
        crate::diagnostics::Severity::Error => 1,
        crate::diagnostics::Severity::Warning => 2,
        crate::diagnostics::Severity::Info => 3,
        crate::diagnostics::Severity::Hint => 4,
    };
    let mut v = json!({
        "range": { "start": { "line": d.start.line, "character": d.start.character },
                   "end": { "line": d.end.line, "character": d.end.character } },
        "severity": severity,
        "message": d.message,
    });
    if let Some(s) = &d.source {
        v["source"] = json!(s);
    }
    if let Some(c) = &d.code {
        v["code"] = json!(c);
    }
    v
}

/// `initialize` parametreleri: istemci yetenekleri (diagnostik, markdown hover, tanım, referans, rename,
/// code action (`codeAction/resolve` yok: `resolveSupport` bildirilmez, sunucu düzenlemeyi hemen verir), tamamlama, document symbol, biçimlendirme, semantic token, progress).
fn initialize_params(root: &Path, init_options: Value) -> Value {
    let root_uri = uri::path_to_uri(root);
    let name = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut params = json!({
        "processId": std::process::id(),
        "clientInfo": { "name": "LESS", "version": env!("CARGO_PKG_VERSION") },
        "rootUri": root_uri,
        "rootPath": root.to_string_lossy(),
        "workspaceFolders": [{ "uri": root_uri, "name": name }],
        "capabilities": {
            "general": { "positionEncodings": ["utf-16"] },
            "window": { "workDoneProgress": true, "showMessage": {} },
            "workspace": {
                "configuration": true,
                "workspaceFolders": true,
                "applyEdit": true,
                "workspaceEdit": { "documentChanges": true },
                "didChangeConfiguration": { "dynamicRegistration": false },
                "executeCommand": {},
                "symbol": {}
            },
            "textDocument": {
                "synchronization": { "didSave": true, "willSave": false, "dynamicRegistration": false },
                "publishDiagnostics": { "relatedInformation": true, "codeDescriptionSupport": true },
                "hover": { "contentFormat": ["markdown", "plaintext"] },
                "definition": { "linkSupport": true },
                "typeDefinition": { "linkSupport": true },
                "implementation": { "linkSupport": true },
                "references": {},
                "rename": { "prepareSupport": false },
                "codeAction": {
                    "codeActionLiteralSupport": { "codeActionKind": { "valueSet": [
                        "", "quickfix", "refactor", "refactor.extract", "refactor.inline", "refactor.rewrite",
                        "source", "source.organizeImports"
                    ]}},
                    "dataSupport": true
                },
                "completion": {
                    "completionItem": {
                        "snippetSupport": false,
                        "documentationFormat": ["markdown", "plaintext"],
                        "resolveSupport": { "properties": ["documentation", "detail", "additionalTextEdits"] },
                        "labelDetailsSupport": true,
                        "deprecatedSupport": true
                    },
                    "completionItemKind": { "valueSet": [
                        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25
                    ]},
                    "contextSupport": true
                },
                "signatureHelp": {
                    "signatureInformation": {
                        "documentationFormat": ["markdown", "plaintext"],
                        "parameterInformation": { "labelOffsetSupport": true },
                        "activeParameterSupport": true
                    },
                    "contextSupport": true
                },
                "documentSymbol": { "hierarchicalDocumentSymbolSupport": true },
                "formatting": {},
                "semanticTokens": {
                    "requests": { "full": true },
                    "tokenTypes": [
                        "namespace", "type", "class", "enum", "interface", "struct", "typeParameter", "parameter",
                        "variable", "property", "enumMember", "event", "function", "method", "macro", "keyword",
                        "modifier", "comment", "string", "number", "regexp", "operator", "decorator"
                    ],
                    "tokenModifiers": [
                        "declaration", "definition", "readonly", "static", "deprecated", "abstract", "async",
                        "modification", "documentation", "defaultLibrary"
                    ],
                    "formats": ["relative"]
                }
            }
        }
    });
    if !init_options.is_null() {
        params["initializationOptions"] = init_options;
    }
    params
}

#[cfg(test)]
mod tests;
