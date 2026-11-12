//! Database panelinin formları, menüleri ve açık insan onayı pencereleri (#46 #68 #88).

use eframe::egui::{self, Align2, Context, FontId, Id, RichText, ScrollArea, Sense, TextEdit, Ui, vec2};
use less_db::{Connection, ConnectionStore, DbType};
use less_input::{Key, KeyEvent};
use less_theme::Palette;

use super::tree::{Action, Row};
use crate::widgets::{self, color, ui_font};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionForm {
    pub original: Option<String>,
    pub ty: DbType,
    pub name: String,
    pub host: String,
    pub port: String,
    pub user: String,
    pub password: String,
    pub database: String,
    pub options: String,
    pub file: String,
    pub error: Option<String>,
}

impl ConnectionForm {
    pub fn new(ty: DbType) -> Self {
        Self {
            original: None,
            ty,
            name: String::new(),
            host: "localhost".into(),
            port: ty.default_port().map(|p| p.to_string()).unwrap_or_default(),
            user: String::new(),
            password: String::new(),
            database: String::new(),
            options: String::new(),
            file: String::new(),
            error: None,
        }
    }

    pub fn edit(connection: &Connection) -> Self {
        Self {
            original: Some(connection.name.clone()),
            ty: connection.ty,
            name: connection.name.clone(),
            host: connection.host.clone().unwrap_or_else(|| "localhost".into()),
            port: connection.port.or_else(|| connection.ty.default_port()).map(|p| p.to_string()).unwrap_or_default(),
            user: connection.user.clone().unwrap_or_default(),
            password: connection.password.clone().unwrap_or_default(),
            database: connection.database.clone().unwrap_or_default(),
            options: connection.options.clone().unwrap_or_default(),
            file: connection.file.clone().unwrap_or_default(),
            error: None,
        }
    }

    pub fn title(&self) -> String {
        format!("{} {} connection", if self.original.is_some() { "Edit" } else { "New" }, self.ty.display_name())
    }

    /// Alanları doğrular ve kayda çevirir. Dönen ilk değer varsa eski bağlantı adıdır.
    pub fn submit(&self, store: &ConnectionStore) -> Result<(Option<String>, Connection), String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err("Name is required".into());
        }
        if self.original.as_deref() != Some(name) && store.get(name).is_some() {
            return Err(format!("A connection named “{name}” already exists"));
        }
        if self.ty.is_file_based() && self.file.trim().is_empty() {
            return Err("File is required".into());
        }
        let port = match self.port.trim() {
            "" => None,
            value => Some(
                value
                    .parse::<u16>()
                    .ok()
                    .filter(|port| *port > 0)
                    .ok_or_else(|| "Port must be a number between 1 and 65535".to_string())?,
            ),
        };
        let text = |value: &str| {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_string())
        };
        let mut connection = Connection::new(name, self.ty);
        connection.host = text(&self.host);
        connection.port = port;
        connection.user = text(&self.user);
        connection.password = text(&self.password);
        connection.database = text(&self.database);
        connection.options = (self.ty.kind() != less_db::types::Kind::Redis).then(|| text(&self.options)).flatten();
        connection.file = text(&self.file);
        Ok((self.original.clone(), connection))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ApprovalKind {
    Install { ty: DbType },
    Delete { name: String },
    Query { connection: String, sql: String },
    Commit { key: String, statements: Vec<String> },
    CloseTab { key: String },
    Quit,
}

#[derive(Clone, Debug)]
pub enum Popup {
    Types { sel: usize },
    Form(ConnectionForm),
    Menu { title: String, entries: Vec<(&'static str, Action)>, sel: usize },
    Approval { kind: ApprovalKind, title: String, preview: String, confirm: &'static str, yes: bool },
    CellEdit { key: String, row: usize, col: usize, title: String, value: String },
    Value { title: String, value: String },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Keep,
    Close,
    Form,
    Type(DbType),
    Action(Action),
    Approve(bool),
    CellApply(String),
}

impl Popup {
    pub fn menu(row: Option<&Row>) -> Self {
        let title = row.map(|r| r.label.clone()).unwrap_or_else(|| "Database".into());
        Self::Menu { title, entries: super::tree::menu(row), sel: 0 }
    }

    pub fn approval(
        kind: ApprovalKind,
        title: impl Into<String>,
        preview: impl Into<String>,
        confirm: &'static str,
    ) -> Self {
        Self::Approval { kind, title: title.into(), preview: preview.into(), confirm, yes: false }
    }

    /// Pencere açıkken tuş; tüm onaylarda Esc/No güvenli iptaldir.
    pub fn key(&mut self, k: &KeyEvent) -> Outcome {
        if k.key == Key::Esc {
            return Outcome::Close;
        }
        let plain = !k.mods.ctrl && !k.mods.alt && !k.mods.cmd;
        match self {
            Popup::Types { sel } => match k.key {
                Key::Up | Key::Left => {
                    *sel = sel.saturating_sub(1);
                    Outcome::Keep
                }
                Key::Down | Key::Right => {
                    *sel = (*sel + 1).min(DbType::ALL.len() - 1);
                    Outcome::Keep
                }
                Key::Enter => Outcome::Type(DbType::ALL[*sel]),
                Key::Char('q') if plain => Outcome::Close,
                _ => Outcome::Keep,
            },
            Popup::Form(_) => match k.key {
                Key::Enter => Outcome::Form,
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
                Key::Enter => entries.get(*sel).map(|(_, a)| Outcome::Action(a.clone())).unwrap_or(Outcome::Close),
                Key::Char('q') if plain => Outcome::Close,
                _ => Outcome::Keep,
            },
            Popup::Approval { yes, .. } => match k.key {
                Key::Char('y' | 'Y') if plain => Outcome::Approve(true),
                Key::Char('n' | 'N') if plain => Outcome::Approve(false),
                Key::Left | Key::Right | Key::Up | Key::Down | Key::Tab => {
                    *yes = !*yes;
                    Outcome::Keep
                }
                Key::Enter => Outcome::Approve(*yes),
                _ => Outcome::Keep,
            },
            Popup::CellEdit { value, .. } => match k.key {
                Key::Enter if k.mods.cmd || (k.mods.ctrl && !value.contains('\n')) => Outcome::CellApply(value.clone()),
                Key::Enter if !value.contains('\n') => Outcome::CellApply(value.clone()),
                Key::Char('s') if k.mods.ctrl => Outcome::CellApply(value.clone()),
                _ => Outcome::Keep,
            },
            Popup::Value { .. } => match k.key {
                Key::Enter => Outcome::Close,
                Key::Char('q') if plain => Outcome::Close,
                _ => Outcome::Keep,
            },
        }
    }

    pub fn show(&mut self, ctx: &Context, p: &Palette) -> Outcome {
        let mut out = Outcome::Keep;
        let response = egui::Modal::new(Id::unique("db-popup")).frame(frame(p)).show(ctx, |ui| match self {
            Popup::Types { sel } => {
                title(ui, p, "New data source");
                for (i, ty) in DbType::ALL.into_iter().enumerate() {
                    if menu_row(ui, p, ty.display_name(), i == *sel) {
                        out = Outcome::Type(ty);
                    }
                }
            }
            Popup::Form(form) => {
                let form_out = show_form(ui, p, form);
                if form_out != Outcome::Keep {
                    out = form_out;
                }
            }
            Popup::Menu { title: heading, entries, sel } => {
                title(ui, p, &heading.clone());
                for (i, (label, action)) in entries.iter().enumerate() {
                    if menu_row(ui, p, label, i == *sel) {
                        out = Outcome::Action(action.clone());
                    }
                }
            }
            Popup::Approval { title: heading, preview, confirm, yes, .. } => {
                title(ui, p, &heading.clone());
                ui.set_width(560.0);
                if !preview.is_empty() {
                    ScrollArea::both().id_salt("db-approval-preview").max_height(260.0).show(ui, |ui| {
                        ui.add(
                            TextEdit::multiline(&mut preview.as_str())
                                .font(FontId::monospace(12.5))
                                .text_color(color(p.fg))
                                .interactive(false)
                                .desired_width(520.0)
                                .frame(egui::Frame::NONE),
                        );
                    });
                    ui.add_space(12.0);
                }
                ui.horizontal(|ui| {
                    if widgets::button(ui, p, None, confirm, *yes, true).clicked() {
                        out = Outcome::Approve(true);
                    }
                    if widgets::button(ui, p, None, "Cancel", !*yes, true).clicked() {
                        out = Outcome::Approve(false);
                    }
                });
            }
            Popup::CellEdit { title: heading, value, .. } => {
                title(ui, p, &heading.clone());
                ui.set_width(500.0);
                let multiline = value.contains('\n');
                let edit = if multiline {
                    TextEdit::multiline(value).desired_rows(8)
                } else {
                    TextEdit::singleline(value).desired_rows(1)
                };
                let response = ui.add(edit.font(FontId::monospace(13.0)).text_color(color(p.fg)).desired_width(500.0));
                if !response.has_focus() && !response.lost_focus() {
                    response.request_focus();
                }
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if widgets::button(ui, p, None, "Apply", true, true).clicked() {
                        out = Outcome::CellApply(value.clone());
                    }
                    if widgets::button(ui, p, None, "Cancel", false, true).clicked() {
                        out = Outcome::Close;
                    }
                });
            }
            Popup::Value { title: heading, value } => {
                title(ui, p, &heading.clone());
                ui.set_width(560.0);
                ScrollArea::both().id_salt("db-full-value").max_height(360.0).show(ui, |ui| {
                    ui.add(
                        TextEdit::multiline(&mut value.as_str())
                            .font(FontId::monospace(13.0))
                            .text_color(color(p.fg))
                            .interactive(false)
                            .desired_width(520.0)
                            .frame(egui::Frame::NONE),
                    );
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if widgets::button(ui, p, Some(crate::fonts::icon::COPY), "Copy", false, true).clicked() {
                        ctx.copy_text(value.clone());
                    }
                    if widgets::button(ui, p, None, "Close", true, true).clicked() {
                        out = Outcome::Close;
                    }
                });
            }
        });
        if response.backdrop_response.clicked() && out == Outcome::Keep {
            out = Outcome::Close;
        }
        out
    }
}

fn show_form(ui: &mut Ui, p: &Palette, form: &mut ConnectionForm) -> Outcome {
    title(ui, p, &form.title());
    ui.set_width(500.0);
    field(ui, p, "Name", "Connection name", &mut form.name, false, true);
    if form.ty.is_file_based() {
        field(ui, p, "File", "Path to the database file", &mut form.file, false, false);
    } else {
        field(ui, p, "Host", "localhost", &mut form.host, false, false);
        field(ui, p, "Port", "Port", &mut form.port, false, false);
        field(ui, p, "User", "User", &mut form.user, false, false);
        field(ui, p, "Password", "Password", &mut form.password, true, false);
        let database = if form.ty == DbType::Redis { "DB index" } else { "Database" };
        field(ui, p, database, database, &mut form.database, false, false);
        if form.ty.kind() != less_db::types::Kind::Redis {
            field(ui, p, "Options", "e.g. sslmode=require", &mut form.options, false, false);
        }
    }
    if let Some(error) = &form.error {
        ui.label(RichText::new(error).color(color(p.error)).font(ui_font(12.5)));
    }
    ui.add_space(10.0);
    let mut out = Outcome::Keep;
    ui.horizontal(|ui| {
        if widgets::button(ui, p, None, "Save", true, true).clicked() {
            out = Outcome::Form;
        }
        if widgets::button(ui, p, None, "Cancel", false, true).clicked() {
            out = Outcome::Close;
        }
    });
    out
}

fn field(ui: &mut Ui, p: &Palette, label: &str, hint: &str, value: &mut String, password: bool, focus: bool) {
    widgets::section_label(ui, p, label);
    let id = Id::unique("db-form-field").with(label);
    if password {
        let response = ui.add(
            TextEdit::singleline(value)
                .id(id)
                .password(true)
                .hint_text(hint)
                .font(ui_font(13.5))
                .text_color(color(p.fg))
                .desired_width(500.0),
        );
        if focus {
            response.request_focus();
        }
    } else {
        widgets::text_input(ui, p, id, value, hint, None, 500.0, focus);
    }
    ui.add_space(5.0);
}

fn menu_row(ui: &mut Ui, p: &Palette, label: &str, selected: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width().max(300.0), 28.0), Sense::click());
    if selected || response.hovered() {
        ui.painter().rect_filled(rect, 5.0, color(p.bg_highlight));
    }
    ui.painter().text(rect.left_center() + vec2(10.0, 0.0), Align2::LEFT_CENTER, label, ui_font(13.0), color(p.fg));
    response.clicked()
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
    ui.add_space(10.0);
}
