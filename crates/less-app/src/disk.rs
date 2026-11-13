//! Açık dosya diskte dışarıdan değişince sorulan "diskten yükle / bellektekiyle devam" penceresi.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use eframe::egui::{self, Id, RichText};
use less_input::{Key, KeyEvent};
use less_theme::Palette;

use crate::widgets::{color, ui_font};

/// Dosyanın diskteki damgası (değiştirilme zamanı, boyut); okunamazsa `None`.
pub type Stamp = Option<(SystemTime, u64)>;

pub fn stamp(path: &Path) -> Stamp {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// Kullanıcının seçimi.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Reload,
    Keep,
}

/// Dışarıdan değişen dosyalar sırayla sorulur; varsayılan seçim diskten yüklemek.
#[derive(Default)]
pub struct Prompt {
    pub paths: Vec<PathBuf>,
    keep: bool,
}

impl Prompt {
    pub fn is_open(&self) -> bool {
        !self.paths.is_empty()
    }

    pub fn push(&mut self, path: PathBuf) {
        if !self.paths.contains(&path) {
            self.paths.push(path);
        }
    }

    /// Enter seçimi onaylar, Esc bellektekini korur, ok/Tab/h/l seçimi değiştirir.
    pub fn key(&mut self, k: &KeyEvent) -> Option<(PathBuf, Choice)> {
        match k.key {
            Key::Enter | Key::Char(' ') => self.decide(if self.keep { Choice::Keep } else { Choice::Reload }),
            Key::Esc => self.decide(Choice::Keep),
            Key::Left | Key::Right | Key::Up | Key::Down | Key::Tab | Key::Char('h' | 'l' | 'j' | 'k') => {
                self.keep = !self.keep;
                None
            }
            Key::Char('r') => self.decide(Choice::Reload),
            _ => None,
        }
    }

    fn decide(&mut self, choice: Choice) -> Option<(PathBuf, Choice)> {
        self.keep = false;
        (!self.paths.is_empty()).then(|| (self.paths.remove(0), choice))
    }

    /// İlk dosya için pencereyi çizer; tıklanan seçimi döner.
    pub fn show(&mut self, ctx: &egui::Context, p: &Palette, name: &str, dirty: bool) -> Option<(PathBuf, Choice)> {
        let mut clicked = None;
        let frame = egui::Frame::NONE
            .fill(color(p.surface))
            .stroke(egui::Stroke::new(1.0, color(p.extra.warn)))
            .corner_radius(10.0)
            .inner_margin(18.0);
        egui::Modal::new(Id::unique("disk-changed")).frame(frame).show(ctx, |ui| {
            ui.set_width(440.0);
            ui.label(RichText::new("File changed on disk").color(color(p.extra.warn)).font(ui_font(14.5)).strong());
            ui.add_space(6.0);
            ui.label(RichText::new(name).color(color(p.fg)).font(egui::FontId::monospace(13.0)));
            let note = if dirty {
                "This tab has unsaved changes. Reloading discards them."
            } else {
                "Another program updated this file."
            };
            ui.label(RichText::new(note).color(color(p.fg_dim)).font(ui_font(12.5)));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                for (choice, label) in [(Choice::Reload, "Reload from disk"), (Choice::Keep, "Keep my version")] {
                    let selected = (choice == Choice::Keep) == self.keep;
                    let text =
                        RichText::new(label).font(ui_font(13.0)).color(color(if selected { p.bg } else { p.fg }));
                    let button = egui::Button::new(text).fill(color(if selected { p.accent } else { p.bg_highlight }));
                    if ui.add(button).clicked() {
                        clicked = Some(choice);
                    }
                }
            });
            if self.paths.len() > 1 {
                ui.add_space(6.0);
                let more = format!("{} more changed file(s) after this one", self.paths.len() - 1);
                ui.label(RichText::new(more).color(color(p.comment)).font(ui_font(12.0)));
            }
            ui.add_space(6.0);
            ui.label(
                RichText::new("Enter: confirm · Tab: switch · Esc: keep my version")
                    .color(color(p.comment))
                    .font(ui_font(12.0)),
            );
        });
        clicked.and_then(|c| self.decide(c))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_reloads_by_default_esc_keeps_and_tab_switches() {
        let mut prompt = Prompt::default();
        prompt.push("/a".into());
        prompt.push("/b".into());
        prompt.push("/a".into());
        assert_eq!(prompt.paths.len(), 2);
        assert_eq!(prompt.key(&KeyEvent::plain(Key::Enter)), Some(("/a".into(), Choice::Reload)));
        assert_eq!(prompt.key(&KeyEvent::plain(Key::Tab)), None);
        assert_eq!(prompt.key(&KeyEvent::plain(Key::Enter)), Some(("/b".into(), Choice::Keep)));
        assert!(!prompt.is_open());
        prompt.push("/c".into());
        assert_eq!(prompt.key(&KeyEvent::plain(Key::Enter)), Some(("/c".into(), Choice::Reload)), "choice resets");
        prompt.push("/d".into());
        assert_eq!(prompt.key(&KeyEvent::plain(Key::Esc)), Some(("/d".into(), Choice::Keep)));
    }
}
