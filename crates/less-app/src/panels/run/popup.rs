//! Run panelinin pencereleri: `(Y/n)` sorusu (#77), eylem menüsü / `Space j` seçicisi (#34), tek satır giriş
//! (profil, Variables değeri #91), çalıştırma ayarları formu (#69) ve çıkıştaki süreç listesi (#80).
//!
//! Tuş mantığı (`key`) egui'den bağımsızdır; çizim `show` ile `egui::Modal` içinde.

use eframe::egui::{self, Align2, Context, Id, RichText, Sense, Ui, vec2};
use less_input::{Key, KeyEvent};
use less_run::config::{format_env, parse_env};
use less_run::shutdown::{Shutdown, ShutdownState};
use less_run::{ItemState, RunConfig, Runner};
use less_theme::Palette;

use super::tree::Action;
use crate::widgets::{self, color, ui_font};

/// Menü satırının sonucu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Then {
    /// Öğeye eylem.
    Do(String, Action),
    /// `Space j`: öğe seçildi, şimdi eylem menüsü.
    Pick(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub icon: &'static str,
    pub label: String,
    pub then: Then,
}

/// Giriş penceresinin amacı.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputFor {
    /// Öğe kimliği.
    Profile(String),
    /// Variables değeri (yol).
    VarSet(String),
    /// Listeye eleman (yol).
    VarAppend(String),
}

/// Çalıştırma ayarları formu (#69); alan sırası: profil (yalnız Spring), ortam, VM / Node seçenekleri, argümanlar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigForm {
    pub id: String,
    pub title: String,
    pub profiles: bool,
    pub node: bool,
    pub profile: String,
    pub env: String,
    pub vm: String,
    pub args: String,
    pub error: Option<String>,
}

impl ConfigForm {
    pub fn new(s: &ItemState) -> Self {
        let c = &s.config;
        Self {
            id: s.spec.id.clone(),
            title: format!("{} · Run configuration", s.spec.label),
            profiles: s.spec.supports_profiles(),
            node: s.spec.vm_env().is_some(),
            profile: c.profile.clone().unwrap_or_default(),
            env: format_env(c.env.as_ref()),
            vm: c.vm_options.clone().unwrap_or_default(),
            args: c.args.clone().unwrap_or_default(),
            error: None,
        }
    }

    /// Alanlardan ayar; ortam hatalıysa İngilizce hata metni.
    pub fn submit(&self) -> Result<RunConfig, String> {
        let env = parse_env(&self.env)?;
        let opt = |s: &str| Some(s.to_string());
        Ok(RunConfig {
            profile: if self.profiles { opt(&self.profile) } else { None },
            env: Some(env),
            vm_options: opt(&self.vm),
            args: opt(&self.args),
        })
    }
}

/// Açık pencere.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Popup {
    /// `yes` = seçili düğme.
    Prompt {
        id: String,
        question: String,
        yes: bool,
    },
    Menu {
        title: String,
        entries: Vec<Entry>,
        sel: usize,
    },
    Input {
        title: String,
        value: String,
        purpose: InputFor,
    },
    Form(Box<ConfigForm>),
}

/// Tuşun / tıklamanın sonucu.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Keep,
    Close,
    Answer { id: String, yes: bool },
    Then(Then),
    Input(InputFor, String),
    Save(String, RunConfig),
}

impl Popup {
    /// Tuş işler (pencere açıkken tüm tuşlar buraya gelir).
    pub fn key(&mut self, k: &KeyEvent) -> Outcome {
        let plain = !k.mods.ctrl && !k.mods.alt && !k.mods.cmd;
        if k.key == Key::Esc {
            return Outcome::Close;
        }
        match self {
            Popup::Prompt { id, yes, .. } => match k.key {
                Key::Char('y' | 'Y') if plain => Outcome::Answer { id: id.clone(), yes: true },
                Key::Char('n' | 'N') if plain => Outcome::Answer { id: id.clone(), yes: false },
                Key::Left | Key::Right | Key::Tab | Key::Up | Key::Down => {
                    *yes = !*yes;
                    Outcome::Keep
                }
                Key::Enter => Outcome::Answer { id: id.clone(), yes: *yes },
                _ => Outcome::Keep,
            },
            Popup::Menu { entries, sel, .. } => match k.key {
                Key::Up => {
                    *sel = sel.saturating_sub(1);
                    Outcome::Keep
                }
                Key::Down => {
                    *sel = (*sel + 1).min(entries.len().saturating_sub(1));
                    Outcome::Keep
                }
                Key::Enter => entries.get(*sel).map(|e| Outcome::Then(e.then.clone())).unwrap_or(Outcome::Close),
                Key::Char('q') if plain => Outcome::Close,
                _ => Outcome::Keep,
            },
            Popup::Input { value, purpose, .. } => match k.key {
                Key::Enter => Outcome::Input(purpose.clone(), value.clone()),
                _ => Outcome::Keep,
            },
            Popup::Form(f) => match k.key {
                Key::Enter => match f.submit() {
                    Ok(cfg) => Outcome::Save(f.id.clone(), cfg),
                    Err(e) => {
                        f.error = Some(e);
                        Outcome::Keep
                    }
                },
                _ => Outcome::Keep,
            },
        }
    }

    /// Pencereyi çizer; düğme / satır tıklamasının sonucu.
    pub fn show(&mut self, ctx: &Context, p: &Palette) -> Outcome {
        let mut out = Outcome::Keep;
        let resp = egui::Modal::new(Id::unique("run-popup")).frame(frame(p)).show(ctx, |ui| {
            ui.set_width(440.0);
            match self {
                Popup::Prompt { id, question, yes } => {
                    title(ui, p, "Run");
                    ui.label(RichText::new(question.as_str()).color(color(p.fg)).font(ui_font(13.5)));
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        if widgets::button(ui, p, None, "Yes", *yes, true).clicked() {
                            out = Outcome::Answer { id: id.clone(), yes: true };
                        }
                        if widgets::button(ui, p, None, "No", !*yes, true).clicked() {
                            out = Outcome::Answer { id: id.clone(), yes: false };
                        }
                    });
                }
                Popup::Menu { title: t, entries, sel } => {
                    title(ui, p, t);
                    for (i, e) in entries.iter().enumerate() {
                        let (rect, r) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
                        if i == *sel || r.hovered() {
                            ui.painter().rect_filled(rect, 5.0, color(p.bg_highlight));
                        }
                        let y = rect.center().y;
                        let painter = ui.painter();
                        painter.text(
                            egui::pos2(rect.left() + 10.0, y),
                            Align2::LEFT_CENTER,
                            e.icon,
                            ui_font(14.0),
                            color(p.accent),
                        );
                        painter.text(
                            egui::pos2(rect.left() + 34.0, y),
                            Align2::LEFT_CENTER,
                            &e.label,
                            ui_font(13.0),
                            color(p.fg),
                        );
                        if r.clicked() {
                            out = Outcome::Then(e.then.clone());
                        }
                    }
                }
                Popup::Input { title: t, value, purpose } => {
                    title(ui, p, t);
                    let r = widgets::text_input(ui, p, Id::unique("run-input"), value, "", None, 440.0, false);
                    if !r.has_focus() && !r.lost_focus() {
                        r.request_focus();
                    }
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if widgets::button(ui, p, None, "OK", true, true).clicked() {
                            out = Outcome::Input(purpose.clone(), value.clone());
                        }
                        if widgets::button(ui, p, None, "Cancel", false, true).clicked() {
                            out = Outcome::Close;
                        }
                    });
                }
                Popup::Form(f) => out = form(ui, p, f),
            }
        });
        if resp.backdrop_response.clicked() && out == Outcome::Keep {
            out = Outcome::Close;
        }
        out
    }
}

fn frame(p: &Palette) -> egui::Frame {
    egui::Frame::NONE
        .fill(color(p.surface))
        .stroke(egui::Stroke::new(1.0, color(p.border)))
        .corner_radius(10.0)
        .inner_margin(18.0)
}

fn title(ui: &mut Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).color(color(p.fg)).font(ui_font(14.5)).strong());
    ui.add_space(8.0);
}

/// Form alanları ve Save / Cancel.
fn form(ui: &mut Ui, p: &Palette, f: &mut ConfigForm) -> Outcome {
    title(ui, p, &f.title.clone());
    let vm_label = if f.node { "Node options" } else { "VM options" };
    let vm_hint = if f.node { "--max-old-space-size=4096" } else { "-Xmx1g -Dfoo=bar" };
    let mut fields: Vec<(&str, &str, &mut String)> = Vec::new();
    if f.profiles {
        fields.push(("Profile", "dev,local", &mut f.profile));
    }
    fields.push(("Environment", "KEY=value; KEY2=value", &mut f.env));
    fields.push((vm_label, vm_hint, &mut f.vm));
    fields.push(("Program arguments", "--server.port=8081", &mut f.args));
    let first = Id::unique("run-form-0");
    let none_focused = !ui.memory(|m| m.focused().is_some());
    for (i, (label, hint, value)) in fields.into_iter().enumerate() {
        widgets::section_label(ui, p, label);
        let id = Id::unique("run-form").with(i);
        let id = if i == 0 { first } else { id };
        widgets::text_input(ui, p, id, value, hint, None, 440.0, i == 0 && none_focused);
        ui.add_space(6.0);
    }
    if let Some(e) = &f.error {
        ui.label(RichText::new(e.as_str()).color(color(p.error)).font(ui_font(12.5)));
    }
    ui.add_space(8.0);
    let mut out = Outcome::Keep;
    ui.horizontal(|ui| {
        if widgets::button(ui, p, None, "Save", true, true).clicked() {
            out = match f.submit() {
                Ok(cfg) => Outcome::Save(f.id.clone(), cfg),
                Err(e) => {
                    f.error = Some(e);
                    Outcome::Keep
                }
            };
        }
        if widgets::button(ui, p, None, "Cancel", false, true).clicked() {
            out = Outcome::Close;
        }
    });
    out
}

/// Çıkışta süren kapanış (#80) ve seçili satır.
pub struct ShutdownView {
    pub shutdown: Shutdown,
    pub sel: usize,
}

/// Kapanış listesinin tuş sonucu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShutdownKey {
    Keep,
    /// `q` / Esc: çıkış iptal (süreçler SIGINT almıştır).
    Cancel,
}

impl ShutdownView {
    /// `k` seçileni, `K` hepsini zorla öldürür; `q` / Esc çıkışı iptal eder.
    pub fn key(&mut self, k: &KeyEvent, runner: &mut Runner) -> ShutdownKey {
        let rows = self.shutdown.rows(runner);
        match k.key {
            Key::Esc | Key::Char('q') => return ShutdownKey::Cancel,
            Key::Up => self.sel = self.sel.saturating_sub(1),
            Key::Down => self.sel = (self.sel + 1).min(rows.len().saturating_sub(1)),
            Key::Char('k') => {
                if let Some(r) = rows.get(self.sel) {
                    self.shutdown.kill(runner, &r.id);
                }
            }
            Key::Char('K') => self.shutdown.kill_all(runner),
            _ => {}
        }
        ShutdownKey::Keep
    }

    /// Listeyi çizer.
    pub fn show(&mut self, ctx: &Context, p: &Palette, runner: &mut Runner) -> ShutdownKey {
        let rows = self.shutdown.rows(runner);
        let mut out = ShutdownKey::Keep;
        let mut kill: Option<String> = None;
        egui::Modal::new(Id::unique("run-shutdown")).frame(frame(p)).show(ctx, |ui| {
            ui.set_width(460.0);
            title(ui, p, "Stopping running processes…");
            for (i, r) in rows.iter().enumerate() {
                let (state, tone) = match r.state {
                    ShutdownState::Stopped => ("stopped".to_string(), p.comment),
                    ShutdownState::Stopping { secs } => (format!("stopping… {secs}s"), p.extra.warn),
                    ShutdownState::Killing => ("killing…".to_string(), p.error),
                };
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
                if i == self.sel {
                    ui.painter().rect_filled(rect, 5.0, color(p.bg_highlight));
                }
                if resp.clicked() {
                    self.sel = i;
                }
                if resp.double_clicked() {
                    kill = Some(r.id.clone());
                }
                let y = rect.center().y;
                let painter = ui.painter();
                painter.text(
                    egui::pos2(rect.left() + 10.0, y),
                    Align2::LEFT_CENTER,
                    &r.label,
                    ui_font(13.0),
                    color(p.fg),
                );
                painter.text(
                    egui::pos2(rect.right() - 10.0, y),
                    Align2::RIGHT_CENTER,
                    state,
                    ui_font(12.5),
                    color(tone),
                );
            }
            ui.add_space(8.0);
            ui.label(
                RichText::new("k: kill selected · K: kill all · q: cancel quit")
                    .color(color(p.comment))
                    .font(ui_font(12.0)),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if widgets::button(ui, p, None, "Kill all", true, true).clicked() {
                    self.shutdown.kill_all(runner);
                }
                if widgets::button(ui, p, None, "Cancel quit", false, true).clicked() {
                    out = ShutdownKey::Cancel;
                }
            });
        });
        if let Some(id) = kill {
            self.shutdown.kill(runner, &id);
        }
        out
    }
}
