//! Diagnostik deposu ve problem göstergesi gezinmesi (#31).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Önem derecesi; sıralama: `Error` en yüksek.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

impl Severity {
    pub const ALL: [Severity; 4] = [Severity::Error, Severity::Warning, Severity::Info, Severity::Hint];

    /// LSP `DiagnosticSeverity` sayısı (1..4); bilinmeyen/eksik → `Error` (LSP varsayılanı istemciye kalmış).
    pub fn from_lsp(value: Option<i64>) -> Severity {
        match value {
            Some(2) => Severity::Warning,
            Some(3) => Severity::Info,
            Some(4) => Severity::Hint,
            _ => Severity::Error,
        }
    }

    /// Unicode ikon (Nerd Font gerekmez): `✖ ⚠ ℹ •`.
    pub fn icon(self) -> &'static str {
        match self {
            Severity::Error => "\u{2716}",
            Severity::Warning => "\u{26a0}",
            Severity::Info => "\u{2139}",
            Severity::Hint => "\u{2022}",
        }
    }
}

/// Konum: 0 tabanlı satır ve LSP karakter sütunu (UTF-16 birimi; ASCII'de bayt ile aynı).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Pos {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub start: Pos,
    pub end: Pos,
    pub severity: Severity,
    pub message: String,
    pub source: Option<String>,
    pub code: Option<String>,
}

impl Diagnostic {
    /// LSP `Diagnostic` JSON'undan.
    pub fn from_lsp(value: &serde_json::Value) -> Option<Diagnostic> {
        let pos = |v: &serde_json::Value| -> Option<Pos> {
            Some(Pos { line: v.get("line")?.as_u64()? as u32, character: v.get("character")?.as_u64()? as u32 })
        };
        let range = value.get("range")?;
        let code = value.get("code").and_then(|c| match c {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            _ => None,
        });
        Some(Diagnostic {
            start: pos(range.get("start")?)?,
            end: pos(range.get("end")?)?,
            severity: Severity::from_lsp(value.get("severity").and_then(|s| s.as_i64())),
            message: value.get("message")?.as_str()?.to_string(),
            source: value.get("source").and_then(|s| s.as_str()).map(str::to_string),
            code,
        })
    }
}

/// Önem derecesine göre sayılar (winbar sayacı: `✖ 1  ⚠ 3`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SeverityCounts {
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
    pub hints: usize,
}

impl SeverityCounts {
    pub fn get(&self, severity: Severity) -> usize {
        match severity {
            Severity::Error => self.errors,
            Severity::Warning => self.warnings,
            Severity::Info => self.infos,
            Severity::Hint => self.hints,
        }
    }

    pub fn total(&self) -> usize {
        self.errors + self.warnings + self.infos + self.hints
    }

    /// Gezinmede kullanılacak en yüksek önem derecesi (problem yoksa `None`).
    pub fn top(&self) -> Option<Severity> {
        Severity::ALL.into_iter().find(|s| self.get(*s) > 0)
    }
}

/// Dosya başına diagnostikler. Her sunucu kendi listesini verir; dosyanın listesi sunucuların birleşimidir.
#[derive(Debug, Default)]
pub struct DiagnosticStore {
    by_file: HashMap<PathBuf, HashMap<String, Vec<Diagnostic>>>,
}

impl DiagnosticStore {
    /// Bir sunucunun bir dosya için listesini değiştirir (boş liste = temizle).
    pub fn set(&mut self, path: &Path, server: &str, mut diags: Vec<Diagnostic>) {
        diags.sort_by(|a, b| a.start.cmp(&b.start).then(a.severity.cmp(&b.severity)));
        let entry = self.by_file.entry(path.to_path_buf()).or_default();
        if diags.is_empty() {
            entry.remove(server);
        } else {
            entry.insert(server.to_string(), diags);
        }
        if entry.is_empty() {
            self.by_file.remove(path);
        }
    }

    /// Sunucu kapanınca onun tüm diagnostikleri silinir.
    pub fn clear_server(&mut self, server: &str) {
        self.by_file.retain(|_, m| {
            m.remove(server);
            !m.is_empty()
        });
    }

    /// Dosyanın tüm diagnostikleri, konuma göre sıralı.
    pub fn get(&self, path: &Path) -> Vec<&Diagnostic> {
        let mut all: Vec<&Diagnostic> = self.by_file.get(path).into_iter().flat_map(|m| m.values().flatten()).collect();
        all.sort_by(|a, b| a.start.cmp(&b.start).then(a.severity.cmp(&b.severity)));
        all
    }

    /// Diagnostiği olan dosyalar.
    pub fn files(&self) -> impl Iterator<Item = &Path> {
        self.by_file.keys().map(PathBuf::as_path)
    }

    pub fn counts(&self, path: &Path) -> SeverityCounts {
        let mut c = SeverityCounts::default();
        for d in self.get(path) {
            match d.severity {
                Severity::Error => c.errors += 1,
                Severity::Warning => c.warnings += 1,
                Severity::Info => c.infos += 1,
                Severity::Hint => c.hints += 1,
            }
        }
        c
    }

    /// Satırın diagnostikleri: önce önem derecesi, sonra sütun (tooltip sırası, #52). Çok satırlı aralık da sayılır.
    pub fn line(&self, path: &Path, line: u32) -> Vec<&Diagnostic> {
        let mut out: Vec<&Diagnostic> = self
            .get(path)
            .into_iter()
            .filter(|d| d.start.line <= line && line <= d.end.line.max(d.start.line))
            .collect();
        out.sort_by(|a, b| a.severity.cmp(&b.severity).then(a.start.character.cmp(&b.start.character)));
        out
    }

    /// Sonraki (`forward`) ya da önceki problem; dosya sonunda başa sarar (#31).
    /// `severity` `None` ise yalnız en yüksek önem derecesindekiler arasında gezilir (↑/↓ okları);
    /// verilirse yalnız o derece (sayaçtaki ✖/⚠ tıklaması).
    pub fn jump(&self, path: &Path, from: Pos, forward: bool, severity: Option<Severity>) -> Option<&Diagnostic> {
        let severity = severity.or_else(|| self.counts(path).top())?;
        let list: Vec<&Diagnostic> = self.get(path).into_iter().filter(|d| d.severity == severity).collect();
        if forward {
            list.iter().find(|d| d.start > from).or(list.first()).copied()
        } else {
            list.iter().rev().find(|d| d.start < from).or(list.last()).copied()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(line: u32, ch: u32, severity: Severity, msg: &str) -> Diagnostic {
        Diagnostic {
            start: Pos { line, character: ch },
            end: Pos { line, character: ch + 1 },
            severity,
            message: msg.into(),
            source: None,
            code: None,
        }
    }

    #[test]
    fn counts_and_highest_severity_first_31() {
        let mut store = DiagnosticStore::default();
        let p = Path::new("/a.java");
        store.set(
            p,
            "jdtls",
            vec![
                d(10, 0, Severity::Warning, "w1"),
                d(2, 4, Severity::Error, "e1"),
                d(20, 0, Severity::Error, "e2"),
                d(5, 0, Severity::Warning, "w0"),
            ],
        );
        let c = store.counts(p);
        assert_eq!((c.errors, c.warnings, c.total()), (2, 2, 4));
        let at = |line| Pos { line, character: 0 };
        assert_eq!(store.jump(p, at(0), true, None).unwrap().message, "e1");
        assert_eq!(store.jump(p, at(3), true, None).unwrap().message, "e2");
        assert_eq!(store.jump(p, at(25), true, None).unwrap().message, "e1", "wraps");
        assert_eq!(store.jump(p, at(1), false, None).unwrap().message, "e2", "wraps backwards");
        assert_eq!(store.jump(p, at(6), true, Some(Severity::Warning)).unwrap().message, "w1");

        store.set(p, "jdtls", vec![d(5, 0, Severity::Warning, "w0")]);
        assert_eq!(store.counts(p).top(), Some(Severity::Warning));
        assert_eq!(store.jump(p, at(9), true, None).unwrap().message, "w0");
        store.set(p, "jdtls", vec![]);
        assert!(store.jump(p, at(0), true, None).is_none());
        assert_eq!(store.files().count(), 0);
    }

    #[test]
    fn line_order_and_lsp_parse_52() {
        let mut store = DiagnosticStore::default();
        let p = Path::new("/a.ts");
        store.set(
            p,
            "vtsls",
            vec![d(1, 9, Severity::Warning, "w"), d(1, 3, Severity::Error, "e2"), d(1, 1, Severity::Error, "e1")],
        );
        store.set(p, "eslint", vec![d(1, 0, Severity::Hint, "h")]);
        let msgs: Vec<_> = store.line(p, 1).iter().map(|d| d.message.as_str()).collect();
        assert_eq!(msgs, ["e1", "e2", "w", "h"]);
        store.clear_server("vtsls");
        assert_eq!(store.get(p).len(), 1);

        let json = serde_json::json!({
            "range": {"start": {"line": 3, "character": 2}, "end": {"line": 3, "character": 8}},
            "severity": 2, "message": "unused", "source": "ts", "code": 6133
        });
        let parsed = Diagnostic::from_lsp(&json).unwrap();
        assert_eq!((parsed.severity, parsed.code.as_deref()), (Severity::Warning, Some("6133")));
    }
}
