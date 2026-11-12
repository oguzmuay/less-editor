//! Run logları (#101): öğe başına pty'siz terminal ızgarası (`less_ai::term::Terminal::feed_only`), ham çıktı
//! `RunEvent::Output` ile beslenir; çizim `GridSnapshot`'tan.

use std::collections::HashMap;

use eframe::egui::{self, Color32, FontId, Rect, Ui, pos2, vec2};
use less_ai::term::{GridSnapshot, Key as TKey, KeyOptions, Modifiers, TermColor, TermSize, Terminal, encode_key};
use less_input::{Key, KeyEvent};
use less_theme::Palette;

use crate::widgets::color;

/// Log yazı boyutu.
pub const LOG_FONT: f32 = 12.5;

/// Öğe kimliğine göre log ızgaraları.
pub struct Logs {
    terms: HashMap<String, Terminal>,
    size: TermSize,
}

impl Default for Logs {
    fn default() -> Self {
        Self { terms: HashMap::new(), size: TermSize::new(120, 24) }
    }
}

impl Logs {
    /// Yeni çalıştırma: ızgara sıfırlanır.
    pub fn reset(&mut self, id: &str) {
        self.terms.insert(id.to_string(), Terminal::feed_only(self.size));
    }

    /// Ham çıktıyı ızgaraya işler.
    pub fn feed(&mut self, id: &str, bytes: &[u8]) {
        let size = self.size;
        self.terms.entry(id.to_string()).or_insert_with(|| Terminal::feed_only(size)).feed(bytes);
    }

    pub fn get(&self, id: &str) -> Option<&Terminal> {
        self.terms.get(id)
    }

    /// Görünüm boyutu değişti mi; değiştiyse tüm ızgaralar yeniden boyutlanır.
    pub fn resize(&mut self, size: TermSize) -> bool {
        if size == self.size {
            return false;
        }
        self.size = size;
        for t in self.terms.values() {
            t.resize(size);
        }
        true
    }

    pub fn size(&self) -> TermSize {
        self.size
    }
}

/// Uygulama tuşunu terminal baytlarına çevirir (log odaktayken sürece yazılır). Cmd'li tuşlar çevrilmez (host
/// kısayolları kalır).
pub fn encode(k: &KeyEvent, app_cursor: bool) -> Option<Vec<u8>> {
    if k.mods.cmd {
        return None;
    }
    let key = match k.key {
        Key::Char(c) => TKey::Char(c),
        Key::Enter => TKey::Enter,
        Key::Esc => TKey::Escape,
        Key::Tab => TKey::Tab,
        Key::Backspace => TKey::Backspace,
        Key::Delete => TKey::Delete,
        Key::Left => TKey::Left,
        Key::Right => TKey::Right,
        Key::Up => TKey::Up,
        Key::Down => TKey::Down,
        Key::Home => TKey::Home,
        Key::End => TKey::End,
        Key::PageUp => TKey::PageUp,
        Key::PageDown => TKey::PageDown,
        Key::F(n) => TKey::F(n),
    };
    let mods = Modifiers { shift: k.mods.shift, ctrl: k.mods.ctrl, alt: k.mods.alt, cmd: false };
    let opts = KeyOptions { app_cursor, ..KeyOptions::default() };
    let bytes: Vec<u8> = encode_key(key, mods, opts).concat();
    (!bytes.is_empty()).then_some(bytes)
}

/// Log ızgarasının hücre boyutu.
pub fn cell_size(ui: &Ui) -> (f32, f32) {
    let font = FontId::monospace(LOG_FONT);
    (ui.fonts_mut(|f| f.glyph_width(&font, 'M')), (LOG_FONT * 1.4).round())
}

/// `rect`'e sığan ızgara boyutu.
pub fn fit(rect: Rect, cell: (f32, f32)) -> TermSize {
    let cols = (rect.width() / cell.0.max(1.0)).floor().max(10.0) as u16;
    let rows = (rect.height() / cell.1.max(1.0)).floor().max(2.0) as u16;
    TermSize::new(cols, rows)
}

/// 0–15 ANSI renkleri (tema zemini ve yazısı uçlar için kullanılır).
const ANSI: [(u8, u8, u8); 16] = [
    (0x41, 0x48, 0x68),
    (0xf7, 0x76, 0x8e),
    (0x9e, 0xce, 0x6a),
    (0xe0, 0xaf, 0x68),
    (0x7a, 0xa2, 0xf7),
    (0xbb, 0x9a, 0xf7),
    (0x7d, 0xcf, 0xff),
    (0xa9, 0xb1, 0xd6),
    (0x56, 0x5f, 0x89),
    (0xff, 0x89, 0x9d),
    (0xb9, 0xf2, 0x7c),
    (0xff, 0x9e, 0x64),
    (0x8d, 0xb0, 0xff),
    (0xc7, 0xa9, 0xff),
    (0xa4, 0xda, 0xff),
    (0xc0, 0xca, 0xf5),
];

/// Hücre rengi; 16–231 xterm küpü, 232–255 gri rampa.
pub fn term_color(c: TermColor, p: &Palette, fg: bool) -> Color32 {
    match c {
        TermColor::DefaultFg => color(p.fg),
        TermColor::DefaultBg => color(p.surface),
        TermColor::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
        TermColor::Indexed(i) if i < 16 => {
            let (r, g, b) = ANSI[i as usize];
            if fg && i == 7 { color(p.fg_dim) } else { Color32::from_rgb(r, g, b) }
        }
        TermColor::Indexed(i) if i < 232 => {
            let n = i - 16;
            let step = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            Color32::from_rgb(step(n / 36), step((n / 6) % 6), step(n % 6))
        }
        TermColor::Indexed(i) => {
            let v = 8 + (i - 232) * 10;
            Color32::from_rgb(v, v, v)
        }
    }
}

/// Izgarayı çizer: aynı biçimdeki ardışık hücreler tek metin parçası.
pub fn paint(ui: &mut Ui, rect: Rect, snap: &GridSnapshot, p: &Palette, cell: (f32, f32)) {
    let painter = ui.painter_at(rect);
    let font = FontId::monospace(LOG_FONT);
    for (r, row) in snap.cells.iter().enumerate() {
        let y = rect.top() + r as f32 * cell.1;
        let mut c = 0;
        while c < row.len() {
            let first = &row[c];
            let (mut fg, mut bg) = (term_color(first.fg, p, true), term_color(first.bg, p, false));
            if first.style.inverse {
                std::mem::swap(&mut fg, &mut bg);
            }
            if first.style.dim {
                fg = fg.gamma_multiply(0.6);
            }
            let mut text = String::new();
            let start = c;
            while c < row.len()
                && row[c].fg == first.fg
                && row[c].bg == first.bg
                && row[c].style == first.style
                && row[c].selected == first.selected
            {
                if !row[c].wide_spacer {
                    text.push(if row[c].style.hidden { ' ' } else { row[c].ch });
                }
                c += 1;
            }
            let x = rect.left() + start as f32 * cell.0;
            let run = Rect::from_min_size(pos2(x, y), vec2((c - start) as f32 * cell.0, cell.1));
            if first.selected {
                painter.rect_filled(run, 0.0, color(p.accent).gamma_multiply(0.35));
            } else if bg != color(p.surface) {
                painter.rect_filled(run, 0.0, bg);
            }
            if !text.trim().is_empty() {
                painter.text(pos2(x, y + cell.1 / 2.0), egui::Align2::LEFT_CENTER, text, font.clone(), fg);
            }
        }
    }
}

/// Log ızgarasındaki tıklanabilir bağlantı: URL ve kapladığı `(satır, ilk sütun, son sütun hariç)` parçaları.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub url: String,
    pub spans: Vec<(usize, usize, usize)>,
}

impl Link {
    /// Hücre bu bağlantının üstünde mi.
    pub fn contains(&self, row: usize, col: usize) -> bool {
        self.spans.iter().any(|&(r, a, b)| r == row && (a..b).contains(&col))
    }
}

/// Izgaradaki http(s) URL'leri (renk ANSI'si hücre metnine girmez). Son sütuna dayanan URL bir sonraki satırın
/// başından devam eder.
// shortcut: sarma bilgisi ızgarada yok, son sütunda biten URL sarılmış sayılır; yanlış birleşirse WRAPLINE bayrağı okunmalı.
pub fn links(snap: &GridSnapshot) -> Vec<Link> {
    let rows: Vec<Vec<char>> = snap.cells.iter().map(|r| r.iter().map(|c| c.ch).collect()).collect();
    let url_char = |c: char| c.is_ascii_graphic() && !matches!(c, '"' | '\'' | '<' | '>' | '`');
    let mut out = Vec::new();
    let (mut r, mut c) = (0, 0);
    while r < rows.len() {
        let row = &rows[r];
        let start = (c..row.len()).find(|&i| {
            let rest = &row[i..];
            (rest.starts_with(&['h', 't', 't', 'p', ':', '/', '/'])
                || rest.starts_with(&['h', 't', 't', 'p', 's', ':', '/', '/']))
                && (i == 0 || !url_char(row[i - 1]) || matches!(row[i - 1], '(' | '['))
        });
        let Some(start) = start else {
            (r, c) = (r + 1, 0);
            continue;
        };
        let mut link = Link { url: String::new(), spans: Vec::new() };
        let (mut lr, mut from) = (r, start);
        loop {
            let line = &rows[lr];
            let end = (from..line.len()).find(|&i| !url_char(line[i])).unwrap_or(line.len());
            link.url.extend(&line[from..end]);
            link.spans.push((lr, from, end));
            (r, c) = (lr, end);
            if end < line.len()
                || end == from
                || lr + 1 >= rows.len()
                || !rows[lr + 1].first().is_some_and(|&ch| url_char(ch))
            {
                break;
            }
            (lr, from) = (lr + 1, 0);
        }
        while link.url.ends_with(['.', ',', ';', ':', '!', '?', ')', ']']) {
            link.url.pop();
            if let Some(last) = link.spans.last_mut() {
                last.2 -= 1;
                if last.2 == last.1 {
                    link.spans.pop();
                }
            }
        }
        if link.url.len() > "https://".len() {
            out.push(link);
        }
    }
    out
}
