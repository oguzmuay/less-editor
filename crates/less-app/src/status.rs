//! Durum satırının sağ tarafı: dosya bilgisi, dil sunucuları, keymap / tema, sistem bilgisi (#73). Pencere daraldıkça
//! parçalar öncelik sırasıyla tek tek düşer ([`fit`]); dil sunucusu göstergesine tıklamak `:options`'tır.

use eframe::egui::{Align2, Color32, CursorIcon, Rect, Sense, Spinner, Ui, pos2, vec2};
use less_theme::Palette;

use crate::editor::lang::{ServerState, ServerStatus};
use crate::widgets::{color, ui_font};

const GAP: &str = "   ·   ";
const SPINNER_W: f32 = 14.0;

/// Sağdaki bir parça; `prio` büyük olan yer kalmayınca önce atılır.
pub enum Seg {
    Text { prio: u8, text: String },
    Servers(Vec<ServerStatus>),
}

impl Seg {
    fn prio(&self) -> u8 {
        match self {
            Self::Text { prio, .. } => *prio,
            Self::Servers(_) => 2,
        }
    }
}

/// Hangi parçaların sığdığı: toplam (aralıklar dahil) `avail`'i aşdıkça en yüksek öncelikli kalan parça atılır.
pub fn fit(widths: &[f32], prios: &[u8], gap: f32, avail: f32) -> Vec<bool> {
    let mut keep = vec![true; widths.len()];
    loop {
        let n = keep.iter().filter(|k| **k).count();
        let total: f32 = widths.iter().zip(&keep).filter(|(_, k)| **k).map(|(w, _)| w).sum::<f32>()
            + gap * n.saturating_sub(1) as f32;
        if total <= avail || n == 0 {
            return keep;
        }
        let drop = (0..widths.len()).filter(|i| keep[*i]).max_by_key(|i| (prios[*i], *i));
        keep[drop.expect("n > 0")] = false;
    }
}

/// Sunucu adının rengi: çalışıyor soluk, başlıyor / meşgul daha soluk (+ dönen simge), kapandıysa hata rengi.
fn server_color(p: &Palette, state: ServerState) -> Color32 {
    match state {
        ServerState::Running => color(p.fg_dim),
        ServerState::Starting | ServerState::Busy => color(p.comment),
        ServerState::Exited => color(p.error),
    }
}

fn spinning(state: ServerState) -> bool {
    matches!(state, ServerState::Starting | ServerState::Busy)
}

/// Herhangi bir sunucu başlıyor / meşgulse: geç gelen diagnostikler için yavaş yeniden çizim gerekir.
pub fn any_pending(servers: &[ServerStatus]) -> bool {
    servers.iter().any(|s| spinning(s.state))
}

/// Parçaları `rect`'in sağına, `left_end`'e kadar sığdığı kadar çizer. Sunucu göstergesine tıklanırsa `true`.
pub fn paint_right(ui: &Ui, p: &Palette, rect: Rect, left_end: f32, segs: &[Seg]) -> bool {
    let font = ui_font(12.0);
    let width = |t: &str| ui.painter().layout_no_wrap(t.to_owned(), font.clone(), Color32::WHITE).size().x;
    let gap = width(GAP);
    let sep = width(" · ");
    let widths: Vec<f32> = segs
        .iter()
        .map(|s| match s {
            Seg::Text { text, .. } => width(text),
            Seg::Servers(list) => {
                list.iter().map(|s| width(&s.name) + if spinning(s.state) { SPINNER_W } else { 0.0 }).sum::<f32>()
                    + sep * list.len().saturating_sub(1) as f32
            }
        })
        .collect();
    let prios: Vec<u8> = segs.iter().map(Seg::prio).collect();
    let keep = fit(&widths, &prios, gap, (rect.right() - left_end - 24.0).max(0.0));
    let y = rect.center().y;
    let dim = color(p.comment);
    let mut x = rect.right();
    let mut clicked = false;
    let mut first = true;
    for (i, seg) in segs.iter().enumerate().rev().filter(|(i, _)| keep[*i]) {
        if !first {
            x -= gap;
            ui.painter().text(pos2(x, y), Align2::LEFT_CENTER, GAP, font.clone(), dim);
        }
        first = false;
        x -= widths[i];
        match seg {
            Seg::Text { text, .. } => {
                ui.painter().text(pos2(x, y), Align2::LEFT_CENTER, text, font.clone(), dim);
            }
            Seg::Servers(list) => {
                let area = Rect::from_min_size(pos2(x, rect.top()), vec2(widths[i], rect.height()));
                let resp = ui.interact(area, ui.scope_id().with("status-servers"), Sense::click());
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
                }
                let mut at = x;
                for (n, s) in list.iter().enumerate() {
                    if n > 0 {
                        ui.painter().text(pos2(at, y), Align2::LEFT_CENTER, " · ", font.clone(), dim);
                        at += sep;
                    }
                    let tint = server_color(p, s.state);
                    if spinning(s.state) {
                        let r = Rect::from_center_size(pos2(at + 5.0, y), vec2(10.0, 10.0));
                        Spinner::new().size(10.0).color(tint).paint_at(ui, r);
                        at += SPINNER_W;
                    }
                    ui.painter().text(pos2(at, y), Align2::LEFT_CENTER, &s.name, font.clone(), tint);
                    at += width(&s.name);
                }
                clicked = resp.on_hover_text("Language servers: click for :options").clicked();
            }
        }
    }
    clicked
}

#[cfg(test)]
fn text(prio: u8, t: &str) -> Seg {
    Seg::Text { prio, text: t.to_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui;

    #[test]
    fn narrow_status_drops_parts_by_priority_73() {
        let widths = [50.0, 80.0, 60.0, 40.0];
        let prios = [1, 7, 3, 9];
        assert_eq!(fit(&widths, &prios, 10.0, 1000.0), [true, true, true, true]);
        assert_eq!(fit(&widths, &prios, 10.0, 210.0), [true, true, true, false]);
        assert_eq!(fit(&widths, &prios, 10.0, 130.0), [true, false, true, false]);
        assert_eq!(fit(&widths, &prios, 10.0, 0.0), [false, false, false, false]);
    }

    #[test]
    fn servers_indicator_paints_and_clicks_73() {
        let ctx = egui::Context::default();
        let p = less_theme::TOKYONIGHT_NIGHT;
        let servers = vec![
            ServerStatus { name: "vtsls".into(), state: ServerState::Running },
            ServerStatus { name: "jdtls".into(), state: ServerState::Busy },
            ServerStatus { name: "eslint".into(), state: ServerState::Exited },
        ];
        assert!(any_pending(&servers));
        assert!(!any_pending(&servers[..1]));
        let segs = [text(1, "Ln 1, Col 1"), Seg::Servers(servers), text(9, "UTF-8")];
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 24.0));
        let run = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0))),
                events,
                ..Default::default()
            };
            let mut clicked = false;
            let mut out = ctx.run_ui(input, |ui| clicked = paint_right(ui, &p, rect, 0.0, &segs));
            out.textures_delta.clear();
            clicked
        };
        assert!(!run(vec![]));
        let at = pos2(800.0 - 120.0, 12.0);
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run(vec![egui::Event::PointerMoved(at)]);
        assert!(run(vec![press(true), press(false)]), "click on the server list");
    }
}
