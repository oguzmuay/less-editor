//! Karşılama ekranının modal formları: New Project (ad + konum + git init) ve New Workspace (ad + klasörler).
//! Klasör seçimi macOS'un yerel seçicisiyle (`rfd`). Form yalnız geçerli girdiyle oluşturur; hata formda gösterilir.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Align2, Frame, Margin, Sense, Stroke, Ui, pos2, vec2};
use less_theme::Palette;
use less_workspace::workspaces::{self, Workspace};
use less_workspace::{paths, project, recent};

use crate::fonts::icon;
use crate::widgets::{button, button_width, color, text_input, ui_font};

const WIDTH: f32 = 520.0;

pub enum Outcome {
    Keep,
    Cancel,
    Folder(PathBuf),
    Workspace(Workspace),
}

pub enum Dialog {
    NewProject(NewProject),
    NewWorkspace(NewWorkspace),
}

impl Dialog {
    pub fn show(&mut self, ctx: &egui::Context, p: &Palette, store: &mut workspaces::Store) -> Outcome {
        let frame = Frame::NONE
            .fill(color(p.surface))
            .stroke(Stroke::new(1.0, color(p.border)))
            .corner_radius(12.0)
            .inner_margin(Margin::same(26));
        let (id, title) = match self {
            Dialog::NewProject(_) => ("new-project", "New Project"),
            Dialog::NewWorkspace(_) => ("new-workspace", "New Workspace"),
        };
        let modal = egui::Modal::new(egui::Id::unique(id))
            .frame(frame)
            .backdrop_color(egui::Color32::from_black_alpha(if p.dark { 150 } else { 70 }))
            .show(ctx, |ui| {
                ui.set_width(WIDTH);
                let (rect, _) = ui.allocate_exact_size(vec2(WIDTH, 30.0), Sense::hover());
                ui.painter().text(rect.left_center(), Align2::LEFT_CENTER, title, ui_font(19.0), color(p.fg));
                ui.add_space(14.0);
                match self {
                    Dialog::NewProject(form) => form.ui(ui, p),
                    Dialog::NewWorkspace(form) => form.ui(ui, p, store),
                }
            });
        match modal.inner {
            Outcome::Keep if modal.should_close() => Outcome::Cancel,
            other => other,
        }
    }
}

fn label(ui: &mut Ui, p: &Palette, text: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::hover());
    ui.painter().text(rect.left_center(), Align2::LEFT_CENTER, text, ui_font(12.5), color(p.fg_dim));
    ui.add_space(4.0);
}

/// Form genişliğine sarılan metin; solda isteğe bağlı ikon. Yüksekliği metne göre ayrılır.
fn wrapped(ui: &mut Ui, glyph: Option<&str>, text: &str, size: f32, tint: egui::Color32) {
    let indent = if glyph.is_some() { 22.0 } else { 0.0 };
    let galley = ui.painter().layout(text.to_owned(), ui_font(size), tint, WIDTH - indent);
    let (rect, _) = ui.allocate_exact_size(vec2(WIDTH, galley.size().y.max(18.0)), Sense::hover());
    if let Some(g) = glyph {
        ui.painter().text(rect.left_top() + vec2(0.0, 1.0), Align2::LEFT_TOP, g, ui_font(14.0), tint);
    }
    ui.painter().galley(rect.left_top() + vec2(indent, 0.0), galley, tint);
}

fn error_line(ui: &mut Ui, p: &Palette, error: &Option<String>) {
    if let Some(e) = error {
        ui.add_space(10.0);
        wrapped(ui, Some(icon::WARNING), e, 12.5, color(p.error));
    }
}

/// Sağa yaslı Cancel + birincil buton satırı; `(cancel, confirm)` tıklamalarını döndürür.
fn footer(ui: &mut Ui, p: &Palette, confirm: &str, enabled: bool) -> (bool, bool) {
    ui.add_space(22.0);
    let mut out = (false, false);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let used = button_width(ui, "Cancel", None) + button_width(ui, confirm, None) + 8.0;
        ui.add_space(ui.available_width() - used);
        out.0 = button(ui, p, None, "Cancel", false, true).clicked();
        out.1 = button(ui, p, None, confirm, true, enabled).clicked();
    });
    out
}

fn pick_folder(start: &Path) -> Option<PathBuf> {
    rfd::FileDialog::new().set_directory(start).pick_folder()
}

pub struct NewProject {
    name: String,
    location: String,
    git_init: bool,
    error: Option<String>,
    focus_frames: u8,
}

impl NewProject {
    /// Konum varsayılanı: en son açılan projenin üst klasörü, yoksa `~/Documents/projects`, o da yoksa ev klasörü.
    pub fn new(recents: &[recent::Recent]) -> Self {
        let last_parent = recents
            .iter()
            .find(|r| r.kind == recent::Kind::Project)
            .and_then(|r| r.dir.parent().map(Path::to_path_buf))
            .filter(|d| d.is_dir());
        let projects = paths::home().join("Documents/projects");
        let location = last_parent.unwrap_or(if projects.is_dir() { projects } else { paths::home() });
        Self {
            name: String::new(),
            location: recent::display_path(&location),
            git_init: true,
            error: None,
            focus_frames: 3,
        }
    }

    fn ui(&mut self, ui: &mut Ui, p: &Palette) -> Outcome {
        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
        label(ui, p, "Name");
        let id = ui.scope_id().with("np-name");
        if text_input(ui, p, id, &mut self.name, "my-project", None, WIDTH, self.focus_frames > 0).changed() {
            self.error = None;
        }
        self.focus_frames = self.focus_frames.saturating_sub(1);
        ui.add_space(14.0);

        label(ui, p, "Location");
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let browse_w = button_width(ui, "Browse…", None);
            let id = ui.scope_id().with("np-location");
            text_input(ui, p, id, &mut self.location, "~/Documents/projects", None, WIDTH - browse_w - 8.0, false);
            if button(ui, p, None, "Browse…", false, true).clicked()
                && let Some(dir) = pick_folder(Path::new(&self.location))
            {
                self.location = dir.display().to_string();
            }
        });
        ui.add_space(14.0);

        let (rect, resp) = ui.allocate_exact_size(vec2(WIDTH, 24.0), Sense::click());
        if resp.clicked() {
            self.git_init = !self.git_init;
        }
        checkbox(ui, p, rect, self.git_init, "Create Git repository");

        let target = self.target();
        ui.add_space(14.0);
        let preview = match &target {
            Some(t) => format!("Creates {}", recent::display_path(t)),
            None => " ".to_owned(),
        };
        wrapped(ui, None, &preview, 12.0, color(p.comment));
        error_line(ui, p, &self.error);

        let (cancel, create) = footer(ui, p, "Create", target.is_some());
        if cancel {
            return Outcome::Cancel;
        }
        if create || (enter && target.is_some()) {
            match project::create(&expand(&self.location), &self.name, self.git_init) {
                Ok(dir) => return Outcome::Folder(dir),
                Err(e) => self.error = Some(e),
            }
        }
        Outcome::Keep
    }

    fn target(&self) -> Option<PathBuf> {
        let name = self.name.trim();
        (!name.is_empty() && !self.location.trim().is_empty()).then(|| expand(&self.location).join(name))
    }
}

/// `~/` ile başlayan yolu ev klasörüne açar.
fn expand(path: &str) -> PathBuf {
    let path = path.trim();
    match path.strip_prefix("~/") {
        Some(rest) => paths::home().join(rest),
        None if path == "~" => paths::home(),
        None => PathBuf::from(path),
    }
}

/// Kutucuklu onay satırı (`rect` tıklama alanıdır).
fn checkbox(ui: &Ui, p: &Palette, rect: egui::Rect, on: bool, text: &str) {
    let box_rect = egui::Rect::from_min_size(pos2(rect.left(), rect.center().y - 9.0), vec2(18.0, 18.0));
    if on {
        ui.painter().rect_filled(box_rect, 4.0, color(p.accent));
        ui.painter().text(box_rect.center(), Align2::CENTER_CENTER, icon::CHECK, ui_font(13.0), color(p.on_accent));
    } else {
        ui.painter().rect(box_rect, 4.0, color(p.bg_dark), Stroke::new(1.0, color(p.border)), egui::StrokeKind::Inside);
    }
    ui.painter().text(
        pos2(box_rect.right() + 10.0, rect.center().y),
        Align2::LEFT_CENTER,
        text,
        ui_font(13.5),
        color(p.fg),
    );
}

pub struct NewWorkspace {
    name: String,
    roots: Vec<PathBuf>,
    error: Option<String>,
    focus_frames: u8,
}

impl NewWorkspace {
    pub fn new() -> Self {
        Self { name: String::new(), roots: Vec::new(), error: None, focus_frames: 3 }
    }

    fn ui(&mut self, ui: &mut Ui, p: &Palette, store: &mut workspaces::Store) -> Outcome {
        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
        label(ui, p, "Name");
        let id = ui.scope_id().with("nw-name");
        if text_input(ui, p, id, &mut self.name, "my-workspace", None, WIDTH, self.focus_frames > 0).changed() {
            self.error =
                store.exists(&self.name).then(|| format!("A workspace named \"{}\" already exists.", self.name.trim()));
        }
        self.focus_frames = self.focus_frames.saturating_sub(1);
        ui.add_space(14.0);

        label(ui, p, "Folders");
        let mut remove = None;
        for (i, root) in self.roots.iter().enumerate() {
            let (rect, _) = ui.allocate_exact_size(vec2(WIDTH, 34.0), Sense::hover());
            ui.painter().rect(rect, 7.0, color(p.bg_dark), Stroke::new(1.0, color(p.border)), egui::StrokeKind::Inside);
            let y = rect.center().y;
            ui.painter().text(
                pos2(rect.left() + 12.0, y),
                Align2::LEFT_CENTER,
                icon::FOLDER,
                ui_font(15.0),
                color(p.icon),
            );
            let text = recent::display_path(root);
            ui.painter().text(pos2(rect.left() + 36.0, y), Align2::LEFT_CENTER, text, ui_font(13.0), color(p.fg));
            let x_rect = egui::Rect::from_center_size(pos2(rect.right() - 18.0, y), vec2(24.0, 24.0));
            let x = ui.interact(x_rect, ui.scope_id().with(("rm", i)), Sense::click());
            let tint = if x.hovered() { color(p.error) } else { color(p.comment) };
            ui.painter().text(x_rect.center(), Align2::CENTER_CENTER, icon::X, ui_font(14.0), tint);
            if x.clicked() {
                remove = Some(i);
            }
            ui.add_space(6.0);
        }
        if let Some(i) = remove {
            self.roots.remove(i);
        }
        if button(ui, p, Some(icon::PLUS), "Add Folders…", false, true).clicked() {
            let start = self.roots.last().and_then(|r| r.parent()).map(Path::to_path_buf).unwrap_or_else(paths::home);
            if let Some(dirs) = rfd::FileDialog::new().set_directory(start).pick_folders() {
                for d in dirs {
                    if !self.roots.contains(&d) {
                        self.roots.push(d);
                    }
                }
            }
        }
        error_line(ui, p, &self.error);

        let ready = !self.name.trim().is_empty() && !self.roots.is_empty() && self.error.is_none();
        let (cancel, create) = footer(ui, p, "Create", ready);
        if cancel {
            return Outcome::Cancel;
        }
        if create || (enter && ready) {
            match store.create(&self.name, self.roots.clone(), recent::now()) {
                Ok(ws) => return Outcome::Workspace(ws),
                Err(e) => self.error = Some(e),
            }
        }
        Outcome::Keep
    }
}
