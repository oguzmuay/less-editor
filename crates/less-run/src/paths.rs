//! Run paneli dosya konumları.
//!
//! | dosya | varsayılan | ortam değişkeni |
//! |---|---|---|
//! | veri klasörü | `~/.local/share/less-rs` | `LESS_RS_DATA_DIR` |
//! | çalıştırma ayarları | `<veri>/run_configs.json` | `LESS_RS_RUN_CONFIGS_FILE` |
//! | breakpoint'ler | `<veri>/breakpoints.json` | `LESS_RS_BREAKPOINTS_FILE` |
//!
//! LESS-nvim kayıtları (`~/.local/share/less/nvim/run_configs.json`, `~/.local/share/nvim/run_configs.json`) yalnız
//! okunur; `LESS_RS_DATA_DIR` verilmişse (test) içe aktarma yapılmaz.

use std::path::PathBuf;

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// LESS-rs veri klasörü (`LESS_RS_DATA_DIR`, yoksa `~/.local/share/less-rs`).
pub fn data_dir() -> PathBuf {
    env_path("LESS_RS_DATA_DIR").unwrap_or_else(|| home().join(".local/share/less-rs"))
}

/// `run_configs.json` konumu.
pub fn run_configs_file() -> PathBuf {
    env_path("LESS_RS_RUN_CONFIGS_FILE").unwrap_or_else(|| data_dir().join("run_configs.json"))
}

/// `breakpoints.json` konumu.
pub fn breakpoints_file() -> PathBuf {
    env_path("LESS_RS_BREAKPOINTS_FILE").unwrap_or_else(|| data_dir().join("breakpoints.json"))
}

/// İçe aktarılacak LESS-nvim `run_configs.json` dosyaları (öncelik sırasıyla). Test ortamında (`LESS_RS_DATA_DIR`
/// verilmiş) boş döner.
pub fn nvim_run_configs_files() -> Vec<PathBuf> {
    if env_path("LESS_RS_DATA_DIR").is_some() || env_path("LESS_RS_RUN_CONFIGS_FILE").is_some() {
        return Vec::new();
    }
    let h = home();
    vec![h.join(".local/share/less/nvim/run_configs.json"), h.join(".local/share/nvim/run_configs.json")]
}

/// Mason `bin` klasörleri (LESS-nvim, sonra normal Neovim).
pub fn mason_bin_dirs() -> Vec<PathBuf> {
    let h = home();
    vec![h.join(".local/share/less/nvim/mason/bin"), h.join(".local/share/nvim/mason/bin")]
}
