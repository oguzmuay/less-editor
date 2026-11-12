//! AI türleri (karar #64): Claude Code, Codex, Gemini, Ollama. Her AI sekmesi bir tür ve posta kutusu adı
//! (`codex-1`) taşır. Son seçilen tür varsayılandır; kayıt `<veri>/ai.json` → `{ "default", "ollama_model" }`.
//! Başlatma: `LESS_AI_DIR` / `LESS_AI_NAME` ortamı + talimat (Claude `--append-system-prompt`, Codex
//! `-c developer_instructions=`, Gemini `-i` ilk istem, Ollama talimatsız). LESS MCP bağlantısı isteğe bağlı
//! [`McpLaunch`] ile (Claude `--mcp-config`, Codex `-c mcp_servers.less.*`; Gemini için [`gemini_settings`]).

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::paths;

/// AI türü.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    Claude,
    Codex,
    Gemini,
    Ollama,
}

impl AgentKind {
    /// Tür seçicideki sıra.
    pub const ORDER: [AgentKind; 4] = [AgentKind::Claude, AgentKind::Codex, AgentKind::Gemini, AgentKind::Ollama];

    pub fn label(self) -> &'static str {
        match self {
            AgentKind::Claude => "Claude",
            AgentKind::Codex => "Codex",
            AgentKind::Gemini => "Gemini",
            AgentKind::Ollama => "Ollama",
        }
    }

    /// Posta kutusu adı öneki ve `ai.json` değeri.
    pub fn id(self) -> &'static str {
        match self {
            AgentKind::Claude => "claude",
            AgentKind::Codex => "codex",
            AgentKind::Gemini => "gemini",
            AgentKind::Ollama => "ollama",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ORDER.into_iter().find(|k| k.id() == id)
    }
}

/// Varsayılan Ollama modeli (ilk soruda öneri).
pub const DEFAULT_OLLAMA_MODEL: &str = "llama3.2";

/// `ai.json`: son seçilen tür ve Ollama modeli.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiState {
    #[serde(default = "default_kind")]
    pub default: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ollama_model: Option<String>,
    #[serde(skip)]
    file: PathBuf,
}

fn default_kind() -> String {
    "claude".into()
}

impl AiState {
    pub fn load() -> Self {
        let legacy = paths::legacy_data("ai.json");
        Self::load_from(&paths::data_dir().join("ai.json"), legacy.as_deref())
    }

    pub fn load_from(file: &Path, legacy: Option<&Path>) -> Self {
        let mut st = std::iter::once(file)
            .chain(legacy)
            .find_map(|p| std::fs::read(p).ok().and_then(|d| serde_json::from_slice::<AiState>(&d).ok()))
            .unwrap_or(AiState { default: default_kind(), ollama_model: None, file: PathBuf::new() });
        st.file = file.to_path_buf();
        st
    }

    /// Boş panelde açılacak tür (son seçilen).
    pub fn default_kind(&self) -> AgentKind {
        AgentKind::from_id(&self.default).unwrap_or(AgentKind::Claude)
    }

    /// Ollama model sorusunun varsayılanı.
    pub fn ollama_model(&self) -> &str {
        self.ollama_model.as_deref().unwrap_or(DEFAULT_OLLAMA_MODEL)
    }

    /// Türü varsayılan yapar (Ollama'da modeli de hatırlar) ve kaydeder.
    pub fn choose(&mut self, kind: AgentKind, ollama_model: Option<&str>) -> std::io::Result<()> {
        self.default = kind.id().to_string();
        if let Some(m) = ollama_model.map(str::trim).filter(|m| !m.is_empty()) {
            self.ollama_model = Some(m.to_string());
        }
        self.save()
    }

    pub fn save(&self) -> std::io::Result<()> {
        paths::write_atomic(&self.file, &serde_json::to_vec(self).map_err(std::io::Error::other)?)
    }
}

/// LESS MCP sunucusunu başlatma komutu (sunucuyu başka bir modül yazar; burada yalnız bağlama argümanları üretilir).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct McpLaunch {
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

/// AI'a MCP sunucusu varken eklenen ipucu (lessmcp `hint`).
pub const MCP_HINT: &str = "Your MCP server `less` has the editor's tools: open files and result lists in the editor, \
the Git panel (stage, unstage, discard, diffs), Run configurations (start, stop, logs, profile, env, VM options, args) \
and the Database panel (query, tables, consoles). Use them when the user asks to show or do something in the editor. \
Database writes and git discards wait for the user's approval in the editor.";

/// Claude için `--mcp-config` argümanları (argv'nin sonunda kalmalı: bayrak birden çok değer alır).
pub fn mcp_argv(kind: AgentKind, mcp: &McpLaunch) -> Vec<String> {
    match kind {
        AgentKind::Claude => {
            let mut server = json!({ "type": "stdio", "command": mcp.command, "args": mcp.args });
            if !mcp.env.is_empty() {
                server["env"] = json!(mcp.env);
            }
            vec!["--mcp-config".into(), json!({ "mcpServers": { "less": server } }).to_string()]
        }
        AgentKind::Codex => {
            let env: Vec<String> = mcp.env.iter().map(|(k, v)| format!("{k} = {}", Value::String(v.clone()))).collect();
            let mut out = vec![
                "-c".into(),
                format!("mcp_servers.less.command={}", Value::String(mcp.command.clone())),
                "-c".into(),
                format!("mcp_servers.less.args={}", json!(mcp.args)),
            ];
            if !env.is_empty() {
                out.extend(["-c".into(), format!("mcp_servers.less.env={{ {} }}", env.join(", "))]);
            }
            out.extend(["-c".into(), "mcp_servers.less.tool_timeout_sec=900".into()]);
            out
        }
        AgentKind::Gemini | AgentKind::Ollama => Vec::new(),
    }
}

/// Gemini'nin oturum başına MCP bayrağı yok: sunucu kullanıcının `settings.json`'una bir kez `mcpServers.less`
/// olarak yazılır (diğer ayarlar korunur; `env` değerleri oturum ortamından genişler). Değişiklik yapıldıysa `true`.
pub fn gemini_settings(file: &Path, mcp: &McpLaunch) -> std::io::Result<bool> {
    let mut data: Value = match std::fs::read(file) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|_| std::io::Error::other(format!("Could not read {}", file.display())))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(e) => return Err(e),
    };
    let mut env: BTreeMap<String, String> = mcp.env.keys().map(|k| (k.clone(), format!("${k}"))).collect();
    env.insert("LESS_AI_NAME".into(), "$LESS_AI_NAME".into());
    let want = json!({ "command": mcp.command, "args": mcp.args, "env": env, "timeout": 900000 });
    let Some(obj) = data.as_object_mut() else { return Err(std::io::Error::other("settings.json is not an object")) };
    let servers = obj.entry("mcpServers").or_insert_with(|| json!({}));
    if !servers.is_object() {
        *servers = json!({});
    }
    if servers.get("less") == Some(&want) {
        return Ok(false);
    }
    servers["less"] = want;
    paths::write_atomic(file, serde_json::to_string_pretty(&data).map_err(std::io::Error::other)?.as_bytes())?;
    Ok(true)
}

/// Projenin posta kutusu klasörü: `<cwd>/.less/ai`.
pub fn mailbox_dir(cwd: &Path) -> PathBuf {
    cwd.join(".less").join("ai")
}

/// Tür için sıradaki ad: açık sekmelerde kullanılmayan en küçük numara (`codex-1`, `codex-2` …).
pub fn next_name<'a>(kind: AgentKind, used: impl IntoIterator<Item = &'a str>) -> String {
    let used: std::collections::HashSet<&str> = used.into_iter().collect();
    (1..).map(|n| format!("{}-{n}", kind.id())).find(|name| !used.contains(name.as_str())).unwrap_or_default()
}

/// AI'a verilen talimat: kim olduğu, posta kutusu ve protokol (+ MCP varsa araç ipucu).
pub fn instructions(name: &str, dir: &Path, with_mcp: bool) -> String {
    let d = dir.display();
    let mut lines = vec![
        format!("You are {name}, one of several AI agents working side by side in the LESS editor."),
        format!("Shared mailbox: {d} (README.md there lists the agents and this protocol)."),
        format!(
            "To message another agent, append to {d}/inbox/<agent>.md a section starting with '## from {name} · <time>'."
        ),
        format!(
            "Your inbox is {d}/inbox/{name}.md; read it when the user asks or when you are told you have new mail."
        ),
        format!("Put files you want to share in {d}/shared/. Only message other agents when the task needs it."),
    ];
    if with_mcp {
        lines.push(MCP_HINT.to_string());
    }
    lines.join("\n")
}

/// Başlatma isteği.
#[derive(Debug, Clone)]
pub struct LaunchRequest<'a> {
    pub kind: AgentKind,
    pub cwd: &'a Path,
    /// Açık AI sekmelerinin posta kutusu adları (ad çakışmasın).
    pub used_names: Vec<&'a str>,
    /// Yalnız Ollama.
    pub model: Option<&'a str>,
    pub mcp: Option<&'a McpLaunch>,
    /// Claude için temel komut (varsayılan `["claude"]`; ör. `--resume` eklenmiş).
    pub claude_cmd: Option<Vec<String>>,
    /// Claude hesap ortamı ([`crate::accounts::Accounts::env`]) ve IDE ortamı gibi ek değişkenler.
    pub extra_env: Vec<(String, String)>,
}

/// Hazırlanmış başlatma: argv + ortam + posta kutusu bilgisi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub kind: AgentKind,
    pub name: String,
    pub dir: PathBuf,
    pub text: String,
    pub argv: Vec<String>,
    pub env: HashMap<String, String>,
}

/// Yeni AI sekmesinin adını, ortamını ve komutunu hazırlar.
pub fn launch(req: &LaunchRequest<'_>) -> Launch {
    let name = next_name(req.kind, req.used_names.iter().copied());
    let dir = mailbox_dir(req.cwd);
    let text = instructions(&name, &dir, req.mcp.is_some());
    let mut argv: Vec<String> = match req.kind {
        AgentKind::Claude => {
            let mut a = req.claude_cmd.clone().filter(|c| !c.is_empty()).unwrap_or_else(|| vec!["claude".into()]);
            a.extend(["--append-system-prompt".into(), text.clone()]);
            a
        }
        AgentKind::Codex => {
            vec!["codex".into(), "-c".into(), format!("developer_instructions={}", Value::String(text.clone()))]
        }
        AgentKind::Gemini => vec!["gemini".into(), "-i".into(), format!("{text}\n\nReply only with: ready.")],
        AgentKind::Ollama => vec!["ollama".into(), "run".into(), req.model.unwrap_or(DEFAULT_OLLAMA_MODEL).to_string()],
    };
    if let Some(mcp) = req.mcp {
        argv.extend(mcp_argv(req.kind, mcp));
    }
    let mut env: HashMap<String, String> = req.extra_env.iter().cloned().collect();
    env.insert("LESS_AI_DIR".into(), dir.to_string_lossy().to_string());
    env.insert("LESS_AI_NAME".into(), name.clone());
    Launch { kind: req.kind, name, dir, text, argv, env }
}
