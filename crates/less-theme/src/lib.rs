//! Tema ve font: IntelliJ paletleri, canlı önizleme, kalıcı seçim.
//! Renkler LESS-nvim'deki temalarla aynıdır (`nvim/lua/theme/intellij.lua`, tokyonight-night).

pub mod fonts;
pub mod semantic;
mod themes;

pub use semantic::{Extra, HighlightKind};
pub use themes::*;

/// sRGB renk (UI kütüphanesinden bağımsız; `less-app` kendi tipine çevirir).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// `0xrrggbb` biçiminde altı haneli hex'ten renk üretir.
    pub const fn hex(v: u32) -> Self {
        Self((v >> 16) as u8, (v >> 8) as u8, v as u8)
    }
}

/// Kod renkleri (önizleme ve ileride vurgulama için).
#[derive(Clone, Copy, Debug)]
pub struct Syntax {
    pub keyword: Rgb,
    pub string: Rgb,
    pub number: Rgb,
    pub function: Rgb,
    pub ty: Rgb,
    pub annotation: Rgb,
    pub comment: Rgb,
}

/// Arayüzün kullandığı anlamsal renkler.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub id: &'static str,
    pub name: &'static str,
    pub dark: bool,
    /// Editör / ana alan zemini.
    pub bg: Rgb,
    /// Giriş kutusu zemini.
    pub bg_dark: Rgb,
    /// Üstüne gelme ve seçili satır zemini.
    pub bg_highlight: Rgb,
    pub fg: Rgb,
    pub fg_dim: Rgb,
    /// Sönük metin (yol, zaman, ipucu).
    pub comment: Rgb,
    pub accent: Rgb,
    pub icon: Rgb,
    pub error: Rgb,
    /// Kenar çubuğu, araç pencereleri ve durum satırı zemini.
    pub surface: Rgb,
    /// Panel ayırıcı çizgiler ve giriş kutusu çerçevesi.
    pub border: Rgb,
    /// Vurgulu (birincil) butonun üstündeki yazı.
    pub on_accent: Rgb,
    /// Proje avatarları için ayırt edici renkler (ada göre seçilir).
    pub avatars: [Rgb; 8],
    pub syntax: Syntax,
    /// Sözdizimi dışı anlamsal renkler (diff, tanılama, ek vurgu grupları).
    pub extra: Extra,
}

const DARK_AVATARS: [Rgb; 8] = [
    Rgb::hex(0x7aa2f7),
    Rgb::hex(0xbb9af7),
    Rgb::hex(0x9ece6a),
    Rgb::hex(0xff9e64),
    Rgb::hex(0x7dcfff),
    Rgb::hex(0xf7768e),
    Rgb::hex(0xe0af68),
    Rgb::hex(0x1abc9c),
];

/// Tokyo Night (night) — LESS-nvim'de tema seçilmemişken kullanılan varsayılan (`tokyonight-night`).
pub const TOKYONIGHT_NIGHT: Palette = Palette {
    id: "tokyonight-night",
    name: "Tokyo Night",
    dark: true,
    bg: Rgb::hex(0x1a1b26),
    bg_dark: Rgb::hex(0x16161e),
    bg_highlight: Rgb::hex(0x292e42),
    fg: Rgb::hex(0xc0caf5),
    fg_dim: Rgb::hex(0xa9b1d6),
    comment: Rgb::hex(0x565f89),
    accent: Rgb::hex(0x7aa2f7),
    icon: Rgb::hex(0x7aa2f7),
    error: Rgb::hex(0xf7768e),
    surface: Rgb::hex(0x16161e),
    border: Rgb::hex(0x24283b),
    on_accent: Rgb::hex(0x1a1b26),
    avatars: DARK_AVATARS,
    syntax: Syntax {
        keyword: Rgb::hex(0xbb9af7),
        string: Rgb::hex(0x9ece6a),
        number: Rgb::hex(0xff9e64),
        function: Rgb::hex(0x7aa2f7),
        ty: Rgb::hex(0x2ac3de),
        annotation: Rgb::hex(0xff9e64),
        comment: Rgb::hex(0x565f89),
    },
    extra: Extra::new([
        0xc0caf5, 0xff9e64, 0x89ddff, 0x9abdf5, 0xf7768e, 0x73daca, 0x2ac3de, 0x9ece6a, 0xf7768e, 0xe0af68, 0x7aa2f7,
        0x7dcfff, 0x1abc9c,
    ]),
};

/// JetBrains "Dark" (New UI).
pub const INTELLIJ_DARK: Palette = Palette {
    id: "intellij-dark",
    name: "IntelliJ Dark",
    dark: true,
    bg: Rgb::hex(0x1e1f22),
    bg_dark: Rgb::hex(0x1e1f22),
    bg_highlight: Rgb::hex(0x393b40),
    fg: Rgb::hex(0xdfe1e5),
    fg_dim: Rgb::hex(0xbcbec4),
    comment: Rgb::hex(0x6f737a),
    accent: Rgb::hex(0x3574f0),
    icon: Rgb::hex(0xa1a3ab),
    error: Rgb::hex(0xf75464),
    surface: Rgb::hex(0x2b2d30),
    border: Rgb::hex(0x393b40),
    on_accent: Rgb::hex(0xffffff),
    avatars: DARK_AVATARS,
    syntax: Syntax {
        keyword: Rgb::hex(0xcf8e6d),
        string: Rgb::hex(0x6aab73),
        number: Rgb::hex(0x2aacb8),
        function: Rgb::hex(0x56a8f5),
        ty: Rgb::hex(0xbcbec4),
        annotation: Rgb::hex(0xb3ae60),
        comment: Rgb::hex(0x7a7e85),
    },
    extra: Extra::new([
        0xbcbec4, 0xc77dbb, 0xbcbec4, 0xbcbec4, 0xd5b778, 0xc77dbb, 0xbcbec4, 0x5fb865, 0xf75464, 0x6c95eb, 0xf2c55c,
        0x56a8f5, 0x7a7e85,
    ]),
};

/// JetBrains "IntelliJ Light".
pub const INTELLIJ_LIGHT: Palette = Palette {
    id: "intellij-light",
    name: "IntelliJ Light",
    dark: false,
    bg: Rgb::hex(0xffffff),
    bg_dark: Rgb::hex(0xffffff),
    bg_highlight: Rgb::hex(0xdfe1e5),
    fg: Rgb::hex(0x080808),
    fg_dim: Rgb::hex(0x3c3f45),
    comment: Rgb::hex(0x8c8c8c),
    accent: Rgb::hex(0x3574f0),
    icon: Rgb::hex(0x6c707e),
    error: Rgb::hex(0xe55757),
    surface: Rgb::hex(0xf7f8fa),
    border: Rgb::hex(0xdfe1e5),
    on_accent: Rgb::hex(0xffffff),
    avatars: [
        Rgb::hex(0x3574f0),
        Rgb::hex(0x871094),
        Rgb::hex(0x067d17),
        Rgb::hex(0xc25e00),
        Rgb::hex(0x0e7c86),
        Rgb::hex(0xd1384e),
        Rgb::hex(0x9e880d),
        Rgb::hex(0x20999d),
    ],
    syntax: Syntax {
        keyword: Rgb::hex(0x0033b3),
        string: Rgb::hex(0x067d17),
        number: Rgb::hex(0x1750eb),
        function: Rgb::hex(0x00627a),
        ty: Rgb::hex(0x080808),
        annotation: Rgb::hex(0x9e880d),
        comment: Rgb::hex(0x8c8c8c),
    },
    extra: Extra::new([
        0x080808, 0x871094, 0x080808, 0x080808, 0x0033b3, 0x871094, 0x080808, 0x067d17, 0xe55757, 0x3574f0, 0x9e880d,
        0x3574f0, 0x8c8c8c,
    ]),
};

/// Klasik "Darcula".
pub const DARCULA: Palette = Palette {
    id: "darcula",
    name: "Darcula",
    dark: true,
    bg: Rgb::hex(0x2b2b2b),
    bg_dark: Rgb::hex(0x313335),
    bg_highlight: Rgb::hex(0x4b4f52),
    fg: Rgb::hex(0xdddddd),
    fg_dim: Rgb::hex(0xa9b7c6),
    comment: Rgb::hex(0x808080),
    accent: Rgb::hex(0x4a88c7),
    icon: Rgb::hex(0xafb1b3),
    error: Rgb::hex(0xbc3f3c),
    surface: Rgb::hex(0x3c3f41),
    border: Rgb::hex(0x515151),
    on_accent: Rgb::hex(0xffffff),
    avatars: DARK_AVATARS,
    syntax: Syntax {
        keyword: Rgb::hex(0xcc7832),
        string: Rgb::hex(0x6a8759),
        number: Rgb::hex(0x6897bb),
        function: Rgb::hex(0xffc66d),
        ty: Rgb::hex(0xa9b7c6),
        annotation: Rgb::hex(0xbbb529),
        comment: Rgb::hex(0x808080),
    },
    extra: Extra::new([
        0xa9b7c6, 0x9876aa, 0xa9b7c6, 0xa9b7c6, 0xe8bf6a, 0x9876aa, 0xa9b7c6, 0x629755, 0xbc3f3c, 0x6897bb, 0xbbb529,
        0x4a88c7, 0x808080,
    ]),
};

/// Tema id'sine göre palet; bilinmeyen id'de varsayılan (Tokyo Night).
pub fn by_id(id: &str) -> Palette {
    ALL.into_iter().find(|p| p.id == id).unwrap_or(TOKYONIGHT_NIGHT)
}

/// Seçicide gösterilen sırayla tüm temalar.
pub const ALL: [Palette; 14] = [
    TOKYONIGHT_NIGHT,
    INTELLIJ_DARK,
    INTELLIJ_LIGHT,
    DARCULA,
    CATPPUCCIN_MOCHA,
    CATPPUCCIN_LATTE,
    GITHUB_DARK,
    GITHUB_LIGHT,
    ONE_DARK,
    KANAGAWA,
    ROSE_PINE,
    GRUVBOX_DARK,
    NIGHTFOX,
    VSCODE_DARK,
];
