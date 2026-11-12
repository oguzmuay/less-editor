//! Bağlantı kaydı: `connections.json` (`{version=1, connections=[…]}`), LESS-nvim ile aynı biçim (#45, #46).

use crate::types::DbType;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Veri klasörünü değiştiren ortam değişkeni (test ve taşınabilir kurulum).
pub const DATA_DIR_ENV: &str = "LESS_RS_DATA_DIR";

/// Tek bağlantı. Alanlar LESS-nvim kaydıyla birebir; eksik alanlar boş kabul edilir.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Connection {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: DbType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl Connection {
    pub fn new(name: &str, ty: DbType) -> Self {
        Connection {
            name: name.into(),
            ty,
            host: None,
            port: None,
            user: None,
            password: None,
            database: None,
            options: None,
            file: None,
            url: None,
        }
    }

    /// AI'a verilebilen özet: parola ve URL yok (#67).
    pub fn public_summary(&self) -> BTreeMap<&'static str, String> {
        let mut m = BTreeMap::new();
        m.insert("name", self.name.clone());
        m.insert("type", self.ty.id().into());
        for (k, v) in [("host", &self.host), ("database", &self.database), ("file", &self.file)] {
            if let Some(v) = v {
                m.insert(k, v.clone());
            }
        }
        if let Some(p) = self.port {
            m.insert("port", p.to_string());
        }
        m
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct FileModel {
    version: u32,
    #[serde(default)]
    connections: Vec<Connection>,
}

/// Bağlantı kaydı deposu (bellekte liste + dosya).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ConnectionStore {
    pub connections: Vec<Connection>,
}

/// LESS-rs veri klasörü: `LESS_RS_DATA_DIR` ya da `~/.local/share/less-rs`.
pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os(DATA_DIR_ENV) {
        return PathBuf::from(d);
    }
    home().join(".local/share/less-rs")
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

/// `connections.json` yolu.
pub fn store_path() -> PathBuf {
    data_dir().join("datasource").join("connections.json")
}

/// LESS-nvim'in bağlantı dosyası adayları (yalnız okunur, #7 taşıma kuralı).
pub fn legacy_paths() -> Vec<PathBuf> {
    let h = home();
    vec![
        h.join(".local/share/less/nvim/datasource/connections.json"),
        h.join(".local/share/nvim/datasource/connections.json"),
    ]
}

impl ConnectionStore {
    /// Dosyadan yükler; yoksa (ya da bozuksa) boş depo.
    pub fn load_from(path: &Path) -> ConnectionStore {
        let parsed = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<FileModel>(&t).ok());
        ConnectionStore { connections: parsed.map(|m| m.connections).unwrap_or_default() }
    }

    /// Kayıt yoksa LESS-nvim dosyalarından bir kez içe aktarır ve yeni kayda yazar; sonra yükler.
    pub fn load_with_import(path: &Path, legacy: &[PathBuf]) -> std::io::Result<ConnectionStore> {
        if path.exists() {
            return Ok(Self::load_from(path));
        }
        for old in legacy {
            if old.exists() {
                let store = Self::load_from(old);
                if !store.connections.is_empty() {
                    store.save_to(path)?;
                    return Ok(store);
                }
            }
        }
        Ok(ConnectionStore::default())
    }

    /// Varsayılan konumdan yükler (içe aktarma dahil).
    pub fn load() -> ConnectionStore {
        Self::load_with_import(&store_path(), &legacy_paths()).unwrap_or_default()
    }

    /// Dosyaya yazar; dosya 0600, yazma atomik (geçici dosya + rename).
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let model = FileModel { version: 1, connections: self.connections.clone() };
        let text = serde_json::to_string_pretty(&model).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("json.tmp");
        write_private(&tmp, text.as_bytes())?;
        std::fs::rename(&tmp, path)
    }

    pub fn save(&self) -> std::io::Result<()> {
        self.save_to(&store_path())
    }

    pub fn get(&self, name: &str) -> Option<&Connection> {
        self.connections.iter().find(|c| c.name == name)
    }

    /// Aynı adlı bağlantı varsa yerine koyar, yoksa ekler.
    pub fn upsert(&mut self, conn: Connection) {
        match self.connections.iter_mut().find(|c| c.name == conn.name) {
            Some(slot) => *slot = conn,
            None => self.connections.push(conn),
        }
    }

    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.connections.len();
        self.connections.retain(|c| c.name != name);
        before != self.connections.len()
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
    f.write_all(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn roundtrip_and_perms_45() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("datasource/connections.json");
        let mut s = ConnectionStore::default();
        let mut c = Connection::new("local", DbType::Postgresql);
        c.host = Some("localhost".into());
        c.port = Some(5432);
        c.password = Some("pw".into());
        s.upsert(c.clone());
        s.save_to(&path).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        let loaded = ConnectionStore::load_from(&path);
        assert_eq!(loaded.get("local"), Some(&c));
        assert!(std::fs::read_to_string(&path).unwrap().contains("\"version\": 1"));
    }

    #[test]
    fn imports_legacy_once_45() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("old.json");
        std::fs::write(
            &legacy,
            r#"{"version":1,"connections":[{"name":"a","type":"mysql","host":"h","port":3306,"user":"u"}]}"#,
        )
        .unwrap();
        let path = dir.path().join("new/connections.json");
        let s = ConnectionStore::load_with_import(&path, std::slice::from_ref(&legacy)).unwrap();
        assert_eq!(s.connections.len(), 1);
        assert!(path.exists());
        std::fs::write(&legacy, r#"{"version":1,"connections":[]}"#).unwrap();
        let again = ConnectionStore::load_with_import(&path, &[legacy]).unwrap();
        assert_eq!(again.connections.len(), 1);
    }

    #[test]
    fn upsert_remove_and_summary_67() {
        let mut s = ConnectionStore::default();
        let mut c = Connection::new("x", DbType::Sqlite);
        c.file = Some("/tmp/a.db".into());
        c.password = Some("secret".into());
        s.upsert(c.clone());
        s.upsert(c.clone());
        assert_eq!(s.connections.len(), 1);
        let sum = c.public_summary();
        assert!(!sum.values().any(|v| v == "secret"));
        assert!(s.remove("x"));
        assert!(!s.remove("x"));
    }

    #[test]
    fn broken_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("c.json");
        std::fs::write(&p, "{nope").unwrap();
        assert!(ConnectionStore::load_from(&p).connections.is_empty());
    }
}
