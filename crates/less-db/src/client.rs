//! Sürücü istemcisi: sürücü ikilisini başlatır, istek/yanıt eşler, zaman aşımı ve çökmeyi hataya çevirir.

use crate::connections::Connection;
use crate::protocol::*;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Sorgu zaman aşımı (#47: 5 dk).
pub const QUERY_TIMEOUT: Duration = Duration::from_secs(300);
/// Üst veri (liste, kolon, bağlanma) zaman aşımı (#47: 20 sn).
pub const METADATA_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, PartialEq)]
pub enum DriverError {
    /// Sürücü süreci başlatılamadı.
    Spawn(String),
    /// Sürücü hata döndürdü (veritabanı hatası vb.).
    Remote {
        code: i64,
        message: String,
    },
    Timeout,
    /// Süreç öldü ya da boru koptu.
    Crashed(String),
    /// Yanıt beklenen biçimde değil.
    Protocol(String),
}

impl std::fmt::Display for DriverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DriverError::Spawn(m) => write!(f, "Could not start the driver: {m}"),
            DriverError::Remote { message, .. } => write!(f, "{message}"),
            DriverError::Timeout => write!(f, "The database did not answer in time"),
            DriverError::Crashed(m) => write!(f, "The driver stopped unexpectedly: {m}"),
            DriverError::Protocol(m) => write!(f, "Unexpected driver response: {m}"),
        }
    }
}

impl std::error::Error for DriverError {}

type Pending = Arc<Mutex<HashMap<u64, Sender<Result<Value, DriverError>>>>>;

/// Çalışan bir sürücü sürecine bağlı istemci. Düşürülünce süreç sonlandırılır.
pub struct DriverClient {
    child: Child,
    stdin: Mutex<ChildStdin>,
    pending: Pending,
    next_id: Mutex<u64>,
    stderr_tail: Arc<Mutex<String>>,
}

impl DriverClient {
    /// Sürücü ikilisini başlatır (bağlantı kurmaz, bkz. `connect`).
    pub fn spawn(binary: &Path) -> Result<DriverClient, DriverError> {
        let mut child = Command::new(binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| DriverError::Spawn(format!("{}: {e}", binary.display())))?;
        let stdin = child.stdin.take().ok_or_else(|| DriverError::Spawn("no stdin".into()))?;
        let stdout = child.stdout.take().ok_or_else(|| DriverError::Spawn("no stdout".into()))?;
        let stderr = child.stderr.take();
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let stderr_tail = Arc::new(Mutex::new(String::new()));

        let p = Arc::clone(&pending);
        let tail = Arc::clone(&stderr_tail);
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let Ok(resp) = serde_json::from_str::<Response>(&line) else { continue };
                let Some(id) = resp.id else { continue };
                let sender = p.lock().ok().and_then(|mut m| m.remove(&id));
                if let Some(s) = sender {
                    let r = match resp.error {
                        Some(e) => Err(DriverError::Remote { code: e.code, message: e.message }),
                        None => Ok(resp.result.unwrap_or(Value::Null)),
                    };
                    let _ = s.send(r);
                }
            }
            let message = tail.lock().map(|t| t.trim().to_string()).unwrap_or_default();
            if let Ok(mut m) = p.lock() {
                for (_, s) in m.drain() {
                    let _ = s.send(Err(DriverError::Crashed(message.clone())));
                }
            }
        });
        if let Some(err) = stderr {
            let tail = Arc::clone(&stderr_tail);
            std::thread::spawn(move || {
                for line in BufReader::new(err).lines().map_while(Result::ok) {
                    if let Ok(mut t) = tail.lock() {
                        t.push_str(&line);
                        t.push('\n');
                        if t.len() > 2000 {
                            let cut = t.len() - 2000;
                            let cut = (cut..t.len()).find(|i| t.is_char_boundary(*i)).unwrap_or(t.len());
                            t.drain(..cut);
                        }
                    }
                }
            });
        }
        Ok(DriverClient { child, stdin: Mutex::new(stdin), pending, next_id: Mutex::new(1), stderr_tail })
    }

    /// Ham istek: yanıt ya da hata. Yanıt gelmezse `timeout` sonunda `Timeout`.
    pub fn call_raw(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, DriverError> {
        let id = {
            let mut n = self.next_id.lock().map_err(|_| DriverError::Protocol("poisoned".into()))?;
            let id = *n;
            *n += 1;
            id
        };
        let (tx, rx): (Sender<_>, Receiver<_>) = mpsc::channel();
        self.pending.lock().map_err(|_| DriverError::Protocol("poisoned".into()))?.insert(id, tx);
        let line = serde_json::to_string(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .map_err(|e| DriverError::Protocol(e.to_string()))?;
        let write = {
            let mut w = self.stdin.lock().map_err(|_| DriverError::Protocol("poisoned".into()))?;
            writeln!(w, "{line}").and_then(|_| w.flush())
        };
        if let Err(e) = write {
            self.forget(id);
            return Err(DriverError::Crashed(format!("{e} {}", self.stderr())));
        }
        match rx.recv_timeout(timeout) {
            Ok(r) => r,
            Err(RecvTimeoutError::Timeout) => {
                self.forget(id);
                Err(DriverError::Timeout)
            }
            Err(RecvTimeoutError::Disconnected) => Err(DriverError::Crashed(self.stderr())),
        }
    }

    fn forget(&self, id: u64) {
        if let Ok(mut m) = self.pending.lock() {
            m.remove(&id);
        }
    }

    fn stderr(&self) -> String {
        self.stderr_tail.lock().map(|t| t.trim().to_string()).unwrap_or_default()
    }

    fn call<T: DeserializeOwned>(
        &self,
        method: &str,
        params: impl Serialize,
        timeout: Duration,
    ) -> Result<T, DriverError> {
        let params = serde_json::to_value(params).map_err(|e| DriverError::Protocol(e.to_string()))?;
        let v = self.call_raw(method, params, timeout)?;
        serde_json::from_value(v).map_err(|e| DriverError::Protocol(format!("{method}: {e}")))
    }

    pub fn connect(&self, config: &Connection) -> Result<ConnectResult, DriverError> {
        self.call(METHOD_CONNECT, ConnectParams { config: config.clone() }, METADATA_TIMEOUT)
    }

    pub fn list_schemas(&self) -> Result<Vec<String>, DriverError> {
        self.call(METHOD_LIST_SCHEMAS, json!({}), METADATA_TIMEOUT)
    }

    pub fn list_tables(&self, schema: Option<&str>) -> Result<Vec<TableInfo>, DriverError> {
        self.call(METHOD_LIST_TABLES, json!({"schema": schema}), METADATA_TIMEOUT)
    }

    pub fn columns(&self, table: &str) -> Result<Vec<ColumnInfo>, DriverError> {
        self.call(METHOD_COLUMNS, json!({"table": table}), METADATA_TIMEOUT)
    }

    pub fn primary_key(&self, table: &str) -> Result<Vec<String>, DriverError> {
        self.call(METHOD_PRIMARY_KEY, json!({"table": table}), METADATA_TIMEOUT)
    }

    /// Sorgu; `read_only` salt okunur oturumda çalıştırır (#68).
    pub fn query(&self, sql: &str, read_only: bool, limit: Option<usize>) -> Result<QueryResult, DriverError> {
        self.call(METHOD_QUERY, json!({"sql": sql, "read_only": read_only, "limit": limit}), QUERY_TIMEOUT)
    }

    pub fn table_data(&self, table: &str, limit: usize, offset: usize) -> Result<QueryResult, DriverError> {
        self.call(METHOD_TABLE_DATA, json!({"table": table, "limit": limit, "offset": offset}), QUERY_TIMEOUT)
    }

    /// İfadeleri tek transaction'da çalıştırır; hata olursa hiçbiri uygulanmaz.
    pub fn transaction(&self, statements: &[String]) -> Result<TransactionResult, DriverError> {
        self.call(METHOD_TRANSACTION, json!({"statements": statements}), QUERY_TIMEOUT)
    }

    /// Çalışan sorguyu keser; başka iş parçacığından çağrılabilir.
    pub fn cancel(&self) -> Result<(), DriverError> {
        self.call_raw(METHOD_CANCEL, Value::Null, METADATA_TIMEOUT).map(|_| ())
    }

    /// Nazikçe kapatır; süreç hemen bitmezse öldürülür.
    pub fn close(mut self) {
        let _ = self.call_raw(METHOD_CLOSE, Value::Null, Duration::from_secs(2));
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for DriverClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
