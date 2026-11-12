//! Workspace kaydı: `<data>/workspaces.json` → `{version:1, workspaces:[{name, roots, opened}]}` (LESS-nvim ile aynı
//! biçim). Kendi dosyası yoksa LESS-nvim'in kaydı okunur; ilk yazmada LESS-rs'in dosyasına kopyalanmış olur.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub name: String,
    #[serde(default)]
    pub roots: Vec<PathBuf>,
    /// Son açılış (Unix saniyesi).
    #[serde(default)]
    pub opened: i64,
}

#[derive(Serialize, Deserialize)]
struct File {
    version: u32,
    #[serde(default)]
    workspaces: Vec<Workspace>,
}

/// Workspace kaydı; `path` testte geçici dosyaya yönlendirilebilir.
pub struct Store {
    path: PathBuf,
    pub items: Vec<Workspace>,
}

impl Store {
    /// LESS-rs kaydını, yoksa LESS-nvim kaydını okur.
    pub fn load() -> Self {
        let own = paths::data_dir().join("workspaces.json");
        let source = if own.is_file() { Some(own.clone()) } else { paths::legacy("share", "workspaces.json") };
        let mut store = Self::open(&own);
        if let Some(src) = source.filter(|s| *s != own) {
            store.items = read(&src);
        }
        store
    }

    /// Verilen dosyayı kayıt olarak açar (yoksa boş).
    pub fn open(path: &Path) -> Self {
        Self { path: path.to_path_buf(), items: read(path) }
    }

    pub fn get(&self, name: &str) -> Option<&Workspace> {
        self.items.iter().find(|w| w.name == name)
    }

    /// Büyük/küçük harf duyarsız ad çakışması (LESS-nvim'deki `workspaces.existing` gibi).
    pub fn exists(&self, name: &str) -> bool {
        self.items.iter().any(|w| w.name.eq_ignore_ascii_case(name.trim()))
    }

    /// Yeni workspace ekler ve kaydeder. Hata metni kullanıcıya gösterilir (İngilizce).
    pub fn create(&mut self, name: &str, roots: Vec<PathBuf>, now: i64) -> Result<Workspace, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Enter a workspace name.".into());
        }
        if name.contains('/') {
            return Err("Workspace names cannot contain '/'.".into());
        }
        if self.exists(name) {
            return Err(format!("A workspace named \"{name}\" already exists."));
        }
        if roots.is_empty() {
            return Err("Add at least one folder.".into());
        }
        let ws = Workspace { name: name.to_owned(), roots, opened: now };
        self.items.push(ws.clone());
        self.save().map_err(|e| format!("Could not save workspaces: {e}"))?;
        Ok(ws)
    }

    /// Açılış zamanını günceller ve kaydeder.
    pub fn touch(&mut self, name: &str, now: i64) -> std::io::Result<()> {
        if let Some(w) = self.items.iter_mut().find(|w| w.name == name) {
            w.opened = now;
        }
        self.save()
    }

    fn save(&self) -> std::io::Result<()> {
        let file = File { version: 1, workspaces: self.items.clone() };
        let json = serde_json::to_vec_pretty(&file).map_err(std::io::Error::other)?;
        paths::write_atomic(&self.path, &json)
    }
}

fn read(path: &Path) -> Vec<Workspace> {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<File>(&b).ok())
        .map(|f| f.workspaces)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("less-rs-ws-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("workspaces.json")
    }

    #[test]
    fn create_validates_and_persists() {
        let path = temp("create");
        let mut store = Store::open(&path);
        assert!(store.create("", vec![PathBuf::from("/a")], 1).is_err());
        assert!(store.create("x", vec![], 1).is_err());
        store.create("Api", vec![PathBuf::from("/a"), PathBuf::from("/b")], 5).unwrap();
        assert!(store.create("api", vec![PathBuf::from("/c")], 6).is_err());
        let reopened = Store::open(&path);
        assert_eq!(reopened.get("Api").unwrap().roots.len(), 2);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
