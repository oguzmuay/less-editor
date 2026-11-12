//! Rope üzerinde satır / sütun / karakter sınıfı yardımcıları. Buffer yalnız `\n` satır sonu taşır.

use ropey::Rope;

/// Vim'in kelime hareketleri için karakter sınıfı.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CharClass {
    /// Boşluk ya da sekme.
    Blank,
    /// `\n`.
    Eol,
    /// Harf, rakam, `_` (büyük kelimede boşluk olmayan her şey).
    Word,
    /// Noktalama ve diğer işaretler.
    Punct,
}

/// `big` (W/B/E) iken boşluk dışındaki her şey tek sınıftır.
pub fn class(c: char, big: bool) -> CharClass {
    if c == '\n' {
        CharClass::Eol
    } else if c == ' ' || c == '\t' || (c.is_whitespace() && c != '\n') {
        CharClass::Blank
    } else if big || is_word_char(c) {
        CharClass::Word
    } else {
        CharClass::Punct
    }
}

/// Vim `iskeyword` varsayılanına yakın: harf, rakam, `_`.
pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

pub fn is_blank(c: char) -> bool {
    c == ' ' || c == '\t'
}

/// Satırın `\n` hariç karakter uzunluğu.
pub fn line_len(rope: &Rope, line: usize) -> usize {
    let slice = rope.line(line);
    let n = slice.len_chars();
    if n > 0 && slice.char(n - 1) == '\n' { n - 1 } else { n }
}

pub fn line_start(rope: &Rope, line: usize) -> usize {
    rope.line_to_char(line)
}

/// Satır sonu konumu: `\n` karakterinin indeksi (son satırda metin uzunluğu).
pub fn line_end(rope: &Rope, line: usize) -> usize {
    line_start(rope, line) + line_len(rope, line)
}

/// Blok imleçte satırdaki son karakter (boş satırda satır başı).
pub fn line_last_char(rope: &Rope, line: usize) -> usize {
    let s = line_start(rope, line);
    let n = line_len(rope, line);
    if n == 0 { s } else { s + n - 1 }
}

pub fn last_line(rope: &Rope) -> usize {
    rope.len_lines().saturating_sub(1)
}

/// Satırdaki ilk boşluk olmayan karakter (yoksa satır sonu).
pub fn first_non_blank(rope: &Rope, line: usize) -> usize {
    let s = line_start(rope, line);
    let e = line_end(rope, line);
    (s..e).find(|&i| !is_blank(rope.char(i))).unwrap_or(e)
}

/// Satırın baştaki boşlukları.
pub fn indent_of(rope: &Rope, line: usize) -> String {
    let s = line_start(rope, line);
    let e = first_non_blank(rope, line);
    rope.slice(s..e).to_string()
}

pub fn line_text(rope: &Rope, line: usize) -> String {
    let s = line_start(rope, line);
    rope.slice(s..s + line_len(rope, line)).to_string()
}

pub fn is_empty_line(rope: &Rope, line: usize) -> bool {
    line_len(rope, line) == 0
}

/// Konumun görsel sütunu (sekmeler `tab` genişliğinde).
pub fn vcol(rope: &Rope, pos: usize, tab: usize) -> usize {
    let line = rope.char_to_line(pos);
    let s = line_start(rope, line);
    let tab = tab.max(1);
    let mut col = 0;
    for c in rope.slice(s..pos).chars() {
        col = if c == '\t' { col + tab - col % tab } else { col + 1 };
    }
    col
}

/// Görsel sütuna denk gelen konum; `allow_end` iken satır sonuna, değilse son karaktere sınırlanır.
pub fn pos_at_vcol(rope: &Rope, line: usize, target: usize, tab: usize, allow_end: bool) -> usize {
    let s = line_start(rope, line);
    let n = line_len(rope, line);
    let tab = tab.max(1);
    let mut col = 0;
    for (i, c) in rope.slice(s..s + n).chars().enumerate() {
        let next = if c == '\t' { col + tab - col % tab } else { col + 1 };
        if next > target {
            return s + i;
        }
        col = next;
    }
    if allow_end || n == 0 { s + n } else { s + n - 1 }
}

/// `\n` konumunda boş satır başlıyor mu (önceki karakter de `\n` ya da metin başı).
pub fn is_empty_line_at(rope: &Rope, i: usize) -> bool {
    rope.char(i) == '\n' && (i == 0 || rope.char(i - 1) == '\n')
}
