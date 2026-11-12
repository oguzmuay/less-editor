//! AI'lar arası posta kutusu (karar #65): projede `.less/ai/` (`.gitignore` = `*`), `README.md` (protokol + AI listesi),
//! `inbox/<ad>.md`, `shared/`. inbox değişince o AI'a bekleyen posta işaretlenir; AI boştaysa terminaline bildirim
//! yazılır (Ollama'ya mesajın kendisi). Sonsuz ping-pong'a karşı AI başına 10 dakikada en fazla 6 bildirim.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use notify::{RecursiveMode, Watcher};

use crate::agents::AgentKind;

/// Pencere içinde en fazla bildirim.
pub const NUDGE_LIMIT: usize = 6;
/// Bildirim penceresi (sn).
pub const NUDGE_WINDOW: u64 = 600;

/// Posta kutusunu hazırlar ve README'yi açık AI'larla (`ad`, tür) yeniden yazar.
pub fn prepare(dir: &Path, agents: &[(String, AgentKind)]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir.join("inbox"))?;
    std::fs::create_dir_all(dir.join("shared"))?;
    let ignore = dir.join(".gitignore");
    if !ignore.exists() {
        std::fs::write(&ignore, "*\n")?;
    }
    let mut lines = vec![
        "# LESS AI mailbox".to_string(),
        String::new(),
        "Agents in this editor talk through this folder.".into(),
        String::new(),
        "- Send: append to `inbox/<agent>.md` a section `## from <your name> · <time>` followed by the message.".into(),
        "- Receive: read `inbox/<your name>.md` (your name is in `$LESS_AI_NAME`). New messages are at the end.".into(),
        "- Share files in `shared/`. Ollama agents cannot read files; they only receive messages.".into(),
        String::new(),
        "## Agents".into(),
        String::new(),
    ];
    for (name, kind) in agents {
        lines.push(format!("- `{name}` ({})", kind.label()));
    }
    std::fs::write(dir.join("README.md"), lines.join("\n") + "\n")
}

/// AI başına kayan pencere bildirim sınırı.
#[derive(Debug, Clone)]
pub struct NudgeLimiter {
    limit: usize,
    window: u64,
    hits: HashMap<String, Vec<u64>>,
}

impl Default for NudgeLimiter {
    fn default() -> Self {
        Self::new(NUDGE_LIMIT, NUDGE_WINDOW)
    }
}

impl NudgeLimiter {
    pub fn new(limit: usize, window: u64) -> Self {
        Self { limit, window, hits: HashMap::new() }
    }

    /// `now` (unix sn) anında `name`'e bildirim yapılabilir mi? İzin verilirse sayılır.
    pub fn allow(&mut self, name: &str, now: u64) -> bool {
        let recent = self.hits.entry(name.to_string()).or_default();
        recent.retain(|&t| now.saturating_sub(t) < self.window);
        if recent.len() >= self.limit {
            return false;
        }
        recent.push(now);
        true
    }
}

/// Bildirim adayı: posta kutusu adı olan bir AI sekmesi.
#[derive(Debug, Clone)]
pub struct Recipient {
    pub name: String,
    pub kind: AgentKind,
    pub cwd: PathBuf,
    /// AI boşta (sırası kullanıcıda) mı?
    pub idle: bool,
}

/// Terminale yazılacak bildirim: metin, kısa bir aradan (~80 ms) sonra `\r`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delivery {
    pub name: String,
    pub text: String,
}

type MailHook = Arc<dyn Fn() + Send + Sync>;

/// inbox izleyicileri + bekleyen posta + bildirim sınırı.
pub struct Mailbox {
    watchers: HashMap<PathBuf, notify::RecommendedWatcher>,
    pending: Arc<Mutex<HashSet<String>>>,
    offsets: HashMap<String, usize>,
    limiter: NudgeLimiter,
    hook: Arc<Mutex<Option<MailHook>>>,
}

impl Default for Mailbox {
    fn default() -> Self {
        Self::new()
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Mailbox {
    pub fn new() -> Self {
        Self {
            watchers: HashMap::new(),
            pending: Arc::new(Mutex::new(HashSet::new())),
            offsets: HashMap::new(),
            limiter: NudgeLimiter::default(),
            hook: Arc::new(Mutex::new(None)),
        }
    }

    /// Posta gelince (izleyici iş parçacığından) çağrılacak kanca; UI burada `poll` için uyanır.
    pub fn set_on_mail(&self, hook: impl Fn() + Send + Sync + 'static) {
        *lock(&self.hook) = Some(Arc::new(hook));
    }

    /// `<dir>/inbox` klasörünü izler (değişen `<ad>.md` → o AI'a bekleyen posta). Zaten izleniyorsa bir şey yapmaz.
    pub fn watch(&mut self, dir: &Path) -> notify::Result<()> {
        if self.watchers.contains_key(dir) {
            return Ok(());
        }
        let pending = self.pending.clone();
        let hook = self.hook.clone();
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            let Ok(ev) = res else { return };
            let mut got = false;
            for p in &ev.paths {
                if p.extension().is_some_and(|e| e == "md")
                    && let Some(stem) = p.file_stem()
                {
                    lock(&pending).insert(stem.to_string_lossy().to_string());
                    got = true;
                }
            }
            if got {
                let h = lock(&hook).clone();
                if let Some(h) = h {
                    h();
                }
            }
        })?;
        watcher.watch(&dir.join("inbox"), RecursiveMode::NonRecursive)?;
        self.watchers.insert(dir.to_path_buf(), watcher);
        Ok(())
    }

    /// Bekleyen posta işaretler (izleyici dışı yol ve test).
    pub fn mark_pending(&self, name: &str) {
        lock(&self.pending).insert(name.to_string());
    }

    /// AI'ın okunmamış postası var mı (özet ekranında "✉ new mail")?
    pub fn has_mail(&self, name: &str) -> bool {
        lock(&self.pending).contains(name)
    }

    /// Bekleyen posta var mı (yoksa durum tazelemeye gerek yok)?
    pub fn any_pending(&self) -> bool {
        !lock(&self.pending).is_empty()
    }

    /// Bekleyen postası olan, boştaki ve sınırı aşmamış AI'lar için bildirimleri üretir.
    /// Ollama'ya inbox'ın son iletimden bu yana eklenen kısmı tek satır olarak gider.
    pub fn deliver(&mut self, recipients: &[Recipient], now: u64) -> Vec<Delivery> {
        let mut out = Vec::new();
        for r in recipients {
            if !r.idle || !self.has_mail(&r.name) || !self.limiter.allow(&r.name, now) {
                continue;
            }
            lock(&self.pending).remove(&r.name);
            let inbox = crate::agents::mailbox_dir(&r.cwd).join("inbox").join(format!("{}.md", r.name));
            let text = if r.kind == AgentKind::Ollama {
                let all = std::fs::read_to_string(&inbox).unwrap_or_default();
                let from = self.offsets.get(&r.name).copied().unwrap_or(0).min(all.len());
                let from = (from..=all.len()).find(|&i| all.is_char_boundary(i)).unwrap_or(all.len());
                self.offsets.insert(r.name.clone(), all.len());
                all[from..].split_whitespace().collect::<Vec<_>>().join(" ")
            } else {
                format!("You have new mail in {} — read the new messages at the end and act on them.", inbox.display())
            };
            if !text.is_empty() {
                out.push(Delivery { name: r.name.clone(), text });
            }
        }
        out
    }
}
