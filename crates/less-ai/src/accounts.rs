//! Claude Code hesapları (karar #63): her hesap ayrı bir `CLAUDE_CONFIG_DIR` (varsayılan `~/.claude`, env verilmez).
//! Kayıt `<veri>/claude_accounts.json` → `{ "current", "accounts": [{ "name", "dir" }] }`. Kayıt yokken LESS (nvim)
//! kaydı okunur, o da yoksa `~/.claude-*` klasörleri hesap sayılır (`~/.claude-work` → `claude-work`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::paths;

/// Bir Claude hesabı.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub name: String,
    pub dir: PathBuf,
}

/// Hesap kaydı.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Accounts {
    pub current: String,
    pub accounts: Vec<Account>,
    #[serde(skip)]
    file: PathBuf,
    #[serde(skip)]
    home: PathBuf,
}

impl Accounts {
    /// Varsayılan konumdan yükler.
    pub fn load() -> Self {
        let file = paths::data_dir().join("claude_accounts.json");
        let legacy = paths::legacy_data("claude_accounts.json");
        Self::load_from(&file, legacy.as_deref(), &paths::home())
    }

    /// Belirli dosya ve ev klasörüyle yükler (test).
    pub fn load_from(file: &Path, legacy: Option<&Path>, home: &Path) -> Self {
        let parsed = std::iter::once(file)
            .chain(legacy)
            .find_map(|p| std::fs::read(p).ok().and_then(|d| serde_json::from_slice::<Accounts>(&d).ok()));
        let mut data = parsed.unwrap_or_else(|| Self::detect(home));
        data.file = file.to_path_buf();
        data.home = home.to_path_buf();
        data
    }

    fn detect(home: &Path) -> Self {
        let default = Account { name: "claude".into(), dir: home.join(".claude") };
        let mut accounts = vec![default];
        let mut extra: Vec<Account> = std::fs::read_dir(home)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter(|e| e.path().is_dir())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.starts_with(".claude-").then(|| Account { name: name[1..].to_string(), dir: e.path() })
            })
            .collect();
        extra.sort_by(|a, b| a.name.cmp(&b.name));
        accounts.extend(extra);
        Self { current: "claude".into(), accounts, file: PathBuf::new(), home: home.to_path_buf() }
    }

    fn default_account(&self) -> Account {
        Account { name: "claude".into(), dir: self.home.join(".claude") }
    }

    pub fn list(&self) -> &[Account] {
        &self.accounts
    }

    /// Birden çok hesap var mı (özet ekranında `@hesap` gösterilir)?
    pub fn is_multi(&self) -> bool {
        self.accounts.len() > 1
    }

    /// Yeni oturumların açılacağı hesap.
    pub fn current(&self) -> Account {
        self.accounts.iter().find(|a| a.name == self.current).cloned().unwrap_or_else(|| self.default_account())
    }

    pub fn is_default(&self, account: &Account) -> bool {
        account.dir == self.home.join(".claude")
    }

    /// Hesabın Claude Code ortamı (varsayılan hesapta boş).
    pub fn env(&self, account: &Account) -> Vec<(String, String)> {
        if self.is_default(account) {
            Vec::new()
        } else {
            vec![("CLAUDE_CONFIG_DIR".into(), account.dir.to_string_lossy().to_string())]
        }
    }

    /// Hesabı seçer ve kaydeder.
    pub fn select(&mut self, name: &str) -> std::io::Result<()> {
        if !self.accounts.iter().any(|a| a.name == name) {
            return Err(std::io::Error::other(format!("Unknown account: {name}")));
        }
        self.current = name.to_string();
        self.save()
    }

    /// Hesap ekler (`dir` boşsa `~/.<ad>`), seçer ve kaydeder.
    pub fn add(&mut self, name: &str, dir: Option<PathBuf>) -> std::io::Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(std::io::Error::other("Account name is empty"));
        }
        if self.accounts.iter().any(|a| a.name == name) {
            return Err(std::io::Error::other(format!("Account already exists: {name}")));
        }
        let dir = dir.unwrap_or_else(|| self.home.join(format!(".{name}")));
        self.accounts.push(Account { name: name.to_string(), dir });
        self.select(name)
    }

    pub fn save(&self) -> std::io::Result<()> {
        let json = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        paths::write_atomic(&self.file, &json)
    }

    /// IDE kilit dosyasını (`<lock_dir>/<port>.lock`) hesabın `ide/` klasörüne symlink'ler; varsayılan hesapta yapılmaz.
    pub fn link_ide(&self, account: &Account, lock_dir: &Path, port: u16) -> std::io::Result<()> {
        if self.is_default(account) {
            return Ok(());
        }
        let dir = account.dir.join("ide");
        std::fs::create_dir_all(&dir)?;
        let dst = dir.join(format!("{port}.lock"));
        if std::fs::symlink_metadata(&dst).is_err() {
            std::os::unix::fs::symlink(lock_dir.join(format!("{port}.lock")), dst)?;
        }
        Ok(())
    }
}
