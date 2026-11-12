//! Yeni proje klasörü oluşturma (karşılama ekranındaki New Project).

use std::path::{Path, PathBuf};
use std::process::Command;

/// `parent/name` klasörünü oluşturur, `git_init` ise içinde `git init` çalıştırır.
/// Hata metni kullanıcıya gösterilir (İngilizce).
pub fn create(parent: &Path, name: &str, git_init: bool) -> Result<PathBuf, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Enter a project name.".into());
    }
    if name.contains('/') || name == "." || name == ".." {
        return Err("Project names cannot contain '/'.".into());
    }
    if !parent.is_dir() {
        return Err(format!("Location does not exist: {}", parent.display()));
    }
    let dir = parent.join(name);
    if dir.exists() {
        return Err(format!("\"{name}\" already exists in this location."));
    }
    std::fs::create_dir(&dir).map_err(|e| format!("Could not create folder: {e}"))?;
    if git_init {
        let status = Command::new("git").args(["init", "-q"]).current_dir(&dir).status();
        if !matches!(status, Ok(s) if s.success()) {
            return Err("Folder created, but `git init` failed.".into());
        }
    }
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_folder_and_rejects_duplicates() {
        let parent = std::env::temp_dir().join(format!("less-rs-proj-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&parent);
        std::fs::create_dir_all(&parent).unwrap();
        assert!(create(&parent, " ", false).is_err());
        assert!(create(&parent, "a/b", false).is_err());
        let dir = create(&parent, "demo", true).unwrap();
        assert!(dir.join(".git").is_dir());
        assert!(create(&parent, "demo", false).is_err());
        let _ = std::fs::remove_dir_all(&parent);
    }
}
