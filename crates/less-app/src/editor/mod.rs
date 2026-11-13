//! Kod alanı: tab listesi ve düzenlenebilir dosya tab'ı (`less-core` editörü + `less-lang` vurgulama).
//! Çizim `view`'da; bu dosya UI'dan bağımsızdır.

pub mod find;
pub mod lang;
pub mod markdown;
pub mod syntax;
pub mod view;

use std::path::{Path, PathBuf};

use less_core::command::{AppCommand, Command};
use less_core::editor::{Editor, Session};
use less_lang::{HighlightSpan, Language, Syntax, TextEdit};
use ropey::Rope;

use syntax::{RopeText, rope_edit};

/// Düzenlenebilir (ya da salt okunur metin) tab'ı.
pub struct EditorTab {
    pub editor: Editor,
    /// Diskteki dosya; salt okunur metin tab'ında `None`.
    pub path: Option<PathBuf>,
    #[allow(dead_code, reason = "B5 editor::lang okur")]
    pub language: Option<Language>,
    /// `Cx::open_text` ile açılan tab: düzenleme geri alınır, kaydedilmez.
    pub read_only: bool,
    /// Yatay kaydırma (görüntü sütunu).
    pub left_col: usize,
    /// Metin aramasından gelince vurgulanan satır ve içindeki bayt aralıkları.
    pub mark: Option<(usize, Vec<(usize, usize)>)>,
    /// Markdown dosyası metin yerine önizleme olarak çiziliyor (#83, Cmd+P).
    pub md_preview: bool,
    /// Son yüklenen ya da kaydedilen dosyanın disk damgası (dışarıdan değişikliği fark etmek için).
    pub disk: crate::disk::Stamp,
    /// Yatay kaydırmanın en son imleci izlediği konum (fare tekerleğiyle kaydırınca geri sıçramasın).
    cursor_seen: Option<usize>,
    syntax: Option<Syntax>,
    snapshot: Rope,
    revision: u64,
    highlights: Option<(u64, usize, Vec<Vec<HighlightSpan>>)>,
    /// Mini haritanın satırları ayrı önbellekte (görünüm önbelleğiyle her karede çekişmesin).
    minimap_highlights: Option<(u64, usize, Vec<Vec<HighlightSpan>>)>,
}

impl EditorTab {
    /// Dosyadan tab; dil uzantıdan, yorum işaretleri dilden gelir.
    pub fn load(path: &Path) -> Result<Self, String> {
        let disk = crate::disk::stamp(path);
        let editor = Editor::load(path).map_err(|e| e.to_string())?;
        Ok(Self { disk, ..Self::with_editor(editor, Some(path.to_path_buf()), Language::from_path(path), false) })
    }

    /// Bellekteki metinden tab (`read_only` ise düzenlenemez).
    pub fn from_text(text: &str, language: Option<Language>, read_only: bool) -> Self {
        Self::with_editor(Editor::from_text(text), None, language, read_only)
    }

    fn with_editor(mut editor: Editor, path: Option<PathBuf>, language: Option<Language>, read_only: bool) -> Self {
        if let Some(lang) = language {
            let t = lang.comment_tokens();
            editor.buffer.comment = less_core::buffer::CommentToken {
                line: t.line.map(str::to_owned),
                block: t.block.map(|(a, b)| (a.to_owned(), b.to_owned())),
            };
        }
        let snapshot = editor.buffer.rope().clone();
        let syntax = language.and_then(|l| Syntax::new(l, &RopeText(&snapshot)));
        let revision = editor.buffer.revision();
        Self {
            editor,
            path,
            language,
            read_only,
            left_col: 0,
            mark: None,
            md_preview: false,
            disk: None,
            cursor_seen: None,
            syntax,
            snapshot,
            revision,
            highlights: None,
            minimap_highlights: None,
        }
    }

    pub fn is_dirty(&self) -> bool {
        !self.read_only && self.editor.buffer.is_dirty()
    }

    /// Komutu uygular; metin değiştiyse sözdizimi ağacı güncellenir ve değişiklik `edit` olarak döner.
    /// Salt okunur tab'da değişiklik geri alınır ve mesaj döner.
    pub fn apply(&mut self, cmd: Command, session: &mut Session) -> Applied {
        let mut app = self.editor.apply(cmd, session);
        if self.read_only && self.editor.buffer.revision() != self.revision {
            self.editor.apply(Command::Undo { count: 1 }, session);
            app = Some(AppCommand::Message("This tab is read-only".to_owned()));
        }
        Applied { app, edit: self.sync() }
    }

    /// Rope son görülen sürümden farklıysa ağaca bildirir.
    fn sync(&mut self) -> Option<TextEdit> {
        let rev = self.editor.buffer.revision();
        if rev == self.revision {
            return None;
        }
        self.revision = rev;
        let rope = self.editor.buffer.rope().clone();
        let edit = rope_edit(&self.snapshot, &rope);
        if let (Some(syntax), Some(e)) = (self.syntax.as_mut(), edit.as_ref()) {
            syntax.update(e, &RopeText(&rope));
        }
        self.snapshot = rope;
        self.highlights = None;
        self.minimap_highlights = None;
        edit
    }

    /// `rows` satırlarının vurguları (sürüm ve aralık değişmedikçe önbellekten).
    pub fn highlights(&mut self, rows: std::ops::Range<usize>) -> &[Vec<HighlightSpan>] {
        cached(&mut self.highlights, self.revision, self.syntax.as_ref(), &self.snapshot, rows)
    }

    /// Mini haritanın satır vurguları (ayrı önbellek).
    pub fn minimap_highlights(&mut self, rows: std::ops::Range<usize>) -> &[Vec<HighlightSpan>] {
        cached(&mut self.minimap_highlights, self.revision, self.syntax.as_ref(), &self.snapshot, rows)
    }

    /// Kaydeder (`path` verilirse farklı kaydet).
    pub fn save(&mut self, path: Option<&Path>) -> std::io::Result<()> {
        if self.read_only {
            return Err(std::io::Error::other("This tab is read-only"));
        }
        match path {
            Some(p) => {
                self.editor.buffer.save_as(p)?;
                self.path = Some(p.to_path_buf());
                self.language = Language::from_path(p);
                self.editor.buffer.comment = match self.language {
                    Some(lang) => {
                        let t = lang.comment_tokens();
                        less_core::buffer::CommentToken {
                            line: t.line.map(str::to_owned),
                            block: t.block.map(|(a, b)| (a.to_owned(), b.to_owned())),
                        }
                    }
                    None => less_core::buffer::CommentToken::for_path(p),
                };
                self.syntax = self.language.and_then(|lang| Syntax::new(lang, &RopeText(&self.snapshot)));
                self.highlights = None;
                self.minimap_highlights = None;
            }
            None => self.editor.save()?,
        }
        self.disk = self.path.as_deref().and_then(crate::disk::stamp);
        Ok(())
    }
}

/// Sürüm ve aralık aynıysa önbellekteki vurgular, değilse yeniden hesaplanır.
fn cached<'a>(
    slot: &'a mut Option<(u64, usize, Vec<Vec<HighlightSpan>>)>,
    rev: u64,
    syntax: Option<&Syntax>,
    rope: &Rope,
    rows: std::ops::Range<usize>,
) -> &'a [Vec<HighlightSpan>] {
    let fresh = matches!(slot, Some((r, top, v)) if *r == rev && *top == rows.start && v.len() == rows.len());
    if !fresh {
        let spans = match syntax {
            Some(s) => s.highlight_lines(&RopeText(rope), rows.clone()),
            None => vec![Vec::new(); rows.len()],
        };
        *slot = Some((rev, rows.start, spans));
    }
    slot.as_ref().map(|h| h.2.as_slice()).unwrap_or_default()
}

/// `EditorTab::apply` sonucu.
pub struct Applied {
    pub app: Option<AppCommand>,
    pub edit: Option<TextEdit>,
}

/// Kod alanındaki tab'ın içeriği.
pub enum TabKind {
    Editor(Box<EditorTab>),
    /// Bir panelin çizdiği tab (diff, veri tablosu…): `owner` panel kimliği, `key` panelin kendi anahtarı.
    Custom {
        owner: &'static str,
        key: String,
    },
}

pub struct Tab {
    pub title: String,
    pub kind: TabKind,
    /// Cmd+P (#30).
    pub pinned: bool,
    /// Önizleme tab'ı (#38): italik, tek tane; Files'tan açılan bir sonraki dosya bunun yerine geçer.
    pub preview: bool,
}

impl Tab {
    pub fn editor(&self) -> Option<&EditorTab> {
        match &self.kind {
            TabKind::Editor(e) => Some(e),
            TabKind::Custom { .. } => None,
        }
    }

    pub fn editor_mut(&mut self) -> Option<&mut EditorTab> {
        match &mut self.kind {
            TabKind::Editor(e) => Some(e),
            TabKind::Custom { .. } => None,
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.editor().and_then(|e| e.path.as_deref())
    }
}

/// Tab listesi ve etkin tab.
#[derive(Default)]
pub struct Tabs {
    pub tabs: Vec<Tab>,
    pub active: Option<usize>,
}

impl Tabs {
    pub fn active(&self) -> Option<&Tab> {
        self.active.and_then(|a| self.tabs.get(a))
    }

    pub fn active_mut(&mut self) -> Option<&mut Tab> {
        self.active.and_then(|a| self.tabs.get_mut(a))
    }

    pub fn active_editor(&self) -> Option<&EditorTab> {
        self.active().and_then(Tab::editor)
    }

    pub fn active_editor_mut(&mut self) -> Option<&mut EditorTab> {
        self.active_mut().and_then(Tab::editor_mut)
    }

    pub fn active_path(&self) -> Option<PathBuf> {
        self.active().and_then(Tab::path).map(Path::to_path_buf)
    }

    /// Aynı yolu gösteren tab.
    pub fn find_path(&self, path: &Path) -> Option<usize> {
        self.tabs.iter().position(|t| t.path() == Some(path))
    }

    pub fn find_custom(&self, owner: &str, key: &str) -> Option<usize> {
        self.tabs
            .iter()
            .position(|t| matches!(&t.kind, TabKind::Custom { owner: o, key: k } if *o == owner && k == key))
    }

    /// Salt okunur metin tab'ı başlığa göre bulunur (aynı başlık yeniden açılınca içerik yenilenir).
    pub fn find_text(&self, title: &str) -> Option<usize> {
        self.tabs.iter().position(|t| t.title == title && t.editor().is_some_and(|e| e.path.is_none() && e.read_only))
    }

    /// Tab ekler ve etkin yapar.
    pub fn push(&mut self, title: String, kind: TabKind) -> usize {
        self.tabs.push(Tab { title, kind, pinned: false, preview: false });
        let i = self.tabs.len() - 1;
        self.active = Some(i);
        i
    }

    /// Tab'ı kaldırır; etkin tab bir sola kayar.
    pub fn remove(&mut self, i: usize) -> Option<Tab> {
        if i >= self.tabs.len() {
            return None;
        }
        let tab = self.tabs.remove(i);
        self.active = match self.active {
            _ if self.tabs.is_empty() => None,
            Some(a) if a > i || a == self.tabs.len() => Some(a.saturating_sub(1)),
            other => other,
        };
        Some(tab)
    }

    /// Cmd+P (#30): etkin tab'ın pinini değiştirir; dönen değer yeni durum.
    #[cfg(test)]
    pub fn toggle_pin(&mut self) -> Option<bool> {
        self.toggle_pin_at(self.active?)
    }

    /// `i` tab'ının pinini değiştirir (#30): pinlenen tab pinli grubun sonuna, pini kalkan grubun hemen arkasına
    /// taşınır; önizleme tab'ı normal tab olur (#38). Etkin tab aynı tab'da kalır.
    pub fn toggle_pin_at(&mut self, i: usize) -> Option<bool> {
        let tab = self.tabs.get_mut(i)?;
        tab.pinned = !tab.pinned;
        tab.preview = false;
        let pinned = tab.pinned;
        let others = self.tabs.iter().enumerate().filter(|(j, t)| *j != i && t.pinned).count();
        self.move_tab(i, others);
        Some(pinned)
    }

    /// Tab'ı `to` sırasına taşır; etkin tab izlenir.
    pub fn move_tab(&mut self, from: usize, to: usize) {
        if from >= self.tabs.len() {
            return;
        }
        let tab = self.tabs.remove(from);
        let to = to.min(self.tabs.len());
        self.tabs.insert(to, tab);
        self.active = self.active.map(|a| if a == from { to } else { shift_index(a, from, to) });
    }

    /// Önizleme tab'ı.
    pub fn preview_index(&self) -> Option<usize> {
        self.tabs.iter().position(|t| t.preview)
    }

    /// Sağ tık menüsündeki toplu kapatmanın kapatacağı tab'lar (büyükten küçüğe, böylece sırayla silinebilir).
    /// Pinli tab'lar yalnız `CloseAll` ile kapanır.
    pub fn close_targets(&self, i: usize, which: CloseWhich) -> Vec<usize> {
        let keep = |j: usize, t: &Tab| match which {
            CloseWhich::Others => j == i || t.pinned,
            CloseWhich::Left => j >= i || t.pinned,
            CloseWhich::Right => j <= i || t.pinned,
            CloseWhich::All => false,
        };
        let mut out: Vec<usize> = self.tabs.iter().enumerate().filter(|(j, t)| !keep(*j, t)).map(|(j, _)| j).collect();
        out.reverse();
        out
    }

    /// Koda geçişte (Cmd+2) gösterilecek tab: en baştaki pinli tab, yoksa etkin tab (#30).
    pub fn focus_target(&self) -> Option<usize> {
        self.tabs.iter().position(|t| t.pinned).or(self.active)
    }

    pub fn dirty_count(&self) -> usize {
        self.tabs.iter().filter(|t| t.editor().is_some_and(EditorTab::is_dirty)).count()
    }
}

/// Sağ tık menüsündeki toplu kapatma.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseWhich {
    Others,
    Left,
    Right,
    All,
}

/// `from` → `to` taşımasından sonra başka bir tab'ın yeni sırası.
fn shift_index(a: usize, from: usize, to: usize) -> usize {
    if from < a && a <= to {
        a - 1
    } else if to <= a && a < from {
        a + 1
    } else {
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use less_core::registers::MemoryClipboard;
    use less_input::{EditMode, Engine, parse_keys};

    fn session() -> Session {
        Session::new(Box::new(MemoryClipboard::default()))
    }

    fn feed(tab: &mut EditorTab, engine: &mut Engine, s: &mut Session, keys: &str) -> Vec<TextEdit> {
        let mut edits = Vec::new();
        for k in parse_keys(keys) {
            for c in engine.feed(k).commands {
                edits.extend(tab.apply(c, s).edit);
            }
        }
        edits
    }

    #[test]
    fn typing_updates_syntax_and_highlights() {
        let mut s = session();
        let mut tab = EditorTab::from_text("x = 1", Some(Language::Lua), false);
        let mut engine = Engine::new(EditMode::Vim);
        let edits = feed(&mut tab, &mut engine, &mut s, "Ilocal <Esc>");
        assert!(!edits.is_empty());
        assert_eq!(tab.editor.text(), "local x = 1");
        let spans = tab.highlights(0..1)[0].clone();
        assert!(spans.iter().any(|sp| sp.start == 0 && sp.end == 5), "{spans:?}");
    }

    #[test]
    fn save_as_detects_language_and_highlighting() {
        let dir = std::env::temp_dir().join(format!("less-b0-save-as-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.rs");
        let mut tab = EditorTab::from_text("fn main() {}", None, false);
        tab.save(Some(&path)).unwrap();
        assert_eq!(tab.path.as_deref(), Some(path.as_path()));
        assert_eq!(tab.language, Some(Language::Rust));
        assert!(tab.highlights(0..1)[0].iter().any(|s| s.kind == less_lang::HighlightKind::Keyword));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn read_only_tab_rejects_edits() {
        let mut s = session();
        let mut tab = EditorTab::from_text("diff", None, true);
        let mut engine = Engine::new(EditMode::Vim);
        let mut msgs = Vec::new();
        for k in parse_keys("x") {
            for c in engine.feed(k).commands {
                msgs.extend(tab.apply(c, &mut s).app);
            }
        }
        assert_eq!(tab.editor.text(), "diff");
        assert!(msgs.contains(&AppCommand::Message("This tab is read-only".to_owned())));
        assert!(!tab.is_dirty());
    }

    fn tabs(n: usize) -> Tabs {
        let mut t = Tabs::default();
        for i in 0..n {
            t.push(format!("t{i}"), TabKind::Custom { owner: "test", key: i.to_string() });
        }
        t
    }

    #[test]
    fn remove_and_cycle_keep_active_valid() {
        let mut t = tabs(3);
        assert_eq!(t.active, Some(2));
        t.remove(2);
        assert_eq!(t.active, Some(1));
        t.remove(0);
        assert_eq!(t.active, Some(0));
        t.remove(0);
        assert_eq!(t.active, None);
        assert_eq!(t.find_custom("test", "1"), None);
    }

    #[test]
    fn pinning_moves_tab_to_front_and_unpinning_after_pinned_group_30() {
        let mut t = tabs(4);
        t.tabs[3].preview = true;
        assert_eq!(t.toggle_pin_at(3), Some(true));
        assert_eq!(t.tabs.iter().map(|x| x.title.as_str()).collect::<Vec<_>>(), ["t3", "t0", "t1", "t2"]);
        assert_eq!(t.active, Some(0), "active follows the moved tab");
        assert!(!t.tabs[0].preview, "pinning promotes a preview tab (#38)");
        t.active = Some(2);
        assert_eq!(t.toggle_pin_at(2), Some(true));
        assert_eq!(t.tabs.iter().map(|x| x.title.as_str()).collect::<Vec<_>>(), ["t3", "t1", "t0", "t2"]);
        assert_eq!(t.active, Some(1));
        assert_eq!(t.toggle_pin_at(0), Some(false));
        assert_eq!(t.tabs.iter().map(|x| x.title.as_str()).collect::<Vec<_>>(), ["t1", "t3", "t0", "t2"]);
        assert_eq!(t.active, Some(0), "t1 slid left");
    }

    #[test]
    fn close_menu_targets_skip_pinned_except_close_all() {
        let mut t = tabs(5);
        t.tabs[0].pinned = true;
        assert_eq!(t.close_targets(2, CloseWhich::Others), [4, 3, 1]);
        assert_eq!(t.close_targets(2, CloseWhich::Left), [1]);
        assert_eq!(t.close_targets(2, CloseWhich::Right), [4, 3]);
        assert_eq!(t.close_targets(2, CloseWhich::All), [4, 3, 2, 1, 0]);
    }

    #[test]
    fn code_focus_prefers_first_pinned_tab_30() {
        let mut t = tabs(3);
        assert_eq!(t.focus_target(), Some(2));
        t.active = Some(1);
        assert_eq!(t.toggle_pin(), Some(true));
        t.active = Some(2);
        assert_eq!(t.focus_target(), Some(0), "pinned tab moved to the front");
    }
}
