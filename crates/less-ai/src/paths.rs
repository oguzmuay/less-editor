//! LESS-rs kayıt klasörü (less-workspace `paths` ile aynı kural) ve LESS (nvim) kayıtlarının içe aktarma yolları.

use std::path::{Path, PathBuf};

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

/// `~/.local/share/less-rs` (testte `LESS_RS_DATA_DIR`).
pub fn data_dir() -> PathBuf {
    std::env::var_os("LESS_RS_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local").join("share").join("less-rs"))
}

/// LESS (nvim) tarafının aynı adlı veri kaydı (yalnız okunur): önce `less/nvim`, sonra `nvim`.
/// `LESS_RS_DATA_DIR` verilmişse (test) içe aktarma yapılmaz.
pub fn legacy_data(file: &str) -> Option<PathBuf> {
    if std::env::var_os("LESS_RS_DATA_DIR").is_some() {
        return None;
    }
    let base = home().join(".local").join("share");
    ["less/nvim", "nvim"].iter().map(|app| base.join(app).join(file)).find(|p| p.is_file())
}

/// Önce geçici dosyaya yazıp yerine taşır.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, path)
}
