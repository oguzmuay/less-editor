//! Yazı tipi kurulumu. Arayüz de kod da JetBrains Mono ile (LESS'in varsayılanı) çizilir; sistemde yoksa egui'nin
//! gömülü fontları kalır. Phosphor ikon fontu (gömülü, MIT) her ailenin sonunda yedektir.

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

const MONO: &str = "JetBrainsMono";
const ICONS: &str = "Phosphor";
const ICON_FONT: &[u8] = include_bytes!("../assets/Phosphor-Regular.ttf");

/// Verilen adlardan ilk okunabilen font dosyasını `dirs` klasörlerinde arar.
fn find(dirs: &[PathBuf], names: &[String]) -> Option<Vec<u8>> {
    names.iter().flat_map(|name| dirs.iter().map(move |d| d.join(name))).find_map(|p| std::fs::read(p).ok())
}

/// macOS font dosyalarında kullanılan yaygın sabit ve değişken adlar. `family` less-theme'deki kullanıcı adıdır.
fn candidates(family: &str) -> Vec<String> {
    let stem = family.replace(' ', "");
    let mut names = vec![
        format!("{stem}-Regular.ttf"),
        format!("{stem}-Regular.otf"),
        format!("{stem}[wght].ttf"),
        format!("{stem}.ttf"),
    ];
    if family == less_theme::fonts::DEFAULT {
        names.push("JetBrainsMonoNerdFont-Regular.ttf".to_owned());
    }
    if family == "Cascadia Code" {
        names.extend(["CascadiaMono.ttf".to_owned(), "CascadiaMono-Regular.ttf".to_owned()]);
    }
    names
}

/// Fontun bilinen kurulum klasörlerinde bulunup bulunmadığı. Hiçbir dış uygulamayı kontrol etmez.
pub fn installed(family: &str) -> bool {
    let dirs = font_dirs();
    let names = candidates(family);
    names.iter().any(|name| dirs.iter().any(|dir| dir.join(name).is_file()))
}

fn font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join("Library/Fonts"));
    }
    dirs.extend(["/Library/Fonts", "/opt/homebrew/share/fonts", "/System/Library/Fonts"].map(PathBuf::from));
    dirs
}

/// Seçili fontu iki egui ailesine de kurar; Phosphor ikonları her zaman yedektir. Font dosyası yoksa `false`
/// döndürür ve egui'nin gömülü fontlarını kullanır. Ghostty ya da başka bir uygulamaya dokunmaz.
pub fn install(ctx: &egui::Context, family: &str) -> bool {
    let dirs = font_dirs();
    let mut defs = FontDefinitions::default();
    let mono = find(&dirs, &candidates(family));
    let has_mono = mono.is_some();
    if let Some(bytes) = mono {
        defs.font_data.insert(MONO.to_owned(), Arc::new(FontData::from_owned(bytes)));
    }
    defs.font_data.insert(ICONS.to_owned(), Arc::new(FontData::from_static(ICON_FONT)));
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        let list = defs.families.entry(family).or_default();
        if has_mono {
            list.insert(0, MONO.to_owned());
        }
        list.push(ICONS.to_owned());
    }
    ctx.set_fonts(defs);
    has_mono
}

/// Kullanılan Phosphor ikonlarının kod noktaları (phosphoricons.com adlarıyla).
pub mod icon {
    pub const MAGNIFYING_GLASS: &str = "\u{E30C}";
    pub const FILE_PLUS: &str = "\u{E236}";
    pub const FOLDERS: &str = "\u{E260}";
    pub const FOLDER_OPEN: &str = "\u{E256}";
    pub const SQUARES_FOUR: &str = "\u{E464}";
    pub const TEXT_AA: &str = "\u{E6EE}";
    pub const PALETTE: &str = "\u{E6C8}";
    pub const KEYBOARD: &str = "\u{E2D8}";
    pub const INFO: &str = "\u{E2CE}";
    pub const CARET_RIGHT: &str = "\u{E13A}";
    pub const CARET_DOWN: &str = "\u{E136}";
    pub const CARET_UP: &str = "\u{E13C}";
    pub const CHECK: &str = "\u{E182}";
    pub const CLOCK: &str = "\u{E1A0}";
    pub const COPY: &str = "\u{E1CA}";
    pub const FILE: &str = "\u{E230}";
    pub const FILE_CODE: &str = "\u{E914}";
    pub const FILE_TEXT: &str = "\u{E23A}";
    pub const FOLDER: &str = "\u{E24A}";
    pub const GIT_BRANCH: &str = "\u{E278}";
    pub const LOCK_SIMPLE: &str = "\u{E308}";
    pub const PLUS: &str = "\u{E3D4}";
    pub const SIGN_OUT: &str = "\u{E42A}";
    pub const TERMINAL: &str = "\u{EAE8}";
    pub const TRASH: &str = "\u{E4A6}";
    pub const WARNING: &str = "\u{E4E0}";
    pub const X: &str = "\u{E4F6}";
}
