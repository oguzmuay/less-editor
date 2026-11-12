//! Git paneli modeli: repo keşfi, `status --porcelain=v2`, stage / unstage / discard (#35).

use crate::GitError;
use crate::git::{run, run_raw};
use std::path::{Path, PathBuf};

/// Panel bölümü.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Conflicts,
    Staged,
    Changes,
    Untracked,
}

/// Dosya değişiklik türü.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Untracked,
    Conflict,
}

/// Bir bölümdeki dosya girdisi; `path` repo köküne göredir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub section: Section,
    pub kind: Kind,
    pub path: String,
    pub orig_path: Option<String>,
}

/// `status` sonucu.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    pub branch: String,
    pub conflicts: Vec<Entry>,
    pub staged: Vec<Entry>,
    pub changes: Vec<Entry>,
    pub untracked: Vec<Entry>,
}

impl Status {
    /// Hiç değişiklik yok mu.
    pub fn is_clean(&self) -> bool {
        self.conflicts.is_empty() && self.staged.is_empty() && self.changes.is_empty() && self.untracked.is_empty()
    }
}

/// Köklerden repo köklerini bulur: kök bir repo ise kendisi, değilse bir alt seviyedeki repo klasörleri. Tekrarsız.
pub fn repos(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut add = |p: PathBuf| {
        if !out.contains(&p) {
            out.push(p);
        }
    };
    for root in roots {
        if let Ok(top) = run(root, &["rev-parse", "--show-toplevel"]) {
            add(PathBuf::from(top.trim()));
            continue;
        }
        let Ok(rd) = std::fs::read_dir(root) else { continue };
        let mut subs: Vec<PathBuf> =
            rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.join(".git").exists()).collect();
        subs.sort();
        for s in subs {
            add(s);
        }
    }
    out
}

fn kind_of(c: u8) -> Kind {
    match c {
        b'A' => Kind::Added,
        b'D' => Kind::Deleted,
        b'R' => Kind::Renamed,
        b'C' => Kind::Copied,
        b'T' => Kind::TypeChanged,
        _ => Kind::Modified,
    }
}

/// `git status --porcelain=v2 -z --branch` çıktısını ayrıştırır. İndekste ve çalışma ağacında değişen dosya iki bölümde de yer alır.
pub fn parse_porcelain_v2(raw: &str) -> Status {
    let mut st = Status::default();
    let mut recs = raw.split('\0').filter(|r| !r.is_empty());
    while let Some(rec) = recs.next() {
        if let Some(h) = rec.strip_prefix("# branch.head ") {
            st.branch = h.to_owned();
            continue;
        }
        if rec.starts_with('#') {
            continue;
        }
        let entry = |section, kind, path: &str, orig: Option<String>| Entry {
            section,
            kind,
            path: path.to_owned(),
            orig_path: orig,
        };
        match rec.as_bytes()[0] {
            b'1' | b'2' => {
                let n = if rec.starts_with('2') { 10 } else { 9 };
                let f: Vec<&str> = rec.splitn(n, ' ').collect();
                if f.len() < n {
                    continue;
                }
                let orig = (n == 10).then(|| recs.next().unwrap_or("").to_owned());
                let xy = f[1].as_bytes();
                if xy[0] != b'.' {
                    st.staged.push(entry(Section::Staged, kind_of(xy[0]), f[n - 1], orig.clone()));
                }
                if xy[1] != b'.' {
                    st.changes.push(entry(Section::Changes, kind_of(xy[1]), f[n - 1], orig));
                }
            }
            b'u' => {
                let f: Vec<&str> = rec.splitn(11, ' ').collect();
                if f.len() == 11 {
                    st.conflicts.push(entry(Section::Conflicts, Kind::Conflict, f[10], None));
                }
            }
            b'?' => st.untracked.push(entry(Section::Untracked, Kind::Untracked, &rec[2..], None)),
            _ => {}
        }
    }
    st
}

/// Reponun durumu (`--no-optional-locks`: kendi yenilememiz izleyiciyi tetiklemesin).
pub fn status(repo: &Path) -> Result<Status, GitError> {
    let raw =
        run(repo, &["--no-optional-locks", "status", "--porcelain=v2", "-z", "--branch", "--untracked-files=all"])?;
    Ok(parse_porcelain_v2(&raw))
}

/// Dosyaları indekse ekler (silinmiş dosyada silmeyi stage eder; conflict'te çözüldü işaretler).
pub fn stage(repo: &Path, paths: &[&str]) -> Result<(), GitError> {
    let mut args = vec!["add", "-A", "--"];
    args.extend_from_slice(paths);
    run(repo, &args).map(|_| ())
}

/// Dosyaları indeksten çıkarır; ilk commit öncesi repoda `rm --cached`.
pub fn unstage(repo: &Path, paths: &[&str]) -> Result<(), GitError> {
    let mut args = vec!["restore", "--staged", "--"];
    args.extend_from_slice(paths);
    if run_raw(repo, &args, None, &[]).is_ok() {
        return Ok(());
    }
    let mut args = vec!["rm", "--cached", "-r", "-q", "--"];
    args.extend_from_slice(paths);
    run(repo, &args).map(|_| ())
}

/// Tek dosyanın değişikliğini atar (geri dönüşsüz; onay UI'dadır). Staged / conflict dosyada reddeder.
pub fn discard(repo: &Path, entry: &Entry) -> Result<(), GitError> {
    match entry.section {
        Section::Staged => Err(GitError("Unstage the file first to discard its changes".into())),
        Section::Conflicts => Err(GitError("Resolve the conflict first".into())),
        Section::Changes => run(repo, &["restore", "--", &entry.path]).map(|_| ()),
        Section::Untracked => {
            let p = repo.join(&entry.path);
            let r = if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
            r.map_err(|e| GitError(e.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::testutil::*;

    #[test]
    fn sections_and_stage_flow_35() {
        let d = init_repo();
        let p = d.path();
        commit_file(p, "a.txt", "1\n", "init");
        commit_file(p, "b.txt", "1\n", "second");
        write(p, "a.txt", "2\n");
        git(p, &["add", "a.txt"]);
        write(p, "a.txt", "3\n");
        write(p, "new.txt", "x\n");
        std::fs::remove_file(p.join("b.txt")).unwrap();
        let st = status(p).unwrap();
        assert_eq!(st.branch, "main");
        assert_eq!(st.staged.len(), 1);
        assert_eq!(st.changes.len(), 2);
        assert_eq!(st.untracked[0].path, "new.txt");
        assert!(st.changes.iter().any(|e| e.kind == Kind::Deleted && e.path == "b.txt"));

        stage(p, &["b.txt", "new.txt"]).unwrap();
        let st = status(p).unwrap();
        assert!(st.untracked.is_empty());
        assert_eq!(st.staged.len(), 3);
        unstage(p, &["new.txt"]).unwrap();
        assert_eq!(status(p).unwrap().untracked.len(), 1);

        let st = status(p).unwrap();
        assert!(discard(p, &st.staged[0]).is_err());
        discard(p, &st.changes[0]).unwrap();
        assert_eq!(std::fs::read_to_string(p.join("a.txt")).unwrap(), "2\n");
        discard(p, &st.untracked[0]).unwrap();
        assert!(!p.join("new.txt").exists());
    }

    #[test]
    fn unstage_in_empty_repo_35() {
        let d = init_repo();
        write(d.path(), "a", "1");
        stage(d.path(), &["a"]).unwrap();
        unstage(d.path(), &["a"]).unwrap();
        assert_eq!(status(d.path()).unwrap().untracked.len(), 1);
    }

    #[test]
    fn rename_and_repos_35() {
        let d = init_repo();
        let p = d.path();
        commit_file(p, "old.txt", "hello\nworld\n", "c");
        git(p, &["mv", "old.txt", "new.txt"]);
        let st = status(p).unwrap();
        assert_eq!(st.staged[0].kind, Kind::Renamed);
        assert_eq!(st.staged[0].orig_path.as_deref(), Some("old.txt"));

        let parent = tempfile::tempdir().unwrap();
        for n in ["r1", "r2"] {
            let sub = parent.path().join(n);
            std::fs::create_dir(&sub).unwrap();
            git(&sub, &["init", "-q"]);
        }
        let found = repos(&[parent.path().to_path_buf()]);
        assert_eq!(found.len(), 2);
        let one = repos(&[p.to_path_buf(), p.to_path_buf()]);
        assert_eq!(one.len(), 1);
    }

    #[test]
    fn conflict_section_35() {
        let d = init_repo();
        let p = d.path();
        commit_file(p, "f", "base\n", "base");
        git(p, &["checkout", "-q", "-b", "other"]);
        commit_file(p, "f", "other\n", "other");
        git(p, &["checkout", "-q", "main"]);
        commit_file(p, "f", "main\n", "main");
        let _ = crate::git::run(p, &["merge", "other"]);
        let st = status(p).unwrap();
        assert_eq!(st.conflicts.len(), 1);
        assert!(discard(p, &st.conflicts[0]).is_err());
    }
}
