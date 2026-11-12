//! LSP istemcisi, tree-sitter vurgulama, jdtls / vtsls başlatma, tooltip ve problem verisi.
//!
//! - [`highlight`]: gömülü grammar'larla artımlı vurgulama ([`Syntax`]).
//! - [`lsp`]: thread tabanlı LSP istemcisi ve [`lsp::LspManager`] (UI her karede `poll()` eder).
//! - [`java`]: jdtls için JDK bulma, `java_root`, çalışma klasörü (#25).
//! - [`diagnostics`]: problem deposu ve gezinme (#31); [`tooltip`]: hata + dokümantasyon tooltip'i (#52, #70).

pub mod cron;
pub mod diagnostics;
pub mod highlight;
pub mod java;
pub mod language;
pub mod lsp;
pub mod servers;
pub mod tooltip;

pub use diagnostics::{Diagnostic, DiagnosticStore, Pos, Severity, SeverityCounts};
pub use highlight::{HighlightKind, HighlightSpan, Syntax, TextEdit, TextPoint, TextSource};
pub use language::{CommentTokens, IndentHints, Language};
pub use lsp::{ContentChange, LspConfig, LspEvent, LspManager, LspResult, RequestId, ServerKey, ServerState};
pub use tooltip::{Tooltip, TooltipLine, TooltipStyle};
