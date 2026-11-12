//! Sonuç/veri/yapı tablolarının egui çizimi (#50): sabit başlık, yatay ve dikey kaydırma, NULL, sayısal hizalama,
//! bekleyen hücre vurguları ve hücrenin sağ tık menüsü.

use eframe::egui::{self, Align2, Color32, FontId, Rect, Sense, Ui, pos2, vec2};
use less_db::grid::{self, ColumnLayout};
use less_theme::Palette;

use super::tabs::{DbTab, TabKind};
use crate::widgets::color;

pub const ROW_H: f32 = 24.0;
const HEADER_H: f32 = 27.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridAction {
    Select(usize, usize),
    ViewAt(usize, usize),
    View,
    Copy,
    Edit,
    SetNull,
    CloneRow,
    Commit,
    Refresh,
}

/// Tabloyu çizer; fare eylemlerini çizimden sonra sırayla uygulanmak üzere döndürür.
pub fn show(ui: &mut Ui, p: &Palette, tab: &DbTab, focused: bool) -> Vec<GridAction> {
    let Some(grid) = tab.grid() else { return Vec::new() };
    if grid.columns.is_empty() {
        let text = match grid.affected {
            Some(n) => format!("Statement executed · {n} row{} affected", if n == 1 { "" } else { "s" }),
            None => "Statement executed (no rows returned)".into(),
        };
        ui.label(egui::RichText::new(text).color(color(p.comment)).font(FontId::monospace(13.0)));
        return Vec::new();
    }
    let rows = tab.rows();
    let layouts = grid::layout(&grid.columns, &rows);
    let font = FontId::monospace(12.5);
    let char_w = ui.fonts_mut(|f| f.glyph_width(&font, 'M')).max(1.0);
    let widths: Vec<f32> = layouts.iter().map(|l| (l.width + 3) as f32 * char_w).collect();
    let total = widths.iter().sum::<f32>().max(ui.available_width());
    let height = ui.available_height().max(HEADER_H + ROW_H);
    let mut actions = Vec::new();
    egui::ScrollArea::horizontal().id_salt("db-grid-x").auto_shrink([false, false]).show(ui, |ui| {
        ui.set_width(total);
        ui.set_min_height(height);
        let (header, _) = ui.allocate_exact_size(vec2(total, HEADER_H), Sense::hover());
        paint_header(ui, p, header, &grid.columns, &widths, char_w);
        egui::ScrollArea::vertical()
            .id_salt("db-grid-y")
            .auto_shrink([false, false])
            .max_height((height - HEADER_H).max(ROW_H))
            .show_rows(ui, ROW_H, rows.len(), |ui, range| {
                for row in range {
                    let (rect, response) = ui.allocate_exact_size(vec2(total, ROW_H), Sense::click());
                    let selected = grid.cursor.0 == row;
                    let fill = if selected && focused {
                        color(p.accent).gamma_multiply(0.20)
                    } else if selected || response.hovered() {
                        color(p.bg_highlight).gamma_multiply(0.65)
                    } else {
                        Color32::TRANSPARENT
                    };
                    if fill != Color32::TRANSPARENT {
                        ui.painter().rect_filled(rect, 0.0, fill);
                    }
                    paint_row(ui, p, tab, row, rect, &rows[row], &layouts, &widths, char_w, focused);
                    if response.double_clicked() {
                        if let Some(col) = column_at(&response, rect, &widths) {
                            actions.push(GridAction::ViewAt(row, col));
                        }
                    } else if response.clicked()
                        && let Some(col) = column_at(&response, rect, &widths)
                    {
                        actions.push(GridAction::Select(row, col));
                    }
                    if response.secondary_clicked()
                        && let Some(col) = column_at(&response, rect, &widths)
                    {
                        actions.push(GridAction::Select(row, col));
                    }
                    response.context_menu(|ui| {
                        context_menu(ui, tab, &mut actions);
                    });
                }
            });
    });
    actions
}

fn paint_header(ui: &mut Ui, p: &Palette, rect: Rect, columns: &[String], widths: &[f32], char_w: f32) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, color(p.surface));
    painter.hline(rect.left()..=rect.right(), rect.bottom() - 0.5, egui::Stroke::new(1.0, color(p.border)));
    let mut x = rect.left();
    for (name, width) in columns.iter().zip(widths) {
        let cell = Rect::from_min_size(pos2(x, rect.top()), vec2(*width, rect.height()));
        let text = grid::fit(name, ((*width / char_w) as usize).saturating_sub(3), false);
        painter.text(
            cell.left_center() + vec2(8.0, 0.0),
            Align2::LEFT_CENTER,
            text,
            FontId::monospace(12.5),
            color(p.fg),
        );
        painter.vline(cell.right(), cell.y_range(), egui::Stroke::new(1.0, color(p.border)));
        x += *width;
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_row(
    ui: &mut Ui,
    p: &Palette,
    tab: &DbTab,
    row: usize,
    rect: Rect,
    values: &[Option<String>],
    layouts: &[ColumnLayout],
    widths: &[f32],
    char_w: f32,
    focused: bool,
) {
    let painter = ui.painter_at(rect);
    let mut x = rect.left();
    let cursor = tab.grid().map(|g| g.cursor.0 == row).unwrap_or(false);
    for (col, (layout, width)) in layouts.iter().zip(widths).enumerate() {
        let cell = Rect::from_min_size(pos2(x, rect.top()), vec2(*width, rect.height()));
        let changed = match &tab.kind {
            TabKind::Data(data) => data.edit.as_ref().is_some_and(|e| e.is_changed(row, col)),
            _ => false,
        };
        if changed {
            let added = match &tab.kind {
                TabKind::Data(data) => data.edit.as_ref().is_some_and(|e| row >= e.base.len()),
                _ => false,
            };
            painter.rect_filled(
                cell,
                0.0,
                color(if added { p.extra.diff_add } else { p.extra.warn }).gamma_multiply(0.18),
            );
        } else if cursor && focused && tab.grid().is_some_and(|g| g.cursor.1 == col) {
            painter.rect_filled(cell, 0.0, color(p.accent).gamma_multiply(0.22));
        }
        let value = values.get(col).and_then(|v| v.as_deref());
        let display = grid::display(value);
        let width_chars = ((*width / char_w) as usize).saturating_sub(3);
        let text = grid::fit(&display, width_chars, layout.numeric);
        let fg = if value.is_none() {
            color(p.comment)
        } else if changed {
            color(p.extra.warn)
        } else {
            color(p.fg)
        };
        painter.text(cell.left_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, text, FontId::monospace(12.5), fg);
        painter.vline(cell.right(), cell.y_range(), egui::Stroke::new(1.0, color(p.border).gamma_multiply(0.45)));
        x += *width;
    }
    painter.hline(
        rect.left()..=rect.right(),
        rect.bottom() - 0.5,
        egui::Stroke::new(1.0, color(p.border).gamma_multiply(0.35)),
    );
}

fn column_at(response: &egui::Response, rect: Rect, widths: &[f32]) -> Option<usize> {
    let x = response.interact_pointer_pos()?.x - rect.left();
    let mut left = 0.0;
    for (col, width) in widths.iter().enumerate() {
        if x >= left && x < left + *width {
            return Some(col);
        }
        left += *width;
    }
    None
}

fn context_menu(ui: &mut Ui, tab: &DbTab, actions: &mut Vec<GridAction>) {
    for (label, value) in [("View full value", GridAction::View), ("Copy value", GridAction::Copy)] {
        if ui.button(label).clicked() {
            actions.push(value);
            ui.close();
        }
    }
    if matches!(&tab.kind, TabKind::Data(data) if data.edit.is_some()) {
        for (label, value) in [
            ("Edit cell…", GridAction::Edit),
            ("Set NULL", GridAction::SetNull),
            ("Clone row", GridAction::CloneRow),
            ("Commit changes…", GridAction::Commit),
        ] {
            if ui.button(label).clicked() {
                actions.push(value);
                ui.close();
            }
        }
    }
    if ui.button("Refresh").clicked() {
        actions.push(GridAction::Refresh);
        ui.close();
    }
}
