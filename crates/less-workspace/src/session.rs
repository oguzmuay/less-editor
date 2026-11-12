//! Proje oturum durumu: açık tablar, aktif tab, dosya başına imleç. `<state>/sessions/<özet>.json`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TabState {
    pub path: PathBuf,
    /// 0 tabanlı satır / sütun.
    #[serde(default)]
    pub line: usize,
    #[serde(default)]
    pub col: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Session {
    #[serde(default)]
    pub version: u32,
    pub dir: PathBuf,
    #[serde(default)]
    pub tabs: Vec<TabState>,
    #[serde(default)]
    pub active: Option<usize>,
}

/// Yoldan kararlı, dosya adına uygun kısa anahtar (FNV-1a 64).
pub fn key(dir: &Path) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in dir.to_string_lossy().bytes() {
        h = (h ^ b as u64).wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

fn file_in(root: &Path, dir: &Path) -> PathBuf {
    root.join(format!("{}.json", key(dir)))
}

impl Session {
    pub fn new(dir: &Path) -> Self {
        Self { version: 1, dir: dir.to_path_buf(), ..Self::default() }
    }

    pub fn save_in(&self, root: &Path) -> std::io::Result<()> {
        paths::write_atomic(&file_in(root, &self.dir), &serde_json::to_vec(self).map_err(std::io::Error::other)?)
    }

    pub fn save(&self) -> std::io::Result<()> {
        self.save_in(&paths::state_dir().join("sessions"))
    }

    /// Oturumu okur; kaybolmuş dosyalar tablardan atılır, `active` düzeltilir.
    pub fn load_in(root: &Path, dir: &Path) -> Option<Self> {
        let mut s: Session = serde_json::from_slice(&std::fs::read(file_in(root, dir)).ok()?).ok()?;
        let active_path = s.active.and_then(|i| s.tabs.get(i)).map(|t| t.path.clone());
        s.tabs.retain(|t| t.path.exists());
        s.active = active_path.and_then(|p| s.tabs.iter().position(|t| t.path == p));
        Some(s)
    }

    pub fn load(dir: &Path) -> Option<Self> {
        Self::load_in(&paths::state_dir().join("sessions"), dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_drops_missing_files_and_fixes_active() {
        let root = std::env::temp_dir().join(format!("less-rs-sess-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let keep = root.join("keep.rs");
        std::fs::write(&keep, "").unwrap();
        let mut s = Session::new(Path::new("/proj"));
        s.tabs = vec![
            TabState { path: root.join("gone.rs"), line: 1, col: 2 },
            TabState { path: keep.clone(), line: 10, col: 3 },
        ];
        s.active = Some(1);
        s.save_in(&root).unwrap();
        let back = Session::load_in(&root, Path::new("/proj")).unwrap();
        assert_eq!(back.tabs.len(), 1);
        assert_eq!(back.active, Some(0));
        assert_eq!(back.tabs[0].line, 10);
        assert!(Session::load_in(&root, Path::new("/other")).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn key_is_stable_and_distinct() {
        assert_eq!(key(Path::new("/a")), key(Path::new("/a")));
        assert_ne!(key(Path::new("/a")), key(Path::new("/b")));
    }
}
