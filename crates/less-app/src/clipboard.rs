//! Sistem panosu (`arboard`, eframe'in de kullandığı). Pano açılamazsa bellekte pano kullanılır.

use less_core::registers::{Clipboard, MemoryClipboard};

/// Editör oturumunun panosu (`"+` register'ı, Cmd+C / Cmd+V).
pub struct SystemClipboard {
    system: Option<arboard::Clipboard>,
    memory: MemoryClipboard,
}

impl SystemClipboard {
    pub fn new() -> Self {
        Self { system: arboard::Clipboard::new().ok(), memory: MemoryClipboard::default() }
    }
}

impl Clipboard for SystemClipboard {
    fn get_text(&mut self) -> Option<String> {
        match &mut self.system {
            Some(c) => c.get_text().ok().map(|t| t.replace("\r\n", "\n")).or_else(|| self.memory.get_text()),
            None => self.memory.get_text(),
        }
    }

    fn set_text(&mut self, text: String) {
        if let Some(c) = &mut self.system
            && c.set_text(text.clone()).is_ok()
        {
            return;
        }
        self.memory.set_text(text);
    }
}
