//! Git paneli, geçmiş grafiği, blame, diff, merge çözümü.

pub mod blame;
pub mod branches;
pub mod compare;
pub mod diff;
mod git;
pub mod graph;
pub mod log;
pub mod merge;
pub mod status;
pub mod watch;

pub use git::GitError;

use std::path::{Path, PathBuf};

/// Klasörün (ya da üst klasörlerinden birinin) `.git`'ini bulur; worktree'deki `gitdir:` dosyasını izler.
fn git_dir(start: &Path) -> Option<PathBuf> {
    start.ancestors().find_map(|dir| {
        let dot = dir.join(".git");
        if dot.is_dir() {
            return Some(dot);
        }
        let text = std::fs::read_to_string(&dot).ok()?;
        let target = text.strip_prefix("gitdir:")?.trim();
        Some(dir.join(target))
    })
}

/// Geçerli dal adı; ayrık HEAD'de kısa commit özeti, repo değilse `None`. `git` çalıştırmaz, `HEAD` dosyasını okur.
pub fn branch(dir: &Path) -> Option<String> {
    let head = std::fs::read_to_string(git_dir(dir)?.join("HEAD")).ok()?;
    let head = head.trim();
    match head.strip_prefix("ref: ") {
        Some(r) => Some(r.strip_prefix("refs/heads/").unwrap_or(r).to_owned()),
        None => Some(head.chars().take(7).collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_branch_and_detached_head() {
        let root = std::env::temp_dir().join(format!("less-rs-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join(".git/HEAD"), "ref: refs/heads/feature/x\n").unwrap();
        assert_eq!(branch(&root.join("sub")).as_deref(), Some("feature/x"));
        std::fs::write(root.join(".git/HEAD"), "0123456789abcdef\n").unwrap();
        assert_eq!(branch(&root).as_deref(), Some("0123456"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
