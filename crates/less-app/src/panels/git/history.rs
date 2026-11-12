//! Git History alt şeridi: renkli commit grafiği ve Local / Remote Branches (#58, #72).

use std::collections::HashSet;
use std::path::PathBuf;

use eframe::egui::{self, Align2, FontId, Rect, Sense, Ui, pos2, vec2};
use less_git::branches::{Branch, BranchList};
use less_git::graph::{self, Cell};
use less_git::log::{Commit, RefKind};
use less_theme::Palette;

use super::{Action, Cx, GitPanel, HistoryCommand};
use crate::widgets::{color, ui_font};

const ROW_H: f32 = 23.0;
const HEAD_H: f32 = 30.0;

#[derive(Default)]
pub struct History {
    pub repo: Option<PathBuf>,
    pub commits: Vec<Commit>,
    pub graph: Vec<Vec<Cell>>,
    pub branches: BranchList,
    pub all: bool,
    pub file: Option<String>,
    pub branch: Option<String>,
    pub error: Option<String>,
    pub cursor: Option<String>,
    pub branch_cursor: Option<String>,
    pub collapsed: HashSet<String>,
}

#[derive(Clone, Debug)]
pub enum BranchRow {
    Group { name: &'static str },
    Branch(Branch),
}

impl BranchRow {
    pub fn id(&self) -> String {
        match self {
            Self::Group { name, .. } => format!("group:{name}"),
            Self::Branch(branch) => format!("branch:{}", branch.name),
        }
    }
}

pub enum HistoryIntent {
    Commit(usize, bool),
    Branch(BranchRow, BranchGesture),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BranchGesture {
    Preview,
    Checkout,
    Menu,
}

pub fn branch_rows(history: &History) -> Vec<BranchRow> {
    let mut rows = vec![BranchRow::Group { name: "Local" }];
    if !history.collapsed.contains("group:Local") {
        rows.extend(history.branches.locals.iter().cloned().map(BranchRow::Branch));
    }
    if !history.branches.remotes.is_empty() {
        rows.push(BranchRow::Group { name: "Remote" });
        if !history.collapsed.contains("group:Remote") {
            rows.extend(history.branches.remotes.iter().cloned().map(BranchRow::Branch));
        }
    }
    rows
}

pub fn draw_branches(panel: &mut GitPanel, ui: &mut Ui, cx: &mut Cx) {
    let palette = cx.palette;
    ui.painter().rect_filled(ui.max_rect(), 0.0, color(palette.surface));
    header(ui, palette, "BRANCHES", cx.focused && panel.history_part == less_workbench::layout::StripPart::Branches);
    let rows = branch_rows(&panel.history);
    let selected =
        panel.history.branch_cursor.as_deref().and_then(|id| rows.iter().position(|r| r.id() == id)).unwrap_or(0);
    let mut intent = None;
    egui::ScrollArea::vertical().id_salt("git-branches").auto_shrink([false, false]).show_rows(
        ui,
        ROW_H,
        rows.len(),
        |ui, range| {
            for (index, row) in rows.iter().enumerate().take(range.end).skip(range.start) {
                let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                draw_branch_row(ui, palette, row, rect, index == selected, panel);
                if response.double_clicked() && matches!(row, BranchRow::Branch(_)) {
                    intent = Some(HistoryIntent::Branch(row.clone(), BranchGesture::Checkout));
                } else if response.clicked() {
                    intent = Some(HistoryIntent::Branch(row.clone(), BranchGesture::Preview));
                }
                if response.secondary_clicked() && matches!(row, BranchRow::Branch(_)) {
                    intent = Some(HistoryIntent::Branch(row.clone(), BranchGesture::Menu));
                }
            }
        },
    );
    if let Some(HistoryIntent::Branch(row, gesture)) = intent {
        panel.history.branch_cursor = Some(row.id());
        panel.branch_gesture(row, gesture, cx);
    }
}

pub fn draw_history(panel: &mut GitPanel, ui: &mut Ui, cx: &mut Cx) {
    let palette = cx.palette;
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0.0, color(palette.surface));
    let title = history_title(panel);
    header(ui, palette, &title, cx.focused && panel.history_part == less_workbench::layout::StripPart::History);
    if let Some(error) = &panel.history.error {
        ui.painter().text(
            rect.left_top() + vec2(14.0, HEAD_H + 14.0),
            Align2::LEFT_TOP,
            error,
            ui_font(12.0),
            color(palette.error),
        );
        return;
    }
    if panel.history.commits.is_empty() {
        ui.painter().text(
            rect.left_top() + vec2(14.0, HEAD_H + 14.0),
            Align2::LEFT_TOP,
            "No commits found",
            ui_font(12.5),
            color(palette.comment),
        );
        return;
    }
    let graph_width = panel.history.graph.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let selected = panel
        .history
        .cursor
        .as_deref()
        .and_then(|hash| panel.history.commits.iter().position(|c| c.hash == hash))
        .unwrap_or(0);
    let mut intent = None;
    egui::ScrollArea::both().id_salt("git-history").auto_shrink([false, false]).show_rows(
        ui,
        ROW_H,
        panel.history.commits.len(),
        |ui, range| {
            for index in range {
                let Some(commit) = panel.history.commits.get(index) else { continue };
                let width = ui.available_width().max(650.0 + graph_width as f32 * 8.0);
                let (rect, response) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::click());
                draw_commit(ui, palette, panel, commit, index, rect, index == selected, graph_width);
                if response.double_clicked() {
                    intent = Some(HistoryIntent::Commit(index, true));
                } else if response.clicked() {
                    intent = Some(HistoryIntent::Commit(index, false));
                }
            }
        },
    );
    if let Some(HistoryIntent::Commit(index, open)) = intent {
        if let Some(commit) = panel.history.commits.get(index) {
            panel.history.cursor = Some(commit.hash.clone());
        }
        if open {
            panel.open_history_commit(index, cx);
        }
    }
}

fn history_title(panel: &GitPanel) -> String {
    let mode = if panel.history.all {
        "all branches".to_owned()
    } else if let Some(branch) = &panel.history.branch {
        format!("branch {branch}")
    } else {
        "current branch".to_owned()
    };
    let file = panel.history.file.as_ref().map(|f| format!(" · file {f}")).unwrap_or_default();
    let repo = panel.history.repo.as_ref().and_then(|r| r.file_name()).map(|n| n.to_string_lossy()).unwrap_or_default();
    format!("GIT HISTORY · {repo} · {mode}{file}")
}

fn header(ui: &mut Ui, palette: &Palette, title: &str, focused: bool) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), HEAD_H), Sense::hover());
    let tint = if focused { palette.accent } else { palette.comment };
    ui.painter().text(rect.left_center() + vec2(12.0, 0.0), Align2::LEFT_CENTER, title, ui_font(11.0), color(tint));
}

fn draw_branch_row(ui: &mut Ui, palette: &Palette, row: &BranchRow, rect: Rect, selected: bool, panel: &GitPanel) {
    if selected {
        ui.painter().rect_filled(rect.shrink2(vec2(3.0, 1.0)), 4.0, color(palette.bg_highlight));
    }
    let painter = ui.painter_at(rect);
    let y = rect.center().y;
    match row {
        BranchRow::Group { name, .. } => {
            let open = !panel.history.collapsed.contains(&row.id());
            let text = format!("{} {name}", if open { "▾" } else { "▸" });
            painter.text(pos2(rect.left() + 12.0, y), Align2::LEFT_CENTER, text, ui_font(12.0), color(palette.fg_dim));
        }
        BranchRow::Branch(branch) => {
            let current = if branch.current { "●" } else { " " };
            painter.text(
                pos2(rect.left() + 16.0, y),
                Align2::LEFT_CENTER,
                current,
                ui_font(11.0),
                color(palette.accent),
            );
            let showing = panel.history.branch.as_deref() == Some(&branch.name)
                || panel.history.branch.is_none() && branch.current && !panel.history.all;
            let tint = if showing {
                palette.accent
            } else if branch.remote.is_some() {
                palette.comment
            } else {
                palette.fg
            };
            painter.text(pos2(rect.left() + 34.0, y), Align2::LEFT_CENTER, &branch.name, ui_font(12.0), color(tint));
            if let Some(track) = &branch.track {
                painter.text(
                    pos2(rect.right() - 8.0, y),
                    Align2::RIGHT_CENTER,
                    track,
                    ui_font(11.0),
                    color(palette.comment),
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_commit(
    ui: &mut Ui,
    palette: &Palette,
    panel: &GitPanel,
    commit: &Commit,
    index: usize,
    rect: Rect,
    selected: bool,
    graph_width: usize,
) {
    if selected {
        ui.painter().rect_filled(rect.shrink2(vec2(3.0, 1.0)), 4.0, color(palette.bg_highlight));
    }
    let painter = ui.painter_at(rect);
    let font = FontId::monospace(11.5);
    let cell_w = 7.5;
    let y = rect.center().y;
    let mut x = rect.left() + 8.0;
    if let Some(cells) = panel.history.graph.get(index) {
        for (i, cell) in cells.iter().enumerate() {
            if cell.ch != ' ' {
                let tint = cell.color.map_or(palette.comment, |n| palette.git_lane(n.saturating_sub(1) as usize));
                painter.text(pos2(x + i as f32 * cell_w, y), Align2::LEFT_CENTER, cell.ch, font.clone(), color(tint));
            }
        }
    }
    x += graph_width as f32 * cell_w + 10.0;
    painter.text(pos2(x, y), Align2::LEFT_CENTER, &commit.short, font.clone(), color(palette.accent));
    x += 72.0;
    painter.text(pos2(x, y), Align2::LEFT_CENTER, &commit.date, font.clone(), color(palette.comment));
    x += 88.0;
    painter.text(pos2(x, y), Align2::LEFT_CENTER, clip(&commit.author, 16), font.clone(), color(palette.extra.info));
    x += 132.0;
    painter.text(pos2(x, y), Align2::LEFT_CENTER, &commit.subject, font.clone(), color(palette.fg));
    x += painter.layout_no_wrap(commit.subject.clone(), font.clone(), color(palette.fg)).size().x + 12.0;
    for reference in &commit.refs {
        let (glyph, tint) = match reference.kind {
            RefKind::Head => ("HEAD ", palette.accent),
            RefKind::Local => ("⎇ ", palette.extra.diff_add),
            RefKind::Remote => ("☁ ", palette.extra.info),
            RefKind::Tag => ("◆ ", palette.extra.warn),
        };
        let text = format!("{glyph}{}", reference.name);
        painter.text(pos2(x, y), Align2::LEFT_CENTER, text.clone(), font.clone(), color(tint));
        x += painter.layout_no_wrap(text, font.clone(), color(tint)).size().x + 8.0;
    }
}

fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        format!("{text:max$}")
    } else {
        text.chars().take(max.saturating_sub(1)).chain(['…']).collect()
    }
}

pub fn branch_menu(branch: &Branch) -> super::Prompt {
    let mut entries = vec![super::MenuEntry {
        label: "Show history".into(),
        action: Action::History(HistoryCommand::PreviewBranch(branch.clone())),
    }];
    if !branch.current {
        entries.push(super::MenuEntry {
            label: "Checkout…".into(),
            action: Action::History(HistoryCommand::Checkout(branch.clone())),
        });
    }
    entries.push(super::MenuEntry { label: "Copy name".into(), action: Action::Copy(branch.name.clone()) });
    super::Prompt::menu(branch.name.clone(), entries)
}

pub fn graph_for(commits: &[Commit]) -> Vec<Vec<Cell>> {
    graph::layout(commits)
}
