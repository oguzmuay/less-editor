//! Mod motoru (Normal / Vim) ve tuş haritası. Varsayılan mod Vim.
//!
//! Akış: arayüz her tuşu `Engine::feed`'e verir, dönen `Output::commands` sırayla `Editor::apply` ile uygulanır.
//! `handled == false` ise tuş editörün değildir (Cmd kısayolları, F tuşları), uygulama işler.

pub mod common;
pub mod ex;
pub mod keys;
pub mod modeless;
pub mod vim;

use less_core::command::{Command, CursorMode};

pub use keys::{Key, KeyEvent, Mods, notation, parse_keys};
pub use modeless::ModelessEngine;
pub use vim::{VimEngine, VimMode};

/// Ayar `mode = "vim" | "normal"` (PLAN.md §2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum EditMode {
    #[default]
    Vim,
    Modeless,
}

/// Bir tuşun sonucu.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Output {
    pub commands: Vec<Command>,
    /// `false`: motor bu tuşu kullanmadı, uygulama kendi kısayoluna bakmalı.
    pub handled: bool,
}

impl Output {
    pub fn handled(commands: Vec<Command>) -> Self {
        Self { commands, handled: true }
    }

    pub fn unhandled() -> Self {
        Self { commands: Vec::new(), handled: false }
    }
}

/// Durum satırı bilgisi.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    /// `NORMAL`, `INSERT`, `VISUAL`, `V-LINE`, `V-BLOCK`, `REPLACE`, `COMMAND`; modeless'ta `EDIT`.
    pub mode_label: &'static str,
    /// Bekleyen tuşlar (`"a2d`).
    pub pending: String,
    /// Makro kaydı sürüyorsa register.
    pub recording: Option<char>,
    /// Komut satırı açıksa ön ekiyle metni (`:w`, `/foo`).
    pub cmdline: Option<String>,
}

/// Seçili moddaki giriş motoru. Mod değişince editör durumu korunur; `mode_commands` imleci uyarlar.
#[derive(Clone, Debug)]
pub enum Engine {
    Vim(Box<VimEngine>),
    Modeless(ModelessEngine),
}

impl Default for Engine {
    fn default() -> Self {
        Self::new(EditMode::Vim)
    }
}

impl Engine {
    pub fn new(mode: EditMode) -> Self {
        match mode {
            EditMode::Vim => Self::Vim(Box::default()),
            EditMode::Modeless => Self::Modeless(ModelessEngine),
        }
    }

    pub fn mode(&self) -> EditMode {
        match self {
            Self::Vim(_) => EditMode::Vim,
            Self::Modeless(_) => EditMode::Modeless,
        }
    }

    /// Motor açılınca ya da mod değişince editöre uygulanacak komutlar (imleç biçimi, görsel mod kapatma).
    pub fn mode_commands(&self) -> Vec<Command> {
        let cursor = match self {
            Self::Vim(_) => CursorMode::Block,
            Self::Modeless(_) => CursorMode::Bar,
        };
        vec![Command::SetOverwrite(false), Command::SetVisual(None), Command::SetCursorMode(cursor)]
    }

    /// Modu değiştirir (`:Mode`); dönen komutlar editöre uygulanmalı.
    pub fn set_mode(&mut self, mode: EditMode) -> Vec<Command> {
        let mut cmds = match self {
            Self::Vim(v) => v.reset(),
            Self::Modeless(_) => Vec::new(),
        };
        if self.mode() != mode {
            *self = Self::new(mode);
        }
        cmds.extend(self.mode_commands());
        cmds
    }

    pub fn feed(&mut self, key: KeyEvent) -> Output {
        match self {
            Self::Vim(v) => v.feed(key),
            Self::Modeless(m) => m.feed(key),
        }
    }

    pub fn status(&self) -> Status {
        match self {
            Self::Vim(v) => v.status(),
            Self::Modeless(m) => m.status(),
        }
    }
}
