//! Debug adaptörleri: js-debug (`js-debug-adapter` DAP sunucusu) ve java-debug (jdtls üzerinden, `JavaDebugStarter`).
//! Attach yapılandırmaları LESS-nvim `providers/node.lua` / `spring.lua` ile aynı.

use std::io::{BufRead, BufReader};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

use crate::paths;
use crate::ports;

/// java-debug oturumunu jdtls'ten açan kanca. UI / less-lang uygular: jdtls `workspace/executeCommand`
/// `vscode.java.startDebugSession` (dönen değer DAP portu) ve `vscode.java.resolveMainClass`. Çağrılar arka plan
/// iş parçacığından yapılır; jdtls hazır olana kadar (LESS-nvim'de en çok 180 sn) bloklayabilir.
pub trait JavaDebugStarter: Send + Sync {
    /// `file`'ı açıp (jdtls'i başlatıp) hazır olunca `vscode.java.startDebugSession` çağırır; DAP portunu döndürür.
    /// Hata metni kullanıcıya gösterilir (İngilizce).
    fn start_debug_session(&self, file: &Path) -> Result<u16, String>;
    /// `vscode.java.resolveMainClass` ile ana sınıfın `projectName`'i (bulunamazsa `None`).
    fn resolve_project_name(&self, file: &Path, main_class: &str) -> Option<String>;
    /// jdtls kök klasörü (attach `cwd`).
    fn root_dir(&self, file: &Path) -> Option<PathBuf>;
}

/// `js-debug-adapter` konumu: Mason bin klasörleri (LESS-nvim, Neovim), sonra PATH.
pub fn find_js_debug() -> Option<PathBuf> {
    for dir in paths::mason_bin_dirs() {
        let p = dir.join("js-debug-adapter");
        if p.is_file() {
            return Some(p);
        }
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join("js-debug-adapter")).find(|p| p.is_file())
}

/// Çalışan js-debug DAP sunucusu (tek örnek, tüm node oturumları ona bağlanır). Düşürülünce öldürülür.
#[derive(Debug)]
pub struct JsDebugServer {
    child: Child,
    pub addr: SocketAddr,
}

impl JsDebugServer {
    /// `js-debug-adapter <port>` başlatır, "listening" satırını (≤10 sn) bekler.
    pub fn start() -> Result<JsDebugServer, String> {
        let program = find_js_debug()
            .ok_or("js-debug-adapter is not installed (install it with Mason: :MasonInstall js-debug-adapter)")?;
        let port = ports::ephemeral_port().ok_or("No free port for js-debug")?;
        let mut child = Command::new(&program)
            .arg(port.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Could not start js-debug-adapter: {e}"))?;
        let stdout = child.stdout.take().ok_or("js-debug-adapter has no stdout")?;
        let (tx, rx) = mpsc::channel();
        let _ = thread::Builder::new().name("js-debug-out".into()).spawn(move || {
            let mut sent = false;
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if !sent && line.to_lowercase().contains("listening") {
                    let _ = tx.send(());
                    sent = true;
                }
            }
        });
        if rx.recv_timeout(Duration::from_secs(10)).is_err() {
            let _ = child.kill();
            return Err("js-debug-adapter did not start".into());
        }
        let candidates: [SocketAddr; 2] = [([127, 0, 0, 1], port).into(), (std::net::Ipv6Addr::LOCALHOST, port).into()];
        let addr = candidates
            .into_iter()
            .find(|a| TcpStream::connect_timeout(a, Duration::from_millis(500)).is_ok())
            .ok_or("Could not connect to js-debug-adapter")?;
        Ok(JsDebugServer { child, addr })
    }

    /// Süreç hâlâ yaşıyor mu.
    pub fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

impl Drop for JsDebugServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Node `pwa-node` attach yapılandırması. Alt süreçler kendi inspector satırlarıyla yakalandığı için js-debug'ın
/// otomatik alt süreç bağlama özelliği kapalı.
pub fn node_attach_config(label: &str, pid: u32, port: u16, cwd: &Path) -> Value {
    let cwd = cwd.display().to_string();
    json!({
        "type": "pwa-node",
        "request": "attach",
        "name": format!("{label} (pid {pid})"),
        "address": "127.0.0.1",
        "port": port,
        "cwd": cwd,
        "sourceMaps": true,
        "autoAttachChildProcesses": false,
        "skipFiles": ["<node_internals>/**"],
        "resolveSourceMapLocations": [format!("{cwd}/**"), "!**/node_modules/**"],
    })
}

/// java-debug attach yapılandırması (`modulePaths` / `classPaths` / `javaExec` launch zenginleştirmesini atlatmak
/// için dolduruluyor).
pub fn java_attach_config(label: &str, port: u16, root: &Path, main_class: &str, project: &str) -> Value {
    json!({
        "type": "java",
        "request": "attach",
        "name": format!("Debug {label}"),
        "hostName": "127.0.0.1",
        "port": port,
        "cwd": root.display().to_string(),
        "mainClass": main_class,
        "projectName": project,
        "modulePaths": [],
        "classPaths": [],
        "javaExec": "java",
    })
}
