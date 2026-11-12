//! Register deposu (Vim `"a`–`"z`, `"0`, `"1`–`"9`, `"-`, `"_`, `"+`) ve sistem panosu soyutlaması.

use std::collections::HashMap;

/// Sistem panosu; arayüz gerçek panoyu sağlar, testler `MemoryClipboard` kullanır.
pub trait Clipboard {
    fn get_text(&mut self) -> Option<String>;
    fn set_text(&mut self, text: String);
}

/// Bellekte pano (testler ve panosuz ortam).
#[derive(Clone, Debug, Default)]
pub struct MemoryClipboard {
    pub text: Option<String>,
}

impl Clipboard for MemoryClipboard {
    fn get_text(&mut self) -> Option<String> {
        self.text.clone()
    }

    fn set_text(&mut self, text: String) {
        self.text = Some(text);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegisterKind {
    Char,
    /// Metin `\n` ile biter.
    Line,
    /// Satırlar `\n` ile ayrılmış dikdörtgen.
    Block,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisterContent {
    pub text: String,
    pub kind: RegisterKind,
}

impl RegisterContent {
    pub fn new(text: impl Into<String>, kind: RegisterKind) -> Self {
        Self { text: text.into(), kind }
    }

    /// Panodan gelen metin: `\n` ile bitiyorsa satır bazlı sayılır (Vim gibi).
    pub fn from_clipboard(text: String) -> Self {
        let kind = if text.ends_with('\n') { RegisterKind::Line } else { RegisterKind::Char };
        Self { text, kind }
    }
}

/// Tüm sekmelerin paylaştığı register'lar. `use_system_clipboard` = LazyVim'in `clipboard=unnamedplus`'ı:
/// adsız register sistem panosuyla eşlenir.
#[derive(Clone, Debug)]
pub struct Registers {
    unnamed: Option<RegisterContent>,
    named: HashMap<char, RegisterContent>,
    numbered: [Option<RegisterContent>; 10],
    small_delete: Option<RegisterContent>,
    pub use_system_clipboard: bool,
}

impl Default for Registers {
    fn default() -> Self {
        Self {
            unnamed: None,
            named: HashMap::new(),
            numbered: Default::default(),
            small_delete: None,
            use_system_clipboard: true,
        }
    }
}

impl Registers {
    /// Kopyalanan / silinen metni yazar. `register` yoksa yank `"0`'a, çok satırlı silme `"1`'e (kayarak), kısa silme `"-`'e.
    pub fn store(
        &mut self,
        register: Option<char>,
        content: RegisterContent,
        is_delete: bool,
        clipboard: &mut dyn Clipboard,
    ) {
        match register {
            Some('_') => return,
            Some(c @ 'a'..='z') => {
                self.named.insert(c, content.clone());
            }
            Some(c @ 'A'..='Z') => {
                let key = c.to_ascii_lowercase();
                let merged = match self.named.remove(&key) {
                    Some(mut old) => {
                        if content.kind == RegisterKind::Line && old.kind == RegisterKind::Char {
                            old.text.push('\n');
                            old.kind = RegisterKind::Line;
                        }
                        old.text.push_str(&content.text);
                        old
                    }
                    None => content.clone(),
                };
                self.named.insert(key, merged.clone());
                self.unnamed = Some(merged.clone());
                if self.use_system_clipboard {
                    clipboard.set_text(merged.text);
                }
                return;
            }
            Some('+') | Some('*') => clipboard.set_text(content.text.clone()),
            _ => {
                if is_delete {
                    if content.kind != RegisterKind::Char || content.text.contains('\n') {
                        for i in (2..=9).rev() {
                            self.numbered[i] = self.numbered[i - 1].take();
                        }
                        self.numbered[1] = Some(content.clone());
                    } else {
                        self.small_delete = Some(content.clone());
                    }
                } else {
                    self.numbered[0] = Some(content.clone());
                }
                if self.use_system_clipboard {
                    clipboard.set_text(content.text.clone());
                }
            }
        }
        self.unnamed = Some(content);
    }

    /// Register içeriği; adsız register sistem panosuyla eşliyse pano okunur.
    pub fn get(&self, register: Option<char>, clipboard: &mut dyn Clipboard) -> Option<RegisterContent> {
        match register {
            None | Some('"') => {
                if self.use_system_clipboard
                    && let Some(text) = clipboard.get_text()
                {
                    return Some(self.match_clipboard(text));
                }
                self.unnamed.clone()
            }
            Some('+') | Some('*') => clipboard.get_text().map(|t| self.match_clipboard(t)),
            Some(c @ 'a'..='z') | Some(c @ 'A'..='Z') => self.named.get(&c.to_ascii_lowercase()).cloned(),
            Some(c @ '0'..='9') => self.numbered[c as usize - '0' as usize].clone(),
            Some('-') => self.small_delete.clone(),
            _ => None,
        }
    }

    fn match_clipboard(&self, text: String) -> RegisterContent {
        match &self.unnamed {
            Some(u) if u.text == text => u.clone(),
            _ => RegisterContent::from_clipboard(text),
        }
    }
}
