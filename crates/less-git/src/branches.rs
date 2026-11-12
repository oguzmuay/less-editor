//! Git History Branches sütunu: local / remote dal listesi ve checkout (#72).

use crate::GitError;
use crate::git::run;
use std::path::Path;

/// Dal girdisi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    pub name: String,
    pub reference: String,
    pub remote: Option<String>,
    pub current: bool,
    pub track: Option<String>,
}

/// Dal listesi; HEAD bir dalda değilse `detached`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BranchList {
    pub locals: Vec<Branch>,
    pub remotes: Vec<Branch>,
    pub detached: bool,
}

/// `ahead 1, behind 2` → `↑1 ↓2`; `gone` korunur.
pub fn short_track(track: &str) -> Option<String> {
    let t = track.trim();
    if t.is_empty() {
        return None;
    }
    if t == "gone" {
        return Some("gone".into());
    }
    let num = |key: &str| {
        t.split(key)
            .nth(1)
            .map(|r| r.chars().take_while(char::is_ascii_digit).collect::<String>())
            .filter(|s| !s.is_empty())
    };
    let mut parts = Vec::new();
    if let Some(a) = num("ahead ") {
        parts.push(format!("↑{a}"));
    }
    if let Some(b) = num("behind ") {
        parts.push(format!("↓{b}"));
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}

/// Local ve remote dalları listeler; `refs/remotes/<remote>/HEAD` atlanır.
pub fn list(root: &Path) -> Result<BranchList, GitError> {
    let fmt = "%(refname)%1f%(refname:short)%1f%(HEAD)%1f%(upstream:track,nobracket)";
    let raw = run(root, &["for-each-ref", &format!("--format={fmt}"), "refs/heads", "refs/remotes"])?;
    let mut remotes: Vec<String> = run(root, &["remote"])?.lines().map(str::to_owned).collect();
    remotes.sort_by_key(|r| std::cmp::Reverse(r.len()));
    let mut out = BranchList { detached: true, ..Default::default() };
    for line in raw.lines() {
        let f: Vec<&str> = line.split('\x1f').collect();
        if f.len() < 3 {
            continue;
        }
        let (reference, name) = (f[0], f[1]);
        if reference.starts_with("refs/heads/") {
            let current = f[2] == "*";
            out.detached &= !current;
            out.locals.push(Branch {
                name: name.into(),
                reference: reference.into(),
                remote: None,
                current,
                track: f.get(3).and_then(|t| short_track(t)),
            });
        } else if !reference.ends_with("/HEAD") {
            let rest = &reference["refs/remotes/".len()..];
            let remote = remotes.iter().find(|r| rest.starts_with(&format!("{r}/"))).cloned();
            out.remotes.push(Branch {
                name: rest.into(),
                reference: reference.into(),
                remote,
                current: false,
                track: None,
            });
        }
    }
    Ok(out)
}

/// Dala geçer. Remote dalda aynı adlı local varsa ona, yoksa `switch -c x --track origin/x`. Çıktı mesajı döner.
pub fn checkout(root: &Path, branch: &Branch, locals: &[Branch]) -> Result<String, GitError> {
    let args: Vec<String> = match &branch.remote {
        Some(remote) => {
            let name = branch.name[remote.len() + 1..].to_owned();
            if locals.iter().any(|b| b.name == name) {
                vec!["switch".into(), name]
            } else {
                vec!["switch".into(), "-c".into(), name, "--track".into(), branch.name.clone()]
            }
        }
        None => vec!["switch".into(), branch.name.clone()],
    };
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    crate::git::run_raw(root, &refs, None, &[]).map(|b| String::from_utf8_lossy(&b).trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::testutil::*;

    #[test]
    fn track_text_72() {
        assert_eq!(short_track("ahead 1, behind 2").as_deref(), Some("↑1 ↓2"));
        assert_eq!(short_track("ahead 3").as_deref(), Some("↑3"));
        assert_eq!(short_track("gone").as_deref(), Some("gone"));
        assert_eq!(short_track(""), None);
    }

    #[test]
    fn checkout_remote_tracks_72() {
        let remote = tempfile::tempdir().unwrap();
        git(remote.path(), &["init", "-q", "--bare", "-b", "main"]);
        let a = init_repo();
        let p = a.path();
        commit_file(p, "f", "1\n", "init");
        git(p, &["remote", "add", "origin", remote.path().to_str().unwrap()]);
        git(p, &["push", "-q", "origin", "main"]);
        git(p, &["checkout", "-q", "-b", "feature/x"]);
        commit_file(p, "f", "2\n", "feat");
        git(p, &["push", "-q", "origin", "feature/x"]);
        git(p, &["checkout", "-q", "main"]);
        git(p, &["branch", "-q", "-D", "feature/x"]);
        git(p, &["fetch", "-q", "origin"]);

        let l = list(p).unwrap();
        assert!(!l.detached);
        assert_eq!(l.locals.len(), 1);
        assert!(l.locals[0].current);
        let rb = l.remotes.iter().find(|b| b.name == "origin/feature/x").unwrap();
        assert_eq!(rb.remote.as_deref(), Some("origin"));
        assert!(l.remotes.iter().all(|b| !b.name.ends_with("/HEAD")));

        checkout(p, rb, &l.locals).unwrap();
        let l2 = list(p).unwrap();
        let cur = l2.locals.iter().find(|b| b.current).unwrap();
        assert_eq!(cur.name, "feature/x");
        assert_eq!(git(p, &["rev-parse", "--abbrev-ref", "feature/x@{upstream}"]).trim(), "origin/feature/x");

        git(p, &["switch", "-q", "main"]);
        let again = list(p).unwrap();
        checkout(p, rb, &again.locals).unwrap();
        assert_eq!(git(p, &["branch", "--show-current"]).trim(), "feature/x");
    }
}
