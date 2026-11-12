//! Tuş, yapıştırma, fare ve odak olaylarını terminale gidecek baytlara çevirir (xterm dizileri).
//! UI bağımsız: egui tarafı kendi olayını [`Key`] + [`Modifiers`]'a çevirip [`encode_key`] çağırır.

use super::grid::ModeFlags;

/// Terminale gönderilebilen tuşlar. Yazdırılabilir karakter UI'nin üretdiği (Shift uygulanmış) haliyle `Char`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Char(char),
    Enter,
    Tab,
    Backspace,
    Escape,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    /// F1–F12.
    F(u8),
}

/// Değiştirici tuşlar. `alt` macOS'ta Option, `cmd` macOS Command (yalnız birkaç kabuk kısayolu üretir).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub cmd: bool,
}

impl Modifiers {
    pub const NONE: Modifiers = Modifiers { shift: false, ctrl: false, alt: false, cmd: false };
    pub const SHIFT: Modifiers = Modifiers { shift: true, ctrl: false, alt: false, cmd: false };
    pub const CTRL: Modifiers = Modifiers { shift: false, ctrl: true, alt: false, cmd: false };
    pub const ALT: Modifiers = Modifiers { shift: false, ctrl: false, alt: true, cmd: false };
    pub const CMD: Modifiers = Modifiers { shift: false, ctrl: false, alt: false, cmd: true };

    /// xterm değiştirici parametresi (`1 + shift + 2·alt + 4·ctrl`); değiştirici yoksa 1.
    fn xterm_param(&self) -> u8 {
        1 + u8::from(self.shift) + 2 * u8::from(self.alt) + 4 * u8::from(self.ctrl)
    }

    fn any_xterm(&self) -> bool {
        self.shift || self.alt || self.ctrl
    }
}

/// Kodlama seçenekleri.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeyOptions {
    /// DECCKM (uygulama imleç tuşları): oklar `ESC O A` biçiminde.
    pub app_cursor: bool,
    /// Shift+Enter yeni satır: `\` ve kısa bir aradan sonra `\r` (Claude Code buffer'larındaki davranış, karar #27).
    pub shift_enter_newline: bool,
    /// Option+Sol/Sağ kelime atlama (`ESC b` / `ESC f`), Cmd+Sol/Sağ satır başı/sonu (Ctrl+A / Ctrl+E), karar #12.
    pub mac_line_editing: bool,
}

impl KeyOptions {
    /// Terminal kiplerinden (DECCKM) ve tab türünden seçenek üretir.
    pub fn from_mode(mode: &ModeFlags, shift_enter_newline: bool) -> Self {
        Self { app_cursor: mode.app_cursor, shift_enter_newline, mac_line_editing: true }
    }
}

/// Tuşu baytlara çevirir. Dönen her parça ayrı yazılır; birden çok parça varsa aralarında ~10 ms beklenir
/// (yalnız Shift+Enter yeni satırı). Boş liste = terminale gönderilecek bir şey yok (ör. bilinmeyen Cmd kısayolu).
pub fn encode_key(key: Key, mods: Modifiers, opts: KeyOptions) -> Vec<Vec<u8>> {
    if key == Key::Enter && mods.shift && !mods.ctrl && !mods.alt && !mods.cmd && opts.shift_enter_newline {
        return vec![b"\\".to_vec(), b"\r".to_vec()];
    }
    let bytes = encode_single(key, mods, opts);
    if bytes.is_empty() { Vec::new() } else { vec![bytes] }
}

/// Tek parçalı kodlama; [`encode_key`]'in Shift+Enter dışındaki her durumu.
pub fn encode_single(key: Key, mods: Modifiers, opts: KeyOptions) -> Vec<u8> {
    if mods.cmd {
        return encode_cmd(key, opts);
    }
    if opts.mac_line_editing && mods.alt && !mods.ctrl && !mods.shift {
        match key {
            Key::Left => return b"\x1bb".to_vec(),
            Key::Right => return b"\x1bf".to_vec(),
            _ => {}
        }
    }
    match key {
        Key::Char(c) => encode_char(c, mods),
        Key::Enter => with_alt(mods, b"\r"),
        Key::Tab if mods.shift => b"\x1b[Z".to_vec(),
        Key::Tab => with_alt(mods, b"\t"),
        Key::Backspace if mods.ctrl => with_alt(mods, b"\x08"),
        Key::Backspace => with_alt(mods, b"\x7f"),
        Key::Escape => with_alt(mods, b"\x1b"),
        Key::Up => cursor_key(b'A', mods, opts),
        Key::Down => cursor_key(b'B', mods, opts),
        Key::Right => cursor_key(b'C', mods, opts),
        Key::Left => cursor_key(b'D', mods, opts),
        Key::Home => cursor_key(b'H', mods, opts),
        Key::End => cursor_key(b'F', mods, opts),
        Key::Insert => tilde_key(2, mods),
        Key::Delete => tilde_key(3, mods),
        Key::PageUp => tilde_key(5, mods),
        Key::PageDown => tilde_key(6, mods),
        Key::F(n) => function_key(n, mods),
    }
}

/// Cmd kombinasyonlarından kabuğa anlamlı olanlar (karar #12): satır başı/sonu ve satır silme.
fn encode_cmd(key: Key, opts: KeyOptions) -> Vec<u8> {
    if !opts.mac_line_editing {
        return Vec::new();
    }
    match key {
        Key::Left => vec![0x01],
        Key::Right => vec![0x05],
        Key::Backspace => vec![0x15],
        _ => Vec::new(),
    }
}

fn with_alt(mods: Modifiers, bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 1);
    if mods.alt {
        out.push(0x1b);
    }
    out.extend_from_slice(bytes);
    out
}

fn encode_char(c: char, mods: Modifiers) -> Vec<u8> {
    if mods.ctrl {
        let lower = c.to_ascii_lowercase();
        let ctrl = match lower {
            'a'..='z' => Some(lower as u8 - b'a' + 1),
            '@' | ' ' | '2' => Some(0),
            '[' | '3' => Some(0x1b),
            '\\' | '4' => Some(0x1c),
            ']' | '5' => Some(0x1d),
            '^' | '6' => Some(0x1e),
            '_' | '-' | '7' => Some(0x1f),
            '?' | '8' => Some(0x7f),
            _ => None,
        };
        if let Some(b) = ctrl {
            return with_alt(mods, &[b]);
        }
    }
    let mut buf = [0u8; 4];
    with_alt(mods, c.encode_utf8(&mut buf).as_bytes())
}

fn cursor_key(letter: u8, mods: Modifiers, opts: KeyOptions) -> Vec<u8> {
    if mods.any_xterm() {
        return format!("\x1b[1;{}{}", mods.xterm_param(), letter as char).into_bytes();
    }
    if opts.app_cursor { vec![0x1b, b'O', letter] } else { vec![0x1b, b'[', letter] }
}

fn tilde_key(code: u8, mods: Modifiers) -> Vec<u8> {
    if mods.any_xterm() {
        format!("\x1b[{code};{}~", mods.xterm_param()).into_bytes()
    } else {
        format!("\x1b[{code}~").into_bytes()
    }
}

fn function_key(n: u8, mods: Modifiers) -> Vec<u8> {
    let ss3 = match n {
        1 => Some(b'P'),
        2 => Some(b'Q'),
        3 => Some(b'R'),
        4 => Some(b'S'),
        _ => None,
    };
    if let Some(letter) = ss3 {
        return if mods.any_xterm() {
            format!("\x1b[1;{}{}", mods.xterm_param(), letter as char).into_bytes()
        } else {
            vec![0x1b, b'O', letter]
        };
    }
    let code = match n {
        5 => 15,
        6 => 17,
        7 => 18,
        8 => 19,
        9 => 20,
        10 => 21,
        11 => 23,
        12 => 24,
        _ => return Vec::new(),
    };
    tilde_key(code, mods)
}

/// Yapıştırılan metni kodlar. Bracketed paste açıksa `ESC[200~ … ESC[201~` ile sarılır (metindeki ESC atılır ki
/// içerik bitiş işaretini taklit edemesin); kapalıysa satır sonları `\r` olur.
pub fn encode_paste(text: &str, bracketed: bool) -> Vec<u8> {
    let normalized = text.replace("\r\n", "\n");
    if bracketed {
        let body: String = normalized.chars().filter(|&c| c != '\x1b').collect();
        let mut out = b"\x1b[200~".to_vec();
        out.extend_from_slice(body.as_bytes());
        out.extend_from_slice(b"\x1b[201~");
        out
    } else {
        normalized.replace('\n', "\r").into_bytes()
    }
}

/// Odak olayı (yalnız uygulama 1004 kipini açtıysa gönderilmeli).
pub fn encode_focus(focused: bool) -> Vec<u8> {
    if focused { b"\x1b[I".to_vec() } else { b"\x1b[O".to_vec() }
}

/// Fare düğmesi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
    /// Hareket olayında basılı düğme yok.
    None,
}

/// Fare olayının türü.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseAction {
    Press,
    Release,
    Motion,
}

/// Görünüm koordinatlarında (0 tabanlı sütun/satır) fare olayı.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseEvent {
    pub button: MouseButton,
    pub action: MouseAction,
    pub col: usize,
    pub row: usize,
    pub mods: Modifiers,
}

/// Fare olayını uygulamanın istediği kipe göre kodlar. Uygulama bu olayı istemiyorsa `None`
/// (UI o zaman olayı kendisi işler: seçim, kaydırma).
pub fn encode_mouse(ev: MouseEvent, mode: &ModeFlags) -> Option<Vec<u8>> {
    if !mode.mouse_reporting() {
        return None;
    }
    let wheel = matches!(ev.button, MouseButton::WheelUp | MouseButton::WheelDown);
    match ev.action {
        MouseAction::Motion if ev.button == MouseButton::None && !mode.mouse_motion => return None,
        MouseAction::Motion if !(mode.mouse_motion || mode.mouse_drag) => return None,
        MouseAction::Release if wheel => return None,
        _ => {}
    }
    let mut code: u32 = match ev.button {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
        MouseButton::None => 3,
        MouseButton::WheelUp => 64,
        MouseButton::WheelDown => 65,
    };
    if ev.action == MouseAction::Motion {
        code += 32;
    }
    code += 4 * u32::from(ev.mods.shift) + 8 * u32::from(ev.mods.alt) + 16 * u32::from(ev.mods.ctrl);
    let (x, y) = (ev.col + 1, ev.row + 1);
    if mode.sgr_mouse {
        let end = if ev.action == MouseAction::Release { 'm' } else { 'M' };
        return Some(format!("\x1b[<{code};{x};{y}{end}").into_bytes());
    }
    if ev.action == MouseAction::Release {
        code = (code & !3) | 3;
    }
    if x > 223 || y > 223 {
        return None;
    }
    Some(vec![0x1b, b'[', b'M', (32 + code) as u8, (32 + x) as u8, (32 + y) as u8])
}
