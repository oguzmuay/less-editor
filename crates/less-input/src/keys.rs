//! Tuş olayları ve Vim gösterimi (`d2w<Esc>`, `<C-r>`, `<D-s>`, `<M-j>`, `<S-Left>`).

/// Mantıksal tuş. Harf tuşlarında büyük/küçük harf karakterin kendisindedir (Shift ayrıca bakılmaz).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Char(char),
    Enter,
    Esc,
    Tab,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
}

/// Değiştirici tuşlar; `cmd` macOS Command (Vim gösteriminde `D-`), `alt` Option (`M-` / `A-`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub cmd: bool,
}

impl Mods {
    pub const NONE: Mods = Mods { shift: false, ctrl: false, alt: false, cmd: false };
    pub const SHIFT: Mods = Mods { shift: true, ctrl: false, alt: false, cmd: false };
    pub const CTRL: Mods = Mods { shift: false, ctrl: true, alt: false, cmd: false };
    pub const ALT: Mods = Mods { shift: false, ctrl: false, alt: true, cmd: false };
    pub const CMD: Mods = Mods { shift: false, ctrl: false, alt: false, cmd: true };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyEvent {
    pub key: Key,
    pub mods: Mods,
}

impl KeyEvent {
    pub fn new(key: Key, mods: Mods) -> Self {
        Self { key, mods }
    }

    pub fn plain(key: Key) -> Self {
        Self { key, mods: Mods::NONE }
    }

    pub fn char(c: char) -> Self {
        Self::plain(Key::Char(c))
    }

    /// Değiştiricisiz (Shift hariç) karakter mi.
    pub fn is_char(&self, c: char) -> bool {
        self.key == Key::Char(c) && !self.mods.ctrl && !self.mods.alt && !self.mods.cmd
    }

    /// Değiştiricisiz (Shift hariç) karakter.
    pub fn plain_char(&self) -> Option<char> {
        match self.key {
            Key::Char(c) if !self.mods.ctrl && !self.mods.alt && !self.mods.cmd => Some(c),
            _ => None,
        }
    }

    /// Yalnız Ctrl ile basılmış harf (`<C-r>` → `Some('r')`).
    pub fn ctrl_char(&self) -> Option<char> {
        match self.key {
            Key::Char(c) if self.mods.ctrl && !self.mods.alt && !self.mods.cmd => Some(c.to_ascii_lowercase()),
            _ => None,
        }
    }

    /// Vim gösterimi: `a`, `<C-r>`, `<D-s>`, `<CR>`, `<S-Left>`.
    pub fn notation(&self) -> String {
        let name = match self.key {
            Key::Char(' ') => "Space".to_string(),
            Key::Char('<') => "lt".to_string(),
            Key::Char(c) => c.to_string(),
            Key::Enter => "CR".into(),
            Key::Esc => "Esc".into(),
            Key::Tab => "Tab".into(),
            Key::Backspace => "BS".into(),
            Key::Delete => "Del".into(),
            Key::Left => "Left".into(),
            Key::Right => "Right".into(),
            Key::Up => "Up".into(),
            Key::Down => "Down".into(),
            Key::Home => "Home".into(),
            Key::End => "End".into(),
            Key::PageUp => "PageUp".into(),
            Key::PageDown => "PageDown".into(),
            Key::F(n) => format!("F{n}"),
        };
        let m = self.mods;
        let shift = m.shift && !matches!(self.key, Key::Char(_));
        let mut prefix = String::new();
        for (on, p) in [(m.ctrl, "C-"), (m.alt, "M-"), (m.cmd, "D-"), (shift, "S-")] {
            if on {
                prefix.push_str(p);
            }
        }
        let bare = matches!(self.key, Key::Char(c) if c != ' ' && c != '<');
        if prefix.is_empty() && bare { name } else { format!("<{prefix}{name}>") }
    }
}

/// Tuş listesinin gösterimi.
pub fn notation(keys: &[KeyEvent]) -> String {
    keys.iter().map(KeyEvent::notation).collect()
}

/// `"d2w<Esc>"` gibi Vim gösterimini olaylara çevirir. Tanınmayan `<...>` harf harf alınır.
pub fn parse_keys(s: &str) -> Vec<KeyEvent> {
    let mut out = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '<'
            && let Some(len) = chars[i + 1..].iter().position(|&c| c == '>')
            && let Some(k) = parse_special(&chars[i + 1..i + 1 + len].iter().collect::<String>())
        {
            out.push(k);
            i += len + 2;
            continue;
        }
        out.push(KeyEvent::char(chars[i]));
        i += 1;
    }
    out
}

fn parse_special(body: &str) -> Option<KeyEvent> {
    let mut mods = Mods::NONE;
    let mut rest = body;
    while rest.len() > 2 && rest.as_bytes()[1] == b'-' {
        match rest.as_bytes()[0].to_ascii_uppercase() {
            b'C' => mods.ctrl = true,
            b'M' | b'A' => mods.alt = true,
            b'D' => mods.cmd = true,
            b'S' => mods.shift = true,
            _ => return None,
        }
        rest = &rest[2..];
    }
    let key = match rest.to_ascii_lowercase().as_str() {
        "cr" | "enter" | "return" => Key::Enter,
        "esc" => Key::Esc,
        "tab" => Key::Tab,
        "bs" | "backspace" => Key::Backspace,
        "del" | "delete" => Key::Delete,
        "left" => Key::Left,
        "right" => Key::Right,
        "up" => Key::Up,
        "down" => Key::Down,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "space" => Key::Char(' '),
        "lt" => Key::Char('<'),
        "bar" => Key::Char('|'),
        "bslash" => Key::Char('\\'),
        f if f.len() > 1 && f.starts_with('f') && f[1..].parse::<u8>().is_ok() => Key::F(f[1..].parse().ok()?),
        _ => {
            let mut cs = rest.chars();
            let c = cs.next()?;
            if cs.next().is_some() || mods == Mods::NONE {
                return None;
            }
            if mods.shift && c.is_alphabetic() {
                mods.shift = false;
                Key::Char(c.to_ascii_uppercase())
            } else if mods.ctrl {
                Key::Char(c.to_ascii_lowercase())
            } else {
                Key::Char(c)
            }
        }
    };
    Some(KeyEvent::new(key, mods))
}
