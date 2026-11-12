//! Düzenlenebilir metin: rope, dosya G/Ç, satır sonu / girinti tespiti, değişiklik geçmişi.

use std::path::{Path, PathBuf};

use ropey::Rope;

use crate::document::{LoadError, MAX_BYTES};
use crate::history::{Edit, History, RawEdit};
use crate::selection::Selection;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    CrLf,
}

impl LineEnding {
    pub fn as_str(self) -> &'static str {
        match self {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        }
    }
}

/// Girinti birimi (`>>`, Tab, `o` için).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Indent {
    Spaces(u8),
    Tabs,
}

impl Indent {
    pub fn unit(self) -> String {
        match self {
            Indent::Spaces(n) => " ".repeat(n as usize),
            Indent::Tabs => "\t".to_string(),
        }
    }

    /// Bir girinti biriminin görsel genişliği.
    pub fn width(self, tab_width: usize) -> usize {
        match self {
            Indent::Spaces(n) => n as usize,
            Indent::Tabs => tab_width,
        }
    }
}

/// Dilin yorum işaretleri; satır yorumu yoksa her satır blok yorumla sarılır.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommentToken {
    pub line: Option<String>,
    pub block: Option<(String, String)>,
}

impl CommentToken {
    pub fn line(token: &str) -> Self {
        Self { line: Some(token.to_string()), block: None }
    }

    pub fn block(open: &str, close: &str) -> Self {
        Self { line: None, block: Some((open.to_string(), close.to_string())) }
    }

    /// Dosya uzantısından yorum işareti (uygulama dil bilgisiyle ezebilir).
    pub fn for_path(path: &Path) -> Self {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_ascii_lowercase();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        if matches!(name.as_str(), "dockerfile" | "makefile" | "brewfile" | ".gitignore") {
            return Self::line("#");
        }
        match ext.as_str() {
            "rs" | "java" | "kt" | "kts" | "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "c" | "h" | "cpp" | "hpp"
            | "cc" | "go" | "swift" | "scala" | "groovy" | "gradle" | "dart" | "zig" | "jsonc" | "scss" | "less"
            | "proto" | "cs" => Self::line("//"),
            "py" | "sh" | "bash" | "zsh" | "fish" | "rb" | "pl" | "yaml" | "yml" | "toml" | "properties" | "conf"
            | "ghostty" | "r" | "ex" | "exs" | "nix" | "mk" | "env" | "ini" => Self::line("#"),
            "lua" | "sql" | "hs" | "elm" => Self::line("--"),
            "vim" => Self::line("\""),
            "lisp" | "clj" | "el" | "scm" => Self::line(";"),
            "css" => Self::block("/*", "*/"),
            "html" | "htm" | "xml" | "md" | "markdown" | "vue" | "svelte" | "svg" => Self::block("<!--", "-->"),
            _ => Self::line("#"),
        }
    }
}

/// Bir sekmedeki metin. İçeride satır sonu her zaman `\n`; dosyadaki son satır sonu `final_newline` olarak tutulur
/// (Vim'in `eol` davranışı: "a\n" tek satırdır).
#[derive(Debug)]
pub struct Buffer {
    rope: Rope,
    pub path: Option<PathBuf>,
    pub line_ending: LineEnding,
    pub final_newline: bool,
    pub indent: Indent,
    pub tab_width: usize,
    pub comment: CommentToken,
    pub(crate) history: History,
    saved_rev: u64,
    revision: u64,
}

impl Default for Buffer {
    fn default() -> Self {
        Self::from_str("")
    }
}

impl Buffer {
    /// Metinden buffer (yolsuz). Satır sonu ve girinti metinden tespit edilir.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(text: &str) -> Self {
        let line_ending = detect_line_ending(text);
        let mut normalized = text.replace("\r\n", "\n");
        let final_newline = normalized.ends_with('\n');
        if final_newline {
            normalized.pop();
        }
        let indent = detect_indent(&normalized);
        Self {
            rope: Rope::from_str(&normalized),
            path: None,
            line_ending,
            final_newline: final_newline || text.is_empty(),
            indent,
            tab_width: 4,
            comment: CommentToken::line("#"),
            history: History::default(),
            saved_rev: 0,
            revision: 0,
        }
    }

    /// Dosyayı açar; 8 MB üstü ve NUL içeren dosyalar reddedilir, geçersiz UTF-8 kayıplı çevrilir.
    pub fn load(path: &Path) -> Result<Self, LoadError> {
        let size = std::fs::metadata(path).map_err(|e| LoadError::Io(e.to_string()))?.len();
        if size > MAX_BYTES {
            return Err(LoadError::TooLarge(size));
        }
        let bytes = std::fs::read(path).map_err(|e| LoadError::Io(e.to_string()))?;
        if bytes.iter().take(8192).any(|&b| b == 0) {
            return Err(LoadError::Binary);
        }
        let text = String::from_utf8_lossy(&bytes);
        let mut buf = Self::from_str(&text);
        buf.path = Some(path.to_path_buf());
        buf.comment = CommentToken::for_path(path);
        Ok(buf)
    }

    /// Kayıtlı yola yazar.
    pub fn save(&mut self) -> std::io::Result<()> {
        let path = self.path.clone().ok_or_else(|| std::io::Error::other("No file name"))?;
        self.save_as(&path)
    }

    /// Verilen yola yazar ve yolu günceller.
    pub fn save_as(&mut self, path: &Path) -> std::io::Result<()> {
        self.history.checkpoint();
        std::fs::write(path, self.file_contents())?;
        self.path = Some(path.to_path_buf());
        self.saved_rev = self.history.current_rev();
        Ok(())
    }

    /// Diske yazılacak metin (satır sonları ve son satır sonu geri konmuş).
    pub fn file_contents(&self) -> String {
        let mut s = self.rope.to_string();
        if self.final_newline && !s.is_empty() {
            s.push('\n');
        }
        if self.line_ending == LineEnding::CrLf { s.replace('\n', "\r\n") } else { s }
    }

    pub fn rope(&self) -> &Rope {
        &self.rope
    }

    /// Metin (`\n` ile, son satır sonu olmadan).
    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn line(&self, line: usize) -> String {
        crate::text::line_text(&self.rope, line)
    }

    /// Kaydedilmemiş değişiklik var mı (geri alınarak kayıtlı duruma dönülünce yine temiz).
    pub fn is_dirty(&self) -> bool {
        self.history.current_rev() != self.saved_rev || self.history.has_open_edits()
    }

    /// Her değişiklikte artar (önbellek anahtarı).
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// `start..end` aralığını `text` ile değiştirir ve geçmişe yazar.
    pub(crate) fn replace(&mut self, start: usize, end: usize, text: &str, sel: &Selection) {
        let removed = self.rope.slice(start..end).to_string();
        if removed.is_empty() && text.is_empty() {
            return;
        }
        self.rope.remove(start..end);
        self.rope.insert(start, text);
        self.revision += 1;
        self.history.record(Edit { pos: start, removed, inserted: text.to_string() }, sel);
    }

    pub(crate) fn undo(&mut self) -> Option<(Selection, Vec<RawEdit>)> {
        let r = self.history.undo(&mut self.rope);
        if r.is_some() {
            self.revision += 1;
        }
        r
    }

    pub(crate) fn redo(&mut self) -> Option<(Selection, Vec<RawEdit>)> {
        let r = self.history.redo(&mut self.rope);
        if r.is_some() {
            self.revision += 1;
        }
        r
    }
}

/// CRLF satırları çoğunluktaysa CRLF.
pub fn detect_line_ending(text: &str) -> LineEnding {
    let lf = text.matches('\n').count();
    let crlf = text.matches("\r\n").count();
    if crlf > 0 && crlf * 2 >= lf { LineEnding::CrLf } else { LineEnding::Lf }
}

/// Sekmeyle başlayan satırlar çoğunluktaysa sekme; değilse ardışık satırların girinti artışlarının en sık görüleni (2/4/8),
/// tespit edilemezse 4 boşluk.
pub fn detect_indent(text: &str) -> Indent {
    let mut tabs = 0;
    let mut spaces = 0;
    let mut counts = [0usize; 9];
    let mut prev = 0usize;
    for line in text.lines().take(2000) {
        if line.trim().is_empty() {
            continue;
        }
        if line.starts_with('\t') {
            tabs += 1;
            continue;
        }
        let width = line.len() - line.trim_start_matches(' ').len();
        if width > 0 {
            spaces += 1;
        }
        if width > prev {
            let d = width - prev;
            if (2..=8).contains(&d) {
                counts[d] += 1;
            }
        }
        prev = width;
    }
    if tabs > spaces {
        return Indent::Tabs;
    }
    let best = [2usize, 4, 8, 3].into_iter().max_by_key(|&d| (counts[d], d == 4)).unwrap_or(4);
    if counts[best] == 0 { Indent::Spaces(4) } else { Indent::Spaces(best as u8) }
}
