//! Kalıcı sorgu konsolu (#47 #89): seçim/imleç bloğu, 300 ms otomatik kayıt ve egui metin görünümü.

use std::fs;
use std::io::Write;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui::{self, FontId, Id, TextEdit};
use less_db::DbType;
use less_theme::Palette;

use crate::widgets::color;

const AUTOSAVE_AFTER: Duration = Duration::from_millis(300);

/// Konsol dosya adında kullanılan güvenli ad.
pub fn slug(name: &str) -> String {
    let out: String =
        name.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') { c } else { '-' }).collect();
    let out = out.trim_matches('-');
    if out.is_empty() { "connection".into() } else { out.into() }
}

/// Bir konsolun metin ve kayıt durumu.
pub struct Console {
    pub path: PathBuf,
    pub text: String,
    pub dirty: bool,
    pub editing: bool,
    changed_at: Option<Instant>,
    cursor: usize,
    selection: Option<Range<usize>>,
    focus_requested: bool,
}

impl Console {
    /// Var olan dosyayı okur; yoksa bağlantıya uygun başlıkla başlatır ve hemen kalıcı dosyayı oluşturur.
    pub fn open(dir: &Path, name: &str, ty: DbType, modeless: bool) -> Result<Self, String> {
        fs::create_dir_all(dir).map_err(|e| format!("Could not create the consoles folder: {e}"))?;
        let path = dir.join(format!("{}.{}", slug(name), ty.console_ext()));
        let new_file = !path.exists();
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let comment = if ty.kind() == less_db::types::Kind::Mongo { "//" } else { "--" };
                format!("{comment} {name} console · Cmd+Enter runs the statement under the cursor or the selection\n\n")
            }
            Err(e) => return Err(format!("Could not read the query console: {e}")),
        };
        let mut console = Self {
            path,
            text,
            dirty: false,
            editing: modeless,
            changed_at: None,
            cursor: 0,
            selection: None,
            focus_requested: modeless,
        };
        if new_file {
            console.cursor = console.text.chars().count();
        }
        if new_file {
            console.save()?;
        }
        Ok(console)
    }

    /// Etkin seçim varsa onu, yoksa imlecin bulunduğu boş satırla ayrılmış bloğu döndürür.
    pub fn query_text(&self) -> Result<String, String> {
        if let Some(range) = &self.selection
            && range.start != range.end
        {
            return Ok(slice_chars(&self.text, range.clone()).trim().to_string());
        }
        let query = block_at(&self.text, self.cursor).trim().to_string();
        if query.is_empty() { Err("Put the cursor on a statement (or select it) to run it".into()) } else { Ok(query) }
    }

    pub fn set_cursor_char(&mut self, cursor: usize) {
        self.cursor = cursor.min(self.text.chars().count());
        self.selection = None;
    }

    pub fn cursor_char(&self) -> usize {
        self.cursor
    }

    #[cfg(test)]
    pub fn set_selection(&mut self, range: Range<usize>) {
        let len = self.text.chars().count();
        self.selection = Some(range.start.min(len)..range.end.min(len));
        self.cursor = range.end.min(len);
    }

    #[cfg(test)]
    pub fn replace_text(&mut self, text: String) {
        self.text = text;
        self.cursor = self.cursor.min(self.text.chars().count());
        self.selection = None;
        self.dirty = true;
        self.changed_at = Some(Instant::now());
    }

    pub fn edit(&mut self) {
        self.editing = true;
        self.focus_requested = true;
    }

    pub fn leave_insert(&mut self) -> Result<(), String> {
        self.editing = false;
        self.save()
    }

    /// Değişiklikten 300 ms sonra yazar; dönen hata kullanıcıya gösterilir.
    pub fn tick(&mut self) -> Option<String> {
        if self.dirty
            && self.changed_at.is_some_and(|at| at.elapsed() >= AUTOSAVE_AFTER)
            && let Err(e) = self.save()
        {
            return Some(e);
        }
        None
    }

    /// Konsolu atomik ve kullanıcıya özel izinlerle kaydeder.
    pub fn save(&mut self) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("Could not create the consoles folder: {e}"))?;
        }
        let tmp = self.path.with_extension("console.tmp");
        let write = || -> std::io::Result<()> {
            use std::os::unix::fs::OpenOptionsExt;
            let mut f = fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
            f.write_all(self.text.as_bytes())?;
            fs::rename(&tmp, &self.path)
        };
        write().map_err(|e| format!("Could not save the query console: {e}"))?;
        self.dirty = false;
        self.changed_at = None;
        Ok(())
    }

    /// Konsolu çizer; imleç/seçim bir sonraki Cmd+Enter için saklanır.
    pub fn show(&mut self, ui: &mut egui::Ui, p: &Palette, id: Id, vim_mode: bool) {
        let before = self.text.clone();
        let output = TextEdit::multiline(&mut self.text)
            .id(id)
            .font(FontId::monospace(13.0))
            .text_color(color(p.fg))
            .desired_width(ui.available_width())
            .desired_rows(20)
            .interactive(self.editing)
            .frame(egui::Frame::NONE)
            .show(ui);
        if let Some(range) = output.cursor_range {
            let sorted = range.as_sorted_char_range();
            self.selection = Some(sorted.start.into()..sorted.end.into());
            self.cursor = range.primary.index.into();
        }
        if self.focus_requested {
            output.response.request_focus();
            self.focus_requested = false;
        }
        if vim_mode && self.editing && output.response.lost_focus() {
            self.editing = false;
        }
        if !self.editing {
            output.response.surrender_focus();
        }
        if self.text != before {
            self.dirty = true;
            self.changed_at = Some(Instant::now());
        }
    }
}

/// Karakter indeksindeki boş satırlarla ayrılmış blok.
pub fn block_at(text: &str, cursor: usize) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    if lines.is_empty() {
        return String::new();
    }
    let cursor = cursor.min(text.chars().count());
    let mut line = 0;
    let mut seen = 0;
    for (i, value) in lines.iter().enumerate() {
        let end = seen + value.chars().count();
        if cursor <= end || i == lines.len() - 1 {
            line = i;
            break;
        }
        seen = end + 1;
    }
    if lines[line].trim().is_empty() {
        return String::new();
    }
    let mut first = line;
    while first > 0 && !lines[first - 1].trim().is_empty() {
        first -= 1;
    }
    let mut last = line;
    while last + 1 < lines.len() && !lines[last + 1].trim().is_empty() {
        last += 1;
    }
    lines[first..=last].join("\n")
}

fn slice_chars(text: &str, range: Range<usize>) -> String {
    text.chars().skip(range.start).take(range.end.saturating_sub(range.start)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_under_cursor_47() {
        let text = "-- head\nselect 1;\nselect 2;\n\nselect 3;";
        assert_eq!(block_at(text, 15), "-- head\nselect 1;\nselect 2;");
        assert_eq!(block_at(text, text.chars().count()), "select 3;");
        assert_eq!(block_at("select 1\n\nselect 2", 9), "");
    }

    #[test]
    fn selection_wins_over_block_47() {
        let dir = std::env::temp_dir().join(format!("less-b4-console-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let mut c = Console::open(&dir, "local", DbType::Sqlite, true).unwrap();
        c.text = "select 1;\nselect 2;".into();
        c.set_selection(10..19);
        assert_eq!(c.query_text().unwrap(), "select 2;");
        c.set_cursor_char(1);
        assert_eq!(c.query_text().unwrap(), "select 1;\nselect 2;");
        let _ = fs::remove_dir_all(&dir);
    }
}
