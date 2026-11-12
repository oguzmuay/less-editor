//! Ekranlar arasında ortak çizim parçaları: renk çevirisi, başlıklar, butonlar, giriş kutuları, bildirim balonu,
//! pencere sürükleme şeridi.

use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align2, Color32, CursorIcon, FontId, Frame, Margin, Rect, Response, Sense, Stroke, StrokeKind, Ui, pos2, vec2,
};
use less_theme::{Palette, Rgb};

use crate::fonts::icon;

/// Başlık çubuğu gizli olduğu için pencerenin tutulup sürüklendiği üst şeridin yüksekliği.
pub const DRAG_STRIP: f32 = 40.0;
const TOAST_FOR: Duration = Duration::from_millis(2600);

/// `less-theme` rengini egui rengine çevirir.
pub fn color(c: Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

/// Arayüz yazı tipi (JetBrains Mono, bkz. `fonts`).
pub fn ui_font(size: f32) -> FontId {
    FontId::proportional(size)
}

/// Pencerenin üst şeridini sürüklenebilir yapar (macOS başlık çubuğu gizli). Çift tık pencereyi büyütür.
pub fn drag_strip(ui: &Ui, rect: Rect) {
    let resp = ui.interact(rect, ui.scope_id().with("drag-strip"), Sense::click_and_drag());
    if resp.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    if resp.double_clicked() {
        let max = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
    }
}

pub fn heading(ui: &mut Ui, p: &Palette, title: &str, subtitle: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::hover());
    ui.painter().text(rect.left_top(), Align2::LEFT_TOP, title, ui_font(26.0), color(p.fg));
    ui.painter().text(rect.left_top() + vec2(0.0, 38.0), Align2::LEFT_TOP, subtitle, ui_font(14.0), color(p.comment));
}

pub fn section_label(ui: &mut Ui, p: &Palette, text: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 16.0), Sense::hover());
    ui.painter().text(rect.left_center(), Align2::LEFT_CENTER, text, ui_font(11.0), color(p.comment));
}

pub fn button_width(ui: &Ui, label: &str, glyph: Option<&str>) -> f32 {
    let galley = ui.painter().layout_no_wrap(label.to_owned(), ui_font(13.5), Color32::WHITE);
    galley.size().x + if glyph.is_some() { 50.0 } else { 28.0 }
}

/// Eylem butonu; `primary` vurgu renginde dolu, diğerleri çerçeveli. `enabled = false` sönük ve tıklanmaz.
pub fn button(ui: &mut Ui, p: &Palette, glyph: Option<&str>, label: &str, primary: bool, enabled: bool) -> Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(vec2(button_width(ui, label, glyph), 36.0), sense);
    let hovered = enabled && resp.hovered();
    let (mut fill, stroke, mut fg) = if primary {
        let fill = if hovered { color(p.accent).gamma_multiply(0.85) } else { color(p.accent) };
        (fill, Stroke::NONE, color(p.on_accent))
    } else {
        let fill = if hovered { color(p.bg_highlight) } else { color(p.surface) };
        (fill, Stroke::new(1.0, color(p.border)), color(p.fg))
    };
    if !enabled {
        fill = fill.gamma_multiply(0.45);
        fg = fg.gamma_multiply(0.55);
    }
    if hovered {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    ui.painter().rect(rect, 7.0, fill, stroke, StrokeKind::Inside);
    let y = rect.center().y;
    let mut x = rect.left() + 14.0;
    if let Some(g) = glyph {
        ui.painter().text(pos2(x, y), Align2::LEFT_CENTER, g, ui_font(15.0), fg);
        x += 22.0;
    }
    ui.painter().text(pos2(x, y), Align2::LEFT_CENTER, label, ui_font(13.5), fg);
    resp
}

/// Çerçeveli tek satır metin girişi; solda isteğe bağlı ikon. `focus` true ise kutu oluşturulduktan sonra odak ister.
/// Dönen yanıt metin kutusununkidir (`lost_focus`, `changed` vb.).
#[allow(clippy::too_many_arguments)]
pub fn text_input(
    ui: &mut Ui,
    p: &Palette,
    id: egui::Id,
    text: &mut String,
    hint: &str,
    glyph: Option<&str>,
    width: f32,
    focus: bool,
) -> Response {
    let (rect, _) = ui.allocate_exact_size(vec2(width.max(120.0), 36.0), Sense::hover());
    let focused = ui.memory(|m| m.has_focus(id));
    let stroke = Stroke::new(1.0, if focused { color(p.accent) } else { color(p.border) });
    ui.painter().rect(rect, 7.0, color(p.bg_dark), stroke, StrokeKind::Inside);
    let mut left = 12.0;
    if let Some(g) = glyph {
        ui.painter().text(
            rect.left_center() + vec2(12.0, 0.0),
            Align2::LEFT_CENTER,
            g,
            ui_font(15.0),
            color(p.comment),
        );
        left = 36.0;
    }
    let inner = Rect::from_min_max(rect.left_top() + vec2(left, 0.0), rect.right_bottom() - vec2(10.0, 0.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    let resp = egui::TextEdit::singleline(text)
        .id(id)
        .hint_text(egui::RichText::new(hint).color(color(p.comment)))
        .font(ui_font(13.5))
        .text_color(color(p.fg))
        .frame(Frame::NONE)
        .margin(Margin::ZERO)
        .desired_width(inner.width())
        .min_size(inner.size())
        .align(Align2::LEFT_CENTER)
        .show(&mut child)
        .response
        .response;
    if focus {
        resp.request_focus();
    }
    resp
}

/// Ortada çerçeveli boş durum kutusu (ikon + metin).
pub fn empty_state(ui: &mut Ui, p: &Palette, glyph: &str, text: &str, hint: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 170.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_stroke(rect, 10.0, Stroke::new(1.0, color(p.border)), StrokeKind::Inside);
    painter.text(rect.center() - vec2(0.0, 30.0), Align2::CENTER_CENTER, glyph, ui_font(28.0), color(p.comment));
    painter.text(rect.center() + vec2(0.0, 8.0), Align2::CENTER_CENTER, text, ui_font(14.0), color(p.fg_dim));
    painter.text(rect.center() + vec2(0.0, 32.0), Align2::CENTER_CENTER, hint, ui_font(12.5), color(p.comment));
}

/// Kısa süreli bildirim balonu (alt ortada); hata ise kırmızı ikonla.
#[derive(Default)]
pub struct Toast {
    current: Option<(String, bool, Instant)>,
}

impl Toast {
    pub fn info(&mut self, text: impl Into<String>) {
        self.current = Some((text.into(), false, Instant::now()));
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.current = Some((text.into(), true, Instant::now()));
    }

    /// `center_x` balonun yatay ortası (kenar çubuğu varken içerik alanının ortası).
    pub fn show(&mut self, ctx: &egui::Context, p: &Palette, center_x: f32, bottom: f32) {
        let Some((text, is_error, at)) = &self.current else { return };
        let left = TOAST_FOR.saturating_sub(at.elapsed());
        if left.is_zero() {
            self.current = None;
            return;
        }
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("less-toast")));
        let galley = painter.layout_no_wrap(text.clone(), ui_font(13.0), color(p.fg));
        let size = galley.size() + vec2(50.0, 20.0);
        let rect = Rect::from_center_size(pos2(center_x, bottom - 36.0), size);
        painter.rect(rect, 8.0, color(p.bg_highlight), Stroke::new(1.0, color(p.border)), StrokeKind::Inside);
        let (glyph, tint) = if *is_error { (icon::WARNING, p.error) } else { (icon::INFO, p.accent) };
        painter.text(rect.left_center() + vec2(14.0, 0.0), Align2::LEFT_CENTER, glyph, ui_font(14.0), color(tint));
        painter.galley(rect.left_top() + vec2(38.0, 10.0), galley, color(p.fg));
        ctx.request_repaint_after(left);
    }
}
