//! Pty içinde süreç: renkli çıktı için gerçek terminal, ham bayt akışı, süreç grubuna sinyal.
//!
//! Süreç `portable-pty` ile kendi oturumunun lideri olarak başlar (setsid), bu yüzden süreç grubu no = pid; mvn → JVM,
//! npm → node gibi alt süreçler aynı gruptadır ve grup sinyali hepsine gider.

use std::io::{self, Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use nix::sys::signal::{Signal, kill, killpg};
use nix::unistd::Pid;
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};

use crate::command::CommandSpec;

/// Süreç çıkış bilgisi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitInfo {
    pub code: u32,
    /// Sinyalle öldüyse sinyal adı.
    pub signal: Option<String>,
}

impl ExitInfo {
    /// Başarılı çıkış (kod 0, sinyal yok).
    pub fn success(&self) -> bool {
        self.code == 0 && self.signal.is_none()
    }
}

/// Arka plan iş parçacıklarından gelen mesaj. `Exit` her zaman son mesajdır ve çıktının tamamından sonra gelir
/// (çıktı kapanmazsa en çok 500 ms beklenir).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcMsg {
    Output(Vec<u8>),
    Exit(ExitInfo),
}

/// Gönderilecek sinyal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sig {
    Int,
    Hup,
    Term,
    Kill,
}

impl Sig {
    fn nix(self) -> Signal {
        match self {
            Sig::Int => Signal::SIGINT,
            Sig::Hup => Signal::SIGHUP,
            Sig::Term => Signal::SIGTERM,
            Sig::Kill => Signal::SIGKILL,
        }
    }
}

/// Çalışan pty süreci. Düşürülünce süreç öldürülmez (yalnız pty kapanır → SIGHUP); önce `stop`/`signal` kullanın.
pub struct PtyProcess {
    pid: u32,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    exited: Arc<AtomicBool>,
}

impl std::fmt::Debug for PtyProcess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PtyProcess").field("pid", &self.pid).finish()
    }
}

/// Komutu pty'de başlatır; çıktı ve çıkış `on_msg` ile (arka plan iş parçacığından) bildirilir.
pub fn spawn(
    spec: &CommandSpec,
    rows: u16,
    cols: u16,
    on_msg: impl Fn(ProcMsg) + Send + Sync + 'static,
) -> io::Result<PtyProcess> {
    let pty = native_pty_system();
    let pair = pty.openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }).map_err(io::Error::other)?;
    let Some((program, rest)) = spec.argv.split_first() else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty command"));
    };
    let mut cmd = CommandBuilder::new(program);
    cmd.args(rest);
    cmd.cwd(&spec.cwd);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    for (k, v) in &spec.env {
        cmd.env(k, v);
    }
    let mut child = pair.slave.spawn_command(cmd).map_err(io::Error::other)?;
    drop(pair.slave);
    let pid = child.process_id().ok_or_else(|| io::Error::other("no pid"))?;
    let mut reader = pair.master.try_clone_reader().map_err(io::Error::other)?;
    let writer = pair.master.take_writer().map_err(io::Error::other)?;
    let exited = Arc::new(AtomicBool::new(false));
    let on_msg = Arc::new(on_msg);
    let (done_tx, done_rx) = mpsc::channel::<()>();
    let out = on_msg.clone();
    thread::Builder::new().name(format!("run-read-{pid}")).spawn(move || {
        let mut buf = [0u8; 16 * 1024];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => out(ProcMsg::Output(buf[..n].to_vec())),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
        let _ = done_tx.send(());
    })?;
    let flag = exited.clone();
    thread::Builder::new().name(format!("run-wait-{pid}")).spawn(move || {
        let status = child.wait();
        let _ = done_rx.recv_timeout(Duration::from_millis(500));
        flag.store(true, Ordering::SeqCst);
        let info = match status {
            Ok(s) => ExitInfo { code: s.exit_code(), signal: s.signal().map(String::from) },
            Err(_) => ExitInfo { code: 1, signal: None },
        };
        on_msg(ProcMsg::Exit(info));
    })?;
    Ok(PtyProcess { pid, master: pair.master, writer, exited })
}

impl PtyProcess {
    /// Sürecin (ve süreç grubunun) pid'i.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Süreç bitti mi.
    pub fn has_exited(&self) -> bool {
        self.exited.load(Ordering::SeqCst)
    }

    /// Terminale (sürecin stdin'ine) yazar.
    pub fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.writer.write_all(bytes)?;
        self.writer.flush()
    }

    /// Pty boyutunu değiştirir (log görünümü yeniden boyutlanınca).
    pub fn resize(&self, rows: u16, cols: u16) -> io::Result<()> {
        self.master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }).map_err(io::Error::other)
    }

    /// Süreç grubuna sinyal gönderir; grup yoksa yalnız sürece.
    pub fn signal(&self, sig: Sig) {
        signal_group(self.pid, sig);
    }

    /// Nazik durdurma (Neovim `jobstop` gibi): SIGHUP, 2 sn sonra hâlâ yaşıyorsa SIGTERM, 2 sn daha sonra SIGKILL.
    pub fn stop(&self) {
        self.signal(Sig::Hup);
        let (pid, flag) = (self.pid, self.exited.clone());
        let _ = thread::Builder::new().name(format!("run-stop-{pid}")).spawn(move || {
            for sig in [Sig::Term, Sig::Kill] {
                for _ in 0..20 {
                    if flag.load(Ordering::SeqCst) {
                        return;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
                signal_group(pid, sig);
            }
        });
    }

    /// Süreç grubunu ve süreci SIGKILL ile öldürür (#80 `k` / `K`).
    pub fn force_kill(&self) {
        signal_group(self.pid, Sig::Kill);
    }
}

/// `pid`'in süreç grubuna sinyal; grup sinyali başarısızsa sürecin kendisine.
pub fn signal_group(pid: u32, sig: Sig) {
    let Ok(raw) = i32::try_from(pid) else { return };
    let p = Pid::from_raw(raw);
    if killpg(p, sig.nix()).is_err() || sig == Sig::Kill {
        let _ = kill(p, sig.nix());
    }
}
