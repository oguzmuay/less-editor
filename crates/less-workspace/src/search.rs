//! Proje içi arama, UI bilmez: dosya listesi (Cmd+F), bulanık eşleştirme (dosya, komut, kısayol listeleri) ve metin
//! araması (Cmd+Shift+F). Metin araması satırları `less_core::Document` ile aynı biçimde böler (sekme = 4 boşluk,
//! `\r` atılır); böylece sonuçtaki sütunlar dosya görünümündeki sütunlarla aynıdır.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Taranmayan klasörler (bağımlılıklar, derleme çıktıları, araç durumları).
pub const SKIP_DIRS: [&str; 14] = [
    ".git",
    "node_modules",
    "target",
    "build",
    "dist",
    "out",
    ".gradle",
    ".idea",
    ".next",
    ".nuxt",
    ".venv",
    "__pycache__",
    ".less",
    ".DS_Store",
];
/// Dosya listesinin üst sınırı (dev bir klasör açılırsa arayüz donmasın).
pub const MAX_FILES: usize = 100_000;
/// Metin aramasında bunun üstündeki dosyalar atlanır.
pub const MAX_GREP_BYTES: u64 = 2 * 1024 * 1024;
const MAX_SNIPPET: usize = 240;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileEntry {
    pub path: PathBuf,
    /// Gösterilen yol: köke göre; birden çok kök varsa başında kök klasörün adı (`backend/src/App.java`).
    pub rel: String,
}

impl FileEntry {
    /// `rel` içinde dosya adının başladığı karakter sırası.
    pub fn name_start(&self) -> usize {
        self.rel.rfind('/').map(|i| self.rel[..=i].chars().count()).unwrap_or(0)
    }
}

/// Köklerdeki dosyaları listeler: her klasörde önce dosyalar (alfabetik), sonra alt klasörler. Sembolik bağlantılı klasörlere girilmez (döngü);
/// sembolik bağlantılı dosyalar alınır. `limit`'e ulaşınca durur.
pub fn list_files(roots: &[PathBuf], limit: usize) -> Vec<FileEntry> {
    let mut out = Vec::new();
    let multi = roots.len() > 1;
    for root in roots {
        let prefix = if multi {
            root.file_name().map(|n| format!("{}/", n.to_string_lossy())).unwrap_or_default()
        } else {
            String::new()
        };
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(read) = std::fs::read_dir(&dir) else { continue };
            let mut entries: Vec<_> = read.flatten().collect();
            entries.sort_by_key(|e| e.file_name().to_string_lossy().to_lowercase());
            let mut dirs = Vec::new();
            for e in entries {
                let name = e.file_name();
                let name = name.to_string_lossy();
                if SKIP_DIRS.contains(&name.as_ref()) {
                    continue;
                }
                let Ok(ft) = e.file_type() else { continue };
                let path = e.path();
                if ft.is_dir() {
                    dirs.push(path);
                } else if ft.is_file() || (ft.is_symlink() && path.is_file()) {
                    let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
                    out.push(FileEntry { rel: format!("{prefix}{rel}"), path });
                    if out.len() >= limit {
                        return out;
                    }
                }
            }
            stack.extend(dirs.into_iter().rev());
        }
    }
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub score: i32,
    /// Eşleşen karakterlerin `candidate` içindeki sıraları (char index), artan.
    pub positions: Vec<usize>,
}

/// Bulanık eşleştirme: sorgudaki harfler (boşluklar yok sayılır) adayda sırayla geçmeli; büyük/küçük harf duyarsız.
/// Puan: eşleşen harf başına, art arda gelen harflere, kelime başlarına (`/ _ - . boşluk` sonrası ve camelCase) ve
/// son `/`'den sonraki kısımda (dosya adı) eşleşmeye ek puan; aradaki boşluklar ve uzun aday puan düşürür.
/// Önce sorgunun tamamı dosya adında aranır, olmazsa tüm yolda. Boş sorgu her adayla 0 puanla eşleşir.
pub fn fuzzy(query: &str, candidate: &str) -> Option<Match> {
    let q: Vec<char> = query.chars().filter(|c| !c.is_whitespace()).flat_map(char::to_lowercase).collect();
    if q.is_empty() {
        return Some(Match { score: 0, positions: Vec::new() });
    }
    let chars: Vec<char> = candidate.chars().collect();
    let name_start = chars.iter().rposition(|&c| c == '/').map(|i| i + 1).unwrap_or(0);
    let in_name = match_from(&q, &chars, name_start);
    let (positions, name_bonus) = match in_name {
        Some(p) => (p, 25),
        None => (match_from(&q, &chars, 0)?, 0),
    };
    let mut score = name_bonus - (chars.len() as i32) / 6;
    let mut prev: Option<usize> = None;
    for &i in &positions {
        score += 10;
        if i >= name_start {
            score += 3;
        }
        if is_word_start(&chars, i) {
            score += 8;
        }
        match prev {
            Some(p) if p + 1 == i => score += 9,
            Some(p) => score -= ((i - p - 1) as i32).min(8),
            None => score -= (i.saturating_sub(name_start) as i32).min(6),
        }
        prev = Some(i);
    }
    Some(Match { score, positions })
}

/// `start`'tan itibaren sorgu harflerini sırayla arar; her harf için bir sonraki harfle bitişik ya da kelime başı
/// olan konumu tercih eder (açgözlü ama okunur sonuç verir).
fn match_from(q: &[char], chars: &[char], start: usize) -> Option<Vec<usize>> {
    let lower: Vec<char> = chars.iter().map(|c| c.to_lowercase().next().unwrap_or(*c)).collect();
    let mut out = Vec::with_capacity(q.len());
    let mut from = start;
    for (k, &qc) in q.iter().enumerate() {
        let first = (from..lower.len()).find(|&i| lower[i] == qc)?;
        let mut pick = first;
        if !out.last().is_some_and(|&l: &usize| l + 1 == first) && !is_word_start(chars, first) {
            let rest_fits = |i: usize| {
                let mut j = i + 1;
                q[k + 1..].iter().all(|&c| match (j..lower.len()).find(|&x| lower[x] == c) {
                    Some(x) => {
                        j = x + 1;
                        true
                    }
                    None => false,
                })
            };
            if let Some(better) = (first + 1..lower.len()).find(|&i| lower[i] == qc && is_word_start(chars, i))
                && rest_fits(better)
            {
                pick = better;
            }
        }
        out.push(pick);
        from = pick + 1;
    }
    Some(out)
}

fn is_word_start(chars: &[char], i: usize) -> bool {
    if i == 0 {
        return true;
    }
    let (prev, cur) = (chars[i - 1], chars[i]);
    matches!(prev, '/' | '_' | '-' | '.' | ' ' | ':') || (prev.is_lowercase() && cur.is_uppercase())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub path: PathBuf,
    pub rel: String,
    /// 0 tabanlı satır.
    pub line: usize,
    /// Dosya görünümündeki satırdaki eşleşmeler: `(başlangıç, bitiş)` char sırası, bitiş hariç.
    pub ranges: Vec<(usize, usize)>,
    /// Gösterim için satır: baştaki boşluk atılmış, en fazla 240 karakter.
    pub snippet: String,
    /// `ranges`'in `snippet`'e göre karşılığı (kırpılan kısımdakiler düşer).
    pub snippet_ranges: Vec<(usize, usize)>,
}

/// Akıllı büyük/küçük harf: sorguda büyük harf varsa duyarlı, yoksa duyarsız düz metin araması.
pub fn case_sensitive(query: &str) -> bool {
    query.chars().any(char::is_uppercase)
}

/// Dosyalarda düz metin arar; her bulunan satır için `on_hit` çağrılır, `false` dönerse arama durur. `cancel`
/// kurulursa dosya sınırında durur. İkili (ilk 8 KB'de NUL) ve `MAX_GREP_BYTES` üstü dosyalar atlanır.
/// Dönen değer: arama sonuna kadar gitti mi.
pub fn grep(files: &[FileEntry], query: &str, cancel: &AtomicBool, mut on_hit: impl FnMut(Hit) -> bool) -> bool {
    if query.is_empty() {
        return true;
    }
    let sensitive = case_sensitive(query);
    let needle: Vec<char> = if sensitive { query.chars().collect() } else { lower_chars(query) };
    for f in files {
        if cancel.load(Ordering::Relaxed) {
            return false;
        }
        let Some(text) = read_text(&f.path) else { continue };
        for (n, raw) in text.split('\n').enumerate() {
            let line = raw.strip_suffix('\r').unwrap_or(raw).replace('\t', "    ");
            let hay: Vec<char> = if sensitive { line.chars().collect() } else { lower_chars(&line) };
            let ranges = find_all(&hay, &needle);
            if ranges.is_empty() {
                continue;
            }
            let hit = snippet_hit(f, n, &line, ranges);
            if !on_hit(hit) {
                return false;
            }
        }
    }
    true
}

/// Karakter başına küçük harf (char sayısı korunur; `İ` gibi çok karakterliye açılanlarda ilki alınır).
fn lower_chars(s: &str) -> Vec<char> {
    s.chars().map(|c| c.to_lowercase().next().unwrap_or(c)).collect()
}

fn find_all(hay: &[char], needle: &[char]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    if needle.is_empty() || hay.len() < needle.len() {
        return out;
    }
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        if hay[i..i + needle.len()] == *needle {
            out.push((i, i + needle.len()));
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

fn snippet_hit(f: &FileEntry, line: usize, text: &str, ranges: Vec<(usize, usize)>) -> Hit {
    let indent = text.chars().take_while(|c| c.is_whitespace()).count();
    let snippet: String = text.chars().skip(indent).take(MAX_SNIPPET).collect();
    let snippet_ranges = ranges
        .iter()
        .filter(|(s, _)| *s >= indent && s - indent < MAX_SNIPPET)
        .map(|(s, e)| (s - indent, (e - indent).min(MAX_SNIPPET)))
        .collect();
    Hit { path: f.path.clone(), rel: f.rel.clone(), line, ranges, snippet, snippet_ranges }
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

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("less-rs-search-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn lists_files_and_skips_build_dirs() {
        let root = tmp("list");
        for d in ["src/app", "node_modules/x", "target/debug", ".git"] {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        for f in ["src/app/Main.java", "src/b.ts", "node_modules/x/i.js", "target/debug/o", ".git/HEAD", "README.md"] {
            std::fs::write(root.join(f), "x").unwrap();
        }
        let rels: Vec<String> = list_files(std::slice::from_ref(&root), 100).into_iter().map(|f| f.rel).collect();
        assert_eq!(rels, ["README.md", "src/b.ts", "src/app/Main.java"]);
        assert_eq!(list_files(std::slice::from_ref(&root), 2).len(), 2);

        let other = tmp("list2");
        std::fs::write(other.join("x.rs"), "").unwrap();
        let both = list_files(&[root.clone(), other.clone()], 100);
        let name = other.file_name().unwrap().to_string_lossy();
        assert_eq!(both.last().unwrap().rel, format!("{name}/x.rs"));
        assert_eq!(both.last().unwrap().name_start(), name.chars().count() + 1);
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&other);
    }

    #[test]
    fn fuzzy_prefers_file_name_and_word_starts() {
        assert!(fuzzy("xyz", "src/main.rs").is_none());
        assert_eq!(fuzzy("", "a").unwrap().score, 0);
        let m = fuzzy("main", "src/main/java/Main.java").unwrap();
        assert_eq!(m.positions, [14, 15, 16, 17]);
        let a = fuzzy("uc", "src/UserController.java").unwrap();
        assert_eq!(a.positions, [4, 8]);
        let b = fuzzy("uc", "src/usecase.java").unwrap();
        assert!(a.score > b.score);
        let short = fuzzy("app", "App.tsx").unwrap().score;
        let deep = fuzzy("app", "web/src/pages/settings/AppearancePanel.tsx").unwrap().score;
        assert!(short > deep);
        assert!(fuzzy("ma ja", "Main.java").is_some());
    }

    #[test]
    fn grep_smart_case_tabs_and_snippets() {
        let root = tmp("grep");
        std::fs::write(root.join("a.txt"), "Hello\n\tsay hello hello\r\nnothing\n").unwrap();
        std::fs::write(root.join("b.bin"), [b'h', 0, b'e']).unwrap();
        let files = list_files(std::slice::from_ref(&root), 100);
        let cancel = AtomicBool::new(false);
        let mut hits = Vec::new();
        assert!(grep(&files, "hello", &cancel, |h| {
            hits.push(h);
            true
        }));
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[1].line, 1);
        assert_eq!(hits[1].ranges, [(8, 13), (14, 19)]);
        assert_eq!(hits[1].snippet, "say hello hello");
        assert_eq!(hits[1].snippet_ranges, [(4, 9), (10, 15)]);

        let mut n = 0;
        grep(&files, "Hello", &cancel, |_| {
            n += 1;
            true
        });
        assert_eq!(n, 1);
        assert!(!grep(&files, "hello", &cancel, |_| false));
        cancel.store(true, Ordering::Relaxed);
        assert!(!grep(&files, "hello", &cancel, |_| true));
        let _ = std::fs::remove_dir_all(&root);
    }
}
