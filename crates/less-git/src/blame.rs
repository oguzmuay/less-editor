//! `git blame --porcelain --contents -` (#22): buffer içeriğiyle satır başına tarih / yazar / commit; yazar lens'i.

use crate::GitError;
use crate::git::run_raw;
use std::collections::HashMap;
use std::path::Path;

const UNCOMMITTED: &str = "0000000000000000000000000000000000000000";

/// Satırın commit bilgisi. `date` UTC `%Y-%m-%d`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlameLine {
    pub sha: String,
    pub author: String,
    pub time: i64,
    pub date: String,
    pub summary: String,
}

/// Unix zamanını UTC ISO tarihine çevirir.
pub fn format_date(epoch: i64) -> String {
    let z = epoch.div_euclid(86400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Porcelain çıktısını ayrıştırır; dizi satır (0 tabanlı) → bilgi, commit edilmemiş satır `None`.
pub fn parse_porcelain(raw: &str) -> Vec<Option<BlameLine>> {
    let mut commits: HashMap<String, BlameLine> = HashMap::new();
    let mut lines: Vec<Option<String>> = Vec::new();
    let mut cur: Option<String> = None;
    for l in raw.lines() {
        let mut it = l.split(' ');
        let (sha, _orig, fin) = (it.next().unwrap_or(""), it.next(), it.next());
        if sha.len() == 40
            && sha.bytes().all(|b| b.is_ascii_hexdigit())
            && fin.is_some_and(|f| f.parse::<usize>().is_ok())
        {
            let n: usize = fin.unwrap().parse().unwrap();
            commits.entry(sha.to_owned()).or_insert_with(|| BlameLine {
                sha: sha.to_owned(),
                author: String::new(),
                time: 0,
                date: String::new(),
                summary: String::new(),
            });
            if lines.len() < n {
                lines.resize(n, None);
            }
            lines[n - 1] = Some(sha.to_owned());
            cur = Some(sha.to_owned());
        } else if let (Some(sha), Some((k, v))) = (&cur, l.split_once(' '))
            && let Some(c) = commits.get_mut(sha)
        {
            match k {
                "author" => c.author = v.to_owned(),
                "author-time" => {
                    c.time = v.parse().unwrap_or(0);
                    c.date = format_date(c.time);
                }
                "summary" => c.summary = v.to_owned(),
                _ => {}
            }
        }
    }
    lines.into_iter().map(|s| s.filter(|s| s != UNCOMMITTED).and_then(|s| commits.get(&s).cloned())).collect()
}

/// Dosyayı verilen buffer içeriğiyle blame eder (kaydedilmemiş satırlar kaymaz). `file` mutlak yol.
pub fn blame(file: &Path, content: &str) -> Result<Vec<Option<BlameLine>>, GitError> {
    let dir = file.parent().ok_or_else(|| GitError("invalid path".into()))?;
    let name = file.file_name().and_then(|n| n.to_str()).ok_or_else(|| GitError("invalid path".into()))?;
    let mut data = content.to_owned();
    if !data.ends_with('\n') {
        data.push('\n');
    }
    let out = run_raw(dir, &["blame", "--porcelain", "--contents", "-", "--", name], Some(data.as_bytes()), &[])?;
    let mut lines = parse_porcelain(&String::from_utf8_lossy(&out));
    lines.resize(content.lines().count().max(lines.len()), None);
    Ok(lines)
}

/// Lens verilecek tanım: `line` tanım satırı, `start..=end` kapsadığı satırlar (0 tabanlı).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decl {
    pub line: usize,
    pub start: usize,
    pub end: usize,
}

/// Tanım satırı sonunda yazılacak "yazar +N" lens'i.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lens {
    pub line: usize,
    pub author: String,
    pub others: usize,
}

impl Lens {
    /// Görünen metin: `<yazar> +N`.
    pub fn text(&self) -> String {
        if self.others > 0 { format!("{} +{}", self.author, self.others) } else { self.author.clone() }
    }
}

/// Her tanımda en çok satırı olan yazar (eşitlikte ilk görülen) + diğer yazar sayısı; blame'i olmayan tanım atlanır.
pub fn author_lens(lines: &[Option<BlameLine>], decls: &[Decl]) -> Vec<Lens> {
    decls
        .iter()
        .filter_map(|d| {
            let mut order: Vec<(&str, usize)> = Vec::new();
            for l in lines.iter().skip(d.start).take(d.end + 1 - d.start).flatten() {
                match order.iter_mut().find(|(a, _)| *a == l.author) {
                    Some(e) => e.1 += 1,
                    None => order.push((&l.author, 1)),
                }
            }
            order.sort_by_key(|e| std::cmp::Reverse(e.1));
            let first = order.first()?;
            Some(Lens { line: d.line, author: first.0.to_owned(), others: order.len() - 1 })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::testutil::*;

    #[test]
    fn date_format_22() {
        assert_eq!(format_date(0), "1970-01-01");
        assert_eq!(format_date(1_700_000_000), "2023-11-14");
    }

    #[test]
    fn blame_with_unsaved_buffer_22() {
        let d = init_repo();
        let p = d.path();
        commit_file(p, "f.txt", "one\ntwo\n", "first");
        git(p, &["config", "user.name", "Second Author"]);
        commit_file(p, "f.txt", "one\ntwo\nthree\n", "add three");
        let b = blame(&p.join("f.txt"), "NEW\none\ntwo\nthree\n").unwrap();
        assert_eq!(b.len(), 4);
        assert!(b[0].is_none());
        assert_eq!(b[1].as_ref().unwrap().author, "Test User");
        assert_eq!(b[3].as_ref().unwrap().author, "Second Author");
        assert_eq!(b[3].as_ref().unwrap().summary, "add three");
        assert_eq!(b[1].as_ref().unwrap().date.len(), 10);

        let lens = author_lens(&b, &[Decl { line: 0, start: 1, end: 3 }, Decl { line: 0, start: 0, end: 0 }]);
        assert_eq!(lens.len(), 1);
        assert_eq!(lens[0].text(), "Test User +1");
    }
}
