//! Klavye: egui olaylarını `less-input` tuşlarına çevirir ve uygulama kısayol tablosunu (`docs/KEYMAP.md`) tutar.
//! UI'dan bağımsız; testler egui olaylarını elle kurar.

use eframe::egui;
use less_input::{Key, KeyEvent, Mods};
use less_workbench::layout::Panel as Area;

use crate::picker::Mode;

/// Uygulama kısayolu: editör motorundan ve odaktaki panelden önce bakılır (kod odaktayken) ya da panel tuşu
/// kullanmazsa (panel odaktayken).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shortcut {
    /// Cmd+0..7: aç / odakla / odaktaysa kapat (#5).
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
            _ => return None,
        },
        _ => return None,
    })
}

/// Bu karedeki egui olaylarından tuş listesi.
///
/// - Yazı üreten tuşlar `Event::Text`'ten gelir (Key olayı atlanır), böylece klavye düzeni (Türkçe Q) doğru çalışır.
/// - Cmd / Ctrl kombinasyonları Key olayından; rakamlarda fiziksel tuş kullanılır (Cmd+Shift+0 düzenden bağımsız, #42).
/// - Option + harf: ardından ASCII yazı gelirse (Türkçe düzende Option+Q = `@`) yazıdır, ASCII dışıysa (`∆`)
///   `<M-j>` gibi Option kısayoludur.
/// - egui'nin Copy / Cut / Paste olayları `<D-c>` / `<D-x>` / `<D-v>` olur; yapıştırma metni panodan editör okur.
pub fn translate(events: &[egui::Event]) -> Vec<KeyEvent> {
    let mut out = Vec::new();
    let mut pending_alt: Option<KeyEvent> = None;
    let mut skip_text = false;
    for ev in events {
        if !matches!(ev, egui::Event::Text(_))
            && let Some(alt) = pending_alt.take()
        {
            out.push(alt);
        }
        match ev {
            egui::Event::Key { key, physical_key, pressed: true, modifiers, .. } => {
                skip_text = false;
                let mods = mods(*modifiers);
                if let Some(k) = special(*key) {
                    out.push(KeyEvent::new(k, mods));
                } else if let Some(c) = char_of(*key, *physical_key, mods) {
                    let ev = KeyEvent::new(Key::Char(c), mods);
                    if mods.cmd || mods.ctrl {
                        out.push(ev);
                        skip_text = true;
                    } else if mods.alt {
                        pending_alt = Some(ev);
                    }
                }
            }
            egui::Event::Text(text) => {
                if let Some(alt) = pending_alt.take() {
                    if !text.is_ascii() {
                        out.push(alt);
                        continue;
                    }
                } else if std::mem::take(&mut skip_text) {
                    continue;
                }
                out.extend(text.chars().map(KeyEvent::char));
            }
            egui::Event::Copy => out.push(KeyEvent::new(Key::Char('c'), Mods::CMD)),
            egui::Event::Cut => out.push(KeyEvent::new(Key::Char('x'), Mods::CMD)),
            egui::Event::Paste(_) => out.push(KeyEvent::new(Key::Char('v'), Mods::CMD)),
            _ => {}
        }
    }
    out.extend(pending_alt);
    out
}

/// egui değiştiricileri. `cmd` macOS Command tuşudur.
fn mods(m: egui::Modifiers) -> Mods {
    // shortcut: Linux/Windows'ta Super tuşu `cmd` sayılmaz (egui `mac_cmd` yalnız macOS'ta); oralara port edilince bakılır.
    Mods { shift: m.shift, ctrl: m.ctrl, alt: m.alt, cmd: m.mac_cmd }
}

/// Yazı üretmeyen tuşlar.
fn special(key: egui::Key) -> Option<Key> {
    use egui::Key as E;
    Some(match key {
        E::Enter => Key::Enter,
        E::Escape => Key::Esc,
        E::Tab => Key::Tab,
        E::Backspace => Key::Backspace,
        E::Delete => Key::Delete,
        E::ArrowLeft => Key::Left,
        E::ArrowRight => Key::Right,
        E::ArrowUp => Key::Up,
        E::ArrowDown => Key::Down,
        E::Home => Key::Home,
        E::End => Key::End,
        E::PageUp => Key::PageUp,
        E::PageDown => Key::PageDown,
        E::F1 => Key::F(1),
        E::F2 => Key::F(2),
        E::F3 => Key::F(3),
        E::F4 => Key::F(4),
        E::F5 => Key::F(5),
        E::F6 => Key::F(6),
        E::F7 => Key::F(7),
        E::F8 => Key::F(8),
        E::F9 => Key::F(9),
        E::F10 => Key::F(10),
        E::F11 => Key::F(11),
        E::F12 => Key::F(12),
        _ => return None,
    })
}

/// Yazı üreten tuşun karakteri (değiştiricili kısayollar için). Harfler Shift ile büyük harf olur.
fn char_of(key: egui::Key, physical: Option<egui::Key>, mods: Mods) -> Option<char> {
    use egui::Key as E;
    let key = match physical {
        Some(p) if (mods.cmd || mods.ctrl) && p.name().len() == 1 && p.name().as_bytes()[0].is_ascii_digit() => p,
        _ => key,
    };
    let c = match key {
        E::Space => ' ',
        E::Minus => '-',
        E::Quote => '\'',
        _ => {
            let s = key.symbol_or_name();
            let mut chars = s.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => c,
                _ => return None,
            }
        }
    };
    Some(if c.is_ascii_alphabetic() && !mods.shift { c.to_ascii_lowercase() } else { c })
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Key as E, Modifiers};

    fn key(k: E, m: Modifiers) -> Event {
        Event::Key { key: k, physical_key: Some(k), pressed: true, repeat: false, modifiers: m }
    }

    fn cmd() -> Modifiers {
        Modifiers { mac_cmd: true, command: true, ..Default::default() }
    }

    #[test]
    fn plain_typing_comes_from_text_events() {
        let evs = [key(E::D, Modifiers::NONE), Event::Text("d".into()), key(E::Num2, Modifiers::SHIFT)];
        let evs: Vec<Event> = evs.into_iter().chain([Event::Text("'".into())]).collect();
        assert_eq!(translate(&evs), vec![KeyEvent::char('d'), KeyEvent::char('\'')]);
    }

    #[test]
    fn cmd_digit_uses_physical_key_42() {
        let ev = Event::Key {
            key: E::Equals,
            physical_key: Some(E::Num0),
            pressed: true,
            repeat: false,
            modifiers: Modifiers { shift: true, ..cmd() },
        };
        let out = translate(&[ev]);
        assert_eq!(out, vec![KeyEvent::new(Key::Char('0'), Mods { shift: true, ..Mods::CMD })]);
        assert_eq!(shortcut(&out[0]), Some(Shortcut::ResetLayout));
    }

    #[test]
    fn option_letter_is_alt_unless_layout_types_ascii() {
        let alt = Modifiers::ALT;
        let us = translate(&[key(E::J, alt), Event::Text("∆".into())]);
        assert_eq!(us, vec![KeyEvent::new(Key::Char('j'), Mods::ALT)]);
        let tr = translate(&[key(E::Q, alt), Event::Text("@".into())]);
        assert_eq!(tr, vec![KeyEvent::char('@')]);
        let bare = translate(&[key(E::K, alt)]);
        assert_eq!(bare, vec![KeyEvent::new(Key::Char('k'), Mods::ALT)]);
    }

    #[test]
    fn clipboard_events_become_cmd_keys() {
        let out = translate(&[Event::Copy, Event::Paste("x".into())]);
        assert_eq!(out, vec![KeyEvent::new(Key::Char('c'), Mods::CMD), KeyEvent::new(Key::Char('v'), Mods::CMD)]);
    }

    #[test]
    fn special_keys_and_released_events() {
        let released =
            Event::Key { key: E::A, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE };
        let out = translate(&[key(E::Escape, Modifiers::NONE), released, key(E::Tab, Modifiers::SHIFT)]);
        assert_eq!(out, vec![KeyEvent::plain(Key::Esc), KeyEvent::new(Key::Tab, Mods::SHIFT)]);
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
