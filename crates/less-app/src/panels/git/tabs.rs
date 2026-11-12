//! Git'in özel kod tab'ları: unified diff (#17), yan yana karşılaştırma (#85), merge (#86) ve blame görünümü (#22).

use std::path::PathBuf;

use eframe::egui::{self, Align2, FontId, Rect, Sense, TextEdit, Ui, pos2, vec2};
use less_git::blame::BlameLine;
use less_git::compare::{self, Block};
use less_git::diff::{DiffDoc, LineKind, Source};
use less_git::merge::{self, Choice};
use less_theme::Palette;

use super::Cx;
use crate::widgets::{color, ui_font};

const ROW_H: f32 = 21.0;
const FONT: f32 = 12.0;

pub enum TabState {
    Diff(DiffView),
    Compare(CompareView),
    Merge(MergeView),
    Blame(BlameView),
}

pub struct DiffView {
    pub repo: PathBuf,
    pub source: Source,
    pub doc: DiffDoc,
    pub cursor: usize,
}

pub struct CompareView {
    pub repo: PathBuf,
    pub path: String,
    pub base: String,
    pub current: String,
    pub cursor: usize,
    pub left: bool,
    pub undo: Vec<String>,
    pub dirty: bool,
    pub editing: bool,
}

pub struct MergeView {
    pub repo: PathBuf,
    pub path: String,
    pub ours: String,
    pub theirs: String,
    pub result: String,
    pub selected: usize,
    pub undo: Vec<String>,
}

pub struct BlameView {
    pub repo: PathBuf,
    pub file: PathBuf,
    pub source: String,
    pub lines: Vec<Option<BlameLine>>,
    pub cursor: usize,
}

#[derive(Clone, Debug)]
pub enum Intent {
    Close,
    DiffJump(usize),
    CompareRollback(usize),
    CompareRollbackAll,
    CompareSave,
    MergeChoice(usize, Choice),
    MergeAll(Choice),
    MergeResolve,
    BlameCommit(String),
}

impl TabState {
    pub fn show(&mut self, ui: &mut Ui, cx: &mut Cx) -> Vec<Intent> {
        match self {
            Self::Diff(tab) => diff_ui(tab, ui, cx.palette),
            Self::Compare(tab) => compare_ui(tab, ui, cx.palette),
            Self::Merge(tab) => merge_ui(tab, ui, cx.palette),
            Self::Blame(tab) => blame_ui(tab, ui, cx.palette),
        }
    }
}

fn diff_ui(tab: &mut DiffView, ui: &mut Ui, palette: &Palette) -> Vec<Intent> {
    let mut intents = Vec::new();
    toolbar(ui, palette, "Unified diff (read-only)", "Enter jumps to the file; R refreshes; q closes");
    if ui.button("Close").clicked() {
        intents.push(Intent::Close);
    }
    egui::ScrollArea::both().id_salt("git-diff-tab").auto_shrink([false, false]).show_rows(
        ui,
        ROW_H,
        tab.doc.lines.len(),
        |ui, range| {
            for index in range {
                let Some(line) = tab.doc.lines.get(index) else { continue };
                let width = ui.available_width().max(900.0);
                let (rect, response) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::click());
                if index == tab.cursor {
                    ui.painter().rect_filled(rect, 0.0, color(palette.bg_highlight));
                }
                let tint = match line.kind {
                    LineKind::Add => palette.extra.diff_add,
                    LineKind::Remove => palette.error,
                    LineKind::Hunk => palette.extra.info,
                    LineKind::Header | LineKind::FileHeader => palette.accent,
                    LineKind::Stat => palette.extra.warn,
                    LineKind::Context => palette.fg_dim,
                };
                ui.painter().text(
                    rect.left_center() + vec2(10.0, 0.0),
                    Align2::LEFT_CENTER,
                    &line.text,
                    FontId::monospace(FONT),
                    color(tint),
                );
                if response.double_clicked() {
                    tab.cursor = index;
                    intents.push(Intent::DiffJump(index));
                } else if response.clicked() {
                    tab.cursor = index;
                }
            }
        },
    );
    intents
}

fn compare_ui(tab: &mut CompareView, ui: &mut Ui, palette: &Palette) -> Vec<Intent> {
    let mut intents = Vec::new();
    let blocks = compare::blocks(&tab.base, &tab.current);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{} change(s){}", blocks.len(), if tab.dirty { " · modified" } else { "" }))
                .color(color(palette.comment))
                .font(ui_font(12.0)),
        );
        if ui.button("⟲ Rollback all").clicked() {
            intents.push(Intent::CompareRollbackAll);
        }
        if ui.button(if tab.editing { "Changes" } else { "Edit" }).clicked() {
            tab.editing = !tab.editing;
        }
        if ui.button("Save").on_hover_text("Write the working-tree file (Cmd+S)").clicked() {
            intents.push(Intent::CompareSave);
        }
        if ui.button("✕ Close").clicked() {
            intents.push(Intent::Close);
        }
    });
    ui.label(
        egui::RichText::new("HEAD (read-only)  │  Working tree · Space r rollback · u undo · e edit · ]c/[c navigate")
            .color(color(palette.comment))
            .font(ui_font(11.0)),
    );
    if tab.editing {
        ui.horizontal_wrapped(|ui| {
            for (index, _) in blocks.iter().enumerate() {
                if ui.button(format!("⟲ Change {}", index + 1)).clicked() {
                    intents.push(Intent::CompareRollback(index));
                }
            }
        });
        let height = ui.available_height().max(100.0);
        let before = tab.current.clone();
        ui.columns(2, |columns| {
            merge_pane(&mut columns[0], "HEAD (read-only)", &mut tab.base.clone(), height, true);
            merge_pane(&mut columns[1], "Working tree (editable)", &mut tab.current, height, false);
        });
        tab.dirty |= tab.current != before;
        return intents;
    }
    let left: Vec<&str> = tab.base.lines().collect();
    let right: Vec<&str> = tab.current.lines().collect();
    let rows = left.len().max(right.len()).max(1);
    let mut clicked_block = None;
    egui::ScrollArea::both().id_salt("git-compare-tab").auto_shrink([false, false]).show_rows(
        ui,
        ROW_H,
        rows,
        |ui, range| {
            for index in range {
                let line = index + 1;
                let width = ui.available_width().max(900.0);
                let (rect, response) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::click());
                let half = rect.width() / 2.0;
                let left_rect = Rect::from_min_size(rect.min, vec2(half, ROW_H));
                let right_rect = Rect::from_min_size(pos2(rect.left() + half, rect.top()), vec2(half, ROW_H));
                if let Some(block) = compare::block_at(&blocks, line, true) {
                    paint_change(ui, left_rect, palette, block, true);
                }
                if let Some(block) = compare::block_at(&blocks, line, false) {
                    paint_change(ui, right_rect, palette, block, false);
                    let first = block.cur_start + 1 == line || block.cur_len == 0 && block.cur_start + 2 == line;
                    if first {
                        let button =
                            Rect::from_min_size(pos2(rect.right() - 32.0, rect.top() + 1.0), vec2(28.0, ROW_H - 2.0));
                        let response = ui.interact(button, ui.scope_id().with(("git-rollback", index)), Sense::click());
                        ui.painter().text(
                            button.center(),
                            Align2::CENTER_CENTER,
                            "⟲",
                            ui_font(14.0),
                            color(palette.extra.warn),
                        );
                        if response.clicked() {
                            clicked_block = Some(blocks.iter().position(|b| b == block).unwrap_or(0));
                        }
                    }
                }
                if line == tab.cursor {
                    ui.painter().rect_stroke(
                        rect,
                        0.0,
                        egui::Stroke::new(1.0, color(palette.accent)),
                        egui::StrokeKind::Inside,
                    );
                }
                let painter = ui.painter_at(rect);
                text_cell(&painter, left_rect, left.get(index).copied().unwrap_or(""), palette.fg_dim);
                text_cell(&painter, right_rect, right.get(index).copied().unwrap_or(""), palette.fg);
                painter.vline(rect.left() + half, rect.y_range(), egui::Stroke::new(1.0, color(palette.border)));
                if response.clicked() {
                    tab.cursor = line;
                    tab.left = response.interact_pointer_pos().is_some_and(|p| p.x < rect.left() + half);
                }
            }
        },
    );
    if let Some(index) = clicked_block {
        intents.push(Intent::CompareRollback(index));
    }
    intents
}

fn paint_change(ui: &mut Ui, rect: Rect, palette: &Palette, block: &Block, left: bool) {
    use less_git::compare::BlockKind;
    let tint = match block.kind {
        BlockKind::Added if !left => palette.extra.diff_add,
        BlockKind::Deleted if left => palette.error,
        BlockKind::Changed => palette.extra.warn,
        _ => palette.comment,
    };
    ui.painter().rect_filled(rect, 0.0, color(tint).gamma_multiply(0.10));
}

fn text_cell(painter: &egui::Painter, rect: Rect, text: &str, tint: less_theme::Rgb) {
    let max = ((rect.width() - 48.0) / 7.0).max(1.0) as usize;
    let text = clip(text, max);
    painter.text(rect.left_center() + vec2(34.0, 0.0), Align2::LEFT_CENTER, text, FontId::monospace(FONT), color(tint));
}

fn merge_ui(tab: &mut MergeView, ui: &mut Ui, palette: &Palette) -> Vec<Intent> {
    let mut intents = Vec::new();
    let conflicts = merge::parse(&tab.result);
    tab.selected = tab.selected.min(conflicts.len().saturating_sub(1));
    ui.horizontal_wrapped(|ui| {
        ui.label(
            egui::RichText::new(format!("{} conflict(s) left", conflicts.len()))
                .color(color(if conflicts.is_empty() { palette.extra.diff_add } else { palette.error }))
                .font(ui_font(12.0)),
        );
        if !conflicts.is_empty() {
            if ui.button("« Accept all left").clicked() {
                intents.push(Intent::MergeAll(Choice::Left));
            }
            if ui.button("Accept all right »").clicked() {
                intents.push(Intent::MergeAll(Choice::Right));
            }
        } else if ui.button("✓ Save & mark resolved").clicked() {
            intents.push(Intent::MergeResolve);
        }
        if ui.button("✕ Close").clicked() {
            intents.push(Intent::Close);
        }
    });
    ui.horizontal_wrapped(|ui| {
        for (index, conflict) in conflicts.iter().enumerate() {
            ui.group(|ui| {
                ui.label(format!("Conflict {} · lines {}–{}", index + 1, conflict.start + 1, conflict.end + 1));
                ui.horizontal(|ui| {
                    if ui.button("Left").clicked() {
                        intents.push(Intent::MergeChoice(index, Choice::Left));
                    }
                    if ui.button("Right").clicked() {
                        intents.push(Intent::MergeChoice(index, Choice::Right));
                    }
                    if ui.button("Both").clicked() {
                        intents.push(Intent::MergeChoice(index, Choice::Both));
                    }
                });
            });
        }
    });
    ui.label(
        egui::RichText::new("Space m l/r/b resolve · Space m L/R accept all · ]x/[x navigate · Space m m save")
            .color(color(palette.comment))
            .font(ui_font(11.0)),
    );
    let height = ui.available_height().max(100.0);
    ui.columns(3, |columns| {
        merge_pane(&mut columns[0], "« Left (ours :2:)", &mut tab.ours.clone(), height, true);
        merge_pane(&mut columns[1], "Result (editable)", &mut tab.result, height, false);
        merge_pane(&mut columns[2], "Right (theirs :3:) »", &mut tab.theirs.clone(), height, true);
    });
    intents
}

fn merge_pane(ui: &mut Ui, title: &str, text: &mut String, height: f32, read_only: bool) {
    ui.label(egui::RichText::new(title).font(ui_font(11.5)).strong());
    ui.add_sized(
        vec2(ui.available_width(), (height - 24.0).max(40.0)),
        TextEdit::multiline(text).font(FontId::monospace(FONT)).interactive(!read_only),
    );
}

fn blame_ui(tab: &mut BlameView, ui: &mut Ui, palette: &Palette) -> Vec<Intent> {
    let mut intents = Vec::new();
    toolbar(ui, palette, "Git blame", "Enter opens the commit; q closes");
    if ui.button("Close").clicked() {
        intents.push(Intent::Close);
    }
    let source: Vec<&str> = tab.source.lines().collect();
    let rows = source.len().max(tab.lines.len());
    egui::ScrollArea::both().id_salt("git-blame-tab").auto_shrink([false, false]).show_rows(
        ui,
        ROW_H,
        rows,
        |ui, range| {
            for index in range {
                let width = ui.available_width().max(1000.0);
                let (rect, response) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::click());
                if index == tab.cursor {
                    ui.painter().rect_filled(rect, 0.0, color(palette.bg_highlight));
                }
                let painter = ui.painter_at(rect);
                let y = rect.center().y;
                let info = tab.lines.get(index).and_then(Option::as_ref);
                let date = info.map(|i| i.date.as_str()).unwrap_or("----------");
                let author = info.map(|i| i.author.as_str()).unwrap_or("Not committed");
                painter.text(
                    pos2(rect.left() + 8.0, y),
                    Align2::LEFT_CENTER,
                    date,
                    FontId::monospace(FONT),
                    color(palette.comment),
                );
                painter.text(
                    pos2(rect.left() + 96.0, y),
                    Align2::LEFT_CENTER,
                    clip(author, 20),
                    FontId::monospace(FONT),
                    color(info.map_or(palette.comment, |_| palette.extra.info)),
                );
                painter.text(
                    pos2(rect.left() + 250.0, y),
                    Align2::LEFT_CENTER,
                    format!("{:5} {}", index + 1, source.get(index).copied().unwrap_or("")),
                    FontId::monospace(FONT),
                    color(palette.fg),
                );
                if response.double_clicked() {
                    if let Some(info) = info {
                        intents.push(Intent::BlameCommit(info.sha.clone()));
                    }
                } else if response.clicked() {
                    tab.cursor = index;
                }
            }
        },
    );
    intents
}

fn toolbar(ui: &mut Ui, palette: &Palette, title: &str, hint: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).font(ui_font(12.0)).strong());
        ui.label(egui::RichText::new(hint).font(ui_font(11.0)).color(color(palette.comment)));
    });
}

fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_owned()
    } else {
        text.chars().take(max.saturating_sub(1)).chain(['…']).collect()
    }
}
