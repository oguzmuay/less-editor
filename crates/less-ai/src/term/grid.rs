//! Render'a hazır terminal ızgarası: UI (egui) yalnız bu düz veriyi çizer, `alacritty_terminal` tiplerini görmez.

/// Hücre rengi. `Indexed` 0–15 ANSI renkleri (UI kendi paletinden çözer), 16–255 256 renk küpü / gri rampa.
/// Uygulama OSC 4 ile bir indeksi değiştirmişse renk `Rgb` olarak gelir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TermColor {
    /// Temanın ön plan rengi.
    DefaultFg,
    /// Temanın arka plan rengi.
    DefaultBg,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

/// Hücre biçimi bayrakları.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct CellStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
    pub dim: bool,
    pub strikeout: bool,
    pub hidden: bool,
}

/// Tek ekran hücresi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridCell {
    /// Hücre karakteri (boş hücre `' '`). Geniş karakterin ikinci hücresinde (`wide_spacer`) `' '`.
    pub ch: char,
    /// Birleşen (sıfır genişlikli) karakterler, varsa (ör. aksan, emoji varyant seçici).
    pub zerowidth: Option<String>,
    pub fg: TermColor,
    pub bg: TermColor,
    pub style: CellStyle,
    /// İki sütun kaplayan karakterin ilk hücresi.
    pub wide: bool,
    /// Geniş karakterin ikinci (boş) hücresi: UI çizmez.
    pub wide_spacer: bool,
    /// Hücre seçimin içinde.
    pub selected: bool,
}

impl Default for GridCell {
    fn default() -> Self {
        Self {
            ch: ' ',
            zerowidth: None,
            fg: TermColor::DefaultFg,
            bg: TermColor::DefaultBg,
            style: CellStyle::default(),
            wide: false,
            wide_spacer: false,
            selected: false,
        }
    }
}

/// İmleç biçimi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorShape {
    Block,
    Underline,
    Beam,
    HollowBlock,
}

/// İmleç: görünüm (viewport) koordinatlarında. Geçmişe kaydırılmışken ekran dışında kalırsa `visible = false`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorView {
    pub row: usize,
    pub col: usize,
    pub shape: CursorShape,
    pub visible: bool,
}

/// Uygulamanın açtığı terminal kipleri (UI tuş/fare/yapıştırma kodlaması için).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModeFlags {
    pub app_cursor: bool,
    pub app_keypad: bool,
    pub bracketed_paste: bool,
    pub alt_screen: bool,
    /// Fare tıklaması raporlanır (1000).
    pub mouse_click: bool,
    /// Basılıyken sürükleme raporlanır (1002).
    pub mouse_drag: bool,
    /// Her hareket raporlanır (1003).
    pub mouse_motion: bool,
    /// SGR fare kodlaması (1006).
    pub sgr_mouse: bool,
    /// Odak giriş/çıkış raporu (1004).
    pub focus_events: bool,
    /// Alt ekranda tekerlek ok tuşuna çevrilir (1007).
    pub alternate_scroll: bool,
}

impl ModeFlags {
    /// Herhangi bir fare raporlama kipi açık mı?
    pub fn mouse_reporting(&self) -> bool {
        self.mouse_click || self.mouse_drag || self.mouse_motion
    }
}

/// Bir anlık çizim verisi. `generation` değişmediyse UI yeniden çizmek zorunda değildir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridSnapshot {
    pub cols: usize,
    pub rows: usize,
    /// `rows` satır, her biri `cols` hücre.
    pub cells: Vec<Vec<GridCell>>,
    pub cursor: CursorView,
    /// Kaç satır geçmişe kaydırılmış (0 = en alt, canlı).
    pub display_offset: usize,
    /// Geçmişte tutulan satır sayısı (kaydırma çubuğu için).
    pub history_size: usize,
    pub mode: ModeFlags,
    pub title: Option<String>,
    pub generation: u64,
}

impl GridSnapshot {
    /// Satırın düz metni (geniş karakter boşlukları atlanır, sondaki boşluklar kırpılır). Test ve arama için.
    pub fn row_text(&self, row: usize) -> String {
        let Some(cells) = self.cells.get(row) else { return String::new() };
        let mut s = String::new();
        for c in cells.iter().filter(|c| !c.wide_spacer) {
            s.push(c.ch);
            if let Some(z) = &c.zerowidth {
                s.push_str(z);
            }
        }
        s.trim_end().to_string()
    }

    /// Tüm görünür satırlar, `\n` ile birleşik (sondaki boş satırlar atılır).
    pub fn text(&self) -> String {
        let lines: Vec<String> = (0..self.rows).map(|r| self.row_text(r)).collect();
        lines.join("\n").trim_end().to_string()
    }
}
