//! 3 bölmeli merge ekranı modeli (#86): çakışma işareti ayrıştırma (diff3 dahil), blok çözümü, `:2:`/`:3:` içerikleri.

use crate::GitError;
use crate::git::{run, run_raw};
use std::path::Path;

/// Çözüm seçimi: sol (ours), sağ (theirs), ikisi (ours + theirs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Left,
    Right,
    Both,
}

/// İşaretli çakışma bloğu. `start`/`end` metindeki satır indeksleri (0 tabanlı, ikisi dahil).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub start: usize,
    pub end: usize,
    pub ours: Vec<String>,
    pub base: Option<Vec<String>>,
    pub theirs: Vec<String>,
    pub ours_label: String,
    pub theirs_label: String,
}

/// Metindeki hâlâ işaretli blokları ayrıştırır; kapanmamış işaret metin sayılır.
pub fn parse(text: &str) -> Vec<Conflict> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if !lines[i].starts_with("<<<<<<<") {
            i += 1;
            continue;
        }
        let (mut ours, mut base, mut theirs) = (Vec::new(), None::<Vec<String>>, Vec::new());
        let mut part = 0;
        let mut j = i + 1;
        let mut closed = None;
        while j < lines.len() {
            let l = lines[j];
            if part == 0 && l.starts_with("|||||||") {
                part = 1;
                base = Some(Vec::new());
            } else if part < 2 && l == "=======" {
                part = 2;
            } else if part == 2 && l.starts_with(">>>>>>>") {
                closed = Some(j);
                break;
            } else if l.starts_with("<<<<<<<") {
                break;
            } else {
                let s = l.to_owned();
                match part {
                    0 => ours.push(s),
                    1 => base.get_or_insert_with(Vec::new).push(s),
                    _ => theirs.push(s),
                }
            }
            j += 1;
        }
        match closed {
            Some(end) => {
                out.push(Conflict {
                    start: i,
                    end,
                    ours,
                    base,
                    theirs,
                    ours_label: lines[i][7..].trim().to_owned(),
                    theirs_label: lines[end][7..].trim().to_owned(),
                });
                i = end + 1;
            }
            None => i += 1,
        }
    }
    out
}

fn join(lines: Vec<String>, original: &str) -> String {
    let mut s = lines.join("\n");
    if original.ends_with('\n') && !lines.is_empty() {
        s.push('\n');
    }
    s
}

fn replacement(c: &Conflict, choice: Choice) -> Vec<String> {
    match choice {
        Choice::Left => c.ours.clone(),
        Choice::Right => c.theirs.clone(),
        Choice::Both => c.ours.iter().chain(&c.theirs).cloned().collect(),
    }
}

/// `index`. işaretli bloğu seçime göre çözer.
pub fn resolve(text: &str, index: usize, choice: Choice) -> Option<String> {
    let blocks = parse(text);
    let c = blocks.get(index)?;
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<String> = lines[..c.start].iter().map(|s| (*s).to_owned()).collect();
    out.extend(replacement(c, choice));
    out.extend(lines[c.end + 1..].iter().map(|s| (*s).to_owned()));
    Some(join(out, text))
}

/// İşareti hâlâ duran tüm blokları aynı yönde çözer (alttan üste; elle çözülenlere dokunmaz).
pub fn accept_all(text: &str, choice: Choice) -> String {
    let mut cur = text.to_owned();
    for i in (0..parse(text).len()).rev() {
        if let Some(n) = resolve(&cur, i, choice) {
            cur = n;
        }
    }
    cur
}

/// Çakışma işareti kaldı mı.
pub fn has_conflicts(text: &str) -> bool {
    !parse(text).is_empty()
}

/// Merge ekranının iki yanı: `:2:` (ours) ve `:3:` (theirs). `path` repo göreli.
pub fn sides(repo: &Path, path: &str) -> Result<(String, String), GitError> {
    let get = |n: u8| -> Result<String, GitError> {
        let spec = format!(":{n}:{path}");
        run_raw(repo, &["show", &spec], None, &[]).map(|b| String::from_utf8_lossy(&b).into_owned())
    };
    Ok((get(2)?, get(3)?))
}

/// Rebase sürüyor mu: ours/theirs yer değiştirir; etiketler işaret satırlarından okunur.
pub fn rebase_in_progress(repo: &Path) -> bool {
    ["rebase-merge", "rebase-apply"]
        .iter()
        .any(|n| run(repo, &["rev-parse", "--git-path", n]).map(|p| repo.join(p.trim()).exists()).unwrap_or(false))
}

/// "Save & mark resolved": `git add`.
pub fn mark_resolved(repo: &Path, path: &str) -> Result<(), GitError> {
    run(repo, &["add", "--", path]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::testutil::*;

    const SAMPLE: &str = "top\n<<<<<<< HEAD\nours1\n||||||| base\nbase1\n=======\ntheirs1\n>>>>>>> other\nmid\n<<<<<<< HEAD\nO2\n=======\nT2\n>>>>>>> other\nend\n";

    #[test]
    fn parse_diff3_and_plain_86() {
        let b = parse(SAMPLE);
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].base.as_deref(), Some(&["base1".to_owned()][..]));
        assert_eq!(b[0].ours_label, "HEAD");
        assert_eq!(b[0].theirs_label, "other");
        assert_eq!((b[0].start, b[0].end), (1, 7));
        assert!(b[1].base.is_none());
        assert_eq!(b[1].theirs, vec!["T2"]);
    }

    #[test]
    fn resolve_choices_86() {
        assert_eq!(
            resolve(SAMPLE, 0, Choice::Left).unwrap(),
            "top\nours1\nmid\n<<<<<<< HEAD\nO2\n=======\nT2\n>>>>>>> other\nend\n"
        );
        assert_eq!(accept_all(SAMPLE, Choice::Right), "top\ntheirs1\nmid\nT2\nend\n");
        assert_eq!(accept_all(SAMPLE, Choice::Both), "top\nours1\ntheirs1\nmid\nO2\nT2\nend\n");
        let half = resolve(SAMPLE, 0, Choice::Left).unwrap();
        assert_eq!(accept_all(&half, Choice::Right), "top\nours1\nmid\nT2\nend\n");
        assert!(!has_conflicts(&accept_all(SAMPLE, Choice::Left)));
        assert!(!has_conflicts("<<<<<<< x\nunterminated\n"));
    }

    #[test]
    fn real_conflict_86() {
        let d = init_repo();
        let p = d.path();
        commit_file(p, "f", "1\nbase\n3\n", "base");
        git(p, &["checkout", "-q", "-b", "other"]);
        commit_file(p, "f", "1\nother\n3\n", "other");
        git(p, &["checkout", "-q", "main"]);
        commit_file(p, "f", "1\nmain\n3\n", "main");
        let _ = run(p, &["merge", "other"]);
        let text = std::fs::read_to_string(p.join("f")).unwrap();
        let b = parse(&text);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].theirs_label, "other");
        let (ours, theirs) = sides(p, "f").unwrap();
        assert_eq!(ours, "1\nmain\n3\n");
        assert_eq!(theirs, "1\nother\n3\n");
        assert!(!rebase_in_progress(p));
        std::fs::write(p.join("f"), accept_all(&text, Choice::Right)).unwrap();
        mark_resolved(p, "f").unwrap();
        assert!(crate::status::status(p).unwrap().conflicts.is_empty());
    }
}
