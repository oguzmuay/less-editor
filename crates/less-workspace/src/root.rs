//! Proje kökü bulma ve "son proje" kaydına uygunluk kuralları (LESS-nvim `autorestore` ile aynı).

use std::path::{Path, PathBuf};

use crate::paths;

/// `.git` dışında bir klasörü "proje" yapan işaretçiler.
pub const MARKERS: [&str; 11] = [
    ".git",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "settings.gradle",
    "settings.gradle.kts",
    "mvnw",
    "package.json",
    "go.mod",
    "Cargo.toml",
    "pyproject.toml",
];

/// Bu önekle başlayan klasörler "son proje" olarak kaydedilmez.
pub const IGNORED_PREFIXES: [&str; 4] = ["/tmp/", "/private/tmp/", "/var/folders/", "/private/var/folders/"];

/// Klasörün kendisinde bir proje işaretçisi var mı?
pub fn has_marker(dir: &Path) -> bool {
    MARKERS.iter().any(|m| dir.join(m).exists())
}

/// `dir`'in içinde bulunduğu projenin kökü: yukarı doğru en yakın `.git`, yoksa en yakın işaretçili klasör.
/// Arama `home`'da ve `/`'de durur; ikisi de proje sayılmaz.
pub fn project_root_in(dir: &Path, home: &Path) -> Option<PathBuf> {
    let mut nearest = None;
    for d in dir.ancestors() {
        if d == home || d == Path::new("/") || d.as_os_str().is_empty() {
            break;
        }
        if d.join(".git").exists() {
            return Some(d.to_path_buf());
        }
        if nearest.is_none() && has_marker(d) {
            nearest = Some(d.to_path_buf());
        }
    }
    nearest
}

/// `project_root_in` ev klasörü `$HOME` olarak.
pub fn project_root(dir: &Path) -> Option<PathBuf> {
    project_root_in(dir, &paths::home())
}

/// Klasör "son proje" olarak kaydedilebilir mi: ev klasörü ve `/` değil, geçici klasör değil, işaretçisi var.
/// `workspace` ise işaretçi aranmaz.
pub fn recordable(dir: &Path, home: &Path, workspace: bool, ignored: &[&str]) -> bool {
    if dir == home || dir == Path::new("/") {
        return false;
    }
    let text = format!("{}/", dir.display());
    if ignored.iter().any(|p| text.starts_with(p)) {
        return false;
    }
    workspace || has_marker(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("less-rs-root-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn git_has_priority_over_nearer_marker_32() {
        let d = tmp("git");
        std::fs::create_dir_all(d.join(".git")).unwrap();
        std::fs::create_dir_all(d.join("web/src")).unwrap();
        std::fs::write(d.join("web/package.json"), "{}").unwrap();
        assert_eq!(project_root_in(&d.join("web/src"), Path::new("/nonexistent-home")), Some(d.clone()));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn nearest_marker_without_git_32() {
        let d = tmp("marker");
        std::fs::create_dir_all(d.join("a/b/src")).unwrap();
        std::fs::write(d.join("a/b/Cargo.toml"), "").unwrap();
        assert_eq!(project_root_in(&d.join("a/b/src"), Path::new("/nonexistent-home")), Some(d.join("a/b")));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn home_is_never_a_project_32() {
        let d = tmp("home");
        std::fs::create_dir_all(d.join(".git")).unwrap();
        std::fs::create_dir_all(d.join("sub")).unwrap();
        assert_eq!(project_root_in(&d.join("sub"), &d), None);
        assert!(!recordable(&d, &d, false, &[]));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn tmp_dirs_are_ignored_32() {
        let d = tmp("ign");
        std::fs::write(d.join("Cargo.toml"), "").unwrap();
        let prefix = format!("{}/", d.parent().unwrap().display());
        assert!(!recordable(&d, Path::new("/h"), false, &[prefix.as_str()]));
        assert!(recordable(&d, Path::new("/h"), false, &[]));
        assert!(recordable(Path::new("/nonexistent/ws"), Path::new("/h"), true, &[]));
        let _ = std::fs::remove_dir_all(&d);
    }
}
