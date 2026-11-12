//! Projede metin arama (Cmd+Shift+F): `rg` (ripgrep) PATH'te ya da Homebrew'da varsa onunla, yoksa dizindeki dosyalarda
//! Rust `regex` ile. İki yolda da satırdaki eşleşme aralıkları aynı derlenmiş regex'ten hesaplanır (rg yalnız satırı
//! bulur), böylece büyük/küçük harf, tam kelime ve regex seçenekleri tutarlıdır.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use less_workspace::search::{FileEntry, Hit, MAX_GREP_BYTES};
use regex::Regex;

use crate::editor::find::FindOptions;

const MAX_SNIPPET: usize = 240;

/// Kullanılabilir `rg` ikilisi (bir kez aranır). GUI uygulamasının PATH'i dar olduğu için Homebrew yolları da denenir.
fn ripgrep() -> Option<&'static Path> {
    static RG: OnceLock<Option<PathBuf>> = OnceLock::new();
    RG.get_or_init(|| {
        let path = std::env::var_os("PATH").unwrap_or_default();
        std::env::split_paths(&path)
            .chain(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from))
            .map(|d| d.join("rg"))
            .find(|p| p.is_file())
    })
    .as_deref()
}

/// Aramayı çalıştırır; her eşleşen satır için `on_hit`, `false` dönerse durur. `cancel` kurulunca durur.
/// Dönen değer: sonuna kadar gitti mi.
pub fn run(
    files: &[FileEntry],
    roots: &[PathBuf],
    query: &str,
    opts: FindOptions,
    cancel: &AtomicBool,
    on_hit: impl FnMut(Hit) -> bool,
) -> bool {
    let Ok(re) = opts.compile(query) else { return true };
    if query.is_empty() {
        return true;
    }
    match ripgrep() {
        Some(rg) if !cfg!(test) => with_ripgrep(rg, roots, query, opts, &re, cancel, on_hit),
        _ => in_files(files, &re, cancel, on_hit),
    }
}

/// Dizindeki dosyalarda arar (ikili ve `MAX_GREP_BYTES` üstü dosyalar atlanır).
fn in_files(files: &[FileEntry], re: &Regex, cancel: &AtomicBool, mut on_hit: impl FnMut(Hit) -> bool) -> bool {
    for f in files {
        if cancel.load(Ordering::Relaxed) {
            return false;
        }
        let Some(text) = read_text(&f.path) else { continue };
        for (n, raw) in text.split('\n').enumerate() {
            let line = raw.strip_suffix('\r').unwrap_or(raw);
            if let Some(hit) = hit(&f.path, &f.rel, n, line, re)
                && !on_hit(hit)
            {
                return false;
            }
        }
    }
    true
}

/// `rg --null` çıktısını satır satır okur: `yol\0satır:metin`.
#[allow(clippy::too_many_arguments)]
fn with_ripgrep(
    rg: &Path,
    roots: &[PathBuf],
    query: &str,
    opts: FindOptions,
    re: &Regex,
    cancel: &AtomicBool,
    mut on_hit: impl FnMut(Hit) -> bool,
) -> bool {
    let mut cmd = Command::new(rg);
    cmd.args(["--null", "--line-number", "--no-heading", "--color", "never", "--no-messages", "--max-filesize"])
        .arg(MAX_GREP_BYTES.to_string())
        .arg(if opts.case { "--case-sensitive" } else { "--ignore-case" });
    if !opts.regex {
        cmd.arg("--fixed-strings");
    }
    if opts.word {
        cmd.arg("--word-regexp");
    }
    cmd.arg("-e").arg(query).arg("--").args(roots).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    let Ok(mut child) = cmd.spawn() else { return true };
    let Some(out) = child.stdout.take() else { return true };
    let mut finished = true;
    for line in BufReader::new(out).split(b'\n') {
        let Ok(line) = line else { break };
        if cancel.load(Ordering::Relaxed) {
            finished = false;
            break;
        }
        let Some((path, rest)) = parse_rg_line(&line) else { continue };
        let Some(hit) = hit(&path, &crate::picker::relative(&path, roots), rest.0, &rest.1, re) else { continue };
        if !on_hit(hit) {
            finished = false;
            break;
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    finished
}

/// `yol\0satır:metin` → `(yol, (0 tabanlı satır, metin))`.
fn parse_rg_line(line: &[u8]) -> Option<(PathBuf, (usize, String))> {
    let nul = line.iter().position(|&b| b == 0)?;
    let path = PathBuf::from(String::from_utf8_lossy(&line[..nul]).into_owned());
    let rest = String::from_utf8_lossy(&line[nul + 1..]);
    let (num, text) = rest.split_once(':')?;
    let text = text.strip_suffix('\r').unwrap_or(text);
    Some((path, (num.parse::<usize>().ok()?.checked_sub(1)?, text.to_owned())))
}

/// Satırda eşleşme varsa sonuç. `ranges` satırdaki bayt aralıkları (kod alanının işaretlediği), `snippet_ranges`
/// gösterim metnindeki karakter aralıkları.
fn hit(path: &Path, rel: &str, line: usize, text: &str, re: &Regex) -> Option<Hit> {
    let ranges: Vec<(usize, usize)> =
        re.find_iter(text).filter(|m| m.end() > m.start()).map(|m| (m.start(), m.end())).collect();
    if ranges.is_empty() {
        return None;
    }
    let indent = text.len() - text.trim_start().len();
    let shown = &text[indent..];
    let snippet: String = shown.chars().take(MAX_SNIPPET).map(|c| if c == '\t' { ' ' } else { c }).collect();
    let char_of = |b: usize| shown[..b.saturating_sub(indent).min(shown.len())].chars().count();
    let snippet_ranges = ranges
        .iter()
        .filter(|(s, _)| *s >= indent && char_of(*s) < MAX_SNIPPET)
        .map(|&(s, e)| (char_of(s), char_of(e).min(MAX_SNIPPET)))
        .collect();
    Some(Hit { path: path.to_path_buf(), rel: rel.to_owned(), line, ranges, snippet, snippet_ranges })
}

fn read_text(path: &Path) -> Option<String> {
    if std::fs::metadata(path).ok()?.len() > MAX_GREP_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    if bytes.iter().take(8192).any(|&b| b == 0) {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_search_honours_case_word_and_regex_options() {
        let dir = std::env::temp_dir().join(format!("less-h-grep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.rs"), "fn Foo() {}\n\tlet food = foo;\n").unwrap();
        let files = vec![FileEntry { path: dir.join("a.rs"), rel: "a.rs".into() }];
        let roots = [dir.clone()];
        let stop = AtomicBool::new(false);
        let search = |q: &str, o: FindOptions| {
            let mut hits = Vec::new();
            run(&files, &roots, q, o, &stop, |h| {
                hits.push(h);
                true
            });
            hits
        };
        let all = search("foo", FindOptions::default());
        assert_eq!(all.len(), 2);
        assert_eq!(all[1].ranges, [(5, 8), (12, 15)], "byte ranges in the raw line");
        assert_eq!(all[1].snippet, "let food = foo;");
        assert_eq!(all[1].snippet_ranges, [(4, 7), (11, 14)]);
        assert_eq!(search("foo", FindOptions { case: true, ..Default::default() }).len(), 1);
        let word = search("foo", FindOptions { word: true, ..Default::default() });
        assert_eq!(word[1].ranges, [(12, 15)]);
        let re = search(r"F\w+\(", FindOptions { regex: true, case: true, word: false });
        assert_eq!((re.len(), re[0].line), (1, 0));
        assert!(search("(", FindOptions { regex: true, ..Default::default() }).is_empty(), "invalid regex");
        if let Some(rg) = ripgrep() {
            let re = FindOptions::default().compile("foo").unwrap();
            let mut rg_hits = Vec::new();
            with_ripgrep(rg, &roots, "foo", FindOptions::default(), &re, &stop, |h| {
                rg_hits.push(h);
                true
            });
            rg_hits.sort_by_key(|h| h.line);
            assert_eq!(rg_hits, all, "ripgrep and the Rust fallback agree");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ripgrep_null_lines_parse() {
        let (p, (line, text)) = parse_rg_line(b"/x/a:b.rs\x0012:let x = 1;\r").unwrap();
        assert_eq!((p, line, text.as_str()), (PathBuf::from("/x/a:b.rs"), 11, "let x = 1;"));
        assert!(parse_rg_line(b"no-nul").is_none());
    }
}
