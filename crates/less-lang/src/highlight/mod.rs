//! tree-sitter tabanlı sözdizimi vurgulama.
//!
//! Kullanım (UI, her düzenlemede):
//! 1. Düzenlemeden önce [`TextEdit::compute`] ile (ya da rope'tan elde edilen bayt/satır bilgisiyle) düzenlemeyi tarif et.
//! 2. Metni değiştir, sonra [`Syntax::update`] çağır (ağaç düzenlenir, eski ağaçla artımlı yeniden ayrıştırılır).
//! 3. Çizimde yalnız görünen satırlar için [`Syntax::highlight_lines`]; sonucu bir sonraki düzenlemeye kadar önbellekle.

mod grammars;
mod kind;

use std::borrow::Cow;
use std::ops::Range;

use streaming_iterator::StreamingIterator;
use tree_sitter::{InputEdit, Node, Parser, Point, QueryCursor, Tree};

pub use grammars::grammar_error;
pub use kind::HighlightKind;

use crate::language::Language;
use grammars::Grammar;

/// Ayrıştırıcıya metin sağlayan kaynak. `str`/`String` için hazır; rope tabanlı buffer bunu uygular,
/// böylece her düzenlemede tüm metin `String`'e kopyalanmaz.
pub trait TextSource {
    /// Toplam bayt uzunluğu.
    fn byte_len(&self) -> usize;
    /// `byte`'tan başlayan ardışık baytlar (bir parça; boş = metin sonu).
    fn chunk_at(&self, byte: usize) -> &[u8];
    /// Bayt aralığının metni.
    fn slice(&self, range: Range<usize>) -> Cow<'_, str>;
    /// 0 tabanlı satırın ilk baytı; satır sayısından büyükse `byte_len()`.
    fn line_to_byte(&self, row: usize) -> usize;
}

impl TextSource for str {
    fn byte_len(&self) -> usize {
        self.len()
    }

    fn chunk_at(&self, byte: usize) -> &[u8] {
        self.as_bytes().get(byte..).unwrap_or_default()
    }

    fn slice(&self, range: Range<usize>) -> Cow<'_, str> {
        Cow::Borrowed(self.get(range).unwrap_or_default())
    }

    fn line_to_byte(&self, row: usize) -> usize {
        if row == 0 {
            return 0;
        }
        self.match_indices('\n').nth(row - 1).map_or(self.len(), |(i, _)| i + 1)
    }
}

impl TextSource for String {
    fn byte_len(&self) -> usize {
        self.as_str().byte_len()
    }

    fn chunk_at(&self, byte: usize) -> &[u8] {
        self.as_str().chunk_at(byte)
    }

    fn slice(&self, range: Range<usize>) -> Cow<'_, str> {
        self.as_str().slice(range)
    }

    fn line_to_byte(&self, row: usize) -> usize {
        self.as_str().line_to_byte(row)
    }
}

/// Metin içinde konum: 0 tabanlı satır ve satır içi **bayt** sütunu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct TextPoint {
    pub row: usize,
    pub column: usize,
}

/// Tek bir metin değişikliği (tree-sitter `InputEdit` karşılığı, bayt tabanlı).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextEdit {
    pub start_byte: usize,
    pub old_end_byte: usize,
    pub new_end_byte: usize,
    pub start: TextPoint,
    pub old_end: TextPoint,
    pub new_end: TextPoint,
}

impl TextEdit {
    /// `old_text` içinde `start..old_end` aralığının `inserted` ile değiştirilmesini tarif eder.
    /// Değişiklik **uygulanmadan önceki** metinle çağrılır.
    pub fn compute(old_text: &str, start_byte: usize, old_end_byte: usize, inserted: &str) -> TextEdit {
        let start = point_at(old_text, start_byte);
        let old_end = point_at(old_text, old_end_byte);
        let new_end = advance(start, inserted);
        TextEdit { start_byte, old_end_byte, new_end_byte: start_byte + inserted.len(), start, old_end, new_end }
    }

    fn input_edit(&self) -> InputEdit {
        InputEdit {
            start_byte: self.start_byte,
            old_end_byte: self.old_end_byte,
            new_end_byte: self.new_end_byte,
            start_position: to_point(self.start),
            old_end_position: to_point(self.old_end),
            new_end_position: to_point(self.new_end),
        }
    }
}

/// `text`'teki bayt ofsetinin satır/sütunu.
pub fn point_at(text: &str, byte: usize) -> TextPoint {
    let byte = byte.min(text.len());
    let before = &text.as_bytes()[..byte];
    let row = before.iter().filter(|b| **b == b'\n').count();
    let line_start = before.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
    TextPoint { row, column: byte - line_start }
}

/// `start` noktasından sonra `inserted` yazıldığında varılan nokta.
fn advance(start: TextPoint, inserted: &str) -> TextPoint {
    match inserted.rfind('\n') {
        Some(last) => TextPoint { row: start.row + inserted.matches('\n').count(), column: inserted.len() - last - 1 },
        None => TextPoint { row: start.row, column: start.column + inserted.len() },
    }
}

fn to_point(p: TextPoint) -> Point {
    Point { row: p.row, column: p.column }
}

/// Bir satır içindeki vurgulu aralık (satır başına göre bayt sütunları, `end` hariç).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighlightSpan {
    pub start: usize,
    pub end: usize,
    pub kind: HighlightKind,
}

/// Capture'ın boyama etkisi.
#[derive(Clone, Copy)]
enum Paint {
    Skip,
    Clear,
    Kind(HighlightKind),
}

fn paint_of(name: &str) -> Paint {
    if name == "none" {
        return Paint::Clear;
    }
    HighlightKind::from_capture(name).map_or(Paint::Skip, Paint::Kind)
}

/// Markdown kod bloğu enjeksiyonu: içerik ayrı bir [`Syntax`] ile vurgulanır.
struct Injection {
    range: Range<usize>,
    start: TextPoint,
    text: String,
    syntax: Syntax,
}

/// Bir buffer'ın sözdizimi ağacı. Dil grammar'ı yoksa (sorgu derlenemedi) [`Syntax::new`] `None` döner.
pub struct Syntax {
    lang: Language,
    grammar: &'static Grammar,
    parser: Parser,
    tree: Option<Tree>,
    inline_parser: Option<Parser>,
    inline_tree: Option<Tree>,
    injections: Vec<Injection>,
}

impl std::fmt::Debug for Syntax {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Syntax").field("lang", &self.lang).field("parsed", &self.tree.is_some()).finish()
    }
}

impl Syntax {
    /// Dili ve ilk metni alıp tam ayrıştırır.
    pub fn new(lang: Language, text: &(impl TextSource + ?Sized)) -> Option<Syntax> {
        let grammar = grammars::grammar(lang)?;
        let mut parser = Parser::new();
        parser.set_language(&grammar.language).ok()?;
        let inline_parser = if lang == Language::Markdown {
            let inline = grammars::markdown_inline()?;
            let mut p = Parser::new();
            p.set_language(&inline.language).ok()?;
            Some(p)
        } else {
            None
        };
        let mut syntax =
            Syntax { lang, grammar, parser, tree: None, inline_parser, inline_tree: None, injections: Vec::new() };
        syntax.reparse(text);
        Some(syntax)
    }

    pub fn language(&self) -> Language {
        self.lang
    }

    /// Ayrıştırma ağacı (yapısal seçim, katlama vb. için).
    pub fn tree(&self) -> Option<&Tree> {
        self.tree.as_ref()
    }

    /// Ağaca bir düzenlemeyi bildirir (metin değiştikten sonra [`Syntax::reparse`] gerekir).
    /// Birden çok düzenleme art arda bildirilip tek `reparse` yapılabilir.
    pub fn edit(&mut self, edit: &TextEdit) {
        if let Some(tree) = self.tree.as_mut() {
            tree.edit(&edit.input_edit());
        }
    }

    /// Eski (düzenlenmiş) ağacı kullanarak artımlı yeniden ayrıştırır.
    pub fn reparse(&mut self, text: &(impl TextSource + ?Sized)) {
        let len = text.byte_len();
        let mut read = |byte: usize, _: Point| if byte >= len { &[][..] } else { text.chunk_at(byte) };
        self.tree = self.parser.parse_with_options(&mut read, self.tree.as_ref(), None);
        if self.lang == Language::Markdown {
            self.reparse_markdown(text);
        }
    }

    /// [`Syntax::edit`] + [`Syntax::reparse`].
    pub fn update(&mut self, edit: &TextEdit, new_text: &(impl TextSource + ?Sized)) {
        self.edit(edit);
        self.reparse(new_text);
    }

    fn reparse_markdown(&mut self, text: &(impl TextSource + ?Sized)) {
        let Some(tree) = self.tree.as_ref() else { return };
        let mut inline_ranges = Vec::new();
        let mut fences = Vec::new();
        walk(tree.root_node(), &mut |node| match node.kind() {
            "inline" | "pipe_table_cell" => {
                inline_ranges.push(node.range());
                false
            }
            "fenced_code_block" => {
                fences.push(node);
                false
            }
            _ => true,
        });
        self.inline_tree = None;
        if let Some(parser) = self.inline_parser.as_mut()
            && !inline_ranges.is_empty()
            && parser.set_included_ranges(&inline_ranges).is_ok()
        {
            let len = text.byte_len();
            let mut read = |byte: usize, _: Point| if byte >= len { &[][..] } else { text.chunk_at(byte) };
            self.inline_tree = parser.parse_with_options(&mut read, None, None);
        }
        self.injections = fences.into_iter().filter_map(|fence| injection(fence, text)).collect();
    }

    /// `rows` aralığındaki her satır için vurgu aralıkları (satır başına bir `Vec`, boş satır = boş `Vec`).
    pub fn highlight_lines(&self, text: &(impl TextSource + ?Sized), rows: Range<usize>) -> Vec<Vec<HighlightSpan>> {
        let line_starts: Vec<usize> = (rows.start..=rows.end).map(|r| text.line_to_byte(r)).collect();
        let start = line_starts[0];
        let end = *line_starts.last().unwrap_or(&start);
        let mut paint = vec![None; end.saturating_sub(start)];
        if let Some(tree) = self.tree.as_ref() {
            paint_tree(self.grammar, tree, text, start..end, &mut paint);
        }
        if let (Some(tree), Some(inline)) = (self.inline_tree.as_ref(), grammars::markdown_inline()) {
            paint_tree(inline, tree, text, start..end, &mut paint);
        }
        for inj in &self.injections {
            if inj.range.end <= start || inj.range.start >= end {
                continue;
            }
            paint_injection(inj, text, start..end, &mut paint);
        }
        line_starts
            .windows(2)
            .map(|w| {
                let mut le = w[1];
                while le > w[0] && matches!(text.slice(le - 1..le).as_ref(), "\n" | "\r") {
                    le -= 1;
                }
                spans(&paint[w[0] - start..le - start])
            })
            .collect()
    }

    /// Bayt ofsetindeki en küçük adlandırılmış düğümün türü bir yorum ya da dize mi (otomatik eşleştirme kararları).
    pub fn node_kind_at(&self, byte: usize) -> Option<&'static str> {
        let tree = self.tree.as_ref()?;
        tree.root_node().named_descendant_for_byte_range(byte, byte).map(|n| n.kind())
    }
}

/// Ağacı önden dolaşır; `visit` `false` dönerse düğümün altına inilmez.
fn walk<'t>(root: Node<'t>, visit: &mut impl FnMut(Node<'t>) -> bool) {
    let mut cursor = root.walk();
    loop {
        let node = cursor.node();
        if visit(node) && cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return;
            }
        }
    }
}

fn injection(fence: Node<'_>, text: &(impl TextSource + ?Sized)) -> Option<Injection> {
    let mut lang = None;
    let mut content = None;
    let mut cursor = fence.walk();
    for child in fence.children(&mut cursor) {
        match child.kind() {
            "info_string" => {
                let mut c = child.walk();
                let name = child.children(&mut c).find(|n| n.kind() == "language").unwrap_or(child);
                lang = Language::from_name(&text.slice(name.byte_range()));
            }
            "code_fence_content" => content = Some(child),
            _ => {}
        }
    }
    let lang = lang.filter(|l| *l != Language::Markdown)?;
    let content = content?;
    let body = text.slice(content.byte_range()).into_owned();
    let syntax = Syntax::new(lang, body.as_str())?;
    let p = content.start_position();
    Some(Injection {
        range: content.byte_range(),
        start: TextPoint { row: p.row, column: p.column },
        text: body,
        syntax,
    })
}

fn paint_tree(
    grammar: &Grammar,
    tree: &Tree,
    text: &(impl TextSource + ?Sized),
    range: Range<usize>,
    paint: &mut [Option<HighlightKind>],
) {
    let names = grammar.query.capture_names();
    let mut hits: Vec<(Range<usize>, usize, Paint)> = Vec::new();
    let mut cursor = QueryCursor::new();
    cursor.set_byte_range(range.clone());
    let provider = |node: Node<'_>| std::iter::once(text.slice(node.byte_range()).into_owned().into_bytes());
    let mut matches = cursor.matches(&grammar.query, tree.root_node(), provider);
    while let Some(m) = matches.next() {
        for cap in m.captures {
            let p = paint_of(names[cap.index as usize]);
            if !matches!(p, Paint::Skip) {
                hits.push((cap.node.byte_range(), m.pattern_index, p));
            }
        }
    }
    hits.sort_by(|a, b| (b.0.end - b.0.start).cmp(&(a.0.end - a.0.start)).then(a.1.cmp(&b.1)));
    for (r, _, p) in hits {
        let s = r.start.max(range.start) - range.start;
        let e = r.end.min(range.end).saturating_sub(range.start);
        if s >= e {
            continue;
        }
        let value = match p {
            Paint::Kind(k) => Some(k),
            _ => None,
        };
        paint[s..e].fill(value);
    }
}

fn paint_injection(
    inj: &Injection,
    text: &(impl TextSource + ?Sized),
    range: Range<usize>,
    paint: &mut [Option<HighlightKind>],
) {
    let rows = inj.text.matches('\n').count() + 1;
    let lines = inj.syntax.highlight_lines(inj.text.as_str(), 0..rows);
    for (i, line) in lines.iter().enumerate() {
        let row = inj.start.row + i;
        let col0 = if i == 0 { inj.start.column } else { 0 };
        let line_start = text.line_to_byte(row) + col0;
        for span in line {
            let s = line_start + span.start;
            let e = line_start + span.end;
            if e <= range.start || s >= range.end || e > inj.range.end {
                continue;
            }
            let s = s.max(range.start) - range.start;
            let e = e.min(range.end) - range.start;
            paint[s..e].fill(Some(span.kind));
        }
    }
}

/// Bayt başına boyamayı ardışık aralıklara sıkıştırır.
fn spans(paint: &[Option<HighlightKind>]) -> Vec<HighlightSpan> {
    let mut out: Vec<HighlightSpan> = Vec::new();
    for (i, p) in paint.iter().enumerate() {
        let Some(kind) = *p else { continue };
        match out.last_mut() {
            Some(last) if last.end == i && last.kind == kind => last.end = i + 1,
            _ => out.push(HighlightSpan { start: i, end: i + 1, kind }),
        }
    }
    out
}

#[cfg(test)]
mod tests;
