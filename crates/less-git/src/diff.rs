//! Unified diff belge modeli (#17, Diff tab'ı): çalışma ağacı / staged / commit; stat satırı → dosya bölümü, hunk satırı → gerçek dosya satırı.

use crate::GitError;
use crate::git::{run, run_raw};
use std::path::Path;

/// Satır türü.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Header,
    Stat,
    FileHeader,
    Hunk,
    Add,
    Remove,
    Context,
}

/// ⏎ ile gidilecek yer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Jump {
    /// Belgede o dosyanın bölümünün ilk satırı (0 tabanlı).
    Section(usize),
    /// Gerçek dosyada satır (1 tabanlı).
    File { path: String, line: usize },
}

/// Belge satırı.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub text: String,
    pub kind: LineKind,
    pub jump: Option<Jump>,
}

/// Diff belgesi.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffDoc {
    pub lines: Vec<DiffLine>,
}

/// Diff kaynağı.
#[derive(Debug, Clone)]
pub enum Source {
    /// `git diff` (path yoksa tüm repo); izlenmeyen dosya /dev/null'a karşı tam ekleme.
    Working { path: Option<String> },
    /// `git diff --cached`.
    Staged { path: Option<String> },
    /// Commit'in ilk ebeveyne göre değişikliği (kök commit dahil); path yoksa başlık + `--stat` + tüm dosyalar.
    Commit { rev: String, path: Option<String> },
}

/// Unified diff metnini belgeye çevirir.
pub fn parse(text: &str) -> DiffDoc {
    let mut lines: Vec<DiffLine> = Vec::new();
    let mut seen_file = false;
    let mut path: Option<String> = None;
    let mut new_line = 0usize;
    let mut in_hunk = false;
    let mut sections: Vec<(String, usize)> = Vec::new();
    for raw in text.lines() {
        let (kind, jump);
        if let Some(rest) = raw.strip_prefix("diff --git ") {
            seen_file = true;
            in_hunk = false;
            path = rest.rsplit_once(" b/").map(|(_, b)| b.to_owned());
            if let Some(p) = &path {
                sections.push((p.clone(), lines.len()));
            }
            (kind, jump) = (LineKind::FileHeader, None);
        } else if !seen_file {
            let stat = parse_stat_path(raw);
            (kind, jump) = (if stat.is_some() { LineKind::Stat } else { LineKind::Header }, None);
        } else if in_hunk && raw.starts_with('+') {
            (kind, jump) = (LineKind::Add, file_jump(&path, new_line));
            new_line += 1;
        } else if in_hunk && raw.starts_with('-') {
            (kind, jump) = (LineKind::Remove, file_jump(&path, new_line));
        } else if let Some(c) = raw.strip_prefix("@@ ").and_then(hunk_new_start) {
            in_hunk = true;
            new_line = c;
            (kind, jump) = (LineKind::Hunk, file_jump(&path, c));
        } else if in_hunk && (raw.starts_with(' ') || raw.is_empty()) {
            (kind, jump) = (LineKind::Context, file_jump(&path, new_line));
            new_line += 1;
        } else if in_hunk && raw.starts_with('\\') {
            (kind, jump) = (LineKind::Context, None);
        } else {
            if let Some(p) = raw.strip_prefix("+++ b/") {
                path = Some(p.to_owned());
            } else if let Some(p) = raw.strip_prefix("--- a/") {
                path = Some(p.to_owned());
            }
            (kind, jump) = (LineKind::FileHeader, None);
        }
        lines.push(DiffLine { text: raw.to_owned(), kind, jump });
    }
    for l in lines.iter_mut().filter(|l| l.kind == LineKind::Stat) {
        if let Some(p) = parse_stat_path(&l.text)
            && let Some((_, idx)) = sections.iter().find(|(s, _)| *s == p)
        {
            l.jump = Some(Jump::Section(*idx));
        }
    }
    DiffDoc { lines }
}

fn file_jump(path: &Option<String>, line: usize) -> Option<Jump> {
    path.as_ref().map(|p| Jump::File { path: p.clone(), line: line.max(1) })
}

fn parse_stat_path(l: &str) -> Option<String> {
    let rest = l.strip_prefix(' ')?;
    let (name, counts) = rest.split_once(" | ")?;
    counts.trim_start().chars().next().filter(char::is_ascii_digit)?;
    Some(name.trim().to_owned())
}

fn hunk_new_start(rest: &str) -> Option<usize> {
    let plus = rest.split_whitespace().find(|t| t.starts_with('+'))?;
    plus[1..].split(',').next()?.parse().ok()
}

/// Diff'i alır; değişiklik yoksa `None`.
pub fn diff(repo: &Path, source: &Source) -> Result<Option<DiffDoc>, GitError> {
    let text = match source {
        Source::Working { path: Some(p) } if is_untracked(repo, p) => {
            let full = repo.join(p);
            let out = run_raw(repo, &["diff", "--no-index", "--", "/dev/null", &full.to_string_lossy()], None, &[1])?;
            let t = String::from_utf8_lossy(&out).into_owned();
            t.replace(&format!("b{}", full.to_string_lossy()), &format!("b/{p}"))
                .replace(&format!(" b/{}", full.to_string_lossy().trim_start_matches('/')), &format!(" b/{p}"))
        }
        Source::Working { path } => run(repo, &with_path(&["diff", "--no-ext-diff"], path.as_deref()))?,
        Source::Staged { path } => run(repo, &with_path(&["diff", "--cached", "--no-ext-diff"], path.as_deref()))?,
        Source::Commit { rev, path: None } => run(
            repo,
            &[
                "show",
                "--no-ext-diff",
                "--root",
                "--stat",
                "--patch",
                "--stat-width=120",
                "--date=format:%Y-%m-%d",
                rev,
            ],
        )?,
        Source::Commit { rev, path } => {
            run(repo, &with_path(&["show", "--no-ext-diff", "--root", "--format=", "--patch", rev], path.as_deref()))?
        }
    };
    if text.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(parse(&text)))
}

fn with_path<'a>(base: &[&'a str], path: Option<&'a str>) -> Vec<&'a str> {
    let mut v = base.to_vec();
    if let Some(p) = path {
        v.extend(["--", p]);
    }
    v
}

fn is_untracked(repo: &Path, path: &str) -> bool {
    run(repo, &["ls-files", "--others", "--exclude-standard", "--", path]).is_ok_and(|o| !o.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::testutil::*;

    #[test]
    fn working_hunk_jumps_17() {
        let d = init_repo();
        let p = d.path();
        commit_file(p, "a.txt", "1\n2\n3\n4\n5\n", "init");
        write(p, "a.txt", "1\n2\nX\n4\n5\n");
        let doc = diff(p, &Source::Working { path: Some("a.txt".into()) }).unwrap().unwrap();
        let hunk = doc.lines.iter().find(|l| l.kind == LineKind::Hunk).unwrap();
        assert_eq!(hunk.jump, Some(Jump::File { path: "a.txt".into(), line: 1 }));
        let add = doc.lines.iter().find(|l| l.kind == LineKind::Add).unwrap();
        assert_eq!(add.jump, Some(Jump::File { path: "a.txt".into(), line: 3 }));
        let ctx = doc.lines.iter().filter(|l| l.kind == LineKind::Context).count();
        assert_eq!(ctx, 4);
        assert!(diff(p, &Source::Staged { path: None }).unwrap().is_none());
    }

    #[test]
    fn untracked_vs_dev_null_17() {
        let d = init_repo();
        commit_file(d.path(), "k", "k\n", "init");
        write(d.path(), "sub/new.txt", "a\nb\n");
        let doc = diff(d.path(), &Source::Working { path: Some("sub/new.txt".into()) }).unwrap().unwrap();
        let adds: Vec<_> = doc.lines.iter().filter(|l| l.kind == LineKind::Add).collect();
        assert_eq!(adds.len(), 2);
        assert_eq!(adds[1].jump, Some(Jump::File { path: "sub/new.txt".into(), line: 2 }));
    }

    #[test]
    fn commit_with_stat_and_root_17() {
        let d = init_repo();
        let p = d.path();
        commit_file(p, "a.txt", "1\n", "root commit");
        commit_file(p, "b.txt", "x\ny\n", "second");
        write(p, "a.txt", "1\n2\n");
        git(p, &["commit", "-q", "-am", "third"]);

        let doc = diff(p, &Source::Commit { rev: "HEAD~1".into(), path: None }).unwrap().unwrap();
        let stat = doc.lines.iter().find(|l| l.kind == LineKind::Stat).unwrap();
        let Some(Jump::Section(i)) = stat.jump else { panic!("stat jump") };
        assert!(doc.lines[i].text.starts_with("diff --git") && doc.lines[i].text.contains("b.txt"));
        assert!(doc.lines.iter().any(|l| l.kind == LineKind::Header && l.text.contains("second")));

        let root = diff(p, &Source::Commit { rev: "HEAD~2".into(), path: None }).unwrap().unwrap();
        assert!(root.lines.iter().any(|l| l.kind == LineKind::Add && l.text == "+1"));
        let one = diff(p, &Source::Commit { rev: "HEAD".into(), path: Some("a.txt".into()) }).unwrap().unwrap();
        assert!(one.lines.iter().all(|l| l.kind != LineKind::Stat));
    }

    #[test]
    fn staged_diff_17() {
        let d = init_repo();
        commit_file(d.path(), "a", "1\n", "i");
        write(d.path(), "a", "2\n");
        git(d.path(), &["add", "a"]);
        assert!(diff(d.path(), &Source::Staged { path: Some("a".into()) }).unwrap().is_some());
        assert!(diff(d.path(), &Source::Working { path: Some("a".into()) }).unwrap().is_none());
    }
}
