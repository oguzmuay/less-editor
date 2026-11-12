//! Seçilebilir fontlar (#36). Varsayılan JetBrains Mono.

/// Tek font girdisi.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontEntry {
    pub family: &'static str,
    /// Ücretsiz ve Homebrew cask'i olan fontlar; `None` ise kullanıcı kendisi kurar.
    pub brew_cask: Option<&'static str>,
}

pub const DEFAULT: &str = "JetBrains Mono";

pub const FONTS: [FontEntry; 12] = [
    FontEntry { family: "JetBrains Mono", brew_cask: Some("font-jetbrains-mono") },
    FontEntry { family: "Geist Mono", brew_cask: Some("font-geist-mono") },
    FontEntry { family: "Berkeley Mono", brew_cask: None },
    FontEntry { family: "Cascadia Code", brew_cask: Some("font-cascadia-code") },
    FontEntry { family: "Fira Code", brew_cask: Some("font-fira-code") },
    FontEntry { family: "Maple Mono", brew_cask: Some("font-maple-mono") },
    FontEntry { family: "IBM Plex Mono", brew_cask: Some("font-ibm-plex-mono") },
    FontEntry { family: "Monaspace Neon", brew_cask: Some("font-monaspace") },
    FontEntry { family: "Monaspace Argon", brew_cask: Some("font-monaspace") },
    FontEntry { family: "Monaspace Xenon", brew_cask: Some("font-monaspace") },
    FontEntry { family: "Monaspace Radon", brew_cask: Some("font-monaspace") },
    FontEntry { family: "Monaspace Krypton", brew_cask: Some("font-monaspace") },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_font_is_listed_36() {
        assert!(FONTS.iter().any(|f| f.family == DEFAULT));
    }
}
