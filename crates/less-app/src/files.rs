//! Files paneli (host'un kendi sol paneli): klasör ağacı, klavye imleci, tık / Enter ile açma.
//!
//! Klavye (odaktayken): ↑↓ gez, → / Enter klasörü aç, ← kapat ya da üst klasöre çık; dosyada Enter önizleme tab'ında
//! açar (odak Files'ta kalır, #38 #55), açık dosyada ikinci Enter normal tab yapıp koda geçer; `r` ağacı diskten yeniler. Fare: tek tık önizleme,
//! çift tık normal tab. İmleç satırı her zaman görünür kalır (üstte / altta iki satır pay).

use std::path::{Path, PathBuf};

use eframe::egui::{self, Align2, Response, Sense, Ui, pos2, vec2};
use less_input::{Key, KeyEvent};
use less_theme::Palette;
use less_workbench::tree::{FileTree, Row};

use crate::fonts::icon;
use crate::widgets::{color, ui_font};

const TREE_ROW: f32 = 26.0;
/// İmleçle görünüm kenarı arasında bırakılan satır (Vim `scrolloff`).
const SCROLL_MARGIN: f32 = 2.0;

/// Files'ın host'tan istediği iş.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FilesOutcome {
    /// Tuş Files'ın değil.
    Unhandled,
    Handled,
    /// Önizleme tab'ında aç, odak Files'ta kalsın (#38 #55).
    Preview(PathBuf),
    /// Normal tab olarak aç ve koda geç (çift tık).
    Open(PathBuf),
    /// Açık dosyada ikinci Enter: önizlemeyi normal tab yap, koda geç.
    FocusCode,
}

pub struct Files {
    pub tree: FileTree,
    /// Klavyeyle seçili satır (yol olarak; ağaç değişince de korunur).
    cursor: Option<PathBuf>,
    reveal_cursor: bool,
    scroll: f32,
}

impl Files {
    pub fn new(roots: &[PathBuf]) -> Self {
        Self { tree: FileTree::new(roots), cursor: None, reveal_cursor: false, scroll: 0.0 }
    }

    /// Odak gelince: imleç geçersizse açık dosyaya, o da görünmüyorsa ilk satıra konur.
    pub fn focus(&mut self, active: Option<&Path>) {
        let rows = self.tree.rows();
        let valid = |p: &PathBuf| rows.iter().any(|r| &r.path == p);
        if !self.cursor.as_ref().is_some_and(valid) {
            self.cursor = active.map(Path::to_path_buf).filter(valid).or_else(|| rows.first().map(|r| r.path.clone()));
        }
        self.reveal_cursor = true;
    }

    /// Dosyanın üst klasörlerini açar ve imleci koyar; dosya ağaçta yoksa `false`.
    pub fn reveal(&mut self, path: &Path) -> bool {
        let ok = self.tree.reveal(path);
        if ok {
            self.cursor = Some(path.to_path_buf());
            self.reveal_cursor = true;
        }
        ok
    }

    /// Ağacı diskten yeniler (açık klasörler ve imleç korunur; imleç satırı silindiyse `key` ilk satırdan devam eder).
    pub fn refresh(&mut self) {
        self.tree.refresh();
        self.reveal_cursor = true;
    }

    /// Cmd+N için hedef klasör: seçili klasörün kendisi, dosyanın üst klasörü, seçim yoksa ilk kök (#53).
    pub fn new_file_dir(&self) -> PathBuf {
        let rows = self.tree.rows();
        self.cursor
            .as_ref()
            .and_then(|path| rows.iter().find(|row| &row.path == path))
            .map(|row| if row.is_dir { row.path.clone() } else { row.path.parent().unwrap_or(&row.path).to_path_buf() })
            .or_else(|| rows.first().map(|row| row.path.clone()))
            .unwrap_or_default()
    }

    /// Odaktayken tuş; `Unhandled` ise uygulama kısayoluna bakılır.
    pub fn key(&mut self, k: &KeyEvent, active: Option<&Path>) -> FilesOutcome {
        if k.mods.cmd || k.mods.alt || k.mods.ctrl {
            return FilesOutcome::Unhandled;
        }
        if k.is_char('r') {
            self.refresh();
            return FilesOutcome::Handled;
        }
        let (up, down, left, right, enter) =
            (k.key == Key::Up, k.key == Key::Down, k.key == Key::Left, k.key == Key::Right, k.key == Key::Enter);
        if !(up || down || left || right || enter) {
            return FilesOutcome::Unhandled;
        }
        let rows = self.tree.rows();
        if rows.is_empty() {
            return FilesOutcome::Handled;
        }
        let idx = self.cursor.as_ref().and_then(|c| rows.iter().position(|r| &r.path == c)).unwrap_or(0);
        let row = &rows[idx];
        let mut next = idx;
        let mut out = FilesOutcome::Handled;
        if up {
            next = idx.saturating_sub(1);
        } else if down {
            next = (idx + 1).min(rows.len() - 1);
        } else if left {
            if row.is_dir && row.expanded {
                self.tree.set_expanded(&row.path, false);
            } else if let Some(parent) = rows[..idx].iter().rposition(|r| r.depth + 1 == row.depth) {
                next = parent;
            }
        } else if row.is_dir {
            if row.expanded && right {
                next = (idx + 1).min(rows.len() - 1);
            } else {
                self.tree.set_expanded(&row.path, !row.expanded || right);
            }
        } else if enter {
            out = if active == Some(row.path.as_path()) {
                FilesOutcome::FocusCode
            } else {
                FilesOutcome::Preview(row.path.clone())
            };
        }
        self.cursor = Some(rows[next].path.clone());
        self.reveal_cursor = true;
        out
    }

    /// Ağacı çizer; tıklanan dosya `Preview` (tek tık) ya da `Open` (çift tık) olarak döner.
    pub fn show(&mut self, ui: &mut Ui, p: &Palette, focused: bool, active: Option<&Path>) -> Option<FilesOutcome> {
        ui.painter().rect_filled(ui.max_rect(), 0.0, color(p.surface));
        ui.spacing_mut().item_spacing.y = 0.0;
        let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
        let head_color = if focused { color(p.accent) } else { color(p.comment) };
        ui.painter().text(
            head.left_center() + vec2(14.0, 0.0),
            Align2::LEFT_CENTER,
            "FILES",
            ui_font(11.0),
            head_color,
        );
        let rows = self.tree.rows();
        let cursor_idx = self.cursor.as_ref().and_then(|c| rows.iter().position(|r| &r.path == c));
        let mut area = egui::ScrollArea::vertical().id_salt("files-tree").auto_shrink([false, false]);
        if std::mem::take(&mut self.reveal_cursor)
            && let Some(i) = cursor_idx
        {
            area = area.vertical_scroll_offset(reveal_offset(self.scroll, i, ui.available_height()));
        }
        let mut clicked: Option<(Row, bool)> = None;
        let out = area.show_rows(ui, TREE_ROW, rows.len(), |ui, range| {
            for (i, row) in rows.iter().enumerate().take(range.end).skip(range.start) {
                let state =
                    RowState { selected: active == Some(row.path.as_path()), cursor: Some(i) == cursor_idx, focused };
                let resp = tree_row(ui, p, row, state);
                if resp.double_clicked() {
                    clicked = Some((row.clone(), true));
                } else if resp.clicked() {
                    clicked = Some((row.clone(), false));
                }
            }
        });
        self.scroll = out.state.clamped_offset().y;
        let (row, double) = clicked?;
        self.cursor = Some(row.path.clone());
        if row.is_dir {
            if !double {
                self.tree.toggle(&row.path);
            }
            None
        } else if double {
            Some(FilesOutcome::Open(row.path))
        } else {
            Some(FilesOutcome::Preview(row.path))
        }
    }
}

/// `row` satırını `view_h` yüksekliğindeki görünümde tutan kaydırma; satır zaten görünürse `scroll` değişmez.
fn reveal_offset(scroll: f32, row: usize, view_h: f32) -> f32 {
    let top = row as f32 * TREE_ROW;
    let margin = (SCROLL_MARGIN * TREE_ROW).min(((view_h - TREE_ROW) / 2.0).max(0.0));
    scroll.min(top - margin).max(top + TREE_ROW + margin - view_h).max(0.0)
}

/// Dosya uzantısına göre ikon.
pub fn file_icon(name: &str) -> &'static str {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "rs" | "java" | "kt" | "js" | "jsx" | "ts" | "tsx" | "lua" | "py" | "go" | "c" | "h" | "cpp" | "swift"
        | "zig" | "sh" | "sql" | "html" | "css" | "scss" | "vue" | "json" | "toml" | "yml" | "yaml" | "xml" => {
            icon::FILE_CODE
        }
        "md" | "txt" | "rst" => icon::FILE_TEXT,
        _ => icon::FILE,
    }
}

/// Ağaç satırının durumu: açık tab'ın dosyası mı, klavye imleci üstünde mi, Files odakta mı.
#[derive(Clone, Copy)]
struct RowState {
    selected: bool,
    cursor: bool,
    focused: bool,
}

/// Ağaç satırı. İmleç satırı Files odaktayken vurgu renginde, değilken sönük çizilir (IntelliJ'deki gibi).
fn tree_row(ui: &mut Ui, p: &Palette, row: &Row, state: RowState) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), TREE_ROW), Sense::click());
    let rect = rect.shrink2(vec2(6.0, 0.0));
    let painter = ui.painter();
    let selected = state.selected || state.cursor;
    if state.cursor && state.focused {
        painter.rect_filled(rect, 5.0, color(p.accent).gamma_multiply(0.35));
    } else if state.cursor || state.selected {
        painter.rect_filled(rect, 5.0, color(p.bg_highlight));
    } else if resp.hovered() {
        painter.rect_filled(rect, 5.0, color(p.bg_highlight).gamma_multiply(0.6));
    }
    let y = rect.center().y;
    let mut x = rect.left() + 8.0 + row.depth as f32 * 14.0;
    if row.is_dir {
        let caret = if row.expanded { icon::CARET_DOWN } else { icon::CARET_RIGHT };
        painter.text(pos2(x, y), Align2::LEFT_CENTER, caret, ui_font(11.0), color(p.comment));
    }
    x += 16.0;
    let (glyph, tint) = match (row.is_dir, row.depth) {
        (true, 0) => (icon::FOLDER_OPEN, p.accent),
        (true, _) => (if row.expanded { icon::FOLDER_OPEN } else { icon::FOLDER }, p.icon),
        (false, _) => (file_icon(&row.name), p.fg_dim),
    };
    painter.text(pos2(x, y), Align2::LEFT_CENTER, glyph, ui_font(14.0), color(tint));
    let text_color = if row.depth == 0 || selected { color(p.fg) } else { color(p.fg_dim) };
    painter.text(pos2(x + 22.0, y), Align2::LEFT_CENTER, &row.name, ui_font(13.0), text_color);
    resp
}

/// Yeni boş dosya oluşturur; var olan yolu asla ezmez (#53).
pub fn create_new(path: &Path) -> std::io::Result<()> {
    std::fs::OpenOptions::new().write(true).create_new(true).open(path).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reveal_keeps_cursor_row_visible_with_margin() {
        let view = 10.0 * TREE_ROW;
        assert_eq!(reveal_offset(0.0, 5, view), 0.0, "visible row does not scroll");
        assert_eq!(reveal_offset(0.0, 8, view), 1.0 * TREE_ROW, "two rows stay below the cursor");
        assert_eq!(reveal_offset(0.0, 40, view), 33.0 * TREE_ROW);
        assert_eq!(reveal_offset(33.0 * TREE_ROW, 34, view), 32.0 * TREE_ROW, "two rows stay above");
        assert_eq!(reveal_offset(100.0, 0, view), 0.0);
    }

    #[test]
    fn files_rows_have_no_hidden_spacing_so_cursor_stays_visible() {
        let dir = std::env::temp_dir().join(format!("less-h-files-scroll-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..60 {
            std::fs::write(dir.join(format!("f{i:02}.txt")), "x").unwrap();
        }
        let mut f = Files::new(std::slice::from_ref(&dir));
        f.focus(None);
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(300.0, 400.0));
        let p = less_theme::TOKYONIGHT_NIGHT;
        let run = |f: &mut Files| {
            let input = egui::RawInput { screen_rect: Some(screen), ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| {
                f.show(ui, &p, true, None);
            });
            out.textures_delta.clear();
        };
        run(&mut f);
        for _ in 0..40 {
            f.key(&KeyEvent::plain(Key::Down), None);
            run(&mut f);
        }
        let view_h = 400.0 - 28.0;
        let top = 40.0 * TREE_ROW - f.scroll;
        assert!(top >= 0.0 && top + TREE_ROW <= view_h - TREE_ROW, "cursor row at {top}, view {view_h}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cmd_n_targets_selected_folder_and_never_overwrites_53() {
        let dir = std::env::temp_dir().join(format!("less-c2-new-file-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.txt"), "x").unwrap();
        let f = Files::new(std::slice::from_ref(&dir));
        assert_eq!(f.new_file_dir(), dir);
        let path = dir.join("new.md");
        create_new(&path).unwrap();
        assert!(create_new(&path).is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn enter_previews_then_second_enter_focuses_code_38_55() {
        let dir = std::env::temp_dir().join(format!("less-b0-files-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "x").unwrap();
        let mut f = Files::new(std::slice::from_ref(&dir));
        f.focus(None);
        let enter = KeyEvent::plain(Key::Enter);
        assert_eq!(f.key(&KeyEvent::plain(Key::Down), None), FilesOutcome::Handled);
        let file = dir.join("a.txt");
        assert_eq!(f.key(&enter, None), FilesOutcome::Preview(file.clone()));
        assert_eq!(f.key(&enter, Some(&file)), FilesOutcome::FocusCode);
        assert_eq!(f.key(&KeyEvent::char('x'), None), FilesOutcome::Unhandled);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
