//! `AppCommand` yönlendirmesinin UI'dan bağımsız kararları: host'un kendi ex komutları (#24), `:q` koruması (#41),
//! host'un bildiği `:set` seçenekleri. Kalan ex / leader / eşlenmemiş diziler panellere gider (`panels::Event`).

use less_core::command::SetValue;
use less_workbench::layout::Panel as Area;

use crate::notes;
use crate::picker::Mode;

/// Host'un kendisinin çalıştırdığı ex komutu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Builtin {
    /// Saf aç / kapat (#24); `Code` yalnız odaklar.
    Toggle(Area),
    ResetLayout,
    Picker(Mode),
    /// `:Minimap`: mini haritayı aç / kapat.
    Minimap,
    /// `:Preview`: Markdown önizlemesini aç / kapat, HTML dosyasını tarayıcıda aç (#83).
    Preview,
    Notes(notes::Command),
    ChooseTheme,
    ChooseFont,
    NewWindow,
}

/// `:Files`, `:AI`, `:UIReset`… (argümansız, büyük/küçük harf duyarlı, Vim'deki gibi).
pub fn builtin_ex(cmd: &str) -> Option<Builtin> {
    Some(match cmd.trim() {
        "Files" => Builtin::Toggle(Area::Files),
        "Code" => Builtin::Toggle(Area::Code),
        "AI" | "Claude" => Builtin::Toggle(Area::Ai),
        "Terminal" => Builtin::Toggle(Area::Terminal),
        "Datasource" => Builtin::Toggle(Area::Database),
        "Maven" => Builtin::Toggle(Area::Maven),
        "Run" => Builtin::Toggle(Area::Run),
        "Git" => Builtin::Toggle(Area::Git),
        "GitHistory" => Builtin::Toggle(Area::GitHistory),
        "Messages" => Builtin::Toggle(Area::Messages),
        "UIReset" => Builtin::ResetLayout,
        "Shortcuts" => Builtin::Picker(Mode::Shortcuts),
        "Minimap" => Builtin::Minimap,
        "Preview" => Builtin::Preview,
        "Notes" => Builtin::Notes(notes::Command::Toggle),
        "Notes new" => Builtin::Notes(notes::Command::New),
        "Notes rename" => Builtin::Notes(notes::Command::Rename),
        "Notes delete" => Builtin::Notes(notes::Command::Delete),
        "Theme" => Builtin::ChooseTheme,
        "Font" => Builtin::ChooseFont,
        "NewWindow" => Builtin::NewWindow,
        _ => return None,
    })
}

/// `:q` / `:qa` kararı.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuitOutcome {
    Refuse(String),
    Close,
}

pub const KEEP_CODE_OPEN: &str = "The code editor stays open. Close tabs with Cmd+W, quit with :qa or Cmd+Q.";

/// #41: tek kod penceresi olduğu için `:q` (ve `:wq`, `:x`) pencereyi kapatmaz; `:qa` kaydedilmemiş dosya varsa
/// `!` ister.
pub fn quit(force: bool, all: bool, dirty: usize) -> QuitOutcome {
    if !all {
        return QuitOutcome::Refuse(KEEP_CODE_OPEN.to_owned());
    }
    if dirty > 0 && !force {
        let files = if dirty == 1 { "1 file has" } else { "files have" };
        let lead = if dirty == 1 { String::new() } else { format!("{dirty} ") };
        return QuitOutcome::Refuse(format!("{lead}{files} unsaved changes (add ! to discard)"));
    }
    QuitOutcome::Close
}

/// Host'un tuttuğu görünüm seçenekleri.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewSettings {
    pub number: bool,
    /// #54: varsayılan açık.
    pub relative_number: bool,
    /// Kod görünümünün sağındaki mini harita (`:Minimap`); varsayılan açık.
    pub minimap: bool,
}

impl Default for ViewSettings {
    fn default() -> Self {
        Self { number: true, relative_number: true, minimap: true }
    }
}

/// Editörün işlemediği `:set`; dönen metin durum satırına yazılır.
pub fn set_option(view: &mut ViewSettings, name: &str, value: &SetValue) -> Option<String> {
    let slot = match name {
        "number" | "nu" => &mut view.number,
        "relativenumber" | "rnu" => &mut view.relative_number,
        _ => return Some(format!("Unknown option: {name}")),
    };
    match value {
        SetValue::On => *slot = true,
        SetValue::Off => *slot = false,
        SetValue::Toggle => *slot = !*slot,
        SetValue::Query => return Some(format!("{}{name}", if *slot { "  " } else { "no" })),
        SetValue::Value(_) => return Some(format!("Invalid argument: {name}")),
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_ex_commands_24() {
        assert_eq!(builtin_ex("Files"), Some(Builtin::Toggle(Area::Files)));
        assert_eq!(builtin_ex("Claude"), Some(Builtin::Toggle(Area::Ai)));
        assert_eq!(builtin_ex("Datasource"), Some(Builtin::Toggle(Area::Database)));
        assert_eq!(builtin_ex("UIReset"), Some(Builtin::ResetLayout));
        assert_eq!(builtin_ex("Minimap"), Some(Builtin::Minimap));
        assert_eq!(builtin_ex("Notes"), Some(Builtin::Notes(notes::Command::Toggle)));
        assert_eq!(builtin_ex("Notes rename"), Some(Builtin::Notes(notes::Command::Rename)));
        assert_eq!(builtin_ex("Theme"), Some(Builtin::ChooseTheme));
        assert_eq!(builtin_ex("Font"), Some(Builtin::ChooseFont));
        assert_eq!(builtin_ex("NewWindow"), Some(Builtin::NewWindow));
        assert!(ViewSettings::default().minimap, "minimap is on by default");
        assert_eq!(builtin_ex("GitBlame"), None, "panel commands are offered to panels");
        assert_eq!(builtin_ex("files"), None);
    }

    #[test]
    fn quit_keeps_last_code_window_41() {
        assert_eq!(quit(false, false, 0), QuitOutcome::Refuse(KEEP_CODE_OPEN.to_owned()));
        assert_eq!(quit(true, false, 0), QuitOutcome::Refuse(KEEP_CODE_OPEN.to_owned()));
        assert!(matches!(quit(false, true, 2), QuitOutcome::Refuse(m) if m.starts_with("2 files")));
        assert_eq!(quit(true, true, 2), QuitOutcome::Close);
        assert_eq!(quit(false, true, 0), QuitOutcome::Close);
    }

    #[test]
    fn relative_number_option_54() {
        let mut v = ViewSettings::default();
        assert!(v.relative_number);
        assert_eq!(set_option(&mut v, "rnu", &SetValue::Toggle), None);
        assert!(!v.relative_number);
        assert_eq!(set_option(&mut v, "rnu", &SetValue::Query), Some("nornu".to_owned()));
        assert!(set_option(&mut v, "wrap", &SetValue::On).is_some());
    }
}
