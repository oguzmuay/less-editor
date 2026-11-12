//! Özet tab'ı çizimi (karar #27, #60): başlık, yardım satırı ve oturum blokları; her bloğun solunda oturumun
//! renginde, durumuna göre hareket eden maskot. Model `less_ai::summary::SummaryView`'dur.

use eframe::egui::{self, Align2, Color32, FontId, Rect, Sense, Ui, pos2, vec2};
use less_ai::summary::{self, LineStyle, StatusTone, SummaryView};
use less_theme::Palette;

use super::view::FONT_SIZE;
use crate::widgets::color;

/// Özette tıklama: `go` = çift tık (oturuma geç), değilse yalnız seçim.
pub struct Picked {
    pub index: usize,
    pub go: bool,
}

fn line_color(style: LineStyle, p: &Palette) -> Color32 {
    match style {
        LineStyle::Title => color(p.accent),
        LineStyle::Comment => color(p.comment),
        LineStyle::Bold => color(p.fg),
        LineStyle::Normal => color(p.fg_dim),
    }
}

fn tone_color(tone: StatusTone, p: &Palette) -> Color32 {
    match tone {
        StatusTone::Ok => color(p.extra.diff_add),
        StatusTone::Error => color(p.error),
        StatusTone::Info => color(p.extra.info),
        StatusTone::Muted => color(p.comment),
    }
}

/// Bloğun satır sayısı: başlık + ayrıntılar, maskot için en az 4.
pub fn block_rows(lines: usize) -> usize {
    (lines + 1).max(4)
}

/// Özeti çizer; `selected` klavyeyle seçili blok, `frame` maskot karesi.
pub fn show(ui: &mut Ui, view: &SummaryView, frame: u64, selected: usize, p: &Palette) -> Option<Picked> {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0.0, color(p.bg));
    let font = FontId::monospace(FONT_SIZE);
    let cw = ui.fonts_mut(|f| f.glyph_width(&font, 'M'));
    let ch = ui.fonts_mut(|f| f.row_height(&font)).ceil();
    let mut picked = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        let width = ui.available_width();
        let (head, _) = ui.allocate_exact_size(vec2(width, ch * 3.0), Sense::hover());
        let x = head.left() + cw;
        ui.painter().text(pos2(x, head.top()), Align2::LEFT_TOP, &view.header, font.clone(), color(p.accent));
        ui.painter().text(pos2(x, head.top() + ch), Align2::LEFT_TOP, &view.help, font.clone(), color(p.comment));
        for (i, block) in view.blocks.iter().enumerate() {
            let rows = block_rows(block.lines.len());
            let (r, resp) = ui.allocate_exact_size(vec2(width, ch * (rows as f32 + 1.0)), Sense::click());
            let body = Rect::from_min_size(r.min, vec2(width, ch * rows as f32));
            if i == selected || resp.hovered() {
                ui.painter().rect_filled(body, 4.0, color(p.bg_highlight));
            }
            let (mr, mg, mb) = block.mascot_color;
            let mascot = summary::frame(block.mascot_status, frame, block.mascot_kind);
            for (row, line) in mascot.iter().enumerate() {
                let pos = pos2(x, r.top() + row as f32 * ch);
                ui.painter().text(pos, Align2::LEFT_TOP, line, font.clone(), Color32::from_rgb(mr, mg, mb));
            }
            let tx = x + (summary::MASCOT_W + 2) as f32 * cw;
            let head = ui.painter().text(pos2(tx, r.top()), Align2::LEFT_TOP, &block.head, font.clone(), color(p.fg));
            let status = tone_color(block.status_tone, p);
            ui.painter().text(pos2(head.right(), r.top()), Align2::LEFT_TOP, &block.status_text, font.clone(), status);
            for (row, (text, style)) in block.lines.iter().enumerate() {
                let pos = pos2(tx, r.top() + (row + 1) as f32 * ch);
                ui.painter().text(pos, Align2::LEFT_TOP, text, font.clone(), line_color(*style, p));
            }
            if resp.double_clicked() {
                picked = Some(Picked { index: i, go: true });
            } else if resp.clicked() {
                picked = Some(Picked { index: i, go: false });
            }
        }
    });
    picked
}
