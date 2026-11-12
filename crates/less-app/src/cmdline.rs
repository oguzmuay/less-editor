//! Yüzen komut satırı (#87, LazyVim'deki noice `cmdline_popup`): `:` / `/` / `?` penceresin üst ortasında başlıklı bir
//! kutuda yazılır, `:`'da altında öneri listesi açılır (ex komutları, `:set` seçenekleri, `:e` / `:w` yolları).
//! Metnin kendisi `less-input` motorunda kalır (`Status::cmdline`); burası yalnız öneri durumu ve çizimdir. Seçilen
//! öneri motora Ctrl+U + karakterler olarak yazılır (motorda ek API gerekmez).

use std::path::{Path, PathBuf};

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use less_input::{Key, KeyEvent, Mods};
use less_theme::Palette;

use crate::fonts::icon;
use crate::widgets::{color, ui_font};

/// Önerilen ex komutları: motorun çalıştırdıkları, host'unkiler (`dispatch::builtin_ex`) ve panellerin aldıkları.
/// shortcut: paneller komut adlarını sözleşmeyle bildirmiyor, liste elle; yeni panel komutu buraya da eklenmeli.
pub const COMMANDS: &[(&str, &str)] = &[
    ("write", "Write the file"),
    ("wq", "Write and close"),
    ("wqa", "Write all and quit"),
    ("xit", "Write if modified and close"),
    ("xa", "Write modified files and quit"),
    ("quit", "Close"),
    ("qa", "Quit LESS"),
    ("edit", "Edit a file (! reloads)"),
    ("set", "Set an option"),
    ("substitute", "Replace text"),
    ("nohlsearch", "Clear search highlight"),
    ("delete", "Delete lines"),
    ("yank", "Yank lines"),
    ("join", "Join lines"),
    ("undo", "Undo"),
    ("redo", "Redo"),
    ("Files", "Toggle Files"),
    ("Code", "Focus the code editor"),
    ("AI", "Toggle the AI panel"),
    ("Claude", "Toggle the AI panel"),
    ("Terminal", "Toggle the terminal"),
    ("Datasource", "Toggle Database"),
    ("Maven", "Toggle Maven"),
    ("MavenNew", "New Maven project (groupId:artifactId)"),
    ("Run", "Toggle Run"),
    ("Git", "Toggle Git"),
    ("GitHistory", "Toggle Git History"),
    ("GitBlame", "Toggle blame annotations"),
    ("Messages", "Toggle message history"),
    ("Notes", "Toggle notes"),
    ("Minimap", "Toggle the minimap"),
    ("Preview", "Preview Markdown / open HTML in the browser"),
    ("Shortcuts", "Search shortcuts"),
    ("Theme", "Choose a theme"),
    ("Font", "Choose a font"),
    ("NewWindow", "Open a new window"),
    ("UIReset", "Reset panel sizes"),
    ("options", "Language server options"),
    ("R", "Rename the AI / terminal tab"),
    ("TerminalNew", "New terminal tab"),
    ("ClaudeNew", "New AI tab"),
    ("TermTabClose", "Close the AI / terminal tab"),
];

/// Komut başına bilinen argümanlar.
const ARGS: &[(&str, &[&str])] = &[("Notes", &["new", "rename", "delete"]), ("Claude", &["account"])];

/// `:set` seçenekleri: editörün (`less-core`) ve host'un (`dispatch::set_option`) bildikleri.
pub const OPTIONS: &[&str] = &[
    "number",
    "relativenumber",
    "tabstop",
    "shiftwidth",
    "expandtab",
    "hlsearch",
    "ignorecase",
    "smartcase",
    "wrapscan",
    "scrolloff",
];

const MAX_ROWS: usize = 10;
const MAX_PATHS: usize = 200;

/// Bir öneri: listede görünen ad, açıklama ve seçilince komut satırının tamamı (ön ek karakteri hariç).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub label: String,
    pub detail: String,
    pub text: String,
}

/// Komut satırı metni (`:` hariç) için öneriler. `root` göreli yolların tabanı.
pub fn complete(text: &str, root: &Path) -> Vec<Item> {
    let range_len = range_prefix(text);
    let (range, rest) = text.split_at(range_len);
    let name_len = rest.chars().take_while(char::is_ascii_alphabetic).count();
    let (name, after) = rest.split_at(name_len);
    if after.is_empty() {
        return commands(range, name);
    }
    let arg = after.strip_prefix('!').unwrap_or(after);
    let Some(arg) = arg.strip_prefix(' ') else { return Vec::new() };
    let head = &text[..text.len() - arg.len()];
    let ab = |full: &str, min: usize| name.len() >= min && full.starts_with(name);
    if ab("set", 2) {
        return set_options(head, arg);
    }
    if ab("edit", 1) || ab("write", 1) {
        return paths(head, arg, root);
    }
    let Some((_, args)) = ARGS.iter().find(|(c, _)| *c == name) else { return Vec::new() };
    args.iter()
        .filter(|a| a.starts_with(arg.trim_start()) && **a != arg)
        .map(|a| Item { label: (*a).to_owned(), detail: String::new(), text: format!("{head}{a}") })
        .collect()
}

/// Baştaki satır aralığı (`%`, `'<,'>`, `1,5`, `.,$+2`) uzunluğu.
fn range_prefix(text: &str) -> usize {
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b' ' | b'0'..=b'9' | b',' | b'.' | b';' | b'$' | b'%' | b'+' | b'-' => i += 1,
            b'\'' if i + 1 < b.len() => i += 2,
            _ => break,
        }
    }
    i
}

/// Komut adı önerileri: önce büyük/küçük harf duyarlı ön ek, sonra duyarsız.
fn commands(range: &str, name: &str) -> Vec<Item> {
    let lower = name.to_ascii_lowercase();
    let exact = COMMANDS.iter().filter(|(c, _)| c.starts_with(name));
    let loose = COMMANDS.iter().filter(|(c, _)| !c.starts_with(name) && c.to_ascii_lowercase().starts_with(&lower));
    exact
        .chain(loose)
        .map(|(c, d)| Item { label: (*c).to_owned(), detail: (*d).to_owned(), text: format!("{range}{c}") })
        .collect()
}

fn set_options(head: &str, arg: &str) -> Vec<Item> {
    let start = arg.rfind(' ').map_or(0, |p| p + 1);
    let (keep, word) = arg.split_at(start);
    let (prefix, word) = ["no", "inv"]
        .iter()
        .find_map(|p| word.strip_prefix(p).filter(|w| OPTIONS.iter().any(|o| o.starts_with(w))).map(|w| (*p, w)))
        .unwrap_or(("", word));
    OPTIONS
        .iter()
        .filter(|o| o.starts_with(word))
        .map(|o| Item {
            label: format!("{prefix}{o}"),
            detail: String::new(),
            text: format!("{head}{keep}{prefix}{o}"),
        })
        .collect()
}

/// `:e` / `:w` yolu: yazılan klasördeki girdiler (klasörler `/` ile, önce), gizli dosyalar yalnız `.` yazılınca.
fn paths(head: &str, arg: &str, root: &Path) -> Vec<Item> {
    let (dir_part, base) = arg.rfind('/').map_or(("", arg), |i| (&arg[..=i], &arg[i + 1..]));
    let dir: PathBuf = match dir_part.strip_prefix("~/") {
        Some(rest) => less_workspace::paths::home().join(rest),
        None if dir_part.starts_with('/') => PathBuf::from(dir_part),
        None => root.join(dir_part),
    };
    let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
    let lower = base.to_lowercase();
    let mut found: Vec<(bool, String)> = entries
        .flatten()
        .take(MAX_PATHS * 10)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let visible = !name.starts_with('.') || base.starts_with('.');
            (visible && name.to_lowercase().starts_with(&lower)).then(|| (e.path().is_dir(), name))
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase())));
    found
        .into_iter()
        .take(MAX_PATHS)
        .map(|(is_dir, name)| {
            let slash = if is_dir { "/" } else { "" };
            Item {
                label: format!("{name}{slash}"),
                detail: if is_dir { "Folder".into() } else { String::new() },
                text: format!("{head}{dir_part}{name}{slash}"),
            }
        })
        .collect()
}

/// Bir tuşun sonucu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyOutcome {
    /// Motora ver.
    Pass,
    /// Burada kullanıldı.
    Consumed,
    /// Komut satırını bu metinle değiştir (ön ek hariç).
    Replace(String),
}

/// Öneri listesinin durumu; komut satırı kapanınca sıfırlanır.
#[derive(Default)]
pub struct Cmdline {
    kind: char,
    line: Option<String>,
    items: Vec<Item>,
    selected: Option<usize>,
    /// Son seçilen önerinin metni: motor bunu geri yansıtınca liste yeniden hesaplanmaz.
    applied: Option<String>,
    /// ↑↓ geçmişe gittiyse liste bir sonraki yazılan tuşa kadar kapalı.
    hidden: bool,
}

impl Cmdline {
    /// Motorun komut satırı (`Status::cmdline`, ön ekli) ile eşler; metin değiştiyse önerileri yeniden hesaplar.
    pub fn sync(&mut self, line: Option<&str>, root: &Path) {
        let Some(line) = line else {
            *self = Self::default();
            return;
        };
        if self.line.as_deref() == Some(line) {
            return;
        }
        self.line = Some(line.to_owned());
        let mut chars = line.chars();
        self.kind = chars.next().unwrap_or(':');
        let text = chars.as_str();
        if self.applied.as_deref() == Some(text) {
            return;
        }
        self.applied = None;
        self.selected = None;
        self.items = if self.kind == ':' && !text.is_empty() { complete(text, root) } else { Vec::new() };
    }

    pub fn list_open(&self) -> bool {
        !self.hidden && !self.items.is_empty()
    }

    /// Tab / Shift+Tab her zaman, ↑↓ liste açıkken öneriler arasında gezer; ↑↓ liste kapalıyken geçmiştir (motor).
    pub fn key(&mut self, k: &KeyEvent) -> KeyOutcome {
        let step = match k.key {
            Key::Tab if self.kind == ':' => {
                self.hidden = false;
                if k.mods.shift { -1 } else { 1 }
            }
            Key::Up | Key::Down if self.list_open() => {
                if k.key == Key::Up {
                    -1
                } else {
                    1
                }
            }
            Key::Up | Key::Down => {
                self.hidden = true;
                return KeyOutcome::Pass;
            }
            _ => {
                if matches!(k.key, Key::Char(_) | Key::Backspace | Key::Delete) {
                    self.hidden = false;
                }
                return KeyOutcome::Pass;
            }
        };
        if self.items.is_empty() {
            return KeyOutcome::Consumed;
        }
        let n = self.items.len() as isize;
        let next = match self.selected {
            None if step > 0 => 0,
            None => n - 1,
            Some(i) => (i as isize + step).rem_euclid(n),
        };
        self.choose(next as usize)
    }

    fn choose(&mut self, i: usize) -> KeyOutcome {
        self.selected = Some(i);
        let text = self.items[i].text.clone();
        self.applied = Some(text.clone());
        KeyOutcome::Replace(text)
    }

    /// Kutu ve öneri listesi; `window` pencerenin tamamı, `top` kutunun üst kenarı. Öneriye tıklanırsa onun metni.
    pub fn show(&mut self, ctx: &egui::Context, p: &Palette, window: Rect, top: f32) -> Option<String> {
        let line = self.line.clone()?;
        let text = line.get(self.kind.len_utf8()..).unwrap_or_default();
        let width = (window.width() - 80.0).clamp(240.0, 640.0);
        let left = window.center().x - width / 2.0;
        let (title, glyph, accent) = match self.kind {
            ':' => ("Cmdline", icon::CARET_RIGHT.to_owned(), p.extra.info),
            '/' => ("Search", format!("{} {}", icon::MAGNIFYING_GLASS, icon::CARET_DOWN), p.extra.warn),
            _ => ("Search", format!("{} {}", icon::MAGNIFYING_GLASS, icon::CARET_UP), p.extra.warn),
        };
        let box_rect = Rect::from_min_size(pos2(left, top), vec2(width, 34.0));
        egui::Area::new(egui::Id::unique("less-cmdline"))
            .order(egui::Order::Foreground)
            .fixed_pos(box_rect.min)
            .interactable(false)
            .show(ctx, |ui| paint_box(ui, p, box_rect, title, &glyph, text, color(accent)));
        if !self.list_open() {
            return None;
        }
        let start = self.selected.map_or(0, |s| s.saturating_sub(MAX_ROWS - 1));
        let rows = self.items.len().min(MAX_ROWS);
        let row_h = 22.0;
        let list = Rect::from_min_size(pos2(left, box_rect.bottom() + 4.0), vec2(width, rows as f32 * row_h + 8.0));
        let mut clicked = None;
        egui::Area::new(egui::Id::unique("less-cmdline-list")).order(egui::Order::Foreground).fixed_pos(list.min).show(
            ctx,
            |ui| {
                let painter = ui.painter();
                painter.rect_filled(list, 6.0, color(p.surface));
                painter.rect_stroke(list, 6.0, Stroke::new(1.0, color(p.border)), egui::StrokeKind::Inside);
                for (row, i) in (start..self.items.len()).take(MAX_ROWS).enumerate() {
                    let item = &self.items[i];
                    let r = Rect::from_min_size(
                        pos2(left + 4.0, list.top() + 4.0 + row as f32 * row_h),
                        vec2(width - 8.0, row_h),
                    );
                    let resp = ui.interact(r, ui.scope_id().with(("cmdline-item", i)), Sense::click());
                    if self.selected == Some(i) || resp.hovered() {
                        ui.painter().rect_filled(r, 4.0, color(p.bg_highlight));
                    }
                    let y = r.center().y;
                    let mono = FontId::monospace(13.0);
                    ui.painter().text(pos2(r.left() + 8.0, y), Align2::LEFT_CENTER, &item.label, mono, color(p.fg));
                    let detail = ui_font(12.0);
                    ui.painter().text(
                        pos2(r.right() - 8.0, y),
                        Align2::RIGHT_CENTER,
                        &item.detail,
                        detail,
                        color(p.comment),
                    );
                    if resp.clicked() {
                        clicked = Some(i);
                    }
                }
            },
        );
        match clicked.map(|i| self.choose(i)) {
            Some(KeyOutcome::Replace(t)) => Some(t),
            _ => None,
        }
    }
}

/// noice tarzı kutu: renkli kenar, üst kenarın ortasında başlık, solda istem simgesi, metin ve çubuk imleç.
fn paint_box(ui: &mut Ui, p: &Palette, rect: Rect, title: &str, glyph: &str, text: &str, accent: Color32) {
    let painter = ui.painter();
    painter.rect_filled(rect.translate(vec2(0.0, 4.0)).expand(2.0), 8.0, Color32::from_black_alpha(40));
    painter.rect_filled(rect, 6.0, color(p.bg));
    painter.rect_stroke(rect, 6.0, Stroke::new(1.0, accent), egui::StrokeKind::Inside);
    let title = painter.layout_no_wrap(format!(" {title} "), ui_font(12.0), accent);
    let title_pos = Pos2::new(rect.center().x - title.size().x / 2.0, rect.top() - title.size().y / 2.0);
    painter.rect_filled(Rect::from_min_size(title_pos, title.size()), 0.0, color(p.bg));
    painter.galley(title_pos, title, accent);
    let y = rect.center().y;
    let icon_g = painter.layout_no_wrap(glyph.to_owned(), ui_font(14.0), accent);
    let text_x = rect.left() + 14.0 + icon_g.size().x + 8.0;
    painter.galley(pos2(rect.left() + 14.0, y - icon_g.size().y / 2.0), icon_g, accent);
    let shown = text.replace('\t', " ");
    let galley = painter.layout_no_wrap(shown, FontId::monospace(14.0), color(p.fg));
    let max_x = rect.right() - 14.0;
    let overflow = (text_x + galley.size().x + 2.0 - max_x).max(0.0);
    let clip = Rect::from_min_max(pos2(text_x, rect.top()), pos2(max_x, rect.bottom()));
    let at = pos2(text_x - overflow, y - galley.size().y / 2.0);
    let cursor_x = at.x + galley.size().x + 1.0;
    let painter = painter.with_clip_rect(clip);
    painter.galley(at, galley, color(p.fg));
    painter.vline(cursor_x, (y - 8.0)..=(y + 8.0), Stroke::new(1.5, color(p.fg)));
}

/// Ctrl+U + karakterler: motorun komut satırını `text` yapan tuşlar.
pub fn replace_keys(text: &str) -> Vec<KeyEvent> {
    std::iter::once(KeyEvent::new(Key::Char('u'), Mods { ctrl: true, ..Mods::NONE }))
        .chain(text.chars().map(KeyEvent::char))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(items: &[Item]) -> Vec<&str> {
        items.iter().map(|i| i.label.as_str()).collect()
    }

    #[test]
    fn commands_complete_by_prefix_with_range_kept_87() {
        let root = Path::new("/nonexistent");
        assert_eq!(labels(&complete("Git", root)), ["Git", "GitHistory", "GitBlame"]);
        let items = complete("'<,'>s", root);
        assert_eq!(items[0].text, "'<,'>set");
        assert!(labels(&complete("op", root)).contains(&"options"));
        assert!(labels(&complete("fi", root)).contains(&"Files"), "case-insensitive fallback");
        assert!(labels(&complete("Min", root)) == ["Minimap"]);
    }

    #[test]
    fn set_options_and_command_args_87() {
        let root = Path::new("/nonexistent");
        let items = complete("set nore", root);
        assert_eq!(labels(&items), ["norelativenumber"]);
        assert_eq!(items[0].text, "set norelativenumber");
        assert_eq!(complete("set ts=4 hl", root)[0].text, "set ts=4 hlsearch");
        assert_eq!(labels(&complete("Notes r", root)), ["rename"]);
        assert!(complete("Files x", root).is_empty());
    }

    #[test]
    fn edit_completes_paths_dirs_first_hiding_dotfiles_87() {
        let dir = std::env::temp_dir().join(format!("less-hs-cmdline-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "").unwrap();
        std::fs::write(dir.join(".env"), "").unwrap();
        std::fs::write(dir.join("src/main.rs"), "").unwrap();
        assert_eq!(labels(&complete("e ", &dir)), ["src/", "Cargo.toml"]);
        assert_eq!(complete("e! sr", &dir)[0].text, "e! src/");
        assert_eq!(complete("w src/m", &dir)[0].text, "w src/main.rs");
        assert_eq!(labels(&complete("e .", &dir)), [".env"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn tab_cycles_and_arrows_fall_back_to_history_87() {
        let root = Path::new("/nonexistent");
        let mut c = Cmdline::default();
        c.sync(Some(":Git"), root);
        assert!(c.list_open());
        assert_eq!(c.key(&KeyEvent::plain(Key::Tab)), KeyOutcome::Replace("Git".into()));
        c.sync(Some(":Git"), root);
        assert_eq!(c.key(&KeyEvent::plain(Key::Down)), KeyOutcome::Replace("GitHistory".into()));
        c.sync(Some(":GitHistory"), root);
        assert_eq!(c.items.len(), 3, "applied text keeps the list");
        let shift_tab = KeyEvent::new(Key::Tab, Mods { shift: true, ..Mods::NONE });
        assert_eq!(c.key(&shift_tab), KeyOutcome::Replace("Git".into()));
        c.sync(None, root);
        c.sync(Some(":"), root);
        assert!(!c.list_open());
        assert_eq!(c.key(&KeyEvent::plain(Key::Up)), KeyOutcome::Pass, "history");
        c.sync(Some(":Git"), root);
        assert!(!c.list_open(), "recalled history keeps the list closed");
        assert_eq!(c.key(&KeyEvent::plain(Key::Up)), KeyOutcome::Pass);
        assert_eq!(c.key(&KeyEvent::char('x')), KeyOutcome::Pass);
        c.sync(Some(":Gitx"), root);
        assert_eq!(c.key(&KeyEvent::plain(Key::Tab)), KeyOutcome::Consumed, "no tab character inserted");
        c.sync(Some("/foo"), root);
        assert_eq!(c.key(&KeyEvent::plain(Key::Tab)), KeyOutcome::Pass, "search keeps the engine's Tab");
    }
}
