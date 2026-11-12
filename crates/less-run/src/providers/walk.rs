//! Basit özyinelemeli dosya taraması (ripgrep `--files` yerine): gizli klasörler, sembolik bağlar ve verilen klasör
//! adları atlanır. `.gitignore` okunmaz.

use std::fs;
use std::path::{Path, PathBuf};

/// `root` altında `accept` ile eşleşen dosyaları sıralı döndürür; adı `skip` içinde olan klasörlere inilmez.
pub fn find_files(root: &Path, skip: &[&str], accept: &dyn Fn(&Path) -> bool) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_dir() {
                if !name.starts_with('.') && !skip.contains(&name.as_ref()) {
                    stack.push(path);
                }
            } else if ft.is_file() && accept(&path) {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// `start` klasöründen yukarı doğru `names`'ten birini içeren ilk klasör (`vim.fs.root`).
pub fn root_with(start: &Path, names: &[&str]) -> Option<PathBuf> {
    start.ancestors().find(|d| names.iter().any(|n| d.join(n).exists())).map(Path::to_path_buf)
}

/// `start`'tan yukarı doğru `name` adlı dosyayı arar (`vim.fs.find(…, { upward = true })`).
pub fn find_upward(start: &Path, name: &str) -> Option<PathBuf> {
    start.ancestors().map(|d| d.join(name)).find(|p| p.is_file())
}
