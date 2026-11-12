//! Özet tab'ı modeli (karar #27, #60): her AI oturumu için durum, token, son iş ve maskot. UI yalnız bu düz veriyi
//! çizer; maskot kareleri saf fonksiyondur ([`frame`]), animasyon sayacı [`MascotClock`] (400 ms).

use std::time::{Duration, Instant};

use crate::agents::AgentKind;
use crate::sessions::{Status, fmt_tokens};
use crate::tabs::{Tab, TabId};

/// Oturum başına maskot renkleri (ilki Claude turuncusu); `tab.n`'e göre sabit.
pub const COLORS: [(u8, u8, u8); 8] = [
    (0xD9, 0x77, 0x57),
    (0x7A, 0xA2, 0xF7),
    (0x9E, 0xCE, 0x6A),
    (0xBB, 0x9A, 0xF7),
    (0xE0, 0xAF, 0x68),
    (0x7D, 0xCF, 0xFF),
    (0xF7, 0x76, 0x8E),
    (0x73, 0xDA, 0xCA),
];

/// Maskot genişliği (görüntü sütunu); UI blok metnini bunun + 2 sütun sağına koyar.
pub const MASCOT_W: usize = 14;
/// Kare süresi.
pub const FRAME_EVERY: Duration = Duration::from_millis(400);

struct Body {
    awake: [&'static str; 3],
    sleep: [&'static str; 3],
    grip: &'static str,
}

fn body(kind: AgentKind) -> Body {
    match kind {
        AgentKind::Claude => Body {
            awake: [" ▐▛███▜▌ ", "▝▜█████▛▘", "  ▘▘ ▝▝  "],
            sleep: [" ▐█████▌ ", "▝▜█████▛▘", "  ▘▘ ▝▝  "],
            grip: "▝▜█████▛",
        },
        AgentKind::Codex => Body {
            awake: [" ▐ >_  ▌ ", "▝▀█████▀▘", "   ▌ ▐   "],
            sleep: [" ▐ -_- ▌ ", "▝▀█████▀▘", "   ▌ ▐   "],
            grip: "▝▀█████▀",
        },
        AgentKind::Gemini => Body {
            awake: ["    ▲    ", "  ◀ ● ▶  ", "    ▼    "],
            sleep: ["    ▲    ", "  ◀ ─ ▶  ", "    ▼    "],
            grip: "  ◀ ● ▶ ",
        },
        AgentKind::Ollama => Body {
            awake: ["  ▗▘ ▝▖  ", "  █• •█  ", "  ▝█▄█▘  "],
            sleep: ["  ▗▘ ▝▖  ", "  █- -█  ", "  ▝█▄█▘  "],
            grip: "  █• •█ ",
        },
    }
}

/// Maskotun 4 satırlık karesi (sağında token taşı): `busy` kazmayı kaldırır, savurur, taşa vurur ve token (◆) sıçrar;
/// `idle` uyur (z Z); `waiting` başında yanıp sönen "!"; diğer durumlar sabit.
pub fn frame(status: Option<Status>, tick: u64, kind: AgentKind) -> [String; 4] {
    let b = body(kind);
    let [r1, r2, r3] = b.awake;
    let rock = "  ▟█▙";
    if status == Some(Status::Busy) {
        return match tick % 3 {
            0 => ["        ┏━┻━┓ ".into(), format!("{r1}╱    "), format!("{r2}     "), format!("{r3}{rock}")],
            1 => ["              ".into(), format!("{r1}━━━┓ "), format!("{r2}   ┃ "), format!("{r3}{rock}")],
            _ => ["             ◆".into(), format!("{r1}  ◆  "), format!("{}━━━┓  ", b.grip), format!("{r3}{rock}")],
        };
    }
    let (mut top, mut rows) = ("              ", b.awake);
    if status == Some(Status::Idle) {
        top = ["      z       ", "     z Z      ", "    z Z z     "][(tick % 3) as usize];
        rows = b.sleep;
    } else if status == Some(Status::Waiting) && tick.is_multiple_of(2) {
        top = "    !         ";
    }
    [top.into(), format!("{}     ", rows[0]), format!("{}     ", rows[1]), format!("{}{rock}", rows[2])]
}

/// Animasyon sayacı: özet görünürken UI her karede `tick()` çağırır; 400 ms geçtiyse kare ilerler (`true` = yeniden çiz).
#[derive(Debug, Clone)]
pub struct MascotClock {
    pub frame: u64,
    last: Instant,
}

impl Default for MascotClock {
    fn default() -> Self {
        Self { frame: 0, last: Instant::now() }
    }
}

impl MascotClock {
    pub fn tick(&mut self) -> bool {
        if self.last.elapsed() < FRAME_EVERY {
            return false;
        }
        self.last = Instant::now();
        self.frame += 1;
        true
    }
}

/// Satır biçimi (UI tema rengine çevirir).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStyle {
    Title,
    Comment,
    Bold,
    Normal,
}

/// Durum metninin rengi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusTone {
    Ok,
    Error,
    Info,
    Muted,
}

/// Bir oturum bloğu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryBlock {
    pub tab: TabId,
    /// Başlık: `1. ● Ad  @hesap  ✉ new mail`.
    pub head: String,
    /// Başlığın sağındaki durum: `  [working]`.
    pub status_text: String,
    pub status_tone: StatusTone,
    /// Ayrıntı satırları (en az 3 satır olacak şekilde boşlukla doldurulur: maskot 4 satır).
    pub lines: Vec<(String, LineStyle)>,
    pub mascot_kind: AgentKind,
    pub mascot_status: Option<Status>,
    pub mascot_color: (u8, u8, u8),
}

/// Özet ekranı: başlık, yardım satırı ve bloklar. Bloğa tık / Enter → `block.tab` seçilir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryView {
    pub header: String,
    pub help: String,
    pub blocks: Vec<SummaryBlock>,
}

pub fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// "12s ago" / "3m ago" / "2h ago".
pub fn ago(t: Option<u64>, now: u64) -> Option<String> {
    let d = now.saturating_sub(t?);
    Some(if d < 60 {
        format!("{d}s ago")
    } else if d < 3600 {
        format!("{}m ago", d / 60)
    } else {
        format!("{}h ago", d / 3600)
    })
}

fn tilde(path: &str) -> String {
    let home = crate::paths::home();
    let home = home.to_string_lossy();
    match path.strip_prefix(home.as_ref()) {
        Some(rest) if !home.is_empty() && home != "/" && (rest.is_empty() || rest.starts_with('/')) => {
            format!("~{rest}")
        }
        _ => path.to_string(),
    }
}

/// AI sekmelerinden özet modelini kurar. Lua'daki başlık saati (`%H:%M:%S`) burada yok; UI yerel saati ekler.
pub fn build(tabs: &[Tab], multi_account: bool, has_mail: impl Fn(&str) -> bool, now: u64) -> SummaryView {
    let total: u64 =
        tabs.iter().filter_map(|t| t.meta.as_ref()?.snapshot.as_ref()).map(|s| s.total + s.subagents).sum();
    let header = format!("AI sessions: {} open · {} Claude tokens", tabs.len(), fmt_tokens(total));
    let help = "Enter / double-click: go to session   Cmd+Shift+Left/Right: tabs   Cmd+T: new AI session".to_string();
    let mut blocks = Vec::new();
    for (i, tab) in tabs.iter().enumerate() {
        let meta = tab.meta.as_ref();
        let status = meta.and_then(|m| m.status);
        let (word, tone) = match status {
            Some(Status::Busy) => ("working", StatusTone::Ok),
            Some(Status::Waiting) => ("waiting for approval/input", StatusTone::Error),
            Some(Status::Idle) => ("idle (your turn)", StatusTone::Info),
            None => ("starting", StatusTone::Muted),
        };
        let mut head = format!("{}. {} {}", i + 1, tab.status_icon(), tab.label());
        if multi_account && let Some(a) = &tab.account {
            head.push_str(&format!("  @{}", a.name));
        }
        if tab.ai_name.as_deref().is_some_and(&has_mail) {
            head.push_str("  ✉ new mail");
        }
        let waiting = meta.and_then(|m| m.snapshot.as_ref()).and_then(|s| s.waiting_for.clone());
        let status_text = format!("  [{word}{}]", waiting.map(|w| format!(": {w}")).unwrap_or_default());
        let kind = tab.agent.unwrap_or(AgentKind::Claude);
        let mut lines = Vec::new();
        match meta {
            Some(m) if m.simple() => {
                lines.push((
                    format!("   {} · mailbox name {}", kind.label(), tab.ai_name.as_deref().unwrap_or("-")),
                    LineStyle::Normal,
                ));
                lines.push((format!("   {}", tilde(&tab.cwd.to_string_lossy())), LineStyle::Comment));
            }
            Some(m) => {
                let s = m.snapshot.as_ref().cloned().unwrap_or_default();
                let u = s.usage;
                lines.push((
                    format!(
                        "   Tokens: {}  (input {} · cache {} · output {}) · context {}",
                        fmt_tokens(s.total),
                        fmt_tokens(u.input + u.cache_create),
                        fmt_tokens(u.cache_read),
                        fmt_tokens(u.output),
                        fmt_tokens(s.context)
                    ),
                    LineStyle::Normal,
                ));
                if s.subagent_count > 0 {
                    lines.push((
                        format!("   Subagents: {} · {} tokens", s.subagent_count, fmt_tokens(s.subagents)),
                        LineStyle::Normal,
                    ));
                }
                let now_label = if s.status == Some(Status::Busy) { "Now:" } else { "Last tool:" };
                lines.push((format!("   {now_label} {}", s.last_tool.as_deref().unwrap_or("-")), LineStyle::Normal));
                lines.push((format!("   Prompt: {}", s.last_prompt.as_deref().unwrap_or("-")), LineStyle::Normal));
                lines.push((format!("   Reply: {}", s.last_text.as_deref().unwrap_or("-")), LineStyle::Normal));
                let where_ = s.cwd.as_deref().map(tilde).unwrap_or_else(|| tilde(&tab.cwd.to_string_lossy()));
                lines.push((
                    format!("   Last activity: {} · {where_}", ago(s.mtime, now).unwrap_or_else(|| "-".into())),
                    LineStyle::Comment,
                ));
            }
            None => lines.push(("   No session info yet (Claude Code is starting)".into(), LineStyle::Comment)),
        }
        while lines.len() < 3 {
            lines.push((String::new(), LineStyle::Normal));
        }
        blocks.push(SummaryBlock {
            tab: tab.id,
            head,
            status_text,
            status_tone: tone,
            lines,
            mascot_kind: kind,
            mascot_status: status,
            mascot_color: COLORS[(tab.n.max(1) as usize - 1) % COLORS.len()],
        });
    }
    SummaryView { header, help, blocks }
}
