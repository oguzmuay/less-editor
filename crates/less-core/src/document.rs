//! Dosyayı satırlara bölerek yükler. Şimdilik salt okunur görüntüleme içindir; düzenleme Faz 1'de gelir.

use std::fmt;
use std::path::{Path, PathBuf};

/// Bu boyutun üstündeki dosyalar açılmaz.
pub const MAX_BYTES: u64 = 8 * 1024 * 1024;
const TAB: &str = "    ";

#[derive(Debug)]
pub struct Document {
    pub path: PathBuf,
    pub lines: Vec<String>,
    /// En uzun satırın karakter sayısı (yatay kaydırma genişliği için).
    pub max_cols: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LoadError {
    TooLarge(u64),
    Binary,
    Io(String),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::TooLarge(n) => write!(f, "File is too large to open ({} MB)", n / (1024 * 1024)),
            LoadError::Binary => f.write_str("Binary file — no preview"),
            LoadError::Io(e) => write!(f, "Could not read file: {e}"),
        }
    }
}

impl Document {
    /// Dosyayı okur; NUL baytı içeren dosya ikili sayılır, geçersiz UTF-8 kayıplı çevrilir, sekmeler 4 boşluk olur.
    pub fn load(path: &Path) -> Result<Self, LoadError> {
        let size = std::fs::metadata(path).map_err(|e| LoadError::Io(e.to_string()))?.len();
        if size > MAX_BYTES {
            return Err(LoadError::TooLarge(size));
        }
        let bytes = std::fs::read(path).map_err(|e| LoadError::Io(e.to_string()))?;
        if bytes.iter().take(8192).any(|&b| b == 0) {
            return Err(LoadError::Binary);
        }
        let text = String::from_utf8_lossy(&bytes);
        let lines: Vec<String> =
            text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l).replace('\t', TAB)).collect();
        let max_cols = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
        Ok(Self { path: path.to_path_buf(), lines, max_cols })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_text_and_rejects_binary() {
        let dir = std::env::temp_dir().join(format!("less-rs-doc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let text = dir.join("a.txt");
        std::fs::write(&text, "one\r\n\ttwo\n").unwrap();
        let doc = Document::load(&text).unwrap();
        assert_eq!(doc.lines, ["one", "    two", ""]);
        assert_eq!(doc.max_cols, 7);
        let bin = dir.join("b.bin");
        std::fs::write(&bin, [1u8, 0, 2]).unwrap();
        assert_eq!(Document::load(&bin).unwrap_err(), LoadError::Binary);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
