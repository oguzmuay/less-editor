//! Git panelinin onay ve eylem menüleri. Klavye kararları egui'den bağımsızdır.

use eframe::egui::{self, Align2, Context, Id, RichText, Sense, vec2};
use less_input::{Key, KeyEvent};
use less_theme::Palette;

use super::Action;
use crate::widgets::{self, color, ui_font};

#[derive(Clone, Debug)]
pub enum Prompt {
    /// Yıkıcı işlerde `confirmed = false`, yani ilk seçenek Cancel'dır.
    Confirm {
        title: String,
        body: String,
        action: Action,
        confirmed: bool,
    },
    Menu {
        title: String,
        entries: Vec<MenuEntry>,
        selected: usize,
    },
}

#[derive(Clone, Debug)]
pub struct MenuEntry {
    pub label: String,
    pub action: Action,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Keep,
    Close,
    Run(Action),
}

impl Prompt {
    pub fn confirm(title: impl Into<String>, body: impl Into<String>, action: Action) -> Self {
        Self::Confirm { title: title.into(), body: body.into(), action, confirmed: false }
    }

    pub fn menu(title: impl Into<String>, entries: Vec<MenuEntry>) -> Self {
        Self::Menu { title: title.into(), entries, selected: 0 }
    }

    pub fn key(&mut self, key: &KeyEvent) -> Outcome {
        let plain = !key.mods.ctrl && !key.mods.alt && !key.mods.cmd;
        if key.key == Key::Esc {
            return Outcome::Close;
        }
        match self {
            Self::Confirm { action, confirmed, .. } => match key.key {
                Key::Char('y' | 'Y') if plain => Outcome::Run(action.clone()),
                Key::Char('n' | 'N') if plain => Outcome::Close,
                Key::Left | Key::Right | Key::Up | Key::Down | Key::Tab => {
                    *confirmed = !*confirmed;
                    Outcome::Keep
                }
                Key::Enter if *confirmed => Outcome::Run(action.clone()),
                Key::Enter => Outcome::Close,
                _ => Outcome::Keep,
            },
            Self::Menu { entries, selected, .. } => match key.key {
                Key::Up => {
                    *selected = selected.saturating_sub(1);
                    Outcome::Keep
                }
                Key::Down => {
                    *selected = (*selected + 1).min(entries.len().saturating_sub(1));
                    Outcome::Keep
                }
                Key::Enter => entries.get(*selected).map(|e| Outcome::Run(e.action.clone())).unwrap_or(Outcome::Close),
                Key::Char('q') if plain => Outcome::Close,
                _ => Outcome::Keep,
            },
        }
    }

    pub fn show(&mut self, ctx: &Context, palette: &Palette) -> Outcome {
        let mut out = Outcome::Keep;
        let response = egui::Modal::new(Id::unique("git-popup")).frame(frame(palette)).show(ctx, |ui| {
            ui.set_width(440.0);
            match self {
                Self::Confirm { title, body, action, confirmed } => {
                    title_ui(ui, palette, title);
                    ui.label(RichText::new(body.as_str()).color(color(palette.fg)).font(ui_font(13.5)));
                    ui.add_space(14.0);
                    let action = action.clone();
                    ui.horizontal(|ui| {
                        if widgets::button(ui, palette, None, "Cancel", !*confirmed, true).clicked() {
                            out = Outcome::Close;
                        }
                        if widgets::button(ui, palette, None, "Confirm", *confirmed, true).clicked() {
                            out = Outcome::Run(action.clone());
                        }
                    });
                }
                Self::Menu { title, entries, selected } => {
                    title_ui(ui, palette, title);
                    for (i, entry) in entries.iter().enumerate() {
                        let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
                        if i == *selected || response.hovered() {
                            ui.painter().rect_filled(rect, 5.0, color(palette.bg_highlight));
                        }
                        ui.painter().text(
                            rect.left_center() + vec2(10.0, 0.0),
                            Align2::LEFT_CENTER,
                            &entry.label,
                            ui_font(13.0),
                            color(palette.fg),
                        );
                        if response.clicked() {
                            out = Outcome::Run(entry.action.clone());
                        }
                    }
                }
            }
        });
        if response.backdrop_response.clicked() && out == Outcome::Keep {
            return Outcome::Close;
        }
        out
    }
}

fn frame(palette: &Palette) -> egui::Frame {
    egui::Frame::NONE
        .fill(color(palette.surface))
        .stroke(egui::Stroke::new(1.0, color(palette.border)))
        .corner_radius(10.0)
        .inner_margin(18.0)
}

fn title_ui(ui: &mut egui::Ui, palette: &Palette, text: &str) {
    ui.label(RichText::new(text).color(color(palette.fg)).font(ui_font(14.5)).strong());
    ui.add_space(8.0);
}
