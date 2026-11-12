//! `:Theme` ve `:Font` seçicileri (#36 #71). Seçim modeli UI'sız test edilir; oklar ve fare canlı önizleme üretir,
//! Enter seçimi kalıcı yapar, Esc açılıştaki değeri döndürür.

use eframe::egui::{self, Key, vec2};
use less_theme::Palette;

use crate::fonts::{self, icon};
use crate::widgets::{button, color, ui_font};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Theme,
    Font,
}

#[derive(Clone, Debug)]
struct Entry {
    id: String,
    label: String,
    available: bool,
    install: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Preview(String),
    Commit(String),
    Cancel(String),
}

pub struct Selector {
    kind: Kind,
    entries: Vec<Entry>,
    original: String,
    selected: usize,
}

impl Selector {
    pub fn theme(current: &str) -> Self {
        let entries = less_theme::ALL
            .iter()
            .map(|p| Entry { id: p.id.to_owned(), label: p.name.to_owned(), available: true, install: None })
            .collect();
        Self::new(Kind::Theme, entries, current)
    }

    pub fn font(current: &str) -> Self {
        let entries = less_theme::fonts::FONTS
            .iter()
            .map(|f| Entry {
                id: f.family.to_owned(),
                label: f.family.to_owned(),
                available: fonts::installed(f.family),
                install: f.brew_cask.map(|c| format!("brew install --cask {c}")),
            })
            .collect();
        Self::new(Kind::Font, entries, current)
    }

    fn new(kind: Kind, entries: Vec<Entry>, current: &str) -> Self {
        let selected = entries.iter().position(|e| e.id == current).unwrap_or(0);
        Self { kind, entries, original: current.to_owned(), selected }
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    #[cfg(test)]
    pub fn current(&self) -> &str {
        &self.entries[self.selected].id
    }

    /// Sonraki kullanılabilir girdiye gider; yeni kimliği canlı önizleme için döndürür.
    pub fn move_by(&mut self, step: isize) -> Option<Command> {
        if self.entries.is_empty() {
            return None;
        }
        let mut index = self.selected;
        for _ in 0..self.entries.len() {
            index = (index as isize + step).rem_euclid(self.entries.len() as isize) as usize;
            if self.entries[index].available {
                return if index != self.selected { self.select(index) } else { None };
            }
        }
        None
    }

    pub fn select(&mut self, index: usize) -> Option<Command> {
        let entry = self.entries.get(index)?;
        if !entry.available {
            return None;
        }
        self.selected = index;
        Some(Command::Preview(entry.id.clone()))
    }

    pub fn commit(&self) -> Option<Command> {
        self.entries.get(self.selected).filter(|e| e.available).map(|e| Command::Commit(e.id.clone()))
    }

    pub fn cancel(&self) -> Command {
        Command::Cancel(self.original.clone())
    }

    /// Klavye oklarla canlı önizler; Enter kalıcı seçim, Esc geri almadır.
    pub fn handle_input(&mut self, ctx: &egui::Context) -> Vec<Command> {
        let (up, down, enter, esc) = ctx.input(|i| {
            (
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::Enter),
                i.key_pressed(Key::Escape),
            )
        });
        let mut out = Vec::new();
        if up {
            out.extend(self.move_by(-1));
        }
        if down {
            out.extend(self.move_by(1));
        }
        if enter {
            out.extend(self.commit());
        }
        if esc {
            out.push(self.cancel());
        }
        out
    }

    pub fn show(&mut self, ctx: &egui::Context, p: &Palette) -> Vec<Command> {
        let mut commands = Vec::new();
        let entries = self.entries.clone();
        let title = match self.kind {
            Kind::Theme => "Theme",
            Kind::Font => "Font",
        };
        egui::Window::new(title)
            .id(egui::Id::unique("appearance-selector"))
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .fixed_size(vec2(560.0, 500.0))
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new("Preview as you move. Enter applies, Esc restores.").color(color(p.comment)),
                );
                ui.add_space(10.0);
                egui::ScrollArea::vertical().id_salt("appearance-list").auto_shrink([false, false]).show(ui, |ui| {
                    for (index, entry) in entries.iter().enumerate() {
                        let selected = index == self.selected;
                        let label = if entry.available {
                            entry.label.clone()
                        } else {
                            format!("{}  (not installed)", entry.label)
                        };
                        let response = ui.add_enabled(
                            entry.available,
                            egui::Button::selectable(selected, egui::RichText::new(label).font(ui_font(14.0))),
                        );
                        if response.clicked() {
                            commands.extend(self.select(index));
                        }
                        if response.double_clicked() {
                            commands.extend(self.commit());
                        }
                        if let Some(install) = &entry.install
                            && !entry.available
                        {
                            response.on_hover_text(format!("Not installed. {install}"));
                        }
                    }
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if button(ui, p, Some(icon::CHECK), "Apply", true, self.commit().is_some()).clicked() {
                        commands.extend(self.commit());
                    }
                    if button(ui, p, None, "Cancel", false, true).clicked() {
                        commands.push(self.cancel());
                    }
                });
                if self.kind == Kind::Font && !self.entries[self.selected].available {
                    ui.colored_label(color(p.comment), "Choose an installed font to enable Apply.");
                }
            });
        commands
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selector() -> Selector {
        Selector::new(
            Kind::Font,
            vec![
                Entry { id: "a".into(), label: "A".into(), available: true, install: None },
                Entry { id: "b".into(), label: "B".into(), available: false, install: None },
                Entry { id: "c".into(), label: "C".into(), available: true, install: None },
            ],
            "a",
        )
    }

    #[test]
    fn theme_and_font_preview_enter_and_esc_36_71() {
        let mut s = selector();
        assert_eq!(s.move_by(1), Some(Command::Preview("c".into())), "unavailable font is skipped");
        assert_eq!(s.current(), "c");
        assert_eq!(s.commit(), Some(Command::Commit("c".into())));
        assert_eq!(s.cancel(), Command::Cancel("a".into()));
        assert_eq!(s.move_by(-1), Some(Command::Preview("a".into())));
        assert_eq!(s.select(1), None, "unavailable font cannot be previewed");
    }

    #[test]
    fn selector_renders_and_escape_restores_headless_71() {
        let mut selector = Selector::theme(less_theme::TOKYONIGHT_NIGHT.id);
        let ctx = egui::Context::default();
        let down = egui::Event::Key {
            key: Key::ArrowDown,
            physical_key: Some(Key::ArrowDown),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let mut output = ctx.run_ui(egui::RawInput { events: vec![down], ..Default::default() }, |ui| {
            assert_eq!(selector.handle_input(ui.ctx()), [Command::Preview(less_theme::INTELLIJ_DARK.id.into())]);
            selector.show(ui.ctx(), &less_theme::TOKYONIGHT_NIGHT);
        });
        output.textures_delta.clear();
        let esc = egui::Event::Key {
            key: Key::Escape,
            physical_key: Some(Key::Escape),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let mut output = ctx.run_ui(egui::RawInput { events: vec![esc], ..Default::default() }, |ui| {
            assert_eq!(selector.handle_input(ui.ctx()), [Command::Cancel(less_theme::TOKYONIGHT_NIGHT.id.into())]);
        });
        output.textures_delta.clear();
    }

    #[test]
    fn current_theme_is_selected_initially_71() {
        let s = Selector::theme(less_theme::DARCULA.id);
        assert_eq!(s.current(), less_theme::DARCULA.id);
        assert_eq!(s.cancel(), Command::Cancel(less_theme::DARCULA.id.into()));
    }
}
