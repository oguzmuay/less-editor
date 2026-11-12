//! LESS-rs'in kendi kayıt klasörleri. LESS (nvim) kayıtlarına hiç yazılmaz; onlar yalnız ilk açılışta içe aktarılır.
//!
//! | Klasör | Varsayılan | Ortam değişkeni (test) |
//! |---|---|---|
//! | ayar | `~/.config/less-rs` | `LESS_RS_CONFIG_DIR` |
//! | durum | `~/.local/state/less-rs` | `LESS_RS_STATE_DIR` |
//! | veri | `~/.local/share/less-rs` | `LESS_RS_DATA_DIR` |

use std::path::PathBuf;

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

fn dir(env: &str, default: &[&str]) -> PathBuf {
    std::env::var_os(env).map(PathBuf::from).unwrap_or_else(|| default.iter().fold(home(), |p, s| p.join(s)))
}

pub fn config_dir() -> PathBuf {
    dir("LESS_RS_CONFIG_DIR", &[".config", "less-rs"])
}

pub fn state_dir() -> PathBuf {
    dir("LESS_RS_STATE_DIR", &[".local", "state", "less-rs"])
}

pub fn data_dir() -> PathBuf {
    dir("LESS_RS_DATA_DIR", &[".local", "share", "less-rs"])
}

/// LESS (nvim) tarafının aynı adlı kaydı: önce LESS.app klasörü (`less/nvim`), sonra düz `nvim`. İçe aktarma için.
pub fn legacy(kind: &str, file: &str) -> Option<PathBuf> {
    let base = home().join(".local").join(kind);
    ["less/nvim", "nvim"].iter().map(|app| base.join(app).join(file)).find(|p| p.is_file())
}

/// JSON'u önce geçici dosyaya yazıp yerine taşır (yarım yazılmış kayıt kalmasın).
pub fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, path)
}
