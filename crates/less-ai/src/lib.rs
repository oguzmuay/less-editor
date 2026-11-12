//! AI paneli: terminal tabları (Claude, Codex, Gemini, Ollama), oturum izleme, .less/ai posta kutusu, LESS MCP sunucusu.
//!
//! - [`term`]: pty + terminal emülatörü, render'a hazır ızgara, tuş kodlama.
//! - [`tabs`]: AI / Terminal sekme modeli (saf mantık) ve [`tabs::TermTabs`] (sekmeler + süreçler).
//! - [`agents`]: AI türleri, argv/ortam üreticileri, `ai.json`.
//! - [`mailbox`]: `.less/ai/` posta kutusu, inbox izleme, bildirim sınırı.
//! - [`sessions`]: Claude Code oturum kaydı + transcript okuyucu.
//! - [`accounts`]: Claude hesapları (`CLAUDE_CONFIG_DIR`).
//! - [`summary`]: özet tab'ı modeli ve maskot kareleri.

pub mod accounts;
pub mod agents;
pub mod mailbox;
pub mod paths;
pub mod sessions;
pub mod summary;
pub mod tabs;
pub mod term;
