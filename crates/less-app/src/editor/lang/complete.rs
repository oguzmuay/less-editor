//! IntelliSense: LSP tamamlama listesi (yazarken gecikmeli, tetikleyici karakterde hemen, Ctrl+Space elle),
//! istemci tarafı bulanık süzme, seçili öğenin dokümantasyonu (`completionItem/resolve`), kabul edince
//! `textEdit` + `additionalTextEdits` (otomatik import) tek geri alma adımında; `(` / `,` ile imza yardımı.
//!
//! Tuşlar `Lang::key`'den motordan önce gelir: liste açıkken ↑↓ / Ctrl+N/P seçer, Enter / Tab kabul eder, Esc
//! kapatır (Vim'de Esc ayrıca motora gider, insert'ten çıkar). Yazılan karakter `key`'de not edilir; düzenleme
//! `changed`'de görülünce tetikleme kararı verilir. Çizim `ui::completion_popups`'ta.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use less_core::command::{Command, CursorMode};
use less_core::editor::Session;
use less_core::selection::{Range, Selection};
use less_input::{EditMode, Key, KeyEvent};
use less_lang::lsp::uri::utf16_to_byte;
use less_lang::{Language, LspResult, Pos, RequestId, ServerKey};
use lsp_types::{CompletionItem, CompletionItemKind, CompletionTextEdit, Documentation, InsertTextFormat};

use super::text::{char_of_pos, pos_of_char};
use super::{EditorTab, Lang};
use crate::panels::Cx;

/// Kelime yazarken isteğin gecikmesi.
const DEBOUNCE: Duration = Duration::from_millis(100);
/// Listede en fazla bu kadar öğe tutulur (süzme sonrası).
const MAX_SHOWN: usize = 200;

/// Tamamlama öğesi ve onu veren sunucu.
#[derive(Clone, Debug)]
pub(super) struct Item {
    pub item: CompletionItem,
    pub key: ServerKey,
    pub resolved: bool,
}

/// Açık (ya da yanıtı beklenen) tamamlama listesi.
#[derive(Clone, Debug)]
pub(super) struct Completion {
    pub path: PathBuf,
    /// İstekteki imleç (karakter indeksi) ve LSP konumu: `textEdit` aralıkları buna göre.
    pub req_at: usize,
    pub req_pos: Pos,
    /// Süzme başlangıcı (kelime başı); imleç bunun gerisine ya da başka satıra geçerse liste kapanır.
    pub anchor: usize,
    pub line: usize,
    pub items: Vec<Item>,
    /// `items` önceki (eksik) listeden: yenisi gelene kadar görünür, ilk yanıtta silinir.
    pub stale: bool,
    /// Süzülmüş, sıralı `items` indeksleri.
    pub shown: Vec<usize>,
    pub selected: usize,
    pub pending: HashSet<RequestId>,
    pub incomplete: bool,
    pub manual: bool,
    /// Bekleyen `completionItem/resolve` ve öğesi.
    pub resolving: Option<(RequestId, usize)>,
    /// Seçim değişti: liste kaydırılıp seçili satır gösterilsin.
    pub scroll: bool,
}

/// Açık imza yardımı.
#[derive(Clone, Debug)]
pub(super) struct Signature {
    pub path: PathBuf,
    pub line: usize,
    /// Tetikleyen karakterin konumu; imleç gerisine geçerse kapanır.
    pub from: usize,
    pub pending: Option<RequestId>,
    pub help: Option<SigView>,
}

/// Çizilecek imza: etiket, etkin parametrenin bayt aralığı, dokümantasyon.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct SigView {
    pub label: String,
    pub active: Option<(usize, usize)>,
    pub param_doc: Option<String>,
    pub doc: Option<String>,
}

impl Lang {
    /// Liste / imza açıkken tuşlar ve Ctrl+Space. `true` = tüketildi (motor görmez).
    pub(super) fn completion_key(
        &mut self,
        k: KeyEvent,
        tab: &mut EditorTab,
        session: &mut Session,
        cx: &mut Cx,
    ) -> bool {
        let plain = !k.mods.ctrl && !k.mods.cmd && !k.mods.alt;
        if k.key == Key::Char(' ') && k.mods.ctrl && !k.mods.cmd {
            if typing(tab, cx) {
                self.request_completion(tab, None, true, cx);
            }
            return true;
        }
        if let Key::Char(c) = k.key
            && !k.mods.ctrl
            && !k.mods.cmd
            && typing(tab, cx)
        {
            self.typed = Some(c);
        }
        if k.key == Key::Esc {
            self.signature = None;
        }
        let visible = self.completion.as_ref().is_some_and(|c| !c.shown.is_empty());
        if !visible {
            return false;
        }
        let ctrl = |c| k.mods.ctrl && !k.mods.cmd && k.key == Key::Char(c);
        let comp = self.completion.as_mut().expect("visible");
        let n = comp.shown.len();
        if (k.key == Key::Down && plain) || ctrl('n') {
            comp.selected = (comp.selected + 1) % n;
            comp.scroll = true;
        } else if (k.key == Key::Up && plain) || ctrl('p') {
            comp.selected = (comp.selected + n - 1) % n;
            comp.scroll = true;
        } else if (k.key == Key::Enter || k.key == Key::Tab) && plain {
            let i = comp.shown[comp.selected];
            self.accept(i, tab, session, cx);
        } else if k.key == Key::Esc {
            self.completion = None;
            return cx.edit_mode == EditMode::Modeless;
        } else {
            return false;
        }
        true
    }

    /// Düzenlemeden sonra: yazılan karakter tetikleyiciyse hemen, kelime karakteriyse gecikmeli istek; açık
    /// listede süzme `completion_frame`'de.
    pub(super) fn completion_changed(&mut self, tab: &EditorTab, cx: &mut Cx) {
        let Some(c) = self.typed.take() else { return };
        let Some(path) = tab.path.clone() else { return };
        let rope = tab.editor.buffer.rope();
        let cursor = tab.editor.cursor();
        if cursor == 0 || rope.char(cursor - 1) != c || self.lsp.server_of(&path).is_none() {
            return;
        }
        let s = c.to_string();
        if self.lsp.signature_triggers(&path).contains(&s) {
            self.request_signature(tab, Some(&s));
        } else if c == ')' {
            self.signature = None;
        }
        if self.lsp.completion_triggers(&path).contains(&s) {
            self.request_completion(tab, Some(&s), false, cx);
            return;
        }
        if is_word(c, tab.language) {
            let refresh = self.completion.as_ref().is_none_or(|comp| comp.incomplete && comp.pending.is_empty());
            if refresh {
                self.debounce = Some((path, Instant::now() + DEBOUNCE));
            }
        } else {
            self.completion = None;
            self.debounce = None;
        }
    }

    /// Her karede: gecikmeli isteği gönderir, açık listeyi imlece göre süzer ya da kapatır.
    pub(super) fn completion_frame(&mut self, tab: &EditorTab, cx: &mut Cx) {
        self.typed = None;
        let path = tab.path.clone();
        let cursor = tab.editor.cursor();
        let rope = tab.editor.buffer.rope();
        let line = rope.char_to_line(cursor.min(rope.len_chars()));
        let live = typing(tab, cx);
        if let Some((p, at)) = &self.debounce {
            if Some(p) != path.as_ref() || !live {
                self.debounce = None;
            } else if let Some(left) = at.checked_duration_since(Instant::now()) {
                cx.ctx.request_repaint_after(left);
            } else {
                self.debounce = None;
                self.request_completion(tab, None, false, cx);
            }
        }
        if let Some(sig) = &self.signature
            && (Some(&sig.path) != path.as_ref() || sig.line != line || cursor < sig.from || !live)
        {
            self.signature = None;
        }
        let Some(comp) = &mut self.completion else { return };
        if Some(&comp.path) != path.as_ref() || comp.line != line || cursor < comp.anchor || !live {
            self.completion = None;
            return;
        }
        filter(comp, rope, cursor);
        if comp.shown.is_empty() && comp.pending.is_empty() && !comp.manual {
            self.completion = None;
            return;
        }
        self.resolve_selected();
    }

    /// İmleçte tamamlama ister. `manual` (Ctrl+Space): sonuç yoksa bildirilir, boş önekle de liste açılır.
    pub(super) fn request_completion(&mut self, tab: &EditorTab, trigger: Option<&str>, manual: bool, cx: &mut Cx) {
        let Some(path) = tab.path.clone() else { return };
        if self.lsp.server_of(&path).is_none() {
            if manual {
                cx.info("No language server for this file");
            }
            return;
        }
        let rope = tab.editor.buffer.rope();
        let cursor = tab.editor.cursor();
        let mut anchor = cursor;
        while anchor > 0 && is_word(rope.char(anchor - 1), tab.language) {
            anchor -= 1;
        }
        let req_pos = pos_of_char(rope, cursor);
        let ids = self.lsp.complete(&path, lsp_position(req_pos), trigger);
        if ids.is_empty() {
            if manual {
                cx.info("Completion is not supported by the language server");
            }
            return;
        }
        let keep = self.completion.take().filter(|c| c.path == path && c.anchor == anchor && trigger.is_none());
        let (items, shown, selected) =
            keep.as_ref().map_or((Vec::new(), Vec::new(), 0), |c| (c.items.clone(), c.shown.clone(), c.selected));
        self.completion = Some(Completion {
            path,
            req_at: cursor,
            req_pos,
            anchor,
            line: rope.char_to_line(cursor),
            stale: !items.is_empty(),
            items,
            shown,
            selected,
            pending: ids.into_iter().collect(),
            incomplete: false,
            manual: manual || keep.as_ref().is_some_and(|c| c.manual),
            resolving: None,
            scroll: true,
        });
    }

    fn request_signature(&mut self, tab: &EditorTab, trigger: Option<&str>) {
        let Some(path) = tab.path.clone() else { return };
        let rope = tab.editor.buffer.rope();
        let cursor = tab.editor.cursor();
        let Some(id) = self.lsp.signature_help(&path, lsp_position(pos_of_char(rope, cursor)), trigger) else { return };
        let keep = self.signature.take().filter(|s| s.path == path && s.line == rope.char_to_line(cursor));
        self.signature = Some(Signature {
            path,
            line: rope.char_to_line(cursor),
            from: keep.as_ref().map_or(cursor.saturating_sub(1), |s| s.from),
            pending: Some(id),
            help: keep.and_then(|s| s.help),
        });
    }

    /// Tamamlama / resolve / imza yanıtı ise işler (`true`).
    pub(super) fn completion_response(
        &mut self,
        id: RequestId,
        key: &ServerKey,
        result: &LspResult,
        cx: &mut Cx,
    ) -> bool {
        if let Some(sig) = self.signature.as_mut().filter(|s| s.pending == Some(id)) {
            sig.pending = None;
            sig.help = match result {
                LspResult::Raw(v) => {
                    serde_json::from_value::<lsp_types::SignatureHelp>(v.clone()).ok().and_then(sig_view)
                }
                _ => None,
            };
            if sig.help.is_none() {
                self.signature = None;
            }
            return true;
        }
        let Some(comp) = &mut self.completion else { return false };
        if let Some((rid, i)) = comp.resolving
            && rid == id
        {
            comp.resolving = None;
            if let LspResult::Raw(v) = result
                && let Ok(resolved) = serde_json::from_value::<CompletionItem>(v.clone())
                && let Some(item) = comp.items.get_mut(i)
            {
                merge_resolved(&mut item.item, resolved);
            }
            return true;
        }
        if !comp.pending.remove(&id) {
            return false;
        }
        match result {
            LspResult::Completion { items, incomplete } => {
                if std::mem::take(&mut comp.stale) {
                    comp.items.clear();
                    comp.shown.clear();
                    comp.resolving = None;
                }
                comp.incomplete |= *incomplete;
                comp.items.extend(items.iter().map(|i| Item { item: i.clone(), key: key.clone(), resolved: false }));
            }
            LspResult::Error { message, .. } if comp.manual => cx.info(message.clone()),
            _ => {}
        }
        if comp.pending.is_empty() && comp.stale {
            comp.items.clear();
            comp.shown.clear();
        }
        if comp.pending.is_empty() && comp.items.is_empty() {
            if comp.manual {
                cx.info("No completions");
            }
            self.completion = None;
        }
        cx.ctx.request_repaint();
        true
    }

    /// Seçili öğe çözülmemişse `completionItem/resolve` (aynı anda bir istek).
    fn resolve_selected(&mut self) {
        let Some(comp) = &mut self.completion else { return };
        if comp.resolving.is_some() {
            return;
        }
        let Some(&i) = comp.shown.get(comp.selected) else { return };
        let item = &mut comp.items[i];
        if item.resolved {
            return;
        }
        item.resolved = true;
        let value = serde_json::to_value(&item.item).unwrap_or_default();
        if let Some(id) = self.lsp.resolve_completion(&item.key, value) {
            comp.resolving = Some((id, i));
        }
    }

    /// Öğeyi uygular: ana düzenleme + `additionalTextEdits` tek geri alma adımında, imleç eklenen metnin sonuna.
    pub(super) fn accept(&mut self, i: usize, tab: &mut EditorTab, session: &mut Session, cx: &mut Cx) {
        let Some(comp) = self.completion.take() else { return };
        let Some(item) = comp.items.get(i) else { return };
        let rope = tab.editor.buffer.rope().clone();
        let cursor = tab.editor.cursor();
        let (start, end, text) = main_edit(&item.item, &comp, &rope, cursor);
        let mut edits: Vec<(usize, usize, String, bool)> = vec![(start, end, text, true)];
        for e in item.item.additional_text_edits.iter().flatten() {
            let a = char_of_pos(&rope, Pos { line: e.range.start.line, character: e.range.start.character });
            let b = char_of_pos(&rope, Pos { line: e.range.end.line, character: e.range.end.character });
            if b <= start || a >= end {
                edits.push((a, b.max(a), e.new_text.clone(), false));
            }
        }
        edits.sort_by_key(|e| std::cmp::Reverse((e.0, e.3)));
        let mode = tab.editor.cursor_mode();
        let mut caret = start;
        self.run(tab, Command::BeginUndoGroup, session, cx);
        self.run(tab, Command::SetCursorMode(CursorMode::Bar), session, cx);
        for (a, b, t, main) in &edits {
            let len = t.chars().count();
            if *main {
                caret = a + len;
            } else if *a < start {
                caret = (caret + len).saturating_sub(b - a);
            }
            tab.editor.set_selection(Selection::single(Range::new(*a, *b)));
            self.run(tab, Command::InsertText(t.clone()), session, cx);
        }
        self.run(tab, Command::SetCursorMode(mode), session, cx);
        self.run(tab, Command::EndUndoGroup, session, cx);
        tab.editor.set_selection(Selection::point(caret.min(tab.editor.buffer.rope().len_chars())));
        self.typed = None;
        if edits.iter().any(|e| e.3 && e.2.ends_with('(')) {
            self.request_signature(tab, Some("("));
        }
    }
}

/// Vim insert ya da modeless: yazı yazılıyor.
fn typing(tab: &EditorTab, cx: &Cx) -> bool {
    !tab.read_only && (cx.edit_mode == EditMode::Modeless || tab.editor.cursor_mode() == CursorMode::Bar)
}

/// Tamamlama kelimesinin karakteri (CSS / HTML'de `-` de: Tailwind sınıfları, CSS özellikleri).
fn is_word(c: char, lang: Option<Language>) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$' || (c == '-' && matches!(lang, Some(Language::Css | Language::Html)))
}

fn lsp_position(p: Pos) -> lsp_types::Position {
    lsp_types::Position { line: p.line, character: p.character }
}

fn pos_le(a: Pos, b: Pos) -> bool {
    (a.line, a.character) <= (b.line, b.character)
}

/// Ana düzenleme (karakter aralığı + metin). `textEdit` istek anındaki metne göredir: istek konumundan sonraki
/// uçlar imleç o zamandan beri ne kadar ilerlediyse o kadar kayar.
fn main_edit(item: &CompletionItem, comp: &Completion, rope: &ropey::Rope, cursor: usize) -> (usize, usize, String) {
    let snippet = item.insert_text_format == Some(InsertTextFormat::SNIPPET);
    let fix = |t: &str| if snippet { strip_snippet(t) } else { t.to_owned() };
    let delta = cursor as isize - comp.req_at as isize;
    let range = match &item.text_edit {
        Some(CompletionTextEdit::Edit(e)) => Some((e.range, e.new_text.as_str())),
        Some(CompletionTextEdit::InsertAndReplace(e)) => Some((e.insert, e.new_text.as_str())),
        None => None,
    };
    let Some((range, text)) = range else {
        let text = item.insert_text.as_deref().unwrap_or(&item.label);
        return (comp.anchor, cursor, fix(text));
    };
    let shift = |p: lsp_types::Position| {
        let p = Pos { line: p.line, character: p.character };
        let ch = char_of_pos(rope, p);
        if pos_le(comp.req_pos, p) { (ch as isize + delta).max(0) as usize } else { ch }
    };
    let line_end = rope.line_to_char(comp.line) + super::line_text(rope, comp.line).chars().count();
    let a = shift(range.start).min(cursor);
    let b = shift(range.end).clamp(cursor, line_end.max(cursor));
    (a, b, fix(text))
}

/// Resolve yanıtını öğeye katar (`textEdit` gibi alanlar resolve'da boş gelebilir; yalnız dolu olanlar).
fn merge_resolved(item: &mut CompletionItem, r: CompletionItem) {
    if r.documentation.is_some() {
        item.documentation = r.documentation;
    }
    if r.detail.is_some() {
        item.detail = r.detail;
    }
    if r.additional_text_edits.is_some() {
        item.additional_text_edits = r.additional_text_edits;
    }
    if r.text_edit.is_some() {
        item.text_edit = r.text_edit;
    }
    if r.insert_text.is_some() {
        item.insert_text = r.insert_text;
    }
}

/// Öğeleri imleçteki önekle süzer ve sıralar (önek eşleşmesi önce, sonra bulanık puan, sonra `sortText`).
pub(super) fn filter(comp: &mut Completion, rope: &ropey::Rope, cursor: usize) {
    let selected = comp.shown.get(comp.selected).copied();
    let line_start = rope.line_to_char(comp.line);
    let mut scored: Vec<(i32, &str, usize)> = Vec::new();
    for (i, it) in comp.items.iter().enumerate() {
        let from = match &it.item.text_edit {
            Some(CompletionTextEdit::Edit(e)) => Some(e.range.start),
            Some(CompletionTextEdit::InsertAndReplace(e)) => Some(e.insert.start),
            None => None,
        }
        .filter(|p| p.line as usize == comp.line)
        .map(|p| char_of_pos(rope, Pos { line: p.line, character: p.character }))
        .filter(|&a| a >= line_start && a <= cursor)
        .unwrap_or(comp.anchor);
        let prefix: String = rope.slice(from.min(cursor)..cursor).to_string();
        let target = it.item.filter_text.as_deref().unwrap_or(&it.item.label);
        if let Some(score) = fuzzy(&prefix, target) {
            scored.push((score, it.item.sort_text.as_deref().unwrap_or(&it.item.label), i));
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)).then(a.2.cmp(&b.2)));
    comp.shown = scored.into_iter().take(MAX_SHOWN).map(|s| s.2).collect();
    comp.selected = selected.and_then(|s| comp.shown.iter().position(|&i| i == s)).unwrap_or(0);
}

/// Büyük/küçük harf duyarsız alt dizi eşleşmesi; `None` = eşleşmez. Önek, ardışık ve kelime başı eşleşmeler
/// daha yüksek puan alır.
pub(super) fn fuzzy(pattern: &str, target: &str) -> Option<i32> {
    if pattern.is_empty() {
        return Some(0);
    }
    let t: Vec<char> = target.chars().collect();
    let mut score = 0;
    let mut ti = 0;
    let mut prev: Option<usize> = None;
    for pc in pattern.chars() {
        let pl = pc.to_lowercase().next().unwrap_or(pc);
        let found = (ti..t.len()).find(|&j| t[j].to_lowercase().next() == Some(pl))?;
        let boundary =
            found == 0 || !t[found - 1].is_alphanumeric() || (t[found].is_uppercase() && t[found - 1].is_lowercase());
        score += 1 + if prev == Some(found.wrapping_sub(1)) { 5 } else { 0 } + if boundary { 3 } else { 0 };
        score += i32::from(t[found] == pc);
        prev = Some(found);
        ti = found + 1;
    }
    let prefix = target.to_lowercase().starts_with(&pattern.to_lowercase());
    Some(score + if prefix { 100 } else { 0 } - (t.len() as i32 / 8))
}

/// Snippet sözdizimini düz metne çevirir: `${1:ad}` → `ad`, `$1` / `$0` / `${2}` → boş, `${1|a,b|}` → `a`,
/// `\$` → `$`.
// shortcut: yer tutucular atlanır (sekmeyle gezinme yok); snippet motoru gerekirse burası değişir.
pub(super) fn strip_snippet(s: &str) -> String {
    fn inner(chars: &[char], i: &mut usize, out: &mut String, close: bool) {
        while *i < chars.len() {
            let c = chars[*i];
            if c == '\\' && *i + 1 < chars.len() {
                out.push(chars[*i + 1]);
                *i += 2;
            } else if close && c == '}' {
                *i += 1;
                return;
            } else if c == '$' && *i + 1 < chars.len() && chars[*i + 1] == '{' {
                *i += 2;
                while *i < chars.len() && (chars[*i].is_alphanumeric() || chars[*i] == '_') {
                    *i += 1;
                }
                match chars.get(*i) {
                    Some(':') => {
                        *i += 1;
                        inner(chars, i, out, true);
                    }
                    Some('|') => {
                        *i += 1;
                        let mut first = true;
                        while *i < chars.len() && !(chars[*i] == '|' && chars.get(*i + 1) == Some(&'}')) {
                            if chars[*i] == ',' {
                                first = false;
                            } else if first {
                                out.push(chars[*i]);
                            }
                            *i += 1;
                        }
                        *i += 2;
                    }
                    _ => {
                        while *i < chars.len() && chars[*i] != '}' {
                            *i += 1;
                        }
                        *i += 1;
                    }
                }
            } else if c == '$' && chars.get(*i + 1).is_some_and(|n| n.is_alphanumeric() || *n == '_') {
                *i += 1;
                while *i < chars.len() && (chars[*i].is_alphanumeric() || chars[*i] == '_') {
                    *i += 1;
                }
            } else {
                out.push(c);
                *i += 1;
            }
        }
    }
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    inner(&chars, &mut 0, &mut out, false);
    out
}

/// Öğenin dokümantasyon metni.
pub(super) fn documentation(item: &CompletionItem) -> Option<String> {
    let text = match item.documentation.as_ref()? {
        Documentation::String(s) => s.clone(),
        Documentation::MarkupContent(m) => m.value.clone(),
    };
    (!text.trim().is_empty()).then_some(text)
}

/// Tür rozeti: harf ve tema vurgu türü.
pub(super) fn kind_badge(kind: Option<CompletionItemKind>) -> (char, less_lang::HighlightKind) {
    use CompletionItemKind as K;
    use less_lang::HighlightKind as H;
    match kind {
        Some(K::METHOD) => ('m', H::Method),
        Some(K::FUNCTION) => ('f', H::Function),
        Some(K::CONSTRUCTOR) => ('c', H::Constructor),
        Some(K::FIELD) | Some(K::PROPERTY) => ('p', H::Property),
        Some(K::VARIABLE) => ('v', H::Variable),
        Some(K::CLASS) => ('C', H::Type),
        Some(K::INTERFACE) => ('I', H::Type),
        Some(K::STRUCT) => ('S', H::Type),
        Some(K::TYPE_PARAMETER) => ('T', H::Type),
        Some(K::MODULE) => ('M', H::Namespace),
        Some(K::ENUM) => ('E', H::Type),
        Some(K::ENUM_MEMBER) => ('e', H::Constant),
        Some(K::CONSTANT) => ('K', H::Constant),
        Some(K::KEYWORD) => ('k', H::Keyword),
        Some(K::SNIPPET) => ('s', H::String),
        Some(K::COLOR) => ('#', H::Number),
        Some(K::VALUE) | Some(K::UNIT) => ('=', H::Number),
        Some(K::FILE) | Some(K::FOLDER) => ('d', H::String),
        Some(K::OPERATOR) => ('o', H::Operator),
        Some(K::EVENT) => ('!', H::Attribute),
        Some(K::REFERENCE) => ('r', H::Variable),
        _ => ('t', H::Comment),
    }
}

/// `SignatureHelp` → çizilecek görünüm (etkin imza, etkin parametre aralığı).
fn sig_view(help: lsp_types::SignatureHelp) -> Option<SigView> {
    let active_sig = help.active_signature.unwrap_or(0) as usize;
    let sig = help.signatures.get(active_sig).or(help.signatures.first())?.clone();
    let active = sig.active_parameter.or(help.active_parameter).unwrap_or(0) as usize;
    let param = sig.parameters.as_ref().and_then(|p| p.get(active));
    let range = param.and_then(|p| match &p.label {
        lsp_types::ParameterLabel::Simple(s) => sig.label.find(s.as_str()).map(|a| (a, a + s.len())),
        lsp_types::ParameterLabel::LabelOffsets([a, b]) => {
            Some((utf16_to_byte(&sig.label, *a), utf16_to_byte(&sig.label, *b)))
        }
    });
    let doc = |d: &Documentation| match d {
        Documentation::String(s) => s.clone(),
        Documentation::MarkupContent(m) => m.value.clone(),
    };
    Some(SigView {
        active: range.filter(|(a, b)| a < b && *b <= sig.label.len()),
        param_doc: param.and_then(|p| p.documentation.as_ref()).map(doc).filter(|s| !s.trim().is_empty()),
        doc: sig.documentation.as_ref().map(doc).filter(|s| !s.trim().is_empty()),
        label: sig.label,
    })
}
