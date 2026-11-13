//! Klavye: uygulama kısayol tablosu (`docs/KEYMAP.md`); egui olaylarının çevirisi `less_input::egui`'de.

use less_input::{Key, KeyEvent};
use less_workbench::layout::Panel as Area;

use crate::picker::Mode;

/// Uygulama kısayolu: editör motorundan ve odaktaki panelden önce bakılır (kod odaktayken) ya da panel tuşu
/// kullanmazsa (panel odaktayken).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shortcut {
    /// Cmd+0..7, Cmd+M (Maven): aç / odakla / odaktaysa kapat (#5).
    Panel(Area),
    /// Cmd+Shift+0: `:UIReset` (#42).
    ResetLayout,
    Picker(Mode),
    /// Cmd+F: dosya içi arama çubuğu (Vim ve modeless).
    Find,
    NextTab,
    PrevTab,
    /// Cmd+T: bağlama göre yeni tab (#27).
    NewTab,
    CloseTab,
    /// Cmd+P: tab'ı pinle (#30).
    Pin,
    CloseProject,
    /// Cmd+Q: yalnız bu pencere (#75).
    Quit,
    /// Cmd+N: Notes; Files odaktayken yeni dosya (#53 #98).
    Notes,
    /// Cmd+Shift+N: yeni LESS-rs örneği (#98).
    NewWindow,
}

/// Kısayol tablosu. Cmd+F8, F7–F9, Cmd+Enter, Cmd+D gibi panel tuşları burada değil: kimse kullanmazsa
/// `Event::Shortcut` ile panellere gider.
pub fn shortcut(k: &KeyEvent) -> Option<Shortcut> {
    let m = k.mods;
    if !m.cmd || m.ctrl {
        return None;
    }
    let shift = m.shift;
    Some(match (k.key, shift, m.alt) {
        (Key::Char('0'), true, false) => Shortcut::ResetLayout,
        (Key::Char(c @ '0'..='7'), false, false) => Shortcut::Panel(match c {
            '0' => Area::Git,
            '1' => Area::Files,
            '2' => Area::Code,
            '3' => Area::Ai,
            '4' => Area::Terminal,
            '5' => Area::Database,
            '6' => Area::Run,
            _ => Area::GitHistory,
        }),
        (Key::Right, true, false) | (Key::Right, false, true) => Shortcut::NextTab,
        (Key::Left, true, false) | (Key::Left, false, true) => Shortcut::PrevTab,
        (Key::Char(c), _, false) => match (c.to_ascii_lowercase(), shift) {
            ('f', false) => Shortcut::Find,
            ('o', false) => Shortcut::Picker(Mode::Files),
            ('f', true) => Shortcut::Picker(Mode::Text),
            ('e', false) => Shortcut::Picker(Mode::Recent),
            ('k', false) => Shortcut::Picker(Mode::Shortcuts),
            ('p', true) => Shortcut::Picker(Mode::Commands),
            ('p', false) => Shortcut::Pin,
            ('t', false) => Shortcut::NewTab,
            ('w', false) => Shortcut::CloseTab,
            ('w', true) => Shortcut::CloseProject,
            ('q', false) => Shortcut::Quit,
            ('n', false) => Shortcut::Notes,
            ('n', true) => Shortcut::NewWindow,
            ('m', false) => Shortcut::Panel(Area::Maven),
            _ => return None,
        },
        _ => return None,
    })
}

/// egui olaylarının çevirisi `less-input`te (`egui` özelliği).
pub use less_input::egui::translate;

#[cfg(test)]
mod tests {
    use super::*;
    use less_input::Mods;

    #[test]
    fn cmd_shift_zero_resets_layout_42() {
        let k = KeyEvent::new(Key::Char('0'), Mods { shift: true, ..Mods::CMD });
        assert_eq!(shortcut(&k), Some(Shortcut::ResetLayout));
    }

    #[test]
    fn panel_shortcuts_follow_keymap_5() {
        let k = |c: char, shift: bool| KeyEvent::new(Key::Char(c), Mods { shift, ..Mods::CMD });
        assert_eq!(shortcut(&k('1', false)), Some(Shortcut::Panel(Area::Files)));
        assert_eq!(shortcut(&k('0', false)), Some(Shortcut::Panel(Area::Git)));
        assert_eq!(shortcut(&k('7', false)), Some(Shortcut::Panel(Area::GitHistory)));
        assert_eq!(shortcut(&k('F', true)), Some(Shortcut::Picker(Mode::Text)));
        assert_eq!(shortcut(&k('f', false)), Some(Shortcut::Find), "Cmd+F searches in the file");
        assert_eq!(shortcut(&k('o', false)), Some(Shortcut::Picker(Mode::Files)), "file search moved to Cmd+O");
        assert_eq!(shortcut(&k('w', false)), Some(Shortcut::CloseTab));
        assert_eq!(shortcut(&k('n', false)), Some(Shortcut::Notes));
        assert_eq!(shortcut(&k('N', true)), Some(Shortcut::NewWindow));
        assert_eq!(shortcut(&k('s', false)), None, "Cmd+S belongs to the editor");
        assert_eq!(shortcut(&KeyEvent::plain(Key::F(8))), None);
    }

    #[test]
    fn tab_navigation_21() {
        let shift_cmd = Mods { shift: true, ..Mods::CMD };
        let opt_cmd = Mods { alt: true, ..Mods::CMD };
        assert_eq!(shortcut(&KeyEvent::new(Key::Right, shift_cmd)), Some(Shortcut::NextTab));
        assert_eq!(shortcut(&KeyEvent::new(Key::Left, opt_cmd)), Some(Shortcut::PrevTab));
        assert_eq!(shortcut(&KeyEvent::new(Key::Left, Mods::CMD)), None, "Cmd+Left is line start (#12)");
    }
}
