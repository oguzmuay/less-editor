//! DAP istemcisi: TCP ya da stdio üzerinden istek/yanıt eşleme, olaylar ve ters istekler.
//!
//! Okuma ayrı bir iş parçacığında yapılır; yanıtlar `seq` ile bekleyen isteğe (`Pending`) gider, olaylar ve ters
//! istekler (`startDebugging`, `runInTerminal`) `Receiver<Incoming>` kanalına düşer. Engelleyen (`*_wait`) çağrılar
//! UI iş parçacığından değil, arka plandan yapılmalıdır.

use std::collections::HashMap;
use std::io::{self, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::protocol::{encode, read_message};
use super::types::*;

/// Varsayılan istek zaman aşımı.
pub const TIMEOUT: Duration = Duration::from_secs(10);

/// İstek hatası (adaptör `success: false` döndü, bağlantı kapandı ya da zaman aşımı).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DapError(pub String);

impl std::fmt::Display for DapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DapError {}

/// Adaptörden gelen istek dışı mesaj.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Event {
        event: String,
        body: Value,
    },
    /// Ters istek; `DapClient::respond` ile cevaplanmalı.
    Request {
        seq: i64,
        command: String,
        arguments: Value,
    },
    /// Bağlantı kapandı.
    Closed,
}

type Reply = Result<Value, DapError>;

struct Inner {
    writer: Mutex<Box<dyn Write + Send>>,
    seq: AtomicI64,
    pending: Mutex<HashMap<i64, Sender<Reply>>>,
    closed: AtomicBool,
}

/// Bekleyen yanıt.
pub struct Pending {
    rx: Receiver<Reply>,
}

impl Pending {
    /// Yanıtı `timeout` kadar bekler.
    pub fn wait(self, timeout: Duration) -> Reply {
        match self.rx.recv_timeout(timeout) {
            Ok(r) => r,
            Err(RecvTimeoutError::Timeout) => Err(DapError("Request timed out".into())),
            Err(RecvTimeoutError::Disconnected) => Err(DapError("Debug adapter connection closed".into())),
        }
    }

    /// Yanıt geldiyse alır (bloklamaz).
    pub fn try_take(&self) -> Option<Reply> {
        self.rx.try_recv().ok()
    }
}

/// DAP istemcisi (klonlanabilir; tüm klonlar aynı bağlantıyı paylaşır).
#[derive(Clone)]
pub struct DapClient {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for DapClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DapClient").field("closed", &self.is_closed()).finish()
    }
}

fn parse<T: DeserializeOwned>(v: Value, key: &str) -> Result<T, DapError> {
    let v = if key.is_empty() { v } else { v.get(key).cloned().unwrap_or(Value::Null) };
    serde_json::from_value(v).map_err(|e| DapError(format!("Bad response: {e}")))
}

impl DapClient {
    /// Okuma / yazma akışlarından istemci kurar.
    pub fn from_streams(
        reader: impl io::Read + Send + 'static,
        writer: impl Write + Send + 'static,
    ) -> (DapClient, Receiver<Incoming>) {
        let inner = Arc::new(Inner {
            writer: Mutex::new(Box::new(writer)),
            seq: AtomicI64::new(1),
            pending: Mutex::new(HashMap::new()),
            closed: AtomicBool::new(false),
        });
        let (tx, rx) = mpsc::channel();
        let shared = inner.clone();
        let _ = thread::Builder::new().name("dap-read".into()).spawn(move || {
            let mut r = BufReader::new(reader);
            while let Ok(Some(msg)) = read_message(&mut r) {
                match msg.get("type").and_then(Value::as_str) {
                    Some("response") => {
                        let seq = msg.get("request_seq").and_then(Value::as_i64).unwrap_or(-1);
                        let waiter = shared.pending.lock().ok().and_then(|mut p| p.remove(&seq));
                        let reply = if msg.get("success").and_then(Value::as_bool).unwrap_or(false) {
                            Ok(msg.get("body").cloned().unwrap_or(Value::Null))
                        } else {
                            let text = msg
                                .pointer("/body/error/format")
                                .or_else(|| msg.get("message"))
                                .and_then(Value::as_str)
                                .unwrap_or("Request failed");
                            Err(DapError(text.to_string()))
                        };
                        if let Some(w) = waiter {
                            let _ = w.send(reply);
                        }
                    }
                    Some("event") => {
                        let event = msg.get("event").and_then(Value::as_str).unwrap_or_default().to_string();
                        let body = msg.get("body").cloned().unwrap_or(Value::Null);
                        let _ = tx.send(Incoming::Event { event, body });
                    }
                    Some("request") => {
                        let _ = tx.send(Incoming::Request {
                            seq: msg.get("seq").and_then(Value::as_i64).unwrap_or(0),
                            command: msg.get("command").and_then(Value::as_str).unwrap_or_default().to_string(),
                            arguments: msg.get("arguments").cloned().unwrap_or(Value::Null),
                        });
                    }
                    _ => {}
                }
            }
            shared.closed.store(true, Ordering::SeqCst);
            if let Ok(mut p) = shared.pending.lock() {
                for (_, w) in p.drain() {
                    let _ = w.send(Err(DapError("Debug adapter connection closed".into())));
                }
            }
            let _ = tx.send(Incoming::Closed);
        });
        (DapClient { inner }, rx)
    }

    /// TCP adaptörüne bağlanır.
    pub fn connect(addr: SocketAddr, timeout: Duration) -> io::Result<(DapClient, Receiver<Incoming>)> {
        let stream = TcpStream::connect_timeout(&addr, timeout)?;
        stream.set_nodelay(true)?;
        let reader = stream.try_clone()?;
        Ok(DapClient::from_streams(reader, stream))
    }

    /// Adaptörü alt süreç olarak başlatıp stdio üzerinden konuşur (debugpy-adapter gibi).
    pub fn spawn_stdio(program: &str, args: &[String]) -> io::Result<(DapClient, Receiver<Incoming>, Child)> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdin = child.stdin.take().ok_or_else(|| io::Error::other("no stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| io::Error::other("no stdout"))?;
        let (client, rx) = DapClient::from_streams(stdout, stdin);
        Ok((client, rx, child))
    }

    /// Bağlantı kapandı mı.
    pub fn is_closed(&self) -> bool {
        self.inner.closed.load(Ordering::SeqCst)
    }

    fn send(&self, msg: &Value) -> io::Result<()> {
        let mut w = self.inner.writer.lock().map_err(|_| io::Error::other("poisoned"))?;
        w.write_all(&encode(msg))?;
        w.flush()
    }

    /// İsteği gönderir; yanıt `Pending` ile alınır.
    pub fn request(&self, command: &str, arguments: Value) -> Pending {
        let (tx, rx) = mpsc::channel();
        let seq = self.inner.seq.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut p) = self.inner.pending.lock() {
            p.insert(seq, tx.clone());
        }
        let mut msg = json!({ "seq": seq, "type": "request", "command": command });
        if !arguments.is_null() {
            msg["arguments"] = arguments;
        }
        if let Err(e) = self.send(&msg) {
            if let Ok(mut p) = self.inner.pending.lock() {
                p.remove(&seq);
            }
            let _ = tx.send(Err(DapError(format!("Could not send {command}: {e}"))));
        }
        Pending { rx }
    }

    /// İstek gönderip yanıtı `TIMEOUT` kadar bekler.
    pub fn request_wait(&self, command: &str, arguments: Value) -> Reply {
        self.request(command, arguments).wait(TIMEOUT)
    }

    /// Ters isteğe yanıt.
    pub fn respond(&self, request_seq: i64, command: &str, success: bool, body: Value) {
        let seq = self.inner.seq.fetch_add(1, Ordering::SeqCst);
        let mut msg = json!({
            "seq": seq, "type": "response", "request_seq": request_seq, "command": command, "success": success,
        });
        if !body.is_null() {
            msg["body"] = body;
        }
        let _ = self.send(&msg);
    }

    /// `initialize`.
    pub fn initialize(&self, adapter_id: &str) -> Result<Capabilities, DapError> {
        let args = json!({
            "clientID": "less", "clientName": "LESS", "adapterID": adapter_id, "pathFormat": "path",
            "linesStartAt1": true, "columnsStartAt1": true, "supportsVariableType": true,
            "supportsRunInTerminalRequest": false, "supportsStartDebuggingRequest": true, "locale": "en",
        });
        parse(self.request_wait("initialize", args)?, "")
    }

    /// `setBreakpoints` (dosyanın tüm breakpoint'leri).
    pub fn set_breakpoints(&self, path: &str, bps: &[SourceBreakpoint]) -> Result<Vec<Breakpoint>, DapError> {
        let args = json!({ "source": { "path": path }, "breakpoints": bps, "lines": bps.iter().map(|b| b.line).collect::<Vec<_>>() });
        parse(self.request_wait("setBreakpoints", args)?, "breakpoints")
    }

    /// `configurationDone`.
    pub fn configuration_done(&self) -> Result<(), DapError> {
        self.request_wait("configurationDone", Value::Null).map(|_| ())
    }

    /// `threads`.
    pub fn threads(&self) -> Result<Vec<Thread>, DapError> {
        parse(self.request_wait("threads", Value::Null)?, "threads")
    }

    /// `stackTrace` (en çok `levels` frame).
    pub fn stack_trace(&self, thread_id: i64, levels: i64) -> Result<Vec<StackFrame>, DapError> {
        let args = json!({ "threadId": thread_id, "startFrame": 0, "levels": levels });
        parse(self.request_wait("stackTrace", args)?, "stackFrames")
    }

    /// `scopes`.
    pub fn scopes(&self, frame_id: i64) -> Result<Vec<Scope>, DapError> {
        parse(self.request_wait("scopes", json!({ "frameId": frame_id }))?, "scopes")
    }

    /// `variables`.
    pub fn variables(&self, reference: i64) -> Result<Vec<Variable>, DapError> {
        parse(self.request_wait("variables", json!({ "variablesReference": reference }))?, "variables")
    }

    /// `setVariable` (değer adaptörde ifade olarak değerlendirilir).
    pub fn set_variable(&self, parent_ref: i64, name: &str, value: &str) -> Result<ValueResponse, DapError> {
        let args = json!({ "variablesReference": parent_ref, "name": name, "value": value });
        parse(self.request_wait("setVariable", args)?, "")
    }

    /// `setExpression`.
    pub fn set_expression(&self, expression: &str, value: &str, frame_id: i64) -> Result<ValueResponse, DapError> {
        let args = json!({ "expression": expression, "value": value, "frameId": frame_id });
        parse(self.request_wait("setExpression", args)?, "")
    }

    /// `evaluate` (`context`: `repl`, `hover`, `watch`).
    pub fn evaluate(&self, expression: &str, frame_id: Option<i64>, context: &str) -> Result<ValueResponse, DapError> {
        let mut args = json!({ "expression": expression, "context": context });
        if let Some(f) = frame_id {
            args["frameId"] = json!(f);
        }
        parse(self.request_wait("evaluate", args)?, "")
    }

    /// `continue` / `next` / `stepIn` / `stepOut` / `pause` (yanıt beklenmez).
    pub fn step(&self, command: StepCommand, thread_id: i64) -> Pending {
        self.request(command.name(), json!({ "threadId": thread_id }))
    }

    /// `disconnect` (debuggee öldürülmez).
    pub fn disconnect(&self) -> Pending {
        self.request("disconnect", json!({ "terminateDebuggee": false }))
    }
}

/// Yürütme komutu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepCommand {
    Continue,
    Next,
    StepIn,
    StepOut,
    Pause,
}

impl StepCommand {
    /// DAP komut adı.
    pub fn name(self) -> &'static str {
        match self {
            StepCommand::Continue => "continue",
            StepCommand::Next => "next",
            StepCommand::StepIn => "stepIn",
            StepCommand::StepOut => "stepOut",
            StepCommand::Pause => "pause",
        }
    }
}
