//! Terminal görünümü: `less_ai::term::Terminal`'in ızgarasını egui ile çizer, fareyi terminale ya da seçime verir.
//!
//! Anlık görüntü (`Terminal::snapshot`) yalnız `generation()` değişince (ya da boyut / tema değişince) alınır;
//! satırlar o anda galley'lere çevrilip saklanır, sonraki karelerde yalnız boyanır.

use std::sync::Arc;

use eframe::egui::{
    self, Color32, FontId, Galley, PointerButton, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, pos2,
    text::{LayoutJob, TextFormat},
    vec2,
};
use less_ai::term::{
    CursorShape, CursorView, GridCell, Key, Modifiers, MouseAction, MouseButton, MouseEvent, SelectionKind, TermColor,
    TermSize, Terminal,
};
use less_theme::Palette;

use crate::widgets::color;

/// Terminal yazı boyutu (kod alanıyla aynı aile, biraz küçük).
pub const FONT_SIZE: f32 = 13.0;

const ANSI_DARK: [u32; 16] = [
    0x15161e, 0xf7768e, 0x9ece6a, 0xe0af68, 0x7aa2f7, 0xbb9af7, 0x7dcfff, 0xa9b1d6, 0x414868, 0xff899d, 0xb9f27c,
    0xffc777, 0x8db0ff, 0xc7a9ff, 0xa4daff, 0xc0caf5,
];
const ANSI_LIGHT: [u32; 16] = [
    0x24292f, 0xcf222e, 0x116329, 0x4d2d00, 0x0969da, 0x8250df, 0x1b7c83, 0x6e7781, 0x57606a, 0xa40e26, 0x1a7f37,
    0x633c01, 0x218bff, 0xa475f9, 0x3192aa, 0x8c959f,
];

fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// 256 renk tablosu: 0–15 temaya göre ANSI, 16–231 6×6×6 küp, 232–255 gri rampa (xterm).
pub fn indexed(i: u8, dark: bool) -> Color32 {
    match i {
        0..=15 => hex(if dark { ANSI_DARK } else { ANSI_LIGHT }[i as usize]),
        16..=231 => {
            let n = i - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            Color32::from_rgb(level(n / 36), level((n / 6) % 6), level(n % 6))
        }
        _ => {
            let g = 8 + (i - 232) * 10;
            Color32::from_rgb(g, g, g)
        }
    }
}

/// Hücre rengi → ekran rengi (varsayılan ön/arka plan tema renkleri).
pub fn term_color(c: TermColor, p: &Palette) -> Color32 {
    match c {
        TermColor::DefaultFg => color(p.fg),
        TermColor::DefaultBg => color(p.bg),
        TermColor::Indexed(i) => indexed(i, p.dark),
        TermColor::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
    }
}

/// Bir satırdaki zemin aralığı `[from, to)` sütun.
#[derive(Debug, Clone, PartialEq)]
pub struct BgSpan {
    pub from: usize,
    pub to: usize,
    pub color: Color32,
}

/// Aynı biçimli metin parçası; `col` başladığı sütun.
#[derive(Debug, Clone, PartialEq)]
pub struct TextSpan {
    pub col: usize,
    pub text: String,
    pub fg: Color32,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
}

/// Hücrenin son ön / arka plan rengi (kalın ANSI 0–7 parlak olur, ters, sönük, gizli, seçim).
fn cell_colors(cell: &GridCell, p: &Palette) -> (Color32, Color32) {
    let fg_src = match cell.fg {
        TermColor::Indexed(i) if cell.style.bold && i < 8 => TermColor::Indexed(i + 8),
        c => c,
    };
    let (mut fg, mut bg) = (term_color(fg_src, p), term_color(cell.bg, p));
    if cell.style.inverse {
        std::mem::swap(&mut fg, &mut bg);
    }
    if cell.style.dim {
        fg = fg.gamma_multiply(0.6);
    }
    if cell.selected {
        bg = color(p.accent).gamma_multiply(0.35);
    }
    if cell.style.hidden {
        fg = bg;
    }
    (fg, bg)
}

/// Satırı zemin ve metin parçalarına böler. ASCII dışı her karakter kendi parçasıdır: yedek fonttan gelen glif
/// genişliği hücre genişliğinden farklı olsa da sonraki sütunlar kaymaz.
pub fn spans(row: &[GridCell], p: &Palette) -> (Vec<BgSpan>, Vec<TextSpan>) {
    let default_bg = color(p.bg);
    let mut bgs: Vec<BgSpan> = Vec::new();
    let mut texts: Vec<TextSpan> = Vec::new();
    let mut open = false;
    for (col, cell) in row.iter().enumerate() {
        if cell.wide_spacer {
            if let Some(b) = bgs.last_mut().filter(|b| b.to == col) {
                b.to = col + 1;
            }
            open = false;
            continue;
        }
        let (fg, bg) = cell_colors(cell, p);
        if bg != default_bg {
            match bgs.last_mut() {
                Some(b) if b.to == col && b.color == bg => b.to = col + 1,
                _ => bgs.push(BgSpan { from: col, to: col + 1, color: bg }),
            }
        }
        let mut s = String::from(cell.ch);
        if let Some(z) = &cell.zerowidth {
            s.push_str(z);
        }
        let ascii = cell.ch.is_ascii() && cell.zerowidth.is_none() && !cell.wide;
        let (italic, underline, strike) = (cell.style.italic, cell.style.underline, cell.style.strikeout);
        match texts.last_mut() {
            Some(t)
                if open
                    && ascii
                    && t.fg == fg
                    && t.italic == italic
                    && t.underline == underline
                    && t.strike == strike =>
            {
                t.text.push_str(&s)
            }
            _ => texts.push(TextSpan { col, text: s, fg, italic, underline, strike }),
        }
        open = ascii;
    }
    texts.retain(|t| !t.text.trim().is_empty() || t.underline || t.strike);
    (bgs, texts)
}

/// Görünüm noktası → (sütun, satır), ızgaraya kırpılmış.
pub fn cell_at(origin: Pos2, cell: (f32, f32), pos: Pos2, size: TermSize) -> (usize, usize) {
    let col = ((pos.x - origin.x) / cell.0).floor().max(0.0) as usize;
    let row = ((pos.y - origin.y) / cell.1).floor().max(0.0) as usize;
    (col.min(size.cols as usize - 1), row.min(size.rows as usize - 1))
}

/// Alan boyutundan terminal boyutu (hücre).
pub fn grid_size(rect: Rect, cell: (f32, f32)) -> TermSize {
    TermSize::new((rect.width() / cell.0).floor() as u16, (rect.height() / cell.1).floor() as u16)
}

struct Run {
    col: usize,
    galley: Arc<Galley>,
}

struct Row {
    bgs: Vec<BgSpan>,
    runs: Vec<Run>,
}

/// Bir sekmenin çizim önbelleği ve fare durumu.
#[derive(Default)]
pub struct TermView {
    key: Option<(u64, TermSize, &'static str, u32)>,
    rows: Vec<Row>,
    cursor: Option<(CursorView, String, Color32, Color32)>,
    alt_scroll: bool,
    down: Option<MouseButton>,
    app_mouse: bool,
    anchor: Option<(usize, usize)>,
    selecting: bool,
    wheel: f32,
}

fn mouse_button(b: PointerButton) -> Option<MouseButton> {
    match b {
        PointerButton::Primary => Some(MouseButton::Left),
        PointerButton::Secondary => Some(MouseButton::Right),
        PointerButton::Middle => Some(MouseButton::Middle),
        _ => None,
    }
}

fn mods(m: egui::Modifiers) -> Modifiers {
    Modifiers { shift: m.shift, ctrl: m.ctrl, alt: m.alt, cmd: m.mac_cmd }
}

impl TermView {
    fn rebuild(&mut self, ui: &Ui, term: &Terminal, font: &FontId, p: &Palette) {
        let snap = term.snapshot();
        self.rows = snap
            .cells
            .iter()
            .map(|cells| {
                let (bgs, texts) = spans(cells, p);
                let runs = texts
                    .into_iter()
                    .map(|t| {
                        let mut fmt = TextFormat::simple(font.clone(), t.fg);
                        fmt.italics = t.italic;
                        if t.underline {
                            fmt.underline = Stroke::new(1.0, t.fg);
                        }
                        if t.strike {
                            fmt.strikethrough = Stroke::new(1.0, t.fg);
                        }
                        let mut job = LayoutJob::default();
                        job.append(&t.text, 0.0, fmt);
                        Run { col: t.col, galley: ui.painter().layout_job(job) }
                    })
                    .collect();
                Row { bgs, runs }
            })
            .collect();
        let c = snap.cursor;
        self.cursor = c.visible.then(|| {
            let cell = snap.cells.get(c.row).and_then(|r| r.get(c.col)).cloned().unwrap_or_default();
            let (fg, bg) = cell_colors(&cell, p);
            (c, cell.ch.to_string(), fg, bg)
        });
        self.alt_scroll = snap.mode.alt_screen && snap.mode.alternate_scroll;
    }

    /// Terminali `ui.max_rect()` içine çizer ve fare olaylarını işler. Alan boyutu değişince terminal yeniden
    /// boyutlanır; dönen boyut yeni sekmelerin başlangıç boyutudur.
    pub fn show(&mut self, ui: &mut Ui, term: &Terminal, p: &Palette, focused: bool) -> TermSize {
        let rect = ui.max_rect();
        let font = FontId::monospace(FONT_SIZE);
        let cw = ui.fonts_mut(|f| f.glyph_width(&font, 'M')).max(1.0);
        let ch = ui.fonts_mut(|f| f.row_height(&font)).ceil().max(1.0);
        let size = grid_size(rect, (cw, ch));
        term.resize(size);
        let key = (term.generation(), size, p.id, ui.pixels_per_point().to_bits());
        if self.key != Some(key) {
            self.rebuild(ui, term, &font, p);
            self.key = Some(key);
        }

        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, color(p.bg));
        let at = |col: usize, row: usize| pos2(rect.left() + col as f32 * cw, rect.top() + row as f32 * ch);
        for (r, row) in self.rows.iter().enumerate() {
            for b in &row.bgs {
                let min = at(b.from, r);
                painter.rect_filled(Rect::from_min_size(min, vec2((b.to - b.from) as f32 * cw, ch)), 0.0, b.color);
            }
            for run in &row.runs {
                painter.galley(at(run.col, r), run.galley.clone(), color(p.fg));
            }
        }
        if let Some((c, text, fg, bg)) = &self.cursor {
            let cell = Rect::from_min_size(at(c.col, c.row), vec2(cw, ch));
            match (focused, c.shape) {
                (true, CursorShape::Block) => {
                    painter.rect_filled(cell, 0.0, *fg);
                    painter.text(cell.min, egui::Align2::LEFT_TOP, text, font.clone(), *bg);
                }
                (true, CursorShape::Beam) => {
                    painter.rect_filled(Rect::from_min_size(cell.min, vec2(2.0, ch)), 0.0, *fg);
                }
                (true, CursorShape::Underline) => {
                    let min = pos2(cell.left(), cell.bottom() - 2.0);
                    painter.rect_filled(Rect::from_min_size(min, vec2(cw, 2.0)), 0.0, *fg);
                }
                _ => {
                    painter.rect_stroke(cell.shrink(0.5), 0.0, Stroke::new(1.0, *fg), StrokeKind::Inside);
                }
            }
        }

        let resp = ui.interact(rect, ui.scope_id().with("term"), Sense::click_and_drag());
        self.mouse(ui, term, rect, (cw, ch), size, &resp);
        size
    }

    fn mouse(&mut self, ui: &Ui, term: &Terminal, rect: Rect, cell: (f32, f32), size: TermSize, resp: &egui::Response) {
        let events = ui.input(|i| i.events.clone());
        let at = |pos: Pos2| cell_at(rect.min, cell, pos, size);
        for ev in events {
            match ev {
                egui::Event::PointerButton { pos, button, pressed, modifiers } => {
                    let Some(b) = mouse_button(button) else { continue };
                    if (pressed && !rect.contains(pos)) || (!pressed && self.down != Some(b)) {
                        continue;
                    }
                    let (col, row) = at(pos);
                    let action = if pressed { MouseAction::Press } else { MouseAction::Release };
                    let me = MouseEvent { button: b, action, col, row, mods: mods(modifiers) };
                    if pressed {
                        self.down = Some(b);
                        self.app_mouse = term.send_mouse(me);
                        if !self.app_mouse && b == MouseButton::Left {
                            self.anchor = Some((col, row));
                            self.selecting = false;
                            term.select_clear();
                        }
                    } else {
                        if self.app_mouse {
                            term.send_mouse(me);
                        }
                        self.down = None;
                        self.anchor = None;
                    }
                }
                egui::Event::PointerMoved(pos) => {
                    let (col, row) = at(pos);
                    match self.down {
                        Some(b) if self.app_mouse => {
                            let me =
                                MouseEvent { button: b, action: MouseAction::Motion, col, row, mods: Modifiers::NONE };
                            term.send_mouse(me);
                        }
                        Some(_) => {
                            if let Some(a) = self.anchor
                                && (self.selecting || a != (col, row))
                            {
                                if !self.selecting {
                                    term.select_start(a.0, a.1, SelectionKind::Simple);
                                    self.selecting = true;
                                }
                                term.select_update(col, row);
                            }
                        }
                        None if rect.contains(pos) => {
                            let me = MouseEvent {
                                button: MouseButton::None,
                                action: MouseAction::Motion,
                                col,
                                row,
                                mods: Modifiers::NONE,
                            };
                            term.send_mouse(me);
                        }
                        None => {}
                    }
                }
                egui::Event::MouseWheel { unit, delta, modifiers, .. } if resp.hovered() => {
                    let lines = match unit {
                        egui::MouseWheelUnit::Point => delta.y / cell.1,
                        egui::MouseWheelUnit::Line => delta.y,
                        egui::MouseWheelUnit::Page => delta.y * size.rows as f32,
                    };
                    self.wheel += lines;
                    let n = self.wheel.trunc();
                    self.wheel -= n;
                    let (col, row) = ui.input(|i| i.pointer.hover_pos()).map(at).unwrap_or((0, 0));
                    for _ in 0..(n.abs() as usize) {
                        let up = n > 0.0;
                        let button = if up { MouseButton::WheelUp } else { MouseButton::WheelDown };
                        let me = MouseEvent { button, action: MouseAction::Press, col, row, mods: mods(modifiers) };
                        if term.send_mouse(me) {
                            continue;
                        }
                        if self.alt_scroll {
                            term.send_key(if up { Key::Up } else { Key::Down }, Modifiers::NONE, false);
                        } else {
                            term.scroll(if up { 1 } else { -1 });
                        }
                    }
                }
                _ => {}
            }
        }
        if !self.app_mouse
            && let Some(pos) = resp.interact_pointer_pos()
        {
            let kind = if resp.triple_clicked() {
                Some(SelectionKind::Line)
            } else if resp.double_clicked() {
                Some(SelectionKind::Word)
            } else {
                None
            };
            if let Some(kind) = kind {
                let (col, row) = at(pos);
                term.select_start(col, row, kind);
                term.select_update(col, row);
            }
        }
    }
}
