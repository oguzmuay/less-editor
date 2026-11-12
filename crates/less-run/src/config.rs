//! Kalıcı çalıştırma ayarları (#69): `run_configs.json`, öğe id'sine göre profil, ortam, VM seçenekleri, argümanlar.
//! Biçim LESS-nvim ile aynı: `{ "<id>": { "profile"?, "env"?: {K: V}, "vm_options"?, "args"? } }`.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Bir öğenin çalıştırma ayarları. Boş alanlar `None` (kaydedilmez).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vm_options: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
}

impl RunConfig {
    /// Boş dizeleri ve boş ortamı atar, kenar boşluklarını kırpar; `profiles` false ise profil silinir.
    pub fn cleaned(&self, profiles: bool) -> RunConfig {
        let s = |v: &Option<String>| v.as_ref().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        RunConfig {
            profile: if profiles { s(&self.profile) } else { None },
            env: self.env.clone().filter(|e| !e.is_empty()),
            vm_options: s(&self.vm_options),
            args: s(&self.args),
        }
    }

    /// Hiç alan yok mu.
    pub fn is_empty(&self) -> bool {
        self.profile.is_none() && self.env.is_none() && self.vm_options.is_none() && self.args.is_none()
    }

    /// Profil dışında ayar (ortam, VM seçenekleri, argüman) var mı (panelde ⚙).
    pub fn has_extra(&self) -> bool {
        self.env.is_some() || self.vm_options.is_some() || self.args.is_some()
    }
}

/// `run_configs.json` deposu. Kendi dosyası yoksa LESS-nvim dosyalarından ilki okunur (yazılmaz); ilk kayıtta tüm
/// içerik kendi dosyasına yazılır.
#[derive(Debug, Clone)]
pub struct ConfigStore {
    path: PathBuf,
    configs: HashMap<String, RunConfig>,
}

fn read_file(path: &Path) -> Option<HashMap<String, RunConfig>> {
    let text = fs::read_to_string(path).ok()?;
    let raw: HashMap<String, serde_json::Value> = serde_json::from_str(&text).ok()?;
    Some(raw.into_iter().filter_map(|(k, v)| Some((k, serde_json::from_value(v).ok()?))).collect())
}

impl ConfigStore {
    /// Varsayılan konum (`paths::run_configs_file`) + LESS-nvim içe aktarımı.
    pub fn open_default() -> ConfigStore {
        ConfigStore::open(crate::paths::run_configs_file(), &crate::paths::nvim_run_configs_files())
    }

    /// `path` okunur; yoksa `import` dosyalarından ilk okunabilen kullanılır.
    pub fn open(path: PathBuf, import: &[PathBuf]) -> ConfigStore {
        let configs = if path.exists() {
            read_file(&path).unwrap_or_default()
        } else {
            import.iter().find_map(|p| read_file(p)).unwrap_or_default()
        };
        ConfigStore { path, configs }
    }

    /// Dosya konumu.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Öğenin ayarları (yoksa boş).
    pub fn get(&self, id: &str) -> RunConfig {
        self.configs.get(id).cloned().unwrap_or_default()
    }

    /// Ayarları temizleyip kaydeder (hepsi boşsa kayıt silinir) ve kaydedilen hali döndürür.
    pub fn set(&mut self, id: &str, cfg: &RunConfig, profiles: bool) -> io::Result<RunConfig> {
        let clean = cfg.cleaned(profiles);
        if clean.is_empty() {
            self.configs.remove(id);
        } else {
            self.configs.insert(id.to_string(), clean.clone());
        }
        self.save()?;
        Ok(clean)
    }

    fn save(&self) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let sorted: BTreeMap<_, _> = self.configs.iter().collect();
        let text = serde_json::to_string(&sorted).map_err(io::Error::other)?;
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, text)?;
        fs::rename(tmp, &self.path)
    }
}

/// `KEY=value; KEY2=value` → ortam (formdaki ortam alanı). Hatalı parça varsa İngilizce hata mesajı.
pub fn parse_env(s: &str) -> Result<BTreeMap<String, String>, String> {
    let mut env = BTreeMap::new();
    for part in s.split(';').map(str::trim).filter(|p| !p.is_empty()) {
        let bad = || format!("Environment must look like KEY=value; KEY2=value (bad: {part})");
        let (k, v) = part.split_once('=').ok_or_else(bad)?;
        let k = k.trim();
        let valid = k.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !valid {
            return Err(bad());
        }
        env.insert(k.to_string(), v.trim_start().to_string());
    }
    Ok(env)
}

/// Ortamı forma yazılacak `KEY=value; KEY2=value` biçimine çevirir (anahtar sırasıyla).
pub fn format_env(env: Option<&BTreeMap<String, String>>) -> String {
    env.map(|e| e.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("; ")).unwrap_or_default()
}
