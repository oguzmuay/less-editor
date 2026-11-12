//! Uygulama durumu ve ekran yönlendirmesi: karşılama ↔ proje, ortak yüzen Notes / Theme / Font pencereleri,
//! yeni LESS-rs örneği ve otomatik açılış (#32 #36 #66 #71 #98). Ekranlar `Action` üretir; kalıcı yazma burada yapılır.

use std::path::{Path, PathBuf};
use std::time::Instant;

use eframe::egui::{self, Color32};
use less_theme::Palette;
use less_workspace::{autorestore, recent};

use crate::appearance::{self, Selector};
use crate::fonts;
use crate::notes::{self, NotesWindow};
use crate::persistence;
use crate::project::ProjectView;
use crate::settings::{Keymap, Settings};
use crate::startup::{self, Launch};
use crate::welcome::Welcome;
use crate::widgets::{Toast, color};
use crate::window;

const APP_ICON: &[u8] = include_bytes!("../assets/icon.png");

/// Açılacak şey: tek klasör (proje) ya da workspace.
#[derive(Clone, Debug)]
pub enum Target {
    Folder(PathBuf),
    Workspace(less_workspace::workspaces::Workspace),
}

/// Ekranların istediği işler.
#[derive(Debug)]
pub enum Action {
    Open(Target),
    CloseProject,
    SetTheme(&'static str),
    SetKeymap(Keymap),
    ChooseTheme,
    ChooseFont,
    Notes { project: PathBuf, command: notes::Command },
    NewWindow(Option<Target>),
    Info(String),
    Error(String),
}

pub struct LessApp {
    settings: Settings,
    palette: Palette,
    started: Instant,
    startup_ms: Option<u128>,
    welcome: Welcome,
    project: Option<ProjectView>,
    appearance: Option<Selector>,
    notes: NotesWindow,
    toast: Toast,
    /// Son gönderilen pencere başlığı; yalnız değişince `ViewportCommand::Title` yollanır.
    window_title: String,
    _instance: autorestore::Instance,
}

impl LessApp {
    /// Ayarları, fontları, temayı, örnek kaydını ve açılış hedefini kurar; `started` süreç başlangıcıdır.
    pub fn new(cc: &eframe::CreationContext<'_>, started: Instant, launch: Launch) -> Self {
        let settings = Settings::load();
        let palette = less_theme::by_id(&settings.theme);
        fonts::install(&cc.egui_ctx, &settings.font);
        apply_visuals(&cc.egui_ctx, &palette);
        let logo = load_logo(&cc.egui_ctx);
        let welcome = Welcome::new(logo);
        let instance = autorestore::Instance::register();
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let target = if persistence::autorestore_enabled() {
            startup::choose_target(
                &launch,
                instance.others,
                autorestore::read(),
                &cwd,
                startup::current_project(&cwd),
                startup::current_has_session(&cwd),
                &welcome.workspaces,
            )
        } else {
            launch.target.as_ref().and_then(|target| match target {
                startup::RequestedTarget::Folder(path) => Some(Target::Folder(path.clone())),
                startup::RequestedTarget::Workspace(name) => {
                    welcome.workspaces.get(name).cloned().map(Target::Workspace)
                }
            })
        };
        let mut app = Self {
            settings,
            palette,
            started,
            startup_ms: None,
            welcome,
            project: None,
            appearance: None,
            notes: NotesWindow::default(),
            toast: Toast::default(),
            window_title: "LESS".to_owned(),
            _instance: instance,
        };
        if let Some(target) = target {
            app.open(target);
        }
        app
    }

    fn apply(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::Open(target) => self.open(target),
            Action::CloseProject => {
                self.notes.close();
                self.project = None;
                self.welcome.reload();
            }
            Action::SetTheme(id) => {
                self.set_theme(ctx, id);
                self.save_settings();
            }
            Action::SetKeymap(k) => {
                self.settings.keymap = k;
                self.save_settings();
            }
            Action::ChooseTheme => {
                self.appearance = Some(Selector::theme(&self.settings.theme));
            }
            Action::ChooseFont => {
                self.appearance = Some(Selector::font(&self.settings.font));
            }
            Action::Notes { project, command } => self.notes.command(command, &project),
            Action::NewWindow(target) => match std::env::current_exe() {
                Ok(executable) => match window::spawn(&executable, target.as_ref()) {
                    Ok(_) => self.toast.info("Started a new LESS window"),
                    Err(e) => self.toast.error(format!("Could not open a new window: {e}")),
                },
                Err(e) => self.toast.error(format!("Could not locate LESS: {e}")),
            },
            Action::Info(t) => self.toast.info(t),
            Action::Error(t) => self.toast.error(t),
        }
    }

    /// Hedefi proje ekranında açar ve son açılanlara yazar. Olmayan klasörler atlanır; hiçbiri yoksa hata gösterilir.
    fn open(&mut self, target: Target) {
        self.notes.close();
        let target = match target {
            Target::Folder(dir) => Target::Folder(normalize_folder(dir)),
            workspace => workspace,
        };
        let now = recent::now();
        let (title, roots, key, workspace) = match &target {
            Target::Folder(dir) => (file_name(dir), vec![dir.clone()], dir.clone(), None),
            Target::Workspace(ws) => {
                (ws.name.clone(), ws.roots.clone(), startup::workspace_dir(&ws.name), Some(ws.name.clone()))
            }
        };
        let existing: Vec<PathBuf> = roots.into_iter().filter(|r| r.is_dir()).collect();
        if existing.is_empty() {
            self.toast.error(format!("Folder not found: {title}"));
            return;
        }
        let saved = match &target {
            Target::Folder(dir) => self.welcome.recents.add_project(dir, now),
            Target::Workspace(ws) => {
                let _ = self.welcome.workspaces.touch(&ws.name, now);
                self.welcome.recents.add_workspace(&ws.name, now)
            }
        };
        if let Err(e) = saved {
            self.toast.error(format!("Could not update recent projects: {e}"));
        }
        if let Err(e) = persistence::remember_project(&key, workspace.as_deref(), now) {
            self.toast.error(format!("Could not update the last project: {e}"));
        }
        self.project = Some(ProjectView::new_with_key(title, existing, workspace.is_some(), self.settings.keymap, key));
    }

    fn set_theme(&mut self, ctx: &egui::Context, id: &str) {
        self.palette = less_theme::by_id(id);
        self.settings.theme = self.palette.id.to_owned();
        apply_visuals(ctx, &self.palette);
    }

    fn set_font(&mut self, ctx: &egui::Context, family: &str) -> bool {
        let installed = fonts::install(ctx, family);
        if installed {
            self.settings.font = family.to_owned();
        } else {
            self.toast.error(format!("Font is not installed: {family}"));
        }
        installed
    }

    fn appearance_command(&mut self, ctx: &egui::Context, command: appearance::Command) {
        let Some(kind) = self.appearance.as_ref().map(Selector::kind) else { return };
        match command {
            appearance::Command::Preview(id) => match kind {
                appearance::Kind::Theme => self.set_theme(ctx, &id),
                appearance::Kind::Font => {
                    self.set_font(ctx, &id);
                }
            },
            appearance::Command::Commit(id) => {
                let applied = match kind {
                    appearance::Kind::Theme => {
                        self.set_theme(ctx, &id);
                        true
                    }
                    appearance::Kind::Font => self.set_font(ctx, &id),
                };
                if applied {
                    self.save_settings();
                    self.appearance = None;
                    self.toast.info(match kind {
                        appearance::Kind::Theme => format!("Theme: {}", self.palette.name),
                        appearance::Kind::Font => format!("Font: {id}"),
                    });
                }
            }
            appearance::Command::Cancel(id) => {
                match kind {
                    appearance::Kind::Theme => self.set_theme(ctx, &id),
                    appearance::Kind::Font => {
                        self.settings.font = id;
                        fonts::install(ctx, &self.settings.font);
                    }
                }
                self.appearance = None;
            }
        }
    }

    fn save_settings(&mut self) {
        if let Err(e) = self.settings.save() {
            self.toast.error(format!("Could not save settings: {e}"));
        }
    }
}

/// CLI'daki göreli klasörü kararlı autorestore anahtarına çevirir; varsa symlink'leri çözer (#32).
fn normalize_folder(dir: PathBuf) -> PathBuf {
    let absolute =
        if dir.is_absolute() { dir } else { std::env::current_dir().map(|cwd| cwd.join(&dir)).unwrap_or(dir) };
    std::fs::canonicalize(&absolute).unwrap_or(absolute)
}

fn file_name(dir: &Path) -> String {
    dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| dir.display().to_string())
}

/// Gömülü uygulama ikonunu (PNG) dokuya çevirir; çözülemezse saydam boş doku döner.
fn load_logo(ctx: &egui::Context) -> egui::TextureHandle {
    let image = match eframe::icon_data::from_png_bytes(APP_ICON) {
        Ok(icon) => egui::ColorImage::from_rgba_unmultiplied([icon.width as usize, icon.height as usize], &icon.rgba),
        Err(_) => egui::ColorImage::filled([1, 1], Color32::TRANSPARENT),
    };
    ctx.load_texture("less-logo", image, egui::TextureOptions::LINEAR)
}

/// egui görünümünü paletin renkleriyle boyar (açık temada açık taban).
fn apply_visuals(ctx: &egui::Context, p: &Palette) {
    let mut v = if p.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = color(p.bg);
    v.window_fill = color(p.surface);
    v.window_stroke = egui::Stroke::new(1.0, color(p.border));
    v.extreme_bg_color = color(p.bg_dark);
    v.override_text_color = Some(color(p.fg));
    v.selection.bg_fill = color(p.accent).gamma_multiply(0.35);
    v.selection.stroke = egui::Stroke::new(1.0, color(p.fg));
    v.text_cursor.stroke = egui::Stroke::new(2.0, color(p.accent));
    v.hyperlink_color = color(p.accent);
    v.widgets.hovered.weak_bg_fill = color(p.bg_highlight);
    v.widgets.hovered.bg_fill = color(p.bg_highlight);
    ctx.set_theme(if p.dark { egui::Theme::Dark } else { egui::Theme::Light });
    ctx.set_visuals(v);
}

impl eframe::App for LessApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let startup_ms = *self.startup_ms.get_or_insert_with(|| self.started.elapsed().as_millis());
        let ctx = ui.ctx().clone();
        let locked = self.appearance.is_some() || self.notes.is_open();
        let had_appearance = self.appearance.is_some();
        let mut actions = Vec::new();
        let preview_commands = self.appearance.as_mut().map(|selector| selector.handle_input(&ctx)).unwrap_or_default();
        for command in preview_commands {
            self.appearance_command(&ctx, command);
        }
        if !had_appearance && self.notes.is_open() {
            self.notes.handle_input(&ctx, Instant::now());
        }
        if self.project.is_none()
            && !locked
            && ctx.input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::N))
        {
            actions.push(Action::NewWindow(None));
        }

        let background_events = if locked { ctx.input_mut(|i| std::mem::take(&mut i.events)) } else { Vec::new() };
        let p = self.palette;
        let center_x = match &mut self.project {
            Some(project) => {
                let recents = self.welcome.recents.entries(&self.welcome.workspaces);
                project.show_with_modal(ui, &p, &self.settings, &recents, locked, &mut actions);
                ui.max_rect().center().x
            }
            None => {
                self.welcome.show_with_modal(ui, &p, &self.settings, startup_ms, locked, &mut actions);
                ui.max_rect().center().x + crate::welcome::SIDEBAR_WIDTH / 2.0
            }
        };
        for action in actions {
            self.apply(&ctx, action);
        }
        if locked {
            ctx.input_mut(|i| i.events = background_events);
        }
        let appearance_commands =
            self.appearance.as_mut().map(|selector| selector.show(&ctx, &self.palette)).unwrap_or_default();
        for command in appearance_commands {
            self.appearance_command(&ctx, command);
        }
        self.notes.show(&ctx, &self.palette);
        for (message, error) in self.notes.drain_messages() {
            if error {
                self.toast.error(message);
            } else {
                self.toast.info(message);
            }
        }
        self.toast.show(&ctx, &self.palette, center_x, ctx.content_rect().bottom() - 26.0);
        let title = self.project.as_ref().map_or("LESS", |project| project.title());
        if title != self.window_title {
            self.window_title = title.to_owned();
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title.clone()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_project_path_becomes_stable_absolute_key_32() {
        let path = normalize_folder(PathBuf::from("."));
        assert!(path.is_absolute());
        assert_eq!(path, std::fs::canonicalize(".").unwrap());
    }
}
