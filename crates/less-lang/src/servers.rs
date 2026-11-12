//! Dil sunucusu kataloğu ve isteğe bağlı kurucu (`:options` → Language Servers).
//!
//! Hiçbir şey kendiliğinden kurulmaz: ağ yalnız kullanıcı Install / Update'e basınca kullanılır. Kurulumlar
//! [`servers_dir`] altına gider (`<dir>/<id>/…`, çalıştırılabilirler `<dir>/bin/` içinde symlink); bu klasör
//! [`crate::LspConfig`]'in arama yollarındadır. PATH'teki ya da Mason'daki ikililer de "kurulu" sayılır.
//! İş arka plan iş parçacığında yürür, ilerleme [`Installer::poll`] ile alınır.

use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::language::Language;
use crate::lsp::LspConfig;

/// Kurulum klasörü: `LESS_RS_SERVERS_DIR` (testler) ya da `~/.local/share/less-rs/servers`.
pub fn servers_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("LESS_RS_SERVERS_DIR") {
        return PathBuf::from(dir);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"));
    home.join(".local/share/less-rs/servers")
}

/// Arşiv türü.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    TarGz,
    Zip,
}

/// Kurulum yöntemi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Method {
    /// `npm install --prefix <dir>/<id> <paketler>`; ikililer `node_modules/.bin/` altında.
    Npm { packages: &'static [&'static str] },
    /// GitHub'daki son sürümün varlığı; API `digest` (SHA-256) verirse doğrulanır.
    GithubRelease { repo: &'static str, asset: &'static str, kind: ArchiveKind },
    /// Doğrudan arşiv adresi; `sha256` verilirse doğrulanır (testler `file://` kullanır).
    Archive { url: &'static str, sha256: Option<&'static str>, kind: ArchiveKind },
    /// `GOBIN=<dir>/<id>/bin go install <modül>@latest`.
    GoInstall { module: &'static str },
    /// `rustup component add <bileşen>`; ikili `rustup which` ile bulunup bağlanır.
    Rustup { component: &'static str },
}

/// Kurulumun ön koşulu (yoksa açık bir hata verilir).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prereq {
    Node,
    Go,
    Rustup,
    Java,
    Python,
}

impl Prereq {
    /// Aranan ikililer (hepsi gerekli).
    fn binaries(self) -> &'static [&'static str] {
        match self {
            Prereq::Node => &["node", "npm"],
            Prereq::Go => &["go"],
            Prereq::Rustup => &["rustup"],
            Prereq::Java => &["java"],
            Prereq::Python => &["python3"],
        }
    }

    fn describe(self) -> &'static str {
        match self {
            Prereq::Node => "Node.js (node and npm) — install with: brew install node",
            Prereq::Go => "Go — install with: brew install go",
            Prereq::Rustup => "rustup — install from https://rustup.rs",
            Prereq::Java => "Java — install with: brew install openjdk",
            Prereq::Python => "Python 3 — install with: brew install python",
        }
    }
}

/// Katalogdaki bir dil sunucusu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    /// Kararlı kimlik (kurulum klasörünün adı).
    pub id: &'static str,
    pub name: &'static str,
    /// Bu kurulumun sağladığı [`crate::lsp::ServerSpec::id`]'ler.
    pub servers: &'static [&'static str],
    pub languages: &'static [Language],
    pub method: Method,
    /// Çalıştırılabilirler: (ad, kurulum klasörüne göreli yol). İlki durum kontrolünde kullanılır.
    pub bins: &'static [(&'static str, &'static str)],
    pub requires: &'static [Prereq],
}

use Language as L;

const WEB: &[Language] = &[L::TypeScript, L::Tsx, L::JavaScript, L::Jsx];

static CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        id: "vtsls",
        name: "TypeScript / JavaScript (vtsls)",
        servers: &["vtsls"],
        languages: WEB,
        method: Method::Npm { packages: &["@vtsls/language-server"] },
        bins: &[("vtsls", "node_modules/.bin/vtsls")],
        requires: &[Prereq::Node],
    },
    CatalogEntry {
        id: "vscode-langservers",
        name: "CSS, HTML, JSON, ESLint (vscode-langservers-extracted)",
        servers: &["cssls", "html", "jsonls", "eslint"],
        languages: &[L::Css, L::Html, L::Json, L::TypeScript, L::Tsx, L::JavaScript, L::Jsx],
        method: Method::Npm { packages: &["vscode-langservers-extracted"] },
        bins: &[
            ("vscode-css-language-server", "node_modules/.bin/vscode-css-language-server"),
            ("vscode-html-language-server", "node_modules/.bin/vscode-html-language-server"),
            ("vscode-json-language-server", "node_modules/.bin/vscode-json-language-server"),
            ("vscode-eslint-language-server", "node_modules/.bin/vscode-eslint-language-server"),
        ],
        requires: &[Prereq::Node],
    },
    CatalogEntry {
        id: "tailwindcss",
        name: "Tailwind CSS",
        servers: &["tailwindcss"],
        languages: &[L::Css, L::Html, L::TypeScript, L::Tsx, L::JavaScript, L::Jsx],
        method: Method::Npm { packages: &["@tailwindcss/language-server"] },
        bins: &[("tailwindcss-language-server", "node_modules/.bin/tailwindcss-language-server")],
        requires: &[Prereq::Node],
    },
    CatalogEntry {
        id: "jdtls",
        name: "Java (Eclipse JDT LS)",
        servers: &["jdtls"],
        languages: &[L::Java],
        method: Method::Archive {
            url: "https://download.eclipse.org/jdtls/snapshots/jdt-language-server-latest.tar.gz",
            sha256: None,
            kind: ArchiveKind::TarGz,
        },
        bins: &[("jdtls", "bin/jdtls")],
        requires: &[Prereq::Java, Prereq::Python],
    },
    CatalogEntry {
        id: "kotlin-language-server",
        name: "Kotlin (kotlin-language-server)",
        servers: &["kotlin_ls"],
        languages: &[L::Kotlin],
        method: Method::GithubRelease {
            repo: "fwcd/kotlin-language-server",
            asset: "server.zip",
            kind: ArchiveKind::Zip,
        },
        bins: &[("kotlin-language-server", "server/bin/kotlin-language-server")],
        requires: &[Prereq::Java],
    },
    CatalogEntry {
        id: "rust-analyzer",
        name: "Rust (rust-analyzer)",
        servers: &["rust_analyzer"],
        languages: &[L::Rust],
        method: Method::Rustup { component: "rust-analyzer" },
        bins: &[("rust-analyzer", "")],
        requires: &[Prereq::Rustup],
    },
    CatalogEntry {
        id: "gopls",
        name: "Go (gopls)",
        servers: &["gopls"],
        languages: &[L::Go],
        method: Method::GoInstall { module: "golang.org/x/tools/gopls" },
        bins: &[("gopls", "bin/gopls")],
        requires: &[Prereq::Go],
    },
    CatalogEntry {
        id: "basedpyright",
        name: "Python (basedpyright)",
        servers: &["basedpyright"],
        languages: &[L::Python],
        method: Method::Npm { packages: &["basedpyright"] },
        bins: &[
            ("basedpyright-langserver", "node_modules/.bin/basedpyright-langserver"),
            ("basedpyright", "node_modules/.bin/basedpyright"),
        ],
        requires: &[Prereq::Node],
    },
];

/// Tüm katalog.
pub fn catalog() -> &'static [CatalogEntry] {
    CATALOG
}

/// Bir sunucu kimliğinin (`ServerSpec::id`) katalog girdisi.
pub fn entry_for_server(server: &str) -> Option<&'static CatalogEntry> {
    CATALOG.iter().find(|e| e.servers.contains(&server))
}

/// Kurulum durumu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallStatus {
    /// `managed` = bizim kurduğumuz (kaldırılabilir); değilse PATH / Mason'dan bulundu.
    Installed {
        path: PathBuf,
        managed: bool,
    },
    NotInstalled,
}

impl InstallStatus {
    pub fn is_installed(&self) -> bool {
        matches!(self, InstallStatus::Installed { .. })
    }
}

/// Girdinin durumu: önce kendi kurulumumuz, sonra PATH + Mason (`config`). rustup'ta `rustup which` (proxy
/// ikilisi bileşen yokken de PATH'te durur).
pub fn status(entry: &CatalogEntry, dir: &Path, config: &LspConfig) -> InstallStatus {
    let Some(&(bin, rel)) = entry.bins.first() else { return InstallStatus::NotInstalled };
    let ours = dir.join(entry.id).join(rel);
    if !rel.is_empty() && is_executable(&ours) {
        return InstallStatus::Installed { path: ours, managed: true };
    }
    let path_dirs = &config.path_dirs;
    if let Method::Rustup { component } = entry.method
        && let Some(rustup) = find_in(path_dirs, "rustup")
    {
        let managed = dir.join("bin").join(bin).exists();
        return match capture(Command::new(rustup).args(["which", component]), path_dirs) {
            Ok(out) => InstallStatus::Installed { path: PathBuf::from(out.trim()), managed },
            Err(_) => InstallStatus::NotInstalled,
        };
    }
    let own_bin = dir.join("bin");
    let found = path_dirs
        .iter()
        .chain(config.extra_bin_dirs.iter().filter(|d| **d != own_bin))
        .map(|d| d.join(bin))
        .find(|p| is_executable(p));
    match found {
        Some(path) => InstallStatus::Installed { path, managed: false },
        None => InstallStatus::NotInstalled,
    }
}

/// Kurucu işi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Job {
    Install,
    Update,
    Uninstall,
}

/// [`Installer::poll`] olayları.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallEvent {
    /// `percent` yalnız indirmede bilinir.
    Progress {
        id: &'static str,
        percent: Option<u8>,
        message: String,
    },
    Finished {
        id: &'static str,
        job: Job,
        result: Result<(), String>,
    },
}

/// Arka planda kurar / günceller / kaldırır.
pub struct Installer {
    dir: PathBuf,
    path_dirs: Vec<PathBuf>,
    /// Durum kontrolünde PATH'ten sonra bakılan klasörler (Mason).
    extra_dirs: Vec<PathBuf>,
    tx: Sender<InstallEvent>,
    rx: Receiver<InstallEvent>,
    running: HashSet<&'static str>,
}

impl Installer {
    /// `dir` kurulum klasörü, `path_dirs` ön koşulların arandığı ve süreçlere verilen PATH (Mason'a bakılmaz).
    pub fn new(dir: PathBuf, path_dirs: Vec<PathBuf>) -> Installer {
        let (tx, rx) = channel();
        Installer { extra_dirs: vec![dir.join("bin")], dir, path_dirs, tx, rx, running: HashSet::new() }
    }

    /// [`servers_dir`] + [`LspConfig::default`]'un PATH'i ve Mason klasörleri.
    pub fn from_env() -> Installer {
        let config = LspConfig::default();
        let mut inst = Installer::new(servers_dir(), config.path_dirs);
        inst.extra_dirs = config.extra_bin_dirs;
        inst
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Durum kontrolü için bu kurucunun klasörü ve PATH'iyle [`LspConfig`].
    pub fn config(&self) -> LspConfig {
        LspConfig { path_dirs: self.path_dirs.clone(), extra_bin_dirs: self.extra_dirs.clone(), ..LspConfig::default() }
    }

    pub fn is_running(&self, id: &str) -> bool {
        self.running.contains(id)
    }

    /// İşi başlatır. Ön koşul eksikse ya da aynı girdi için iş sürüyorsa hemen `Err`.
    pub fn start(&mut self, entry: &CatalogEntry, job: Job) -> Result<(), String> {
        if self.running.contains(entry.id) {
            return Err(format!("{} is already being processed", entry.name));
        }
        if job != Job::Uninstall {
            self.check_prereqs(entry)?;
        }
        self.running.insert(entry.id);
        let (entry, dir, path_dirs, tx) = (entry.clone(), self.dir.clone(), self.path_dirs.clone(), self.tx.clone());
        std::thread::spawn(move || {
            let work = Work { entry: &entry, dir: &dir, path_dirs: &path_dirs, tx: &tx };
            let result = match job {
                Job::Install | Job::Update => work.install(job == Job::Update),
                Job::Uninstall => work.uninstall(),
            };
            let _ = tx.send(InstallEvent::Finished { id: entry.id, job, result });
        });
        Ok(())
    }

    fn check_prereqs(&self, entry: &CatalogEntry) -> Result<(), String> {
        let missing: Vec<&str> = entry
            .requires
            .iter()
            .filter(|p| p.binaries().iter().any(|b| find_in(&self.path_dirs, b).is_none()))
            .map(|p| p.describe())
            .collect();
        if missing.is_empty() {
            return Ok(());
        }
        Err(format!("{} needs {}", entry.name, missing.join("; ")))
    }

    /// Olayları bloklamadan toplar.
    pub fn poll(&mut self) -> Vec<InstallEvent> {
        let events: Vec<InstallEvent> = self.rx.try_iter().collect();
        for e in &events {
            if let InstallEvent::Finished { id, .. } = e {
                self.running.remove(id);
            }
        }
        events
    }
}

/// İş parçacığında yürüyen tek iş.
struct Work<'a> {
    entry: &'a CatalogEntry,
    dir: &'a Path,
    path_dirs: &'a [PathBuf],
    tx: &'a Sender<InstallEvent>,
}

impl Work<'_> {
    fn progress(&self, percent: Option<u8>, message: impl Into<String>) {
        let _ = self.tx.send(InstallEvent::Progress { id: self.entry.id, percent, message: message.into() });
    }

    fn tool(&self, name: &str) -> Result<PathBuf, String> {
        find_in(self.path_dirs, name).ok_or_else(|| format!("{name} not found"))
    }

    fn install(&self, update: bool) -> Result<(), String> {
        let home = self.dir.join(self.entry.id);
        std::fs::create_dir_all(self.dir.join("bin")).map_err(|e| e.to_string())?;
        match &self.entry.method {
            Method::Npm { packages } => {
                self.progress(None, "Running npm install…");
                let pkgs = packages.iter().map(|p| if update { format!("{p}@latest") } else { p.to_string() });
                let mut cmd = Command::new(self.tool("npm")?);
                cmd.arg("install").arg("--prefix").arg(&home).args(["--no-audit", "--no-fund"]).args(pkgs);
                capture(&mut cmd, self.path_dirs)?;
            }
            Method::GoInstall { module } => {
                self.progress(None, "Running go install…");
                let mut cmd = Command::new(self.tool("go")?);
                cmd.arg("install").arg(format!("{module}@latest")).env("GOBIN", home.join("bin"));
                capture(&mut cmd, self.path_dirs)?;
            }
            Method::Rustup { component } => {
                self.progress(None, "Running rustup component add…");
                let rustup = self.tool("rustup")?;
                capture(Command::new(&rustup).args(["component", "add", component]), self.path_dirs)?;
                let path = capture(Command::new(&rustup).args(["which", component]), self.path_dirs)?;
                let (bin, _) = self.entry.bins[0];
                link(Path::new(path.trim()), &self.dir.join("bin").join(bin))?;
                return Ok(());
            }
            Method::GithubRelease { repo, asset, kind } => {
                self.progress(Some(0), "Looking up the latest release…");
                let api = format!("https://api.github.com/repos/{repo}/releases/latest");
                let json = capture(Command::new(self.tool("curl")?).args(["-fsSL", &api]), self.path_dirs)?;
                let (url, sha) = release_asset(&json, asset).ok_or_else(|| format!("No {asset} in {repo} release"))?;
                self.archive(&url, sha.as_deref(), *kind, &home)?;
            }
            Method::Archive { url, sha256, kind } => self.archive(url, *sha256, *kind, &home)?,
        }
        for (name, rel) in self.entry.bins {
            let target = home.join(rel);
            if !target.exists() {
                return Err(format!("Installed, but {rel} is missing"));
            }
            make_executable(&target);
            link(&target, &self.dir.join("bin").join(name))?;
        }
        self.progress(Some(100), "Done");
        Ok(())
    }

    /// İndirir (yüzde curl'ün ilerleme çubuğundan), SHA-256'yı doğrular, açar ve `home`'un yerine koyar.
    fn archive(&self, url: &str, sha256: Option<&str>, kind: ArchiveKind, home: &Path) -> Result<(), String> {
        let tmp = self.dir.join(format!(".{}.tmp", self.entry.id));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("out")).map_err(|e| e.to_string())?;
        let file = tmp.join("download");
        self.progress(Some(0), "Downloading…");
        let mut child = Command::new(self.tool("curl")?)
            .args(["-fL", "--progress-bar", "-o"])
            .arg(&file)
            .arg(url)
            .env("PATH", join_path(self.path_dirs))
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("curl: {e}"))?;
        let mut stderr = child.stderr.take().map(std::io::BufReader::new);
        let mut last = String::new();
        let mut buf = [0u8; 256];
        let mut shown = None;
        while let Some(Ok(n @ 1..)) = stderr.as_mut().map(|s| s.read(&mut buf)) {
            last.push_str(&String::from_utf8_lossy(&buf[..n]));
            if let Some(p) = curl_percent(&last).filter(|p| Some(*p) != shown) {
                shown = Some(p);
                self.progress(Some((p as u32 * 9 / 10) as u8), format!("Downloading… {p}%"));
            }
            if last.len() > 4096 {
                last.drain(..last.len() - 512);
            }
        }
        let status = child.wait().map_err(|e| e.to_string())?;
        if !status.success() {
            let why = last.lines().map(str::trim).rfind(|l| l.starts_with("curl:")).unwrap_or("download failed");
            return Err(format!("Download failed: {why}"));
        }
        if let Some(want) = sha256 {
            let bytes = std::fs::read(&file).map_err(|e| e.to_string())?;
            let got = format!("{:x}", Sha256::digest(&bytes));
            if !got.eq_ignore_ascii_case(want.trim()) {
                let _ = std::fs::remove_dir_all(&tmp);
                return Err(format!("Checksum mismatch (expected {want}, got {got})"));
            }
        }
        self.progress(Some(92), "Extracting…");
        let out = tmp.join("out");
        let mut cmd = match kind {
            ArchiveKind::TarGz => {
                let mut c = Command::new(self.tool("tar")?);
                c.arg("-xzf").arg(&file).arg("-C").arg(&out);
                c
            }
            ArchiveKind::Zip => {
                let mut c = Command::new(self.tool("unzip")?);
                c.arg("-q").arg("-o").arg(&file).arg("-d").arg(&out);
                c
            }
        };
        capture(&mut cmd, self.path_dirs)?;
        let _ = std::fs::remove_dir_all(home);
        std::fs::rename(&out, home).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_dir_all(&tmp);
        Ok(())
    }

    fn uninstall(&self) -> Result<(), String> {
        let home = self.dir.join(self.entry.id);
        for (name, _) in self.entry.bins {
            let link = self.dir.join("bin").join(name);
            if link.symlink_metadata().is_ok() {
                std::fs::remove_file(&link).map_err(|e| e.to_string())?;
            }
        }
        if let Method::Rustup { component } = self.entry.method {
            self.progress(None, "Running rustup component remove…");
            capture(Command::new(self.tool("rustup")?).args(["component", "remove", component]), self.path_dirs)?;
        }
        if home.exists() {
            std::fs::remove_dir_all(&home).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

/// GitHub `releases/latest` yanıtından varlığın adresi ve (API verdiyse) SHA-256'sı.
pub fn release_asset(json: &str, asset: &str) -> Option<(String, Option<String>)> {
    let v: Value = serde_json::from_str(json).ok()?;
    let a = v["assets"].as_array()?.iter().find(|a| a["name"] == asset)?;
    let url = a["browser_download_url"].as_str()?.to_string();
    let sha = a["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).map(str::to_string);
    Some((url, sha))
}

/// curl `--progress-bar` çıktısındaki son yüzde (`###   45.2%`).
fn curl_percent(text: &str) -> Option<u8> {
    let tail = text.rsplit(['\r', '\n']).find(|s| s.trim_end().ends_with('%'))?;
    let num = tail.trim_end().trim_end_matches('%').rsplit(' ').next()?;
    num.parse::<f32>().ok().map(|p| p.clamp(0.0, 100.0) as u8)
}

fn join_path(dirs: &[PathBuf]) -> std::ffi::OsString {
    std::env::join_paths(dirs).unwrap_or_default()
}

/// Komutu çalıştırır; başarıda stdout, hatada stderr'in son satırları.
fn capture(cmd: &mut Command, path_dirs: &[PathBuf]) -> Result<String, String> {
    let program = cmd.get_program().to_string_lossy().into_owned();
    let out = cmd.env("PATH", join_path(path_dirs)).stdin(Stdio::null()).output();
    let out = out.map_err(|e| format!("{program}: {e}"))?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let lines: Vec<&str> = err.lines().filter(|l| !l.trim().is_empty()).collect();
    let tail = lines[lines.len().saturating_sub(3)..].join(" / ");
    let name = Path::new(&program).file_name().map_or(program.clone(), |n| n.to_string_lossy().into_owned());
    Err(format!("{name} failed ({}): {tail}", out.status.code().map_or("signal".into(), |c| c.to_string())))
}

fn find_in(dirs: &[PathBuf], name: &str) -> Option<PathBuf> {
    dirs.iter().map(|d| d.join(name)).find(|p| is_executable(p))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(m) = path.metadata()
        && m.is_file()
        && m.permissions().mode() & 0o111 == 0
    {
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(m.permissions().mode() | 0o755));
    }
}

fn link(target: &Path, at: &Path) -> Result<(), String> {
    if at.symlink_metadata().is_ok() {
        std::fs::remove_file(at).map_err(|e| e.to_string())?;
    }
    std::os::unix::fs::symlink(target, at).map_err(|e| format!("Could not link {}: {e}", at.display()))
}

#[cfg(test)]
mod tests;
