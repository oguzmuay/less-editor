//! Tek bir dil sunucusuyla bağlantı: yazıcı ve okuyucu iş parçacıkları.
//!
//! Tasarım (thread tabanlı, tokio yok): her sunucu için
//! - **yazıcı**: kanaldan gelen JSON mesajlarını çerçeveleyip stdin'e yazar (UI iş parçacığı boru dolsa da bloklanmaz);
//! - **okuyucu**: stdout'tan çerçeve okur. Sunucunun *istekleri* (`workspace/configuration`,
//!   `client/registerCapability`, `window/workDoneProgress/create`, jdtls `workspace/executeClientCommand`) burada
//!   hemen yanıtlanır, çünkü sunucu (ör. jdtls) yanıtı beklerken durabilir ve UI'nin `poll()` etmesini beklemek
//!   açılışı geciktirirdi. Diğer her şey ortak kanala gider;
//! - **stderr**: son satırlar saklanır (sunucu kapanınca olayda gösterilir).

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::{Value, json};

use super::ServerKey;
use super::jsonrpc::{Message, read_message, write_message};

const STDERR_TAIL: usize = 40;

/// Okuyucu iş parçacığından yöneticiye giden olaylar.
#[derive(Debug)]
pub(crate) enum Incoming {
    Message(ServerKey, Message),
    /// Sunucu isteği okuyucuda yanıtlandı ama UI'nin de bilmesi gerekiyor (`workspace/applyEdit`, `showMessageRequest`).
    Answered(ServerKey, String, Value),
    Closed(ServerKey),
}

pub(crate) struct Connection {
    writer: Sender<Value>,
    child: Option<Child>,
    stderr: Arc<Mutex<VecDeque<String>>>,
}

impl Connection {
    /// Süreci başlatır (`cwd` = kök, `PATH` = `path`).
    pub fn spawn(
        program: &std::path::Path,
        args: &[String],
        path: &std::ffi::OsStr,
        key: ServerKey,
        settings: Value,
        events: Sender<Incoming>,
    ) -> std::io::Result<Connection> {
        let mut child = Command::new(program)
            .args(args)
            .current_dir(&key.root)
            .env("PATH", path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdin = child.stdin.take().ok_or_else(|| std::io::Error::other("no stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| std::io::Error::other("no stdout"))?;
        let stderr_pipe = child.stderr.take();
        let mut conn = Connection::from_streams(stdout, stdin, key, settings, events);
        if let Some(pipe) = stderr_pipe {
            let tail = Arc::clone(&conn.stderr);
            thread::spawn(move || {
                for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                    let mut t = tail.lock().unwrap_or_else(|e| e.into_inner());
                    if t.len() == STDERR_TAIL {
                        t.pop_front();
                    }
                    t.push_back(line);
                }
            });
        }
        conn.child = Some(child);
        Ok(conn)
    }

    /// Hazır akışlar üstünde bağlantı (testlerde boru, ya da soket).
    pub fn from_streams(
        reader: impl Read + Send + 'static,
        writer: impl Write + Send + 'static,
        key: ServerKey,
        settings: Value,
        events: Sender<Incoming>,
    ) -> Connection {
        let (tx, rx) = channel::<Value>();
        let mut writer = writer;
        thread::spawn(move || {
            for msg in rx {
                if write_message(&mut writer, &msg).is_err() {
                    break;
                }
            }
        });
        let reply = tx.clone();
        thread::spawn(move || {
            let mut reader = BufReader::new(reader);
            while let Ok(Some(value)) = read_message(&mut reader) {
                let Some(msg) = Message::classify(value) else { continue };
                if let Message::Request { id, method, params } = msg {
                    let result = answer(&method, &params, &settings);
                    let _ = reply.send(json!({ "jsonrpc": "2.0", "id": id, "result": result }));
                    if matches!(
                        method.as_str(),
                        "workspace/applyEdit" | "window/showMessageRequest" | "window/showDocument"
                    ) && events.send(Incoming::Answered(key.clone(), method, params)).is_err()
                    {
                        break;
                    }
                    continue;
                }
                if events.send(Incoming::Message(key.clone(), msg)).is_err() {
                    break;
                }
            }
            let _ = events.send(Incoming::Closed(key));
        });
        Connection { writer: tx, child: None, stderr: Arc::default() }
    }

    pub fn send(&self, message: Value) {
        let _ = self.writer.send(message);
    }

    /// Son stderr satırları.
    pub fn stderr_tail(&self) -> Vec<String> {
        self.stderr.lock().map(|t| t.iter().cloned().collect()).unwrap_or_default()
    }

    /// Çıkış kodu; stdout kapandıktan sonra çağrılır, süreç en fazla ~0,5 sn beklenir (UI'yi kilitlememek için).
    pub fn exit_code(&mut self) -> Option<i32> {
        let child = self.child.as_mut()?;
        for _ in 0..50 {
            if let Ok(Some(status)) = child.try_wait() {
                return status.code();
            }
            thread::sleep(std::time::Duration::from_millis(10));
        }
        None
    }

    pub fn kill(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Süreç hâlâ çalışıyor mu (akış bağlantısında her zaman `true`).
    pub fn is_running(&mut self) -> bool {
        self.child.as_mut().is_none_or(|c| matches!(c.try_wait(), Ok(None)))
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.kill();
    }
}

/// Sunucu isteklerine otomatik yanıt.
fn answer(method: &str, params: &Value, settings: &Value) -> Value {
    match method {
        "workspace/configuration" => {
            let items = params.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
            Value::Array(
                items
                    .iter()
                    .map(|item| match item.get("section").and_then(Value::as_str).filter(|s| !s.is_empty()) {
                        Some(section) => {
                            section.split('.').try_fold(settings, |v, k| v.get(k)).cloned().unwrap_or(Value::Null)
                        }
                        None => settings.clone(),
                    })
                    .collect(),
            )
        }
        "workspace/applyEdit" => json!({ "applied": true }),
        "workspace/executeClientCommand" => json!([]),
        "window/showDocument" => json!({ "success": true }),
        "workspace/workspaceFolders" => Value::Null,
        _ => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::answer;
    use serde_json::json;

    #[test]
    fn configuration_sections_resolve_from_settings() {
        let settings = json!({ "java": { "configuration": { "runtimes": [1] } } });
        let params = json!({ "items": [
            { "section": "java.configuration" }, { "section": "missing" }, {}, { "section": "", "scopeUri": "file:///a" }
        ] });
        let all = json!([{ "runtimes": [1] }, null, settings, settings]);
        assert_eq!(answer("workspace/configuration", &params, &settings), all, "empty section = everything (eslint)");
        assert_eq!(answer("workspace/executeClientCommand", &json!({}), &settings), json!([]));
    }
}
