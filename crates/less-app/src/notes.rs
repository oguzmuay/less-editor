//! Ortada yüzen, tab'lı Markdown notları (#98). Dosya işlemleri `less-workspace::notes` deposundadır; bu modül
//! düzenleme, 300 ms otomatik kayıt, sekme / adlandırma / silme ve proje başına son not durumunu ekler.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use eframe::egui::{self, FontId, Key, vec2};
use less_theme::Palette;
use less_workspace::notes::{self, LastNotes, Note};
use less_workspace::paths;

use crate::fonts::icon;
use crate::widgets::{button, color, text_input, ui_font};

const AUTOSAVE: Duration = Duration::from_millis(300);

#[derive(Clone, Debug)]
struct NoteTab {
    note: Note,
    text: String,
    dirty: bool,
    edited_at: Option<Instant>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NameKind {
    New,
    Rename,
}

#[derive(Clone, Debug)]
struct NamePrompt {
    kind: NameKind,
    value: String,
    focus: bool,
    error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Toggle,
    New,
    Rename,
    Delete,
}

#[derive(Default)]
pub struct NotesWindow {
    open: bool,
    project: Option<PathBuf>,
    tabs: Vec<NoteTab>,
    active: usize,
    last: LastNotes,
    name_prompt: Option<NamePrompt>,
    confirm_delete: bool,
    messages: Vec<(String, bool)>,
}

impl NotesWindow {
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Cmd+N: açıksa kaydedip kapatır; kapalıysa projenin son notunu (yoksa "Notes") açar.
    pub fn toggle(&mut self, project: &Path) {
        if self.open {
            self.close();
        } else {
            self.open(project);
        }
    }

    pub fn command(&mut self, command: Command, project: &Path) {
        if command == Command::Toggle {
            self.toggle(project);
            return;
        }
        if !self.open {
            self.open(project);
        }
        match command {
            Command::Toggle => {}
            Command::New => self.new_prompt(),
            Command::Rename => self.rename_prompt(),
            Command::Delete => self.ask_delete(),
        }
    }

    pub fn open(&mut self, project: &Path) {
        self.save_all();
        self.project = Some(project.to_path_buf());
        match notes::list(project) {
            Ok(items) => {
                let selected = self.last.current(project).map(|n| n.path);
                if items.is_empty() {
                    match notes::create(project, Some("Notes")) {
                        Ok(note) => self.load(project, Some(note.path)),
                        Err(e) => self.report("Could not create the notes folder", e),
                    }
                } else {
                    self.load(project, selected);
                }
            }
            Err(e) => self.report("Could not open notes", e),
        }
    }

    fn load(&mut self, project: &Path, selected: Option<PathBuf>) {
        match notes::list(project) {
            Ok(items) => {
                self.tabs = items
                    .into_iter()
                    .map(|note| {
                        let text = std::fs::read_to_string(&note.path).unwrap_or_default();
                        NoteTab { note, text, dirty: false, edited_at: None }
                    })
                    .collect();
                self.active = selected.and_then(|path| self.tabs.iter().position(|t| t.note.path == path)).unwrap_or(0);
                self.open = !self.tabs.is_empty();
                if let Some(tab) = self.tabs.get(self.active) {
                    self.last.set(project, &tab.note.path);
                }
            }
            Err(e) => self.report("Could not load notes", e),
        }
    }

    pub fn close(&mut self) {
        self.save_all();
        self.open = false;
        self.name_prompt = None;
        self.confirm_delete = false;
    }

    pub fn new_prompt(&mut self) {
        if !self.open {
            return;
        }
        let name = format!("Note {}", self.tabs.len() + 1);
        self.name_prompt = Some(NamePrompt { kind: NameKind::New, value: name, focus: true, error: None });
    }

    pub fn rename_prompt(&mut self) {
        let Some(tab) = self.tabs.get(self.active) else { return };
        self.name_prompt =
            Some(NamePrompt { kind: NameKind::Rename, value: tab.note.name.clone(), focus: true, error: None });
    }

    pub fn ask_delete(&mut self) {
        if self.open && !self.tabs.is_empty() {
            self.confirm_delete = true;
        }
    }

    pub fn select(&mut self, index: usize) {
        if index >= self.tabs.len() || index == self.active {
            return;
        }
        self.save_current();
        self.active = index;
        self.remember_active();
    }

    pub fn cycle(&mut self, step: isize) {
        if self.tabs.is_empty() {
            return;
        }
        let next = (self.active as isize + step).rem_euclid(self.tabs.len() as isize) as usize;
        self.select(next);
    }

    fn remember_active(&mut self) {
        if let (Some(project), Some(tab)) = (&self.project, self.tabs.get(self.active)) {
            self.last.set(project, &tab.note.path);
        }
    }

    /// Yazma değişikliğini işaretler; kayıt `tick` içinde 300 ms sonra yapılır.
    #[cfg(test)]
    fn edited(&mut self, now: Instant) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.dirty = true;
            tab.edited_at = Some(now);
        }
    }

    pub fn tick(&mut self, now: Instant) {
        if !self.open {
            return;
        }
        let due = self.tabs.iter().any(|tab| tab.dirty && tab.edited_at.is_some_and(|at| now >= at + AUTOSAVE));
        if due {
            self.save_all();
        }
    }

    pub fn save_current(&mut self) {
        if let Some(tab) = self.tabs.get_mut(self.active)
            && tab.dirty
        {
            match paths::write_atomic(&tab.note.path, tab.text.as_bytes()) {
                Ok(()) => {
                    tab.dirty = false;
                    tab.edited_at = None;
                }
                Err(e) => self.report("Could not save note", e),
            }
        }
    }

    pub fn save_all(&mut self) {
        let active = self.active;
        for index in 0..self.tabs.len() {
            self.active = index;
            self.save_current();
        }
        self.active = active.min(self.tabs.len().saturating_sub(1));
    }

    fn apply_name_prompt(&mut self) {
        let Some(mut prompt) = self.name_prompt.take() else { return };
        let Some(project) = self.project.clone() else { return };
        let result = match prompt.kind {
            NameKind::New => notes::create(&project, Some(&prompt.value)),
            NameKind::Rename => match self.tabs.get(self.active).map(|tab| tab.note.clone()) {
                Some(note) if notes::safe_name(&prompt.value) == note.name => return,
                Some(note) => {
                    self.save_current();
                    notes::rename(&note, &prompt.value)
                }
                None => return,
            },
        };
        match result {
            Ok(note) => {
                self.last.set(&project, &note.path);
                self.load(&project, Some(note.path));
            }
            Err(e) => {
                prompt.error = Some(e.to_string());
                self.name_prompt = Some(prompt);
            }
        }
    }

    fn delete_current(&mut self) {
        self.confirm_delete = false;
        let Some(project) = self.project.clone() else { return };
        let Some(tab) = self.tabs.get(self.active).cloned() else { return };
        match notes::delete(&tab.note) {
            Ok(()) => {
                let selected = self
                    .tabs
                    .get(self.active + 1)
                    .or_else(|| self.active.checked_sub(1).and_then(|index| self.tabs.get(index)))
                    .map(|t| t.note.path.clone());
                self.load(&project, selected);
                if self.tabs.is_empty() {
                    self.open = false;
                }
            }
            Err(e) => self.report("Could not delete note", e),
        }
    }

    /// Not penceresi açıkken uygulama kısayollarını işler; host bu tuşları kod editörüne göndermez.
    pub fn handle_input(&mut self, ctx: &egui::Context, now: Instant) {
        if !self.open {
            return;
        }
        if ctx.input(|i| i.viewport().focused == Some(false)) {
            self.save_all();
        }
        let (cmd, shift, alt, n, t, w, s, y, enter, esc, left, right) = ctx.input(|i| {
            (
                i.modifiers.command,
                i.modifiers.shift,
                i.modifiers.alt,
                i.key_pressed(Key::N),
                i.key_pressed(Key::T),
                i.key_pressed(Key::W),
                i.key_pressed(Key::S),
                i.key_pressed(Key::Y),
                i.key_pressed(Key::Enter),
                i.key_pressed(Key::Escape),
                i.key_pressed(Key::ArrowLeft),
                i.key_pressed(Key::ArrowRight),
            )
        });
        if self.name_prompt.is_some() {
            if enter {
                self.apply_name_prompt();
            } else if esc {
                self.name_prompt = None;
            }
            return;
        }
        if self.confirm_delete {
            if enter || esc || n {
                self.confirm_delete = false;
            } else if y {
                self.delete_current();
            }
            return;
        }
        if (cmd && n) || (cmd && w) || esc {
            self.close();
        } else if cmd && t {
            self.new_prompt();
        } else if cmd && s {
            self.save_current();
        } else if cmd && (shift || alt) && left {
            self.cycle(-1);
        } else if cmd && (shift || alt) && right {
            self.cycle(1);
        }
        if self.tabs.get(self.active).is_some_and(|tab| tab.dirty) {
            ctx.request_repaint_after(AUTOSAVE);
        }
        self.tick(now);
    }

    pub fn show(&mut self, ctx: &egui::Context, p: &Palette) {
        if !self.open {
            return;
        }
        let available = ctx.content_rect().size();
        let size = vec2((available.x * 0.6).clamp(560.0, 960.0), (available.y * 0.6).clamp(320.0, 640.0));
        let mut select = None;
        let mut rename = false;
        let mut delete = false;
        let mut new = false;
        let window = egui::Window::new("Notes")
            .id(egui::Id::unique("notes-window"))
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .fixed_size(size)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (index, tab) in self.tabs.iter().enumerate() {
                        let response = ui.selectable_label(index == self.active, &tab.note.name);
                        if response.clicked() {
                            select = Some(index);
                        }
                        if response.double_clicked() || response.secondary_clicked() {
                            select = Some(index);
                            rename = true;
                        }
                        if ui.small_button(icon::X).on_hover_text("Delete note").clicked() {
                            select = Some(index);
                            delete = true;
                        }
                    }
                    if ui.button(icon::PLUS).on_hover_text("New note (Cmd+T)").clicked() {
                        new = true;
                    }
                });
                ui.separator();
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    let response = egui::TextEdit::multiline(&mut tab.text)
                        .id(egui::Id::unique("note-editor"))
                        .font(FontId::monospace(14.0))
                        .desired_width(ui.available_width())
                        .desired_rows((ui.available_height() / 22.0).max(8.0) as usize)
                        .show(ui);
                    if response.response.changed() {
                        tab.dirty = true;
                        tab.edited_at = Some(Instant::now());
                    }
                    if response.response.lost_focus() {
                        self.save_current();
                    }
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(".less/notes").font(ui_font(12.0)).color(color(p.comment)));
                    ui.label(egui::RichText::new("Saved automatically").font(ui_font(12.0)).color(color(p.comment)));
                    if button(ui, p, None, "Rename", false, true).clicked() {
                        rename = true;
                    }
                    if button(ui, p, Some(icon::TRASH), "Delete", false, true).clicked() {
                        delete = true;
                    }
                });
            });
        let outside = window.as_ref().is_some_and(|window| {
            ctx.input(|i| {
                i.pointer.primary_pressed()
                    && i.pointer.interact_pos().is_some_and(|pos| !window.response.rect.contains(pos))
            })
        });
        if outside && self.name_prompt.is_none() && !self.confirm_delete {
            self.close();
            return;
        }
        if let Some(index) = select {
            self.select(index);
        }
        if new {
            self.new_prompt();
        }
        if rename {
            self.rename_prompt();
        }
        if delete {
            self.ask_delete();
        }
        self.name_window(ctx, p);
        self.delete_window(ctx, p);
    }

    fn name_window(&mut self, ctx: &egui::Context, p: &Palette) {
        let Some(prompt) = &mut self.name_prompt else { return };
        let title = if prompt.kind == NameKind::New { "New note" } else { "Rename note" };
        let mut submit = false;
        let mut cancel = false;
        egui::Window::new(title)
            .id(egui::Id::unique("note-name"))
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .fixed_size(vec2(420.0, 120.0))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                let response = text_input(
                    ui,
                    p,
                    egui::Id::unique("note-name-input"),
                    &mut prompt.value,
                    "Note name",
                    None,
                    380.0,
                    prompt.focus,
                );
                prompt.focus = false;
                submit = response.lost_focus() && ctx.input(|i| i.key_pressed(Key::Enter));
                if let Some(error) = &prompt.error {
                    ui.colored_label(color(p.error), error);
                }
                ui.horizontal(|ui| {
                    if button(ui, p, Some(icon::CHECK), "Save", true, true).clicked() {
                        submit = true;
                    }
                    if button(ui, p, None, "Cancel", false, true).clicked() {
                        cancel = true;
                    }
                });
            });
        if submit {
            self.apply_name_prompt();
        } else if cancel {
            self.name_prompt = None;
        }
    }

    fn delete_window(&mut self, ctx: &egui::Context, p: &Palette) {
        if !self.confirm_delete {
            return;
        }
        let mut cancel = false;
        let mut delete = false;
        let name = self.tabs.get(self.active).map(|t| t.note.name.clone()).unwrap_or_default();
        egui::Window::new("Delete note")
            .id(egui::Id::unique("note-delete"))
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .fixed_size(vec2(420.0, 120.0))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(format!("Delete note '{name}'? This removes the Markdown file."));
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if button(ui, p, None, "Cancel", true, true).clicked() {
                        cancel = true;
                    }
                    if button(ui, p, Some(icon::TRASH), "Delete", false, true).clicked() {
                        delete = true;
                    }
                });
            });
        if cancel {
            self.confirm_delete = false;
        } else if delete {
            self.delete_current();
        }
    }

    fn report(&mut self, context: &str, error: std::io::Error) {
        self.messages.push((format!("{context}: {error}"), true));
    }

    pub fn drain_messages(&mut self) -> Vec<(String, bool)> {
        std::mem::take(&mut self.messages)
    }
}

impl Drop for NotesWindow {
    fn drop(&mut self) {
        self.save_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("less-c2-notes-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn notes_autosave_tabs_rename_delete_and_remember_last_98() {
        let project = project("flow");
        let mut window = NotesWindow::default();
        window.toggle(&project);
        assert!(window.open);
        assert_eq!(window.tabs[0].note.name, "Notes");
        assert_eq!(std::fs::read_to_string(project.join(".less/notes/.gitignore")).unwrap(), "*\n");

        window.tabs[0].text = "# first".into();
        window.edited(Instant::now() - AUTOSAVE);
        window.tick(Instant::now());
        assert!(!window.tabs[0].dirty);
        assert_eq!(std::fs::read_to_string(&window.tabs[0].note.path).unwrap(), "# first");

        notes::create(&project, Some("Second")).unwrap();
        window.load(&project, Some(project.join(".less/notes/Second.md")));
        assert_eq!(window.tabs[window.active].note.name, "Second");
        window.cycle(1);
        assert_eq!(window.tabs[window.active].note.name, "Notes");
        window.close();
        window.open(&project);
        assert_eq!(window.tabs[window.active].note.name, "Notes", "last note survives closing the window");

        window.rename_prompt();
        window.name_prompt.as_mut().unwrap().value = "Renamed".into();
        window.apply_name_prompt();
        assert_eq!(window.tabs[window.active].note.name, "Renamed");
        assert!(project.join(".less/notes/Renamed.md").is_file());
        window.ask_delete();
        assert!(window.confirm_delete);
        window.delete_current();
        assert!(!project.join(".less/notes/Renamed.md").exists());
        assert_eq!(window.tabs.len(), 1);
        window.close();
        let _ = std::fs::remove_dir_all(project);
    }

    #[test]
    fn closing_notes_saves_without_deleting_the_file_98() {
        let project = project("close");
        let mut window = NotesWindow::default();
        window.open(&project);
        let path = window.tabs[0].note.path.clone();
        window.tabs[0].text = "draft".into();
        window.tabs[0].dirty = true;
        window.close();
        assert!(path.is_file());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "draft");
        assert!(!window.open);
        let _ = std::fs::remove_dir_all(project);
    }

    #[test]
    fn notes_window_and_name_prompt_render_headless_98() {
        let project = project("draw");
        let mut window = NotesWindow::default();
        window.open(&project);
        window.new_prompt();
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            window.handle_input(ui.ctx(), Instant::now());
            window.show(ui.ctx(), &less_theme::TOKYONIGHT_NIGHT);
        });
        output.textures_delta.clear();
        window.close();
        let _ = std::fs::remove_dir_all(project);
    }
}
