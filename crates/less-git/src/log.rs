//! Git History: `git log --topo-order` ayrıştırma, dosya geçmişi (`--follow`), tüm dallar (#17, #58).

use crate::GitError;
use crate::git::run;
use std::path::Path;

/// Ref etiketi türü (#58): HEAD / local / remote / tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    Head,
    Local,
    Remote,
    Tag,
}

/// Commit üstündeki ref etiketi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefLabel {
    pub name: String,
    pub kind: RefKind,
}

/// Geçmiş satırı. `date` ISO `%Y-%m-%d`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub hash: String,
    pub short: String,
    pub parents: Vec<String>,
    pub author: String,
    pub date: String,
    pub subject: String,
    pub refs: Vec<RefLabel>,
}

/// Geçmiş sorgusu seçenekleri.
#[derive(Debug, Clone, Default)]
pub struct LogOptions {
    pub all: bool,
    pub file: Option<String>,
    pub limit: Option<usize>,
}

/// `%D` metnini sınıflandırır (`HEAD -> main, origin/main, tag: v1`). `remotes` remote adlarıdır.
pub fn parse_refs(decor: &str, remotes: &[String]) -> Vec<RefLabel> {
    let mut out = Vec::new();
    for part in decor.split(", ").map(str::trim).filter(|p| !p.is_empty()) {
        let mut push = |name: &str, kind| out.push(RefLabel { name: name.to_owned(), kind });
        if let Some(t) = part.strip_prefix("tag: ") {
            push(t, RefKind::Tag);
        } else if let Some(b) = part.strip_prefix("HEAD -> ") {
            push("HEAD", RefKind::Head);
            push(b, RefKind::Local);
        } else if part == "HEAD" {
            push("HEAD", RefKind::Head);
        } else if remotes.iter().any(|r| part.starts_with(&format!("{r}/"))) {
            push(part, RefKind::Remote);
        } else {
            push(part, RefKind::Local);
        }
    }
    out
}

/// Çıktı satırlarını (`\x1f` ayraçlı) commit listesine çevirir. Dosya modunda parent'lar listedeki sıradaki commit olur (tek şerit).
pub fn parse_log(raw: &str, remotes: &[String], file_mode: bool) -> Vec<Commit> {
    let mut commits: Vec<Commit> = raw
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\x1f').collect();
            if f.len() < 7 {
                return None;
            }
            Some(Commit {
                hash: f[0].to_owned(),
                short: f[1].to_owned(),
                parents: f[2].split_whitespace().map(str::to_owned).collect(),
                author: f[3].to_owned(),
                date: f[4].to_owned(),
                subject: f[5].to_owned(),
                refs: parse_refs(f[6], remotes),
            })
        })
        .collect();
    if file_mode {
        let next: Vec<Option<String>> =
            (0..commits.len()).map(|i| commits.get(i + 1).map(|c| c.hash.clone())).collect();
        for (c, n) in commits.iter_mut().zip(next) {
            c.parents = n.into_iter().collect();
        }
    }
    commits
}

/// Geçmişi okur.
pub fn log(repo: &Path, opts: &LogOptions) -> Result<Vec<Commit>, GitError> {
    let remotes: Vec<String> = run(repo, &["remote"])?.lines().map(str::to_owned).collect();
    let limit = opts.limit.map(|n| format!("-n{n}"));
    let mut args: Vec<&str> = vec![
        "log",
        "--topo-order",
        "--date=format:%Y-%m-%d",
        "--decorate=short",
        "--pretty=format:%H%x1f%h%x1f%P%x1f%an%x1f%ad%x1f%s%x1f%D",
    ];
    if opts.all {
        args.push("--all");
    }
    if let Some(l) = &limit {
        args.push(l);
    }
    if let Some(f) = &opts.file {
        args.extend(["--follow", "--", f]);
    }
    let raw = match run(repo, &args) {
        Ok(r) => r,
        Err(e) if e.0.contains("does not have any commits") => String::new(),
        Err(e) => return Err(e),
    };
    Ok(parse_log(&raw, &remotes, opts.file.is_some()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::testutil::*;

    #[test]
    fn refs_classified_58() {
        let r = parse_refs("HEAD -> main, origin/main, tag: v1, feature/x", &["origin".to_owned()]);
        let kinds: Vec<_> = r.iter().map(|x| (x.name.as_str(), x.kind)).collect();
        assert_eq!(
            kinds,
            vec![
                ("HEAD", RefKind::Head),
                ("main", RefKind::Local),
                ("origin/main", RefKind::Remote),
                ("v1", RefKind::Tag),
                ("feature/x", RefKind::Local)
            ]
        );
    }

    #[test]
    fn log_all_and_file_17() {
        let d = init_repo();
        let p = d.path();
        assert!(log(p, &LogOptions::default()).unwrap().is_empty());
        commit_file(p, "a", "1\n", "one");
        commit_file(p, "b", "1\n", "other file");
        git(p, &["checkout", "-q", "-b", "side"]);
        commit_file(p, "a", "2\n", "side a");
        git(p, &["checkout", "-q", "main"]);
        git(p, &["tag", "v1"]);

        let main_only = log(p, &LogOptions::default()).unwrap();
        assert_eq!(main_only.len(), 2);
        assert_eq!(main_only[0].date.len(), 10);
        assert!(main_only[0].refs.iter().any(|r| r.kind == RefKind::Tag));
        assert_eq!(log(p, &LogOptions { all: true, ..Default::default() }).unwrap().len(), 3);

        let f = log(p, &LogOptions { all: true, file: Some("a".into()), limit: None }).unwrap();
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].parents, vec![f[1].hash.clone()]);
        assert!(f[1].parents.is_empty());
        assert_eq!(log(p, &LogOptions { limit: Some(1), ..Default::default() }).unwrap().len(), 1);
    }
}
