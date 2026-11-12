//! Kullanıcı ayarları: `<config>/config.toml` (varsayılan `~/.config/less-rs/config.toml`, testte
//! `LESS_RS_CONFIG_DIR`). Bilinmeyen ya da bozuk değerler varsayılana düşer; dosya yalnız bir ayar değişince yazılır.

use std::path::PathBuf;

use less_workspace::paths;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Keymap {
    #[default]
    Vim,
    Normal,
}

impl Keymap {
    pub fn label(self) -> &'static str {
        match self {
            Keymap::Vim => "Vim",
            Keymap::Normal => "Normal",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub keymap: Keymap,
    pub theme: String,
    /// LESS-rs'in kendi egui arayüzünde ve kod alanında kullandığı yazı tipi (#36).
    pub font: String,
    /// Spring Boot projelerinde Spring Boot Language Server'ı jdtls'in yanında çalıştır (`:options`).
    pub spring_boot: bool,
    /// jdtls yalnız açılan dosyaların modüllerini yükler, diğerlerini `~/.m2` jar'larından okur (`:options`).
    pub java_on_demand: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            keymap: Keymap::Vim,
            theme: less_theme::TOKYONIGHT_NIGHT.id.to_owned(),
            font: less_theme::fonts::DEFAULT.to_owned(),
            spring_boot: true,
            java_on_demand: true,
        }
    }
}

fn file() -> PathBuf {
    paths::config_dir().join("config.toml")
}

impl Settings {
    pub fn load() -> Self {
        std::fs::read_to_string(file()).ok().and_then(|s| toml::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let text = toml::to_string(self).map_err(std::io::Error::other)?;
        paths::write_atomic(&file(), text.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_and_font_are_persistent_settings_36_71() {
        let settings =
            Settings { theme: less_theme::DARCULA.id.into(), font: "Fira Code".into(), ..Settings::default() };
        let text = toml::to_string(&settings).unwrap();
        let back: Settings = toml::from_str(&text).unwrap();
        assert_eq!(back, settings);
        let defaults: Settings = toml::from_str("keymap = \"vim\"").unwrap();
        assert_eq!(defaults.font, less_theme::fonts::DEFAULT);
        assert!(defaults.spring_boot, "Spring Boot LS is on unless turned off");
        assert!(defaults.java_on_demand, "jdtls on-demand import is on unless turned off");
    }
}
