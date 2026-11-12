//! Editör ile `less-lang` arasındaki köprü: rope'u tree-sitter'a `TextSource` olarak verir, iki rope sürümü
//! arasındaki değişikliği `TextEdit` olarak bulur, vurgu türünü tema rengine eşler.

use std::borrow::Cow;
use std::ops::Range;

use less_lang::{HighlightKind, TextEdit, TextPoint, TextSource};
use less_theme::{Palette, Rgb};
use ropey::Rope;

/// `less-core` rope'u için `TextSource` (yetim kuralı yüzünden yeni tip; kopyalamadan okur).
pub struct RopeText<'a>(pub &'a Rope);

impl TextSource for RopeText<'_> {
    fn byte_len(&self) -> usize {
        self.0.len_bytes()
    }

    fn chunk_at(&self, byte: usize) -> &[u8] {
        if byte >= self.0.len_bytes() {
            return &[];
        }
        let (chunk, start, _, _) = self.0.chunk_at_byte(byte);
        &chunk.as_bytes()[byte - start..]
    }

    fn slice(&self, range: Range<usize>) -> Cow<'_, str> {
        let r = self.0;
        let end = range.end.min(r.len_bytes());
        let start = range.start.min(end);
        let s = r.slice(r.byte_to_char(start)..r.byte_to_char(end));
        match s.as_str() {
            Some(t) => Cow::Borrowed(t),
            None => Cow::Owned(s.to_string()),
        }
    }

    fn line_to_byte(&self, row: usize) -> usize {
        if row > self.0.len_lines() { self.0.len_bytes() } else { self.0.line_to_byte(row) }
    }
}

/// `old` → `new` değişikliğini tek bir `TextEdit` olarak tarif eder (ortak önek / sonek dışı); aynıysa `None`.
/// Undo, çoklu imleç gibi dağınık değişiklikler de tek (geniş) düzenleme olur; tree-sitter için doğru sonuç verir.
pub fn rope_edit(old: &Rope, new: &Rope) -> Option<TextEdit> {
    let (ol, nl) = (old.len_bytes(), new.len_bytes());
    let mut prefix = 0;
    let (mut a, mut b) = (old.bytes(), new.bytes());
    while let (Some(x), Some(y)) = (a.next(), b.next()) {
        if x != y {
            break;
        }
        prefix += 1;
    }
    if prefix == ol && prefix == nl {
        return None;
    }
    let max_suffix = ol.min(nl) - prefix;
    let mut suffix = 0;
    let (mut a, mut b) = (old.bytes_at(ol), new.bytes_at(nl));
    while suffix < max_suffix {
        match (a.prev(), b.prev()) {
            (Some(x), Some(y)) if x == y => suffix += 1,
            _ => break,
        }
    }
    let start = old.char_to_byte(old.byte_to_char(prefix));
    let old_end = ceil_char(old, ol - suffix);
    let new_end = ceil_char(new, nl - suffix);
    Some(TextEdit {
        start_byte: start,
        old_end_byte: old_end,
        new_end_byte: new_end,
        start: point(old, start),
        old_end: point(old, old_end),
        new_end: point(new, new_end),
    })
}

/// Karakter sınırına yukarı yuvarlanmış bayt ofseti.
fn ceil_char(r: &Rope, byte: usize) -> usize {
    let c = r.byte_to_char(byte);
    let floor = r.char_to_byte(c);
    if floor == byte { byte } else { r.char_to_byte(c + 1) }
}

fn point(r: &Rope, byte: usize) -> TextPoint {
    let row = r.byte_to_line(byte);
    TextPoint { row, column: byte - r.line_to_byte(row) }
}

/// Vurgu türünün tema rengi. Ayrıntılı tür temada yoksa kaba türe (`fallback`) iner; hiç yoksa düz metin rengi.
pub fn kind_color(p: &Palette, kind: HighlightKind) -> Rgb {
    use less_theme::HighlightKind as T;
    let theme_kind = match kind {
        HighlightKind::Heading | HighlightKind::Strong => Some(T::Keyword),
        HighlightKind::Emphasis | HighlightKind::Label => Some(T::Keyword),
        HighlightKind::Link => Some(T::Function),
        HighlightKind::Literal | HighlightKind::Embedded => Some(T::String),
        HighlightKind::Error => Some(T::DiagError),
        k => T::from_capture(k.name()).or_else(|| T::from_capture(k.fallback().name())),
    };
    theme_kind.map_or(p.fg, |k| p.color(k))
}

/// Satırın her karakterinin görüntü sütunu (sekme `tab_width`'e genişler) ve sondaki toplam genişlik:
/// dönen dizi `chars + 1` uzunluktadır.
pub fn display_cols(line: &str, tab_width: usize) -> Vec<usize> {
    let tw = tab_width.max(1);
    let mut cols = Vec::with_capacity(line.len() + 1);
    let mut col = 0;
    for c in line.chars() {
        cols.push(col);
        // shortcut: geniş (CJK, emoji) karakterler 1 sütun sayılır; gerekirse unicode-width ile düzeltilir.
        col += if c == '\t' { tw - col % tw } else { 1 };
    }
    cols.push(col);
    cols
}

/// Görüntü sütunundaki karakter (satır sonundan ötesi son karaktere değil satır sonuna düşer).
pub fn char_at_col(cols: &[usize], col: usize) -> usize {
    cols.windows(2).position(|w| col < w[1]).unwrap_or(cols.len().saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use less_lang::{Language, Syntax};

    fn edit_of(a: &str, b: &str) -> Option<TextEdit> {
        rope_edit(&Rope::from_str(a), &Rope::from_str(b))
    }

    #[test]
    fn rope_edit_matches_text_edit_compute() {
        let cases = [("hello world", "hello brave world"), ("a\nbc\nd", "a\nd"), ("ğüş", "ğxüş"), ("abc", "abc!")];
        for (a, b) in cases {
            let ours = edit_of(a, b).expect("change");
            let start = ours.start_byte;
            let inserted = &b[start..ours.new_end_byte];
            assert_eq!(ours, TextEdit::compute(a, start, ours.old_end_byte, inserted), "{a:?} -> {b:?}");
        }
        assert_eq!(edit_of("same", "same"), None);
    }

    #[test]
    fn rope_edit_repeated_chars_do_not_overlap() {
        let e = edit_of("aaa", "aaaa").unwrap();
        assert!(e.start_byte <= e.old_end_byte && e.old_end_byte <= 3 && e.new_end_byte - e.start_byte == 1);
    }

    #[test]
    fn rope_text_drives_incremental_highlighting() {
        let old = Rope::from_str("fn main() {}\n");
        let mut syntax = Syntax::new(Language::Rust, &RopeText(&old)).expect("rust grammar");
        let new = Rope::from_str("fn main() { let x = 1; }\n");
        syntax.update(&rope_edit(&old, &new).unwrap(), &RopeText(&new));
        let fresh = Syntax::new(Language::Rust, &RopeText(&new)).unwrap();
        let rows = 0..1;
        assert_eq!(
            syntax.highlight_lines(&RopeText(&new), rows.clone()),
            fresh.highlight_lines(&new.to_string(), rows)
        );
        let spans = &fresh.highlight_lines(&RopeText(&new), 0..1)[0];
        assert!(spans.iter().any(|s| s.kind == HighlightKind::Keyword && s.start == 0));
    }

    #[test]
    fn rope_text_line_and_slice() {
        let r = Rope::from_str("ab\nçd\n");
        let t = RopeText(&r);
        assert_eq!(t.line_to_byte(1), 3);
        assert_eq!(t.line_to_byte(99), r.len_bytes());
        assert_eq!(t.slice(3..6), "çd");
        assert_eq!(t.chunk_at(r.len_bytes()), b"");
    }

    #[test]
    fn display_cols_expand_tabs() {
        assert_eq!(display_cols("\tab", 4), vec![0, 4, 5, 6]);
        assert_eq!(display_cols("a\tb", 4), vec![0, 1, 4, 5]);
        let cols = display_cols("\tab", 4);
        assert_eq!(char_at_col(&cols, 2), 0);
        assert_eq!(char_at_col(&cols, 4), 1);
        assert_eq!(char_at_col(&cols, 50), 3);
    }

    #[test]
    fn every_highlight_kind_gets_a_theme_color() {
        let p = less_theme::TOKYONIGHT_NIGHT;
        assert_eq!(kind_color(&p, HighlightKind::Keyword), p.syntax.keyword);
        assert_eq!(kind_color(&p, HighlightKind::Method), p.syntax.function);
        assert_eq!(kind_color(&p, HighlightKind::Parameter), p.extra.variable);
    }
}
