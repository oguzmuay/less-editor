//! UI'dan bağımsız parçalar: karakter ↔ LSP konumu (satır + UTF-16), LSP düzenlemelerinin rope'a / diske
//! uygulanması, semantic token → [`HighlightKind`] eşlemesi.

use std::path::Path;

use less_lang::lsp::uri::{byte_to_utf16, utf16_to_byte};
use less_lang::{HighlightKind, Pos};
use ropey::Rope;

use crate::editor::view::line_text;

/// LSP metin düzenlemesi, less-app'in adını yazabildiği tiplerle (aralık LSP konumu, `text` yeni metin).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub start: Pos,
    pub end: Pos,
    pub text: String,
}

/// Karakter indeksinin LSP konumu.
pub fn pos_of_char(rope: &Rope, ch: usize) -> Pos {
    let ch = ch.min(rope.len_chars());
    let line = rope.char_to_line(ch);
    let text = line_text(rope, line);
    let col = ch - rope.line_to_char(line);
    let byte = text.char_indices().nth(col).map_or(text.len(), |(b, _)| b);
    Pos { line: line as u32, character: byte_to_utf16(&text, byte) }
}

/// LSP konumunun karakter indeksi; satır ya da sütun taşarsa kırpılır.
pub fn char_of_pos(rope: &Rope, p: Pos) -> usize {
    let line = p.line as usize;
    if line >= rope.len_lines() {
        return rope.len_chars();
    }
    let text = line_text(rope, line);
    let byte = utf16_to_byte(&text, p.character).min(text.len());
    rope.line_to_char(line) + text[..byte].chars().count()
}

/// Düzenlemeleri karakter aralıklarına çevirir, uygulama sırasıyla (sondan başa). Aynı yerdeki eklemeler
/// LSP'deki dizi sırasıyla sonuçlansın diye eşitlikte sonraki önce uygulanır.
pub fn char_edits(rope: &Rope, edits: &[Edit]) -> Vec<(usize, usize, String)> {
    let mut out: Vec<(usize, usize, usize, String)> = edits
        .iter()
        .enumerate()
        .map(|(i, e)| (char_of_pos(rope, e.start), char_of_pos(rope, e.end), i, e.text.clone()))
        .collect();
    out.sort_by_key(|e| std::cmp::Reverse((e.0, e.2)));
    out.into_iter().map(|(a, b, _, t)| (a, b.max(a), t)).collect()
}

/// Rope'a uygular (açık olmayan dosyalar ve testler).
pub fn apply_to_rope(rope: &mut Rope, edits: &[Edit]) {
    for (a, b, t) in char_edits(rope, edits) {
        rope.remove(a..b);
        rope.insert(a, &t);
    }
}

/// Kod alanında açık olmayan dosyaya diskte uygular.
pub fn apply_to_file(path: &Path, edits: &[Edit]) -> std::io::Result<()> {
    let mut rope = Rope::from_str(&std::fs::read_to_string(path)?);
    apply_to_rope(&mut rope, edits);
    std::fs::write(path, rope.to_string())
}

/// İmlecin altındaki kelime (yeniden adlandırma isteminin ilk değeri).
pub fn word_at(rope: &Rope, ch: usize) -> String {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let len = rope.len_chars();
    let mut a = ch.min(len);
    while a > 0 && is_word(rope.char(a - 1)) {
        a -= 1;
    }
    let mut b = ch.min(len);
    while b < len && is_word(rope.char(b)) {
        b += 1;
    }
    rope.slice(a..b).to_string()
}

/// Standart semantic token türü (+ değiştiriciler) için tema rengi türü (#71 IntelliJ şemaları jdtls / vtsls
/// token'larını renklendirir). Bilinmeyen tür `None` (tree-sitter vurgusu kalır).
#[allow(dead_code, reason = "host semantic token çizimini henüz bağlamadı (B5.md, C3)")]
pub fn semantic_kind(token_type: &str, modifiers: &[&str]) -> Option<HighlightKind> {
    let has = |m: &str| modifiers.contains(&m);
    let constant = has("readonly") && (has("static") || token_type == "enumMember");
    Some(match token_type {
        "namespace" | "package" => HighlightKind::Namespace,
        "type" | "class" | "enum" | "interface" | "struct" | "typeParameter" | "record" => HighlightKind::Type,
        "parameter" => HighlightKind::Parameter,
        "variable" if has("readonly") || has("constant") => HighlightKind::Constant,
        "variable" if has("defaultLibrary") => HighlightKind::VariableBuiltin,
        "variable" => HighlightKind::Variable,
        "property" | "event" if constant => HighlightKind::Constant,
        "property" | "event" => HighlightKind::Property,
        "enumMember" => HighlightKind::Constant,
        "function" if has("defaultLibrary") => HighlightKind::FunctionBuiltin,
        "function" => HighlightKind::Function,
        "method" if has("constructor") => HighlightKind::Constructor,
        "method" => HighlightKind::Method,
        "macro" => HighlightKind::FunctionMacro,
        "keyword" | "modifier" => HighlightKind::Keyword,
        "comment" => HighlightKind::Comment,
        "string" => HighlightKind::String,
        "regexp" => HighlightKind::StringSpecial,
        "number" => HighlightKind::Number,
        "operator" => HighlightKind::Operator,
        "decorator" | "annotation" | "annotationMember" => HighlightKind::Attribute,
        _ => return None,
    })
}

/// Göreli kodlanmış semantic token verisini (`deltaLine, deltaStart, length, type, modifiers`) mutlak
/// `(satır, UTF-16 başlangıç, uzunluk, tür)` listesine çevirir; `types` / `modifiers` sunucunun legend'ı.
#[allow(dead_code, reason = "host semantic token çizimini henüz bağlamadı (B5.md, C3)")]
pub fn semantic_spans(data: &[[u32; 5]], types: &[&str], modifiers: &[&str]) -> Vec<(u32, u32, u32, HighlightKind)> {
    let (mut line, mut start) = (0, 0);
    let mut out = Vec::new();
    for &[dl, ds, len, ty, mods] in data {
        start = if dl == 0 { start + ds } else { ds };
        line += dl;
        let mods: Vec<&str> =
            (0..32).filter(|b| mods & (1 << b) != 0).filter_map(|b| modifiers.get(b).copied()).collect();
        if let Some(kind) = types.get(ty as usize).and_then(|t| semantic_kind(t, &mods)) {
            out.push((line, start, len, kind));
        }
    }
    out
}
