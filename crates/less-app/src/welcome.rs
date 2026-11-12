//! Karşılama penceresi (proje açılmadan önceki ekran): solda marka + gezinme, sağda seçili görünüm
//! (Projects / Workspaces / Customize / Shortcuts), altta durum satırı.
//!
//! Klavye: ↑↓ seçim, Enter aç, Cmd+F arama, Cmd+N yeni proje, Cmd+O klasör aç, Esc aramayı temizler.
//! Satıra sağ tık: Open, Reveal in Finder, Copy Path, Remove from Recent.

use eframe::egui::{
    self, Align2, Color32, CursorIcon, FontId, Frame, Margin, Pos2, Rect, Response, Sense, Stroke, StrokeKind,
    TextureHandle, Ui, pos2, vec2,
};
use less_theme::Palette;
use less_workspace::recent::{self, Kind, Recent};
use less_workspace::{paths, workspaces};

use crate::app::{Action, Target};
use crate::dialogs::{Dialog, NewProject, NewWorkspace, Outcome};
use crate::fonts::icon;
use crate::settings::{Keymap, Settings};
use crate::widgets::{
    button, button_width, color, drag_strip, empty_state, heading, section_label, text_input, ui_font,
};

pub const SIDEBAR_WIDTH: f32 = 248.0;
const STATUS_HEIGHT: f32 = 26.0;
const CONTENT_MAX_WIDTH: f32 = 860.0;
const ROW_HEIGHT: f32 = 58.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Projects,
    Workspaces,
    Customize,
    Shortcuts,
}

/// Kısayollar görünümü: `docs/KEYMAP.md`'deki taşınan kısayolların karşılama ekranında anlamlı olan kısmı.
const SHORTCUTS: [(&str, &str); 15] = [
    ("Cmd 1", "Files"),
    ("Cmd 2", "Code editor"),
    ("Cmd 3", "AI"),
    ("Cmd 4", "Terminal"),
    ("Cmd 5", "Database"),
    ("Cmd 6", "Run"),
    ("Cmd 7", "Git History"),
    ("Cmd 0", "Git"),
    ("Cmd F", "Find file"),
    ("Cmd Shift F", "Find in project"),
    ("Cmd E", "Recent files"),
    ("Cmd K", "Search shortcuts"),
    ("Cmd Shift P", "Command palette"),
    ("Cmd N", "Notes"),
    ("Cmd Shift N", "New window"),
];

/// Satıra sağ tıkla seçilen iş.
enum RowAction {
    Open,
    Reveal,
    CopyPath,
    Remove,
}

pub struct Welcome {
    view: View,
    query: String,
    selected: usize,
    scroll_to_selected: bool,
    focus_search: bool,
    pub recents: recent::Store,
    pub workspaces: workspaces::Store,
    entries: Vec<Recent>,
    logo: TextureHandle,
    dialog: Option<Dialog>,
}

impl Welcome {
    pub fn new(logo: TextureHandle) -> Self {
        let mut welcome = Self {
            view: View::Projects,
            query: String::new(),
            selected: 0,
            scroll_to_selected: false,
            focus_search: false,
            recents: recent::Store::load(),
            workspaces: workspaces::Store::load(),
            entries: Vec::new(),
            logo,
            dialog: None,
        };
        welcome.reload();
        welcome
    }

    /// Listeyi kayıtlardan yeniden kurar (proje açıldıktan / kayıt silindikten sonra).
    pub fn reload(&mut self) {
        self.entries = self.recents.entries(&self.workspaces);
    }

    /// Görünen satırlar: Projects'te son açılanlar, Workspaces'te tüm workspace'ler; ikisi de aramaya göre süzülür.
    fn items(&self) -> Vec<Recent> {
        let all: Vec<Recent> = match self.view {
            View::Workspaces => {
                let mut ws: Vec<Recent> = self
                    .workspaces
                    .items
                    .iter()
                    .map(|w| Recent {
                        kind: Kind::Workspace,
                        name: w.name.clone(),
                        dir: w.roots.first().cloned().unwrap_or_default(),
                        roots: w.roots.clone(),
                        time: w.opened,
                    })
                    .collect();
                ws.sort_by_key(|r| std::cmp::Reverse(r.time));
                ws
            }
            _ => self.entries.clone(),
        };
        let q = self.query.trim().to_lowercase();
        all.into_iter()
            .filter(|r| {
                q.is_empty()
                    || r.name.to_lowercase().contains(&q)
                    || r.dir.to_string_lossy().to_lowercase().contains(&q)
                    || r.roots.iter().any(|d| d.to_string_lossy().to_lowercase().contains(&q))
            })
            .collect()
    }

    fn open_action(&self, r: &Recent) -> Action {
        match r.kind {
            Kind::Project => Action::Open(Target::Folder(r.dir.clone())),
            Kind::Workspace => match self.workspaces.get(&r.name) {
                Some(ws) => Action::Open(Target::Workspace(ws.clone())),
                None => Action::Error(format!("Workspace \"{}\" no longer exists", r.name)),
            },
        }
    }

    /// Ortak bir modal açıkken alttaki karşılama ekranı klavye girdisi almaz.
    pub fn show_with_modal(
        &mut self,
        ui: &mut Ui,
        p: &Palette,
        settings: &Settings,
        startup_ms: u128,
        modal: bool,
        actions: &mut Vec<Action>,
    ) {
        if self.dialog.is_none() && !modal {
            self.keyboard(ui, actions);
        }
        egui::Panel::bottom("status")
            .exact_size(STATUS_HEIGHT)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::NONE.fill(color(p.surface)).inner_margin(Margin::symmetric(14, 0)))
            .show(ui, |ui| status_bar(ui, p, settings, startup_ms));
        egui::Panel::left("sidebar")
            .exact_size(SIDEBAR_WIDTH)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::NONE.fill(color(p.surface)).inner_margin(Margin { left: 14, right: 14, top: 44, bottom: 16 }))
            .show(ui, |ui| self.sidebar(ui, p, settings, actions));
        egui::CentralPanel::default().frame(Frame::NONE.fill(color(p.bg)).inner_margin(Margin::same(40))).show(
            ui,
            |ui| {
                let full = ui.max_rect();
                let width = full.width().min(CONTENT_MAX_WIDTH);
                let rect =
                    Rect::from_min_size(pos2(full.center().x - width / 2.0, full.top()), vec2(width, full.height()));
                ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| match self.view {
                    View::Projects | View::Workspaces => self.list_view(ui, p, actions),
                    View::Customize => customize(ui, p, settings, actions),
                    View::Shortcuts => shortcuts_view(ui, p),
                });
            },
        );
        let border = ui.max_rect();
        ui.painter().vline(
            border.left() + SIDEBAR_WIDTH,
            border.top()..=border.bottom() - STATUS_HEIGHT,
            Stroke::new(1.0, color(p.border)),
        );
        drag_strip(ui, Rect::from_min_size(border.left_top() + vec2(80.0, 0.0), vec2(border.width() - 80.0, 30.0)));
        self.dialog_ui(ui.ctx(), p, actions);
    }

    fn dialog_ui(&mut self, ctx: &egui::Context, p: &Palette, actions: &mut Vec<Action>) {
        let Some(dialog) = &mut self.dialog else { return };
        match dialog.show(ctx, p, &mut self.workspaces) {
            Outcome::Keep => {}
            Outcome::Cancel => self.dialog = None,
            Outcome::Folder(dir) => {
                self.dialog = None;
                actions.push(Action::Open(Target::Folder(dir)));
            }
            Outcome::Workspace(ws) => {
                self.dialog = None;
                actions.push(Action::Open(Target::Workspace(ws)));
            }
        }
    }

    fn keyboard(&mut self, ui: &Ui, actions: &mut Vec<Action>) {
        let (up, down, enter, find, new, open, esc) = ui.input(|i| {
            let cmd = i.modifiers.command;
            (
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::ArrowDown),
                i.key_pressed(egui::Key::Enter),
                cmd && i.key_pressed(egui::Key::F),
                cmd && !i.modifiers.shift && i.key_pressed(egui::Key::N),
                cmd && i.key_pressed(egui::Key::O),
                i.key_pressed(egui::Key::Escape),
            )
        });
        if new {
            self.dialog = Some(Dialog::NewProject(NewProject::new(&self.entries)));
            return;
        }
        if open {
            self.open_folder(actions);
            return;
        }
        if !matches!(self.view, View::Projects | View::Workspaces) {
            return;
        }
        if find {
            self.focus_search = true;
        }
        if esc {
            self.query.clear();
            self.selected = 0;
        }
        let count = self.items().len();
        if count == 0 {
            return;
        }
        if up || down {
            self.selected = if up { self.selected.saturating_sub(1) } else { (self.selected + 1).min(count - 1) };
            self.scroll_to_selected = true;
        }
        if enter && let Some(item) = self.items().get(self.selected) {
            actions.push(self.open_action(item));
        }
    }

    fn open_folder(&self, actions: &mut Vec<Action>) {
        let start =
            self.entries.first().and_then(|r| r.dir.parent()).map(|p| p.to_path_buf()).unwrap_or_else(paths::home);
        if let Some(dir) = rfd::FileDialog::new().set_title("Open Folder").set_directory(start).pick_folder() {
            actions.push(Action::Open(Target::Folder(dir)));
        }
    }

    fn sidebar(&mut self, ui: &mut Ui, p: &Palette, settings: &Settings, actions: &mut Vec<Action>) {
        let (brand, _) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover());
        let logo = Rect::from_min_size(brand.left_top() + vec2(4.0, 2.0), vec2(40.0, 40.0));
        ui.painter().image(self.logo.id(), logo, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
        let text_x = logo.right() + 12.0;
        ui.painter().text(pos2(text_x, brand.top() + 5.0), Align2::LEFT_TOP, "LESS", ui_font(17.0), color(p.fg));
        let version = format!("version {}", env!("CARGO_PKG_VERSION"));
        ui.painter().text(pos2(text_x, brand.top() + 26.0), Align2::LEFT_TOP, version, ui_font(12.0), color(p.comment));
        ui.add_space(28.0);

        let items = [
            (View::Projects, icon::FOLDERS, "Projects", Some(self.entries.len())),
            (View::Workspaces, icon::SQUARES_FOUR, "Workspaces", Some(self.workspaces.items.len())),
            (View::Customize, icon::PALETTE, "Customize", None),
            (View::Shortcuts, icon::KEYBOARD, "Shortcuts", None),
        ];
        for (view, glyph, label, count) in items {
            if nav_item(ui, p, glyph, label, count, self.view == view).clicked() && self.view != view {
                self.view = view;
                self.selected = 0;
            }
            ui.add_space(2.0);
        }

        let bottom = ui.max_rect().bottom();
        let rect = Rect::from_min_max(pos2(ui.max_rect().left(), bottom - 64.0), pos2(ui.max_rect().right(), bottom));
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            let at = rect.left_top() + vec2(4.0, 0.0);
            ui.painter().text(at, Align2::LEFT_TOP, "KEYMAP", ui_font(11.0), color(p.comment));
            ui.add_space(20.0);
            if let Some(mode) = segmented(ui, p, settings.keymap) {
                actions.push(Action::SetKeymap(mode));
            }
        });
    }

    /// Projects ve Workspaces görünümü: başlık, arama + eylem butonları, liste.
    fn list_view(&mut self, ui: &mut Ui, p: &Palette, actions: &mut Vec<Action>) {
        let workspaces_view = self.view == View::Workspaces;
        let (title, subtitle) = if workspaces_view {
            ("Workspaces", "Folders from different places, opened together as one project.")
        } else {
            ("Welcome to LESS", "less noise, more code")
        };
        heading(ui, p, title, subtitle);
        ui.add_space(24.0);

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let buttons: &[(&str, &str, bool)] = if workspaces_view {
                &[(icon::SQUARES_FOUR, "New Workspace", true)]
            } else {
                &[
                    (icon::FILE_PLUS, "New Project", true),
                    (icon::FOLDER_OPEN, "Open Folder", false),
                    (icon::SQUARES_FOUR, "New Workspace", false),
                ]
            };
            let buttons_width: f32 =
                buttons.iter().map(|(g, l, _)| button_width(ui, l, Some(g))).sum::<f32>() + 8.0 * buttons.len() as f32;
            let id = ui.scope_id().with("search");
            let hint = if workspaces_view { "Search workspaces" } else { "Search projects" };
            let search = text_input(
                ui,
                p,
                id,
                &mut self.query,
                hint,
                Some(icon::MAGNIFYING_GLASS),
                ui.available_width() - buttons_width,
                std::mem::take(&mut self.focus_search),
            );
            if search.changed() {
                self.selected = 0;
            }
            for &(glyph, label, primary) in buttons {
                if button(ui, p, Some(glyph), label, primary, true).clicked() {
                    match label {
                        "New Project" => self.dialog = Some(Dialog::NewProject(NewProject::new(&self.entries))),
                        "Open Folder" => self.open_folder(actions),
                        _ => self.dialog = Some(Dialog::NewWorkspace(NewWorkspace::new())),
                    }
                }
            }
        });
        ui.add_space(28.0);

        let items = self.items();
        self.selected = self.selected.min(items.len().saturating_sub(1));
        section_label(ui, p, if workspaces_view { "ALL WORKSPACES" } else { "RECENT PROJECTS" });
        ui.add_space(8.0);
        if items.is_empty() {
            let (text, hint) = match (self.query.is_empty(), workspaces_view) {
                (false, _) => ("No matches", "Press Esc to clear the search"),
                (true, true) => ("No workspaces yet", "Create one to open several folders together"),
                (true, false) => ("Nothing opened yet", "Open a folder or create a new project"),
            };
            empty_state(ui, p, icon::FOLDER_OPEN, text, hint);
            return;
        }
        let now = recent::now();
        let mut row_actions = Vec::new();
        egui::ScrollArea::vertical().id_salt("welcome-list").auto_shrink([false, false]).show(ui, |ui| {
            for (i, item) in items.iter().enumerate() {
                let resp = recent_row(ui, p, item, now, i == self.selected);
                if i == self.selected && self.scroll_to_selected {
                    resp.scroll_to_me(None);
                }
                if resp.clicked() {
                    self.selected = i;
                    row_actions.push((RowAction::Open, item.clone()));
                }
                resp.context_menu(|ui| {
                    if let Some(a) = row_menu(ui, item, workspaces_view) {
                        row_actions.push((a, item.clone()));
                    }
                });
            }
        });
        self.scroll_to_selected = false;
        for (action, item) in row_actions {
            self.row_action(ui.ctx(), action, &item, actions);
        }
    }

    fn row_action(&mut self, ctx: &egui::Context, action: RowAction, item: &Recent, actions: &mut Vec<Action>) {
        let path = item.roots.first().unwrap_or(&item.dir).clone();
        match action {
            RowAction::Open => actions.push(self.open_action(item)),
            RowAction::Reveal => {
                if std::process::Command::new("open").arg("-R").arg(&path).spawn().is_err() {
                    actions.push(Action::Error("Could not open Finder".into()));
                }
            }
            RowAction::CopyPath => {
                let text = if item.roots.is_empty() {
                    item.dir.display().to_string()
                } else {
                    item.roots.iter().map(|r| r.display().to_string()).collect::<Vec<_>>().join("\n")
                };
                ctx.copy_text(text);
                actions.push(Action::Info("Path copied".into()));
            }
            RowAction::Remove => {
                let key = match item.kind {
                    Kind::Project => item.dir.to_string_lossy().into_owned(),
                    Kind::Workspace => item.name.clone(),
                };
                match self.recents.remove(item.kind, &key) {
                    Ok(()) => {
                        self.reload();
                        actions.push(Action::Info(format!("Removed {} from recent projects", item.name)));
                    }
                    Err(e) => actions.push(Action::Error(format!("Could not update recent projects: {e}"))),
                }
            }
        }
    }
}

/// Satırın sağ tık menüsü; seçilen işi döndürür. Workspaces görünümünde "Remove from Recent" yoktur (o liste kayıttır).
fn row_menu(ui: &mut Ui, item: &Recent, workspaces_view: bool) -> Option<RowAction> {
    ui.set_min_width(200.0);
    let mut picked = None;
    let mut entry = |ui: &mut Ui, glyph: &str, label: &str, action: RowAction| {
        if ui.button(format!("{glyph}  {label}")).clicked() {
            picked = Some(action);
            ui.close();
        }
    };
    entry(ui, icon::FOLDER_OPEN, "Open", RowAction::Open);
    entry(ui, icon::MAGNIFYING_GLASS, "Reveal in Finder", RowAction::Reveal);
    let copy = if item.roots.len() > 1 { "Copy Paths" } else { "Copy Path" };
    entry(ui, icon::COPY, copy, RowAction::CopyPath);
    if !workspaces_view {
        ui.separator();
        entry(ui, icon::TRASH, "Remove from Recent", RowAction::Remove);
    }
    picked
}

fn status_bar(ui: &mut Ui, p: &Palette, settings: &Settings, startup_ms: u128) {
    let rect = ui.max_rect();
    let y = rect.center().y;
    let font = ui_font(12.0);
    let painter = ui.painter();
    let left = format!("Ready in {startup_ms} ms");
    painter.text(pos2(rect.left(), y), Align2::LEFT_CENTER, left, font.clone(), color(p.comment));
    let right = format!("{}   ·   {}   ·   UTF-8", settings.keymap.label(), p.name);
    painter.text(pos2(rect.right(), y), Align2::RIGHT_CENTER, right, font, color(p.comment));
}

/// Kenar çubuğu girişi: ikon + ad, sağda sayaç; seçiliyse vurgulu zemin ve solda ince çubuk.
fn nav_item(ui: &mut Ui, p: &Palette, glyph: &str, label: &str, count: Option<usize>, selected: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
    let painter = ui.painter();
    if selected || resp.hovered() {
        let fill = if selected { color(p.bg_highlight) } else { color(p.bg_highlight).gamma_multiply(0.5) };
        painter.rect_filled(rect, 6.0, fill);
    }
    if selected {
        let bar = Rect::from_min_size(rect.left_top() + vec2(0.0, 9.0), vec2(3.0, 16.0));
        painter.rect_filled(bar, 2.0, color(p.accent));
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    let y = rect.center().y;
    let icon_color = if selected { color(p.accent) } else { color(p.fg_dim) };
    painter.text(pos2(rect.left() + 14.0, y), Align2::LEFT_CENTER, glyph, ui_font(16.0), icon_color);
    let text_color = if selected { color(p.fg) } else { color(p.fg_dim) };
    painter.text(pos2(rect.left() + 42.0, y), Align2::LEFT_CENTER, label, ui_font(14.0), text_color);
    if let Some(n) = count {
        painter.text(
            pos2(rect.right() - 12.0, y),
            Align2::RIGHT_CENTER,
            n.to_string(),
            ui_font(12.0),
            color(p.comment),
        );
    }
    resp
}

/// Vim / Normal anahtarı; değişirse yeni değeri döndürür.
fn segmented(ui: &mut Ui, p: &Palette, current: Keymap) -> Option<Keymap> {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover());
    ui.painter().rect(rect, 7.0, color(p.bg), Stroke::new(1.0, color(p.border)), StrokeKind::Inside);
    let half = rect.width() / 2.0;
    let mut picked = None;
    for (i, opt) in [Keymap::Vim, Keymap::Normal].into_iter().enumerate() {
        let cell =
            Rect::from_min_size(rect.left_top() + vec2(half * i as f32, 0.0), vec2(half, rect.height())).shrink(3.0);
        let resp = ui.interact(cell, ui.scope_id().with(("keymap", i)), Sense::click());
        let on = opt == current;
        if on {
            ui.painter().rect_filled(cell, 5.0, color(p.bg_highlight));
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        let text_color = if on { color(p.fg) } else { color(p.comment) };
        ui.painter().text(cell.center(), Align2::CENTER_CENTER, opt.label(), ui_font(13.0), text_color);
        if resp.clicked() && !on {
            picked = Some(opt);
        }
    }
    picked
}

/// Projenin ad baş harfleri (`hedefnet-panel-frontend` → `HP`, `iflow` → `IF`).
fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split(['-', '_', ' ', '.']).filter(|w| !w.is_empty()).collect();
    let letters: String = match words.as_slice() {
        [one] => one.chars().take(2).collect(),
        [first, second, ..] => first.chars().take(1).chain(second.chars().take(1)).collect(),
        [] => "?".to_owned(),
    };
    letters.to_uppercase()
}

fn avatar_color(p: &Palette, name: &str) -> Color32 {
    let hash = name.bytes().fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
    color(p.avatars[hash as usize % p.avatars.len()])
}

/// Liste satırı: renkli baş harf kutusu, ad + tür etiketi, altında yol (workspace'te kökler), sağda zaman.
/// Klavyeyle seçili satır vurgulu zemin + solda vurgu çubuğuyla çizilir.
fn recent_row(ui: &mut Ui, p: &Palette, item: &Recent, now: i64, selected: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
    let painter = ui.painter();
    if selected || resp.hovered() {
        painter.rect_filled(rect, 8.0, color(p.bg_highlight));
    }
    if selected {
        let bar = Rect::from_min_size(rect.left_top() + vec2(0.0, 14.0), vec2(3.0, ROW_HEIGHT - 28.0));
        painter.rect_filled(bar, 2.0, color(p.accent));
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    let missing = match item.kind {
        Kind::Project => !item.dir.is_dir(),
        Kind::Workspace => !item.roots.iter().any(|r| r.is_dir()),
    };
    let tint = avatar_color(p, &item.name);
    let avatar = Rect::from_center_size(pos2(rect.left() + 30.0, rect.center().y), vec2(36.0, 36.0));
    painter.rect_filled(avatar, 8.0, tint.gamma_multiply(0.22));
    painter.text(avatar.center(), Align2::CENTER_CENTER, initials(&item.name), ui_font(14.0), tint);

    let x = avatar.right() + 14.0;
    let name_color = if missing { color(p.comment) } else { color(p.fg) };
    let name = painter.layout_no_wrap(item.name.clone(), ui_font(15.0), name_color);
    let mut badge_x = x + name.size().x + 10.0;
    painter.galley(pos2(x, rect.top() + 10.0), name, name_color);
    let mut badge = |text: &str, tint: Color32| {
        let galley = painter.layout_no_wrap(text.to_owned(), ui_font(11.0), tint);
        let r = Rect::from_min_size(pos2(badge_x, rect.top() + 11.0), galley.size() + vec2(14.0, 4.0));
        painter.rect_filled(r, 9.0, tint.gamma_multiply(0.16));
        painter.galley(r.left_top() + vec2(7.0, 2.0), galley, tint);
        badge_x = r.right() + 6.0;
    };
    if item.kind == Kind::Workspace {
        badge("workspace", color(p.avatars[1]));
    }
    if missing {
        badge("missing", color(p.error));
    }
    let detail = match item.kind {
        Kind::Workspace if !item.roots.is_empty() => {
            let names: Vec<String> = item
                .roots
                .iter()
                .map(|r| r.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
                .collect();
            format!("{} folders  ·  {}", names.len(), names.join(", "))
        }
        _ => recent::display_path(&item.dir),
    };
    painter.text(pos2(x, rect.top() + 34.0), Align2::LEFT_TOP, detail, ui_font(12.5), color(p.comment));
    let when = if item.time > 0 { recent::relative_time(item.time, now) } else { String::new() };
    let at = pos2(rect.right() - 16.0, rect.center().y);
    painter.text(at, Align2::RIGHT_CENTER, when, ui_font(12.0), color(p.comment));
    resp
}

/// Customize görünümü: tema kartları (tıkla = uygula ve kaydet), keymap kartları, font satırı, kod önizlemesi.
fn customize(ui: &mut Ui, p: &Palette, settings: &Settings, actions: &mut Vec<Action>) {
    heading(ui, p, "Customize", "Make LESS look and feel like yours. Changes are saved right away.");
    ui.add_space(26.0);

    section_label(ui, p, "COLOR THEME");
    ui.add_space(10.0);
    let gap = 12.0;
    let card_w = (ui.available_width() - gap * 3.0) / 4.0;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for theme in less_theme::ALL {
            if theme_card(ui, p, &theme, card_w, theme.id == p.id).clicked() && theme.id != p.id {
                actions.push(Action::SetTheme(theme.id));
            }
        }
    });
    ui.add_space(24.0);

    section_label(ui, p, "KEYMAP");
    ui.add_space(10.0);
    let half = (ui.available_width() - gap) / 2.0;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        let modes = [
            (Keymap::Vim, "Modal editing: Normal, Insert, Visual. The default."),
            (Keymap::Normal, "Modeless: type right away, Shift+arrows select."),
        ];
        for (mode, desc) in modes {
            if keymap_card(ui, p, mode, desc, half, settings.keymap == mode).clicked() && settings.keymap != mode {
                actions.push(Action::SetKeymap(mode));
            }
        }
    });
    ui.add_space(24.0);

    section_label(ui, p, "PREVIEW");
    ui.add_space(10.0);
    theme_preview(ui, p);
    ui.add_space(10.0);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::hover());
    let text = format!("{}  Editor font: {}, 14", icon::TEXT_AA, settings.font);
    ui.painter().text(rect.left_center(), Align2::LEFT_CENTER, text, ui_font(12.5), color(p.comment));
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if button(ui, p, Some(icon::PALETTE), "Choose Theme", false, true).clicked() {
            actions.push(Action::ChooseTheme);
        }
        if button(ui, p, Some(icon::TEXT_AA), "Choose Font", false, true).clicked() {
            actions.push(Action::ChooseFont);
        }
    });
}

/// Tema kartı: temanın kendi renkleriyle küçük bir editör çizimi ve altında adı; seçiliyse vurgu çerçeveli.
fn theme_card(ui: &mut Ui, p: &Palette, theme: &Palette, width: f32, selected: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 118.0), Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    let painter = ui.painter();
    let mock = Rect::from_min_size(rect.left_top(), vec2(width, 84.0));
    painter.rect_filled(mock, 8.0, color(theme.bg));
    let side = Rect::from_min_size(mock.left_top(), vec2(width * 0.28, mock.height()));
    painter.rect_filled(side, egui::CornerRadius { nw: 8, sw: 8, ne: 0, se: 0 }, color(theme.surface));
    let s = theme.syntax;
    let lines: [&[(f32, less_theme::Rgb)]; 4] = [
        &[(0.18, s.keyword), (0.28, s.ty)],
        &[(0.12, s.keyword), (0.22, s.function), (0.12, theme.fg_dim)],
        &[(0.30, theme.fg_dim), (0.16, s.string)],
        &[(0.20, s.comment)],
    ];
    for (i, line) in lines.iter().enumerate() {
        let mut x = side.right() + 10.0;
        let y = mock.top() + 16.0 + i as f32 * 15.0;
        for (frac, c) in line.iter() {
            let w = (width - side.width()) * frac;
            painter.rect_filled(Rect::from_min_size(pos2(x, y), vec2(w, 5.0)), 2.5, color(*c));
            x += w + 5.0;
        }
    }
    for i in 0..3 {
        let y = mock.top() + 16.0 + i as f32 * 15.0;
        let r = Rect::from_min_size(pos2(side.left() + 8.0, y), vec2(side.width() - 16.0, 5.0));
        painter.rect_filled(r, 2.5, color(theme.comment).gamma_multiply(0.7));
    }
    let stroke = if selected {
        Stroke::new(2.0, color(p.accent))
    } else if resp.hovered() {
        Stroke::new(1.0, color(p.fg_dim))
    } else {
        Stroke::new(1.0, color(p.border))
    };
    painter.rect_stroke(mock, 8.0, stroke, StrokeKind::Inside);
    let label_y = mock.bottom() + 17.0;
    let mut x = rect.left() + 2.0;
    if selected {
        painter.text(pos2(x, label_y), Align2::LEFT_CENTER, icon::CHECK, ui_font(14.0), color(p.accent));
        x += 20.0;
    }
    let text_color = if selected { color(p.fg) } else { color(p.fg_dim) };
    painter.text(pos2(x, label_y), Align2::LEFT_CENTER, theme.name, ui_font(13.5), text_color);
    resp
}

fn keymap_card(ui: &mut Ui, p: &Palette, mode: Keymap, desc: &str, width: f32, selected: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 64.0), Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    let fill = if resp.hovered() { color(p.bg_highlight) } else { color(p.surface) };
    let stroke = if selected { Stroke::new(2.0, color(p.accent)) } else { Stroke::new(1.0, color(p.border)) };
    let painter = ui.painter();
    painter.rect(rect, 9.0, fill, stroke, StrokeKind::Inside);
    let dot = pos2(rect.left() + 22.0, rect.top() + 22.0);
    painter.circle_stroke(dot, 7.0, Stroke::new(1.5, if selected { color(p.accent) } else { color(p.comment) }));
    if selected {
        painter.circle_filled(dot, 3.5, color(p.accent));
    }
    painter.text(
        pos2(rect.left() + 40.0, rect.top() + 22.0),
        Align2::LEFT_CENTER,
        mode.label(),
        ui_font(14.5),
        color(p.fg),
    );
    painter.text(
        pos2(rect.left() + 40.0, rect.top() + 44.0),
        Align2::LEFT_CENTER,
        desc,
        ui_font(12.0),
        color(p.comment),
    );
    resp
}

/// Temanın kod üzerinde nasıl göründüğünü gösteren küçük, sabit bir örnek.
fn theme_preview(ui: &mut Ui, p: &Palette) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 140.0), Sense::hover());
    let painter = ui.painter();
    painter.rect(rect, 10.0, color(p.bg_dark), Stroke::new(1.0, color(p.border)), StrokeKind::Inside);
    let s = p.syntax;
    let lines: [&[(&str, less_theme::Rgb)]; 5] = [
        &[("@SpringBootApplication", s.annotation)],
        &[("public class ", s.keyword), ("Application", s.ty), (" {", p.fg)],
        &[("    public static void ", s.keyword), ("main", s.function), ("(String[] args) {", p.fg)],
        &[("        SpringApplication.run(Application.class, args); ", p.fg), ("// less noise", s.comment)],
        &[("        log(", p.fg), ("\"ready in \"", s.string), (" + ", p.fg), ("42", s.number), (");", p.fg)],
    ];
    let font = FontId::monospace(13.0);
    for (i, line) in lines.iter().enumerate() {
        let y = rect.top() + 18.0 + i as f32 * 21.0;
        let num = format!("{:>2}", i + 1);
        painter.text(pos2(rect.left() + 18.0, y), Align2::LEFT_TOP, num, font.clone(), color(p.comment));
        let mut x = rect.left() + 52.0;
        for (text, c) in line.iter() {
            let galley = painter.layout_no_wrap((*text).to_owned(), font.clone(), color(*c));
            let w = galley.size().x;
            painter.galley(pos2(x, y), galley, color(*c));
            x += w;
        }
    }
}

fn shortcuts_view(ui: &mut Ui, p: &Palette) {
    heading(ui, p, "Shortcuts", "The keys LESS adds on top of the editor. Press Cmd K anywhere to search them.");
    ui.add_space(28.0);
    let col = (ui.available_width() - 24.0) / 2.0;
    let half = SHORTCUTS.len().div_ceil(2);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 24.0;
        for chunk in SHORTCUTS.chunks(half) {
            ui.vertical(|ui| {
                ui.set_width(col);
                for (keys, what) in chunk {
                    shortcut_row(ui, p, keys, what);
                }
            });
        }
    });
}

/// Kısayol satırı: solda açıklama, sağda tuşlar (her biri ayrı tuş kapağı).
pub fn shortcut_row(ui: &mut Ui, p: &Palette, keys: &str, what: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
    let painter = ui.painter();
    painter.hline(rect.left()..=rect.right(), rect.bottom(), Stroke::new(1.0, color(p.border)));
    painter.text(rect.left_center(), Align2::LEFT_CENTER, what, ui_font(14.0), color(p.fg_dim));
    keycaps(ui, p, keys, rect.right(), rect.center().y);
}

/// Tuşları sağdan sola tuş kapağı olarak çizer; `right` son kapağın sağ kenarıdır.
pub fn keycaps(ui: &Ui, p: &Palette, keys: &str, right: f32, y: f32) {
    let painter = ui.painter();
    let mut x = right;
    for key in keys.split(' ').rev() {
        let galley = painter.layout_no_wrap(key.to_owned(), FontId::monospace(12.0), color(p.fg));
        let w = galley.size().x + 14.0;
        let cap = Rect::from_min_size(pos2(x - w, y - 11.0), vec2(w, 22.0));
        painter.rect(cap, 5.0, color(p.surface), Stroke::new(1.0, color(p.border)), StrokeKind::Inside);
        painter.galley(cap.left_top() + vec2(7.0, (22.0 - galley.size().y) / 2.0), galley, color(p.fg));
        x = cap.left() - 6.0;
    }
}
