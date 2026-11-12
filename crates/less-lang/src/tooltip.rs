//! Tooltip metni (#52 hata/uyarı, #70 fare üstünde dokümantasyon): satırın diagnostikleri + LSP hover markdown'ı.

use serde_json::Value;

use crate::diagnostics::{Diagnostic, Severity};

/// Tooltip en fazla bu kadar sütun genişliğinde sarılır (Lua'daki MAX_WIDTH).
pub const MAX_WIDTH: usize = 80;

/// Tooltip satırının rolü (tema rengi seçimi için).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TooltipStyle {
    /// Diagnostik mesajı; `prefix_len` baştaki ikon+boşluk bayt uzunluğu (renkli çizilir).
    Diagnostic { severity: Severity, prefix_len: usize },
    /// Satır sonundaki ` source(code)` eki (soluk), mesajın `from` baytından itibaren.
    DiagnosticWithSource { severity: Severity, prefix_len: usize, source_from: usize },
    /// Diagnostikler ile dokümantasyon arasındaki `─` çizgisi.
    Separator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TooltipLine {
    pub text: String,
    pub style: TooltipStyle,
}

/// Birleştirilmiş tooltip: üstte diagnostik satırları, altta (varsa) markdown dokümantasyon.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Tooltip {
    pub lines: Vec<TooltipLine>,
    /// Hover içeriği markdown olarak (`plaintext` geldiyse `markdown == false`).
    pub doc: Option<String>,
    pub doc_is_markdown: bool,
}

impl Tooltip {
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty() && self.doc.is_none()
    }

    /// Pencere genişliği: en geniş satır, en fazla [`MAX_WIDTH`] ve `columns - 4` (Lua `show`); en az 1.
    /// shortcut: genişlik `char` sayısı (geniş/CJK karakterler 1 sayılır), UI display width verirse onu kullan.
    pub fn width(&self, columns: usize) -> usize {
        let doc = self.doc.as_deref().unwrap_or_default().lines();
        let widest = self.lines.iter().map(|l| l.text.as_str()).chain(doc).map(|l| l.chars().count()).max();
        widest.unwrap_or(1).clamp(1, MAX_WIDTH).min(columns.saturating_sub(4).max(1))
    }
}

/// Diagnostikleri (sıralı verilmeli: [`crate::diagnostics::DiagnosticStore::line`]) ve her sunucunun hover
/// sonucunu birleştirir; dokümantasyon sunucu sırasıyla boş satırla eklenir, herhangi biri markdown ise markdown.
pub fn assemble(diags: &[&Diagnostic], hovers: &[Value]) -> Tooltip {
    let mut lines = Vec::new();
    for d in diags {
        let icon = format!("{} ", d.severity.icon());
        let pad = " ".repeat(icon.chars().count());
        let src = d.source.as_ref().map(|s| match &d.code {
            Some(code) => format!(" {s}({code})"),
            None => format!(" {s}"),
        });
        let msg_lines: Vec<&str> = d.message.trim().split('\n').collect();
        let count = msg_lines.len();
        for (i, text) in msg_lines.into_iter().enumerate() {
            let prefix = if i == 0 { icon.clone() } else { pad.clone() };
            let mut line = format!("{prefix}{text}");
            let mut style = TooltipStyle::Diagnostic { severity: d.severity, prefix_len: prefix.len() };
            if i + 1 == count
                && let Some(src) = &src
            {
                let source_from = line.len();
                line.push_str(src);
                style =
                    TooltipStyle::DiagnosticWithSource { severity: d.severity, prefix_len: prefix.len(), source_from };
            }
            lines.push(TooltipLine { text: line, style });
        }
    }
    let docs: Vec<(String, bool)> = hovers.iter().filter_map(hover_markdown).collect();
    let doc_is_markdown = docs.iter().any(|(_, md)| *md);
    let doc = (!docs.is_empty()).then(|| docs.into_iter().map(|(t, _)| t).collect::<Vec<_>>().join("\n\n"));
    if doc.is_some() && !lines.is_empty() {
        lines.push(TooltipLine { text: "\u{2500}".repeat(40), style: TooltipStyle::Separator });
    }
    Tooltip { lines, doc, doc_is_markdown }
}

/// LSP `Hover` sonucundan (ya da yalnız `contents`'ten) markdown metni; boşsa `None`.
/// `MarkedString { language, value }` kod bloğuna çevrilir; dizi elemanları boş satırla ayrılır.
pub fn hover_markdown(hover: &Value) -> Option<(String, bool)> {
    let contents = hover.get("contents").unwrap_or(hover);
    let mut markdown = true;
    let mut parts = Vec::new();
    let mut push = |v: &Value| match v {
        Value::String(s) => parts.push(s.clone()),
        Value::Object(o) => {
            if let Some(kind) = o.get("kind").and_then(Value::as_str) {
                markdown &= kind != "plaintext";
                parts.push(o.get("value").and_then(Value::as_str).unwrap_or_default().to_string());
            } else if let Some(value) = o.get("value").and_then(Value::as_str) {
                let lang = o.get("language").and_then(Value::as_str).unwrap_or("");
                parts.push(format!("```{lang}\n{value}\n```"));
            }
        }
        _ => {}
    };
    match contents {
        Value::Array(items) => items.iter().for_each(&mut push),
        other => push(other),
    }
    let text = parts.into_iter().filter(|p| !p.trim().is_empty()).collect::<Vec<_>>().join("\n\n");
    let text = text.trim_end().to_string();
    (!text.is_empty()).then_some((text, markdown))
}

/// Fare/imleç sütunundaki kelimenin (harf, rakam, `_`) başlangıç baytı; kelime üstünde değilse `None`
/// (hover yalnız kelime üstündeyken istenir).
pub fn word_start(line: &str, byte_col: usize) -> Option<usize> {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let c = line.get(byte_col..)?.chars().next()?;
    if !is_word(c) {
        return None;
    }
    let start =
        line[..byte_col].char_indices().rev().take_while(|(_, c)| is_word(*c)).last().map_or(byte_col, |(i, _)| i);
    Some(start)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::Pos;
    use serde_json::json;

    fn diag(sev: Severity, msg: &str, source: Option<&str>, code: Option<&str>) -> Diagnostic {
        Diagnostic {
            start: Pos::default(),
            end: Pos::default(),
            severity: sev,
            message: msg.into(),
            source: source.map(Into::into),
            code: code.map(Into::into),
        }
    }

    #[test]
    fn diagnostics_then_separator_then_hover_52_70() {
        let e = diag(Severity::Error, "Type mismatch\ncannot convert", Some("Java"), Some("16777233"));
        let w = diag(Severity::Warning, "unused", None, None);
        let hover = json!({ "contents": { "kind": "markdown", "value": "**String** foo()\n\nDocs." } });
        let tip = assemble(&[&e, &w], std::slice::from_ref(&hover));
        let texts: Vec<&str> = tip.lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts[0], "\u{2716} Type mismatch");
        assert_eq!(texts[1], "  cannot convert Java(16777233)");
        assert_eq!(texts[2], "\u{26a0} unused");
        assert_eq!(tip.lines[3].style, TooltipStyle::Separator);
        assert!(matches!(tip.lines[1].style, TooltipStyle::DiagnosticWithSource { source_from: 16, .. }));
        assert_eq!(tip.doc.as_deref(), Some("**String** foo()\n\nDocs."));
        assert!(tip.doc_is_markdown);

        let only_hover = assemble(&[], std::slice::from_ref(&hover));
        assert!(only_hover.lines.is_empty());
        assert!(assemble(&[], &[json!({"contents": ""})]).is_empty());
        assert_eq!(tip.width(200), 40);
        assert_eq!(tip.width(20), 16);
    }

    #[test]
    fn hovers_from_several_servers_merge_70() {
        let a = json!({ "contents": { "kind": "plaintext", "value": "from a" } });
        let b = json!({ "contents": { "kind": "markdown", "value": "from b" } });
        let tip = assemble(&[], &[a, json!(null), b]);
        assert_eq!(tip.doc.as_deref(), Some("from a\n\nfrom b"));
        assert!(tip.doc_is_markdown);
        let long = Tooltip { doc: Some("x".repeat(200)), ..Default::default() };
        assert_eq!(long.width(500), MAX_WIDTH);
    }

    #[test]
    fn hover_marked_strings_and_plaintext_70() {
        let h = json!({ "contents": [{ "language": "java", "value": "int x" }, "Some doc"] });
        assert_eq!(hover_markdown(&h), Some(("```java\nint x\n```\n\nSome doc".into(), true)));
        let p = json!({ "contents": { "kind": "plaintext", "value": "plain" } });
        assert_eq!(hover_markdown(&p), Some(("plain".into(), false)));
    }

    #[test]
    fn empty_hover_and_plaintext_boundaries_52_70() {
        let d = diag(Severity::Info, "details", Some("server"), None);
        let empty =
            [json!(null), json!({ "contents": [] }), json!({ "contents": { "kind": "plaintext", "value": "  " } })];
        let tip = assemble(&[&d], &empty);
        assert_eq!(tip.lines.len(), 1);
        assert_eq!(tip.lines[0].text, "\u{2139} details server");
        assert!(tip.doc.is_none());
        assert!(!tip.lines.iter().any(|line| line.style == TooltipStyle::Separator));

        let plain = assemble(&[], &[json!({ "contents": { "kind": "plaintext", "value": "plain" } })]);
        assert_eq!(plain.doc.as_deref(), Some("plain"));
        assert!(!plain.doc_is_markdown);
    }

    #[test]
    fn word_start_under_mouse_70() {
        assert_eq!(word_start("foo.barBaz()", 6), Some(4));
        assert_eq!(word_start("foo.barBaz()", 3), None);
        assert_eq!(word_start("çağrı(x)", 3), Some(0));
        assert_eq!(word_start("a😀b", 2), None, "middle of a UTF-8 character");
        assert_eq!(word_start("abc", 10), None);
    }
}
