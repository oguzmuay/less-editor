//! Git sol panelinin satır modeli ve çizimi (#35).

use std::path::PathBuf;

use eframe::egui::{self, Align2, Rect, Sense, Ui, pos2, vec2};
use less_git::status::{Entry, Kind, Section, Status};

use super::{Action, Cx, GitPanel, StatusCommand};
use crate::widgets::{color, ui_font};

pub const ROW_H: f32 = 23.0;

#[derive(Clone, Debug)]
pub struct RepoStatus {
    pub root: PathBuf,
    pub result: Result<Status, String>,
    pub track: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StatusRow {
    Repo(PathBuf),
    Section { repo: PathBuf, section: Section, entries: Vec<Entry> },
    File { repo: PathBuf, entry: Entry },
}

impl StatusRow {
    pub fn id(&self) -> String {
        match self {
            Self::Repo(repo) => format!("repo:{}", repo.display()),
            Self::Section { repo, section, .. } => format!("section:{}:{section:?}", repo.display()),
            Self::File { repo, entry } => format!("file:{}:{:?}:{}", repo.display(), entry.section, entry.path),
        }
    }

    pub fn repo(&self) -> &PathBuf {
        match self {
            Self::Repo(repo) | Self::Section { repo, .. } | Self::File { repo, .. } => repo,
        }
    }
}

pub fn rows(repos: &[RepoStatus], collapsed: &std::collections::HashSet<String>) -> Vec<StatusRow> {
    let mut out = Vec::new();
    for repo in repos {
        out.push(StatusRow::Repo(repo.root.clone()));
        let Ok(status) = &repo.result else { continue };
        for (section, entries) in sections(status) {
            if entries.is_empty() {
                continue;
            }
            let row = StatusRow::Section { repo: repo.root.clone(), section, entries: entries.to_vec() };
            let id = row.id();
            out.push(row);
            if collapsed.contains(&id) {
                continue;
            }
            out.extend(entries.iter().cloned().map(|entry| StatusRow::File { repo: repo.root.clone(), entry }));
        }
    }
    out
}

pub fn sections(status: &Status) -> [(Section, &[Entry]); 4] {
    [
        (Section::Conflicts, &status.conflicts),
        (Section::Staged, &status.staged),
        (Section::Changes, &status.changes),
        (Section::Untracked, &status.untracked),
    ]
}

pub fn section_name(section: Section) -> &'static str {
    match section {
        Section::Conflicts => "Conflicts",
        Section::Staged => "Staged",
        Section::Changes => "Changes",
        Section::Untracked => "Untracked",
    }
}

pub fn kind_mark(kind: Kind) -> &'static str {
    match kind {
        Kind::Added | Kind::Untracked => "A",
        Kind::Modified => "M",
        Kind::Deleted => "D",
        Kind::Renamed => "R",
        Kind::Copied => "C",
        Kind::TypeChanged => "T",
        Kind::Conflict => "!",
    }
}

pub enum Intent {
    Select(String),
    Activate(StatusRow),
    Menu(StatusRow),
}

pub fn draw(panel: &mut GitPanel, ui: &mut Ui, cx: &mut Cx) {
    let palette = cx.palette;
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0.0, color(palette.surface));
    header(ui, palette, cx.focused);
    let rows = panel.status_rows();
    if rows.is_empty() {
        ui.painter().text(
            rect.left_top() + vec2(14.0, 46.0),
            Align2::LEFT_TOP,
            "No Git repository found",
            ui_font(12.5),
            color(palette.comment),
        );
        return;
    }
    let cursor = panel.status_cursor_index(&rows);
    let mut intent = None;
    egui::ScrollArea::vertical().id_salt("git-status").auto_shrink([false, false]).show_rows(
        ui,
        ROW_H,
        rows.len(),
        |ui, range| {
            for (index, row) in rows.iter().enumerate().take(range.end).skip(range.start) {
                let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                draw_row(ui, palette, row, rect, index == cursor, cx.focused, panel);
                if response.double_clicked() {
                    intent = Some(Intent::Activate(row.clone()));
                } else if response.clicked() {
                    intent = Some(Intent::Select(row.id()));
                }
                if response.secondary_clicked() {
                    intent = Some(Intent::Menu(row.clone()));
                }
            }
        },
    );
    match intent {
        Some(Intent::Select(id)) => panel.status_cursor = Some(id),
        Some(Intent::Activate(row)) => {
            panel.status_cursor = Some(row.id());
            panel.activate_status(row, cx);
        }
        Some(Intent::Menu(row)) => {
            panel.status_cursor = Some(row.id());
            panel.prompt = Some(panel.status_menu(&row));
        }
        None => {}
    }
}

fn header(ui: &mut Ui, palette: &less_theme::Palette, focused: bool) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
    let tint = if focused { palette.accent } else { palette.comment };
    ui.painter().text(rect.left_center() + vec2(12.0, 0.0), Align2::LEFT_CENTER, "GIT", ui_font(11.0), color(tint));
    ui.painter().text(
        rect.right_center() - vec2(10.0, 0.0),
        Align2::RIGHT_CENTER,
        "s stage · u unstage · d diff · X discard",
        ui_font(10.5),
        color(palette.comment),
    );
}

fn draw_row(
    ui: &mut Ui,
    palette: &less_theme::Palette,
    row: &StatusRow,
    rect: Rect,
    selected: bool,
    focused: bool,
    panel: &GitPanel,
) {
    let collapsed = panel.collapsed.contains(&row.id());
    if selected {
        let fill = if focused { color(palette.accent).gamma_multiply(0.30) } else { color(palette.bg_highlight) };
        ui.painter().rect_filled(rect.shrink2(vec2(3.0, 1.0)), 4.0, fill);
    } else if ui.rect_contains_pointer(rect) {
        ui.painter().rect_filled(rect, 0.0, color(palette.bg_highlight).gamma_multiply(0.45));
    }
    let painter = ui.painter_at(rect);
    let y = rect.center().y;
    match row {
        StatusRow::Repo(repo) => {
            let name =
                repo.file_name().map(|n| n.to_string_lossy()).unwrap_or_else(|| repo.as_os_str().to_string_lossy());
            let status = panel.statuses.iter().find(|status| &status.root == repo);
            let detail = match status.map(|status| &status.result) {
                Some(Ok(status)) => {
                    let track =
                        panel.statuses.iter().find(|item| &item.root == repo).and_then(|item| item.track.as_deref());
                    format!(" · ⎇ {}{}", status.branch, track.map(|track| format!(" {track}")).unwrap_or_default())
                }
                Some(Err(error)) => format!(" · {error}"),
                None => String::new(),
            };
            painter.text(
                pos2(rect.left() + 10.0, y),
                Align2::LEFT_CENTER,
                format!("{name}{detail}"),
                ui_font(12.5),
                color(palette.fg),
            );
        }
        StatusRow::Section { section, entries, .. } => {
            let caret = if collapsed { "▸" } else { "▾" };
            painter.text(
                pos2(rect.left() + 12.0, y),
                Align2::LEFT_CENTER,
                caret,
                ui_font(11.0),
                color(palette.comment),
            );
            let text = format!("{} ({})", section_name(*section), entries.len());
            painter.text(pos2(rect.left() + 30.0, y), Align2::LEFT_CENTER, text, ui_font(12.0), color(palette.fg_dim));
        }
        StatusRow::File { entry, .. } => {
            let tint = match entry.section {
                Section::Conflicts => palette.error,
                Section::Staged => palette.extra.diff_add,
                Section::Changes | Section::Untracked => palette.extra.warn,
            };
            painter.text(
                pos2(rect.left() + 42.0, y),
                Align2::LEFT_CENTER,
                kind_mark(entry.kind),
                egui::FontId::monospace(12.0),
                color(tint),
            );
            painter.text(
                pos2(rect.left() + 60.0, y),
                Align2::LEFT_CENTER,
                &entry.path,
                egui::FontId::monospace(12.0),
                color(palette.fg),
            );
        }
    }
}

pub fn menu_entries(row: &StatusRow) -> Vec<super::MenuEntry> {
    use super::MenuEntry;
    let mut entries = Vec::new();
    let mut add = |label: &str, action| entries.push(MenuEntry { label: label.to_owned(), action });
    match row {
        StatusRow::File { entry, .. } => {
            add("Open file", Action::Status(StatusCommand::Open, row.clone()));
            add("Show diff", Action::Status(StatusCommand::Diff, row.clone()));
            if entry.section == Section::Staged {
                add("Unstage", Action::Status(StatusCommand::Unstage, row.clone()));
            } else {
                let label = if entry.section == Section::Conflicts { "Mark as resolved (stage)" } else { "Stage" };
                add(label, Action::Status(StatusCommand::Stage, row.clone()));
            }
            match entry.section {
                Section::Changes => add("Discard changes…", Action::Status(StatusCommand::Discard, row.clone())),
                Section::Untracked => add("Delete file…", Action::Status(StatusCommand::Discard, row.clone())),
                _ => {}
            }
        }
        StatusRow::Section { section, .. } => {
            if *section == Section::Staged {
                add("Unstage section", Action::Status(StatusCommand::Unstage, row.clone()));
            } else {
                add("Stage section", Action::Status(StatusCommand::Stage, row.clone()));
            }
        }
        StatusRow::Repo(_) => {}
    }
    add("Stage all", Action::Status(StatusCommand::StageAll, row.clone()));
    add("Unstage all", Action::Status(StatusCommand::UnstageAll, row.clone()));
    add("Refresh", Action::RefreshStatus);
    entries
}
