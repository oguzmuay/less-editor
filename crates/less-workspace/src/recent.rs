//! Son açılan projeler / workspace'ler: `<state>/recents.json` → `{version:1, items:[{kind, dir, name?, time}]}`
//! (LESS-nvim ile aynı biçim). LESS-rs'in dosyası yoksa LESS-nvim'in kaydı okunur (tek yönlü içe aktarma); ilk
//! yazmada LESS-rs'in kendi dosyasına geçilir, LESS-nvim'in dosyasına hiç yazılmaz.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::paths;
use crate::workspaces;

const LIMIT: usize = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Project,
    Workspace,
}

/// Listede gösterilecek tek kayıt.
#[derive(Clone, Debug)]
pub struct Recent {
    pub kind: Kind,
    pub name: String,
    pub dir: PathBuf,
    /// Workspace'in kök klasörleri (proje için boş).
    pub roots: Vec<PathBuf>,
    /// Son açılış, Unix saniyesi.
    pub time: i64,
}

#[derive(Clone, Serialize, Deserialize)]
struct Item {
    #[serde(default = "one")]
    version: u32,
    kind: Kind,
    dir: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default)]
    time: i64,
}

fn one() -> u32 {
    1
}

impl Item {
    fn name(&self) -> String {
        self.name.clone().unwrap_or_else(|| file_name(&self.dir))
    }

    /// Proje klasörüyle, workspace adıyla tanınır.
    fn same(&self, kind: Kind, key: &str) -> bool {
        self.kind == kind
            && match kind {
                Kind::Project => self.dir == Path::new(key),
                Kind::Workspace => self.name() == key,
            }
    }
}

#[derive(Serialize, Deserialize)]
struct File {
    #[serde(default = "one")]
    version: u32,
    #[serde(default)]
    items: Vec<Item>,
}

pub struct Store {
    path: PathBuf,
    items: Vec<Item>,
}

impl Store {
    /// LESS-rs kaydını, yoksa LESS-nvim kaydını okur.
    pub fn load() -> Self {
        let own = paths::state_dir().join("recents.json");
        let mut store = Self::open(&own);
        if !own.is_file()
            && let Some(src) = paths::legacy("state", "recents.json")
        {
            store.items = read(&src);
        }
        store
    }

    pub fn open(path: &Path) -> Self {
        Self { path: path.to_path_buf(), items: read(path) }
    }

    /// Kayıtları en yeniden eskiye döndürür; workspace kökleri `ws` kaydından doldurulur.
    pub fn entries(&self, ws: &workspaces::Store) -> Vec<Recent> {
        let mut out: Vec<Recent> = self
            .items
            .iter()
            .map(|it| {
                let name = it.name();
                let roots = match it.kind {
                    Kind::Workspace => ws.get(&name).map(|w| w.roots.clone()).unwrap_or_default(),
                    Kind::Project => Vec::new(),
                };
                Recent { kind: it.kind, name, dir: it.dir.clone(), roots, time: it.time }
            })
            .collect();
        out.sort_by_key(|r| std::cmp::Reverse(r.time));
        out
    }

    /// Projeyi listenin başına koyar (aynısı varsa yer değiştirir) ve kaydeder.
    pub fn add_project(&mut self, dir: &Path, now: i64) -> std::io::Result<()> {
        let key = dir.to_string_lossy().into_owned();
        self.push(
            Kind::Project,
            &key,
            Item { version: 1, kind: Kind::Project, dir: dir.to_path_buf(), name: None, time: now },
        )
    }

    /// Workspace'i listenin başına koyar. `dir` LESS-nvim'deki gibi workspace'in (sanal) klasörüdür.
    pub fn add_workspace(&mut self, name: &str, now: i64) -> std::io::Result<()> {
        let dir = paths::data_dir().join("workspaces").join(name);
        let item = Item { version: 1, kind: Kind::Workspace, dir, name: Some(name.to_owned()), time: now };
        self.push(Kind::Workspace, name, item)
    }

    /// Kaydı listeden çıkarır (klasöre dokunmaz). `key` proje için klasör yolu, workspace için ad.
    pub fn remove(&mut self, kind: Kind, key: &str) -> std::io::Result<()> {
        self.items.retain(|it| !it.same(kind, key));
        self.save()
    }

    fn push(&mut self, kind: Kind, key: &str, item: Item) -> std::io::Result<()> {
        self.items.retain(|it| !it.same(kind, key));
        self.items.insert(0, item);
        self.items.sort_by_key(|it| std::cmp::Reverse(it.time));
        self.items.truncate(LIMIT);
        self.save()
    }

    fn save(&self) -> std::io::Result<()> {
        let file = File { version: 1, items: self.items.clone() };
        let json = serde_json::to_vec(&file).map_err(std::io::Error::other)?;
        paths::write_atomic(&self.path, &json)
    }
}

fn read(path: &Path) -> Vec<Item> {
    std::fs::read(path).ok().and_then(|b| serde_json::from_slice::<File>(&b).ok()).map(|f| f.items).unwrap_or_default()
}

fn file_name(dir: &Path) -> String {
    dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| dir.display().to_string())
}

/// Ev klasörünü `~` ile kısaltır.
pub fn display_path(path: &Path) -> String {
    match path.strip_prefix(paths::home()) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

/// "just now", "5m ago", "3h ago", "2d ago", "4w ago" biçiminde göreli zaman.
pub fn relative_time(time: i64, now: i64) -> String {
    let secs = (now - time).max(0);
    match secs {
        0..60 => "just now".to_owned(),
        60..3600 => format!("{}m ago", secs / 60),
        3600..86_400 => format!("{}h ago", secs / 3600),
        86_400..604_800 => format!("{}d ago", secs / 86_400),
        _ => format!("{}w ago", secs / 604_800),
    }
}

/// Şu anki Unix saniyesi.
pub fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_time_buckets() {
        assert_eq!(relative_time(100, 110), "just now");
        assert_eq!(relative_time(0, 300), "5m ago");
        assert_eq!(relative_time(0, 7200), "2h ago");
        assert_eq!(relative_time(0, 3 * 86_400), "3d ago");
        assert_eq!(relative_time(0, 15 * 86_400), "2w ago");
    }

    #[test]
    fn add_moves_to_front_and_remove_drops() {
        let dir = std::env::temp_dir().join(format!("less-rs-recent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("recents.json");
        let ws = workspaces::Store::open(&dir.join("workspaces.json"));
        let mut store = Store::open(&path);
        store.add_project(Path::new("/p/a"), 1).unwrap();
        store.add_project(Path::new("/p/b"), 2).unwrap();
        store.add_project(Path::new("/p/a"), 3).unwrap();
        let names: Vec<String> = Store::open(&path).entries(&ws).into_iter().map(|r| r.name).collect();
        assert_eq!(names, ["a", "b"]);
        store.remove(Kind::Project, "/p/a").unwrap();
        assert_eq!(Store::open(&path).entries(&ws).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
