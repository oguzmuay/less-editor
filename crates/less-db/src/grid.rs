//! Tablo görünümü yardımcıları (#50): kolon genişliği (en çok 40 görüntü sütunu, `…`), sayısal kolon sağa yaslı.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Kolon en fazla bu kadar görüntü sütunu.
pub const MAX_WIDTH: usize = 40;

/// Bir kolonun yerleşimi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnLayout {
    pub width: usize,
    pub numeric: bool,
}

/// Hücre metninin tek satırlık görünümü (satır sonu `↵`, sekme boşluk, NULL `NULL`).
pub fn display(v: Option<&str>) -> String {
    match v {
        None => "NULL".into(),
        Some(s) => s.replace("\r\n", "↵").replace('\n', "↵").replace('\t', " "),
    }
}

/// Görüntü genişliği.
pub fn display_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// Metni `width` sütuna keser (`…` ile) ve doldurur; `right` ise sağa yaslar.
pub fn fit(s: &str, width: usize, right: bool) -> String {
    let mut text = s.to_string();
    let mut w = display_width(s);
    if w > width {
        let mut out = String::new();
        let mut acc = 0;
        for ch in s.chars() {
            let cw = ch.width().unwrap_or(0);
            if acc + cw > width.saturating_sub(1) {
                break;
            }
            out.push(ch);
            acc += cw;
        }
        out.push('…');
        w = acc + 1;
        text = out;
    }
    let pad = " ".repeat(width.saturating_sub(w));
    if right { format!("{pad}{text}") } else { format!("{text}{pad}") }
}

/// `^-?\d+\.?\d*$`.
fn looks_numeric(s: &str) -> bool {
    let s = s.strip_prefix('-').unwrap_or(s);
    let (int, frac) = match s.split_once('.') {
        Some((i, f)) => (i, f),
        None => (s, ""),
    };
    !int.is_empty() && int.bytes().all(|b| b.is_ascii_digit()) && frac.bytes().all(|b| b.is_ascii_digit())
}

/// Kolon genişlikleri ve sayısal olup olmadıkları. Sayısal: boş olmayan her değer sayı ve en az bir tane var.
pub fn layout(columns: &[String], rows: &[Vec<Option<String>>]) -> Vec<ColumnLayout> {
    columns
        .iter()
        .enumerate()
        .map(|(c, name)| {
            let mut width = display_width(name).min(MAX_WIDTH);
            let mut any = false;
            let mut numeric = true;
            for row in rows {
                let v = row.get(c).and_then(|x| x.as_deref());
                width = width.min(MAX_WIDTH).max(display_width(&display(v)).min(MAX_WIDTH));
                if let Some(s) = v.filter(|s| !s.is_empty()) {
                    any = true;
                    numeric &= looks_numeric(s);
                }
            }
            ColumnLayout { width, numeric: numeric && any }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> Option<String> {
        Some(v.to_string())
    }

    #[test]
    fn fit_truncates_and_pads_50() {
        assert_eq!(fit("abcdef", 4, false), "abc…");
        assert_eq!(fit("ab", 4, false), "ab  ");
        assert_eq!(fit("ab", 4, true), "  ab");
        assert_eq!(fit("日本語", 5, false), "日本…");
        assert_eq!(display_width(&fit("日本語テキスト", 6, false)), 6);
    }

    #[test]
    fn layout_caps_at_40_and_detects_numeric_50() {
        let cols = vec!["id".to_string(), "name".to_string(), "empty".to_string()];
        let rows = vec![vec![s("1"), s(&"x".repeat(100)), None], vec![s("-2.5"), s("b"), s("")]];
        let l = layout(&cols, &rows);
        assert_eq!(l[0], ColumnLayout { width: 4, numeric: true });
        assert_eq!(l[1], ColumnLayout { width: 40, numeric: false });
        assert_eq!(l[2], ColumnLayout { width: 5, numeric: false });
    }

    #[test]
    fn numeric_mixed_and_null_50() {
        let cols = vec!["a".to_string()];
        assert!(!layout(&cols, &[vec![s("1")], vec![s("x")]])[0].numeric);
        assert!(layout(&cols, &[vec![s("1")], vec![None]])[0].numeric);
        assert!(!looks_numeric("1e5") && !looks_numeric("-") && !looks_numeric(".5") && looks_numeric("5."));
    }

    #[test]
    fn display_newlines() {
        assert_eq!(display(Some("a\r\nb\tc\nd")), "a↵b c↵d");
        assert_eq!(display(None), "NULL");
    }

    #[test]
    fn header_wider_than_cells() {
        let l = layout(&["longheader".to_string()], &[vec![s("1")]]);
        assert_eq!(l[0].width, 10);
    }
}
