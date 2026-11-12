//! Claude Code oturum okuyucu: `<config>/sessions/<pid>.json` (durum) + `<config>/projects/<kaçışlı cwd>/<id>.jsonl`
//! (transcript; başlık, token, son etkinlik). Okuma artımlıdır: her çağrıda yalnız yeni eklenen kısım (en fazla 8 MB).
//! Aynı assistant mesajı içerik bloğu başına ayrı satıra aynı usage ile yazılır → `message.id` ile tekilleştirilir.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

/// Tek okumada en fazla okunacak bayt.
pub const MAX_CHUNK: u64 = 8 * 1024 * 1024;

/// Claude Code sürecinin durumu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Busy,
    Idle,
    Waiting,
}

/// `sessions/<pid>.json` kaydı (bilinmeyen alanlar yok sayılır).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    #[serde(default, deserialize_with = "lenient_status")]
    pub status: Option<Status>,
    pub waiting_for: Option<String>,
    pub name: Option<String>,
    pub name_source: Option<String>,
}

fn lenient_status<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Status>, D::Error> {
    let v = Option::<Value>::deserialize(d)?;
    Ok(v.and_then(|v| serde_json::from_value(v).ok()))
}

/// Bir mesajın token değerleri.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_create: u64,
}

impl Usage {
    pub fn total(&self) -> u64 {
        self.input + self.output + self.cache_read + self.cache_create
    }
}

/// Bir transcript dosyasının artımlı okuma durumu.
#[derive(Debug, Clone, Default)]
pub struct TranscriptState {
    /// Okunan bayt.
    pub offset: u64,
    rest: Vec<u8>,
    ids: HashMap<String, Usage>,
    pub usage: Usage,
    /// Son ana akış isteğinin bağlam boyutu.
    pub context: u64,
    pub ai_title: Option<String>,
    pub custom_title: Option<String>,
    pub last_tool: Option<String>,
    pub last_text: Option<String>,
    pub last_prompt: Option<String>,
    /// Dosyanın son değişme zamanı (unix sn).
    pub mtime: Option<u64>,
}

/// Oturumun anlık durumu (özet ekranı ve tab adı için).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub status: Option<Status>,
    pub waiting_for: Option<String>,
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    /// `/rename` adı > kullanıcının verdiği ad > Claude'un ürettiği başlık.
    pub title: Option<String>,
    pub usage: Usage,
    pub total: u64,
    pub context: u64,
    /// Alt ajanların harcadığı token.
    pub subagents: u64,
    pub subagent_count: usize,
    pub last_tool: Option<String>,
    pub last_text: Option<String>,
    pub last_prompt: Option<String>,
    pub mtime: Option<u64>,
}

/// Metni tek satıra indirir (ilk dolu satır, boşluklar sadeleşir) ve `max` karakterde `…` ile keser.
pub fn one_line(s: &str, max: usize) -> Option<String> {
    let line = s.lines().find(|l| !l.trim().is_empty())?;
    let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() > max {
        let cut: String = line.chars().take(max.saturating_sub(1)).collect();
        return Some(cut + "…");
    }
    Some(line)
}

fn basename(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

/// Araç çağrısını "Araç: ayrıntı" biçiminde özetler.
fn tool_summary(name: &str, input: &Value) -> String {
    let get = |k: &str| input.get(k).and_then(Value::as_str);
    let path = get("file_path").or(get("notebook_path")).or(get("path"));
    let detail = get("description")
        .or(path.map(basename))
        .or(get("pattern"))
        .or(get("command"))
        .or(get("url"))
        .or(get("query"))
        .or(get("prompt"))
        .or_else(|| input.as_object().and_then(|o| o.values().find_map(Value::as_str)));
    match detail.and_then(|d| one_line(d, 100)) {
        Some(d) => format!("{name}: {d}"),
        None => name.to_string(),
    }
}

fn line_kind(line: &str) -> Option<&str> {
    let after = line.strip_prefix("{\"type\":\"")?;
    let end = after.find('"')?;
    Some(&after[..end])
}

impl TranscriptState {
    fn add_usage(&mut self, id: String, usage: &Value, sidechain: bool) {
        let n = |k: &str| usage.get(k).and_then(Value::as_u64).unwrap_or(0);
        let new = Usage {
            input: n("input_tokens"),
            output: n("output_tokens"),
            cache_read: n("cache_read_input_tokens"),
            cache_create: n("cache_creation_input_tokens"),
        };
        let old = self.ids.insert(id, new).unwrap_or_default();
        self.usage.input = self.usage.input + new.input - old.input;
        self.usage.output = self.usage.output + new.output - old.output;
        self.usage.cache_read = self.usage.cache_read + new.cache_read - old.cache_read;
        self.usage.cache_create = self.usage.cache_create + new.cache_create - old.cache_create;
        if !sidechain {
            self.context = new.input + new.cache_read + new.cache_create;
        }
    }

    /// Tek JSONL satırını işler; JSON çözümlemesinden önce düz metin ön filtresi uygulanır.
    pub fn handle_line(&mut self, line: &str) {
        let kind = line_kind(line).or_else(|| {
            if line.contains("\"type\":\"assistant\"") {
                Some("assistant")
            } else if line.contains("\"type\":\"user\"") {
                Some("user")
            } else {
                None
            }
        });
        let Some(kind) = kind else { return };
        if kind == "user" && (line.contains("\"tool_use_id\"") || line.contains("\"isMeta\":true")) {
            return;
        }
        if !matches!(kind, "assistant" | "user" | "ai-title" | "custom-title") {
            return;
        }
        let Ok(rec) = serde_json::from_str::<Value>(line) else { return };
        let sidechain = rec.get("isSidechain").and_then(Value::as_bool) == Some(true);
        match rec.get("type").and_then(Value::as_str) {
            Some("ai-title") => {
                if let Some(t) = rec.get("aiTitle").and_then(Value::as_str) {
                    self.ai_title = Some(t.to_string());
                }
            }
            Some("custom-title") => {
                if let Some(t) = rec.get("customTitle").and_then(Value::as_str) {
                    self.custom_title = Some(t.to_string());
                }
            }
            Some("assistant") => {
                let Some(msg) = rec.get("message").filter(|m| m.is_object()) else { return };
                if let Some(usage) = msg.get("usage").filter(|u| u.is_object()) {
                    let id = msg
                        .get("id")
                        .or(rec.get("uuid"))
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .unwrap_or_else(|| self.offset.to_string());
                    self.add_usage(id, usage, sidechain);
                }
                if sidechain {
                    return;
                }
                for c in msg.get("content").and_then(Value::as_array).into_iter().flatten() {
                    match c.get("type").and_then(Value::as_str) {
                        Some("tool_use") => {
                            let name = c.get("name").and_then(Value::as_str).unwrap_or("?");
                            self.last_tool = Some(tool_summary(name, c.get("input").unwrap_or(&Value::Null)));
                        }
                        Some("text") => {
                            if let Some(t) = c.get("text").and_then(Value::as_str).and_then(|t| one_line(t, 120)) {
                                self.last_text = Some(t);
                            }
                        }
                        _ => {}
                    }
                }
            }
            Some("user") if !sidechain => {
                let Some(content) = rec.get("message").and_then(|m| m.get("content")) else { return };
                let text = match content {
                    Value::String(s) => Some(s.as_str()),
                    Value::Array(items) => items
                        .iter()
                        .find(|c| c.get("type").and_then(Value::as_str) == Some("text"))
                        .and_then(|c| c.get("text"))
                        .and_then(Value::as_str),
                    _ => None,
                };
                if let Some(t) = text.filter(|t| !t.trim_start().starts_with('<'))
                    && let Some(l) = one_line(t, 120)
                {
                    self.last_prompt = Some(l);
                }
            }
            _ => {}
        }
    }

    /// Yeni baytları işler; tamamlanmamış son satır bir sonraki okumaya kalır.
    fn consume(&mut self, data: &[u8]) {
        self.offset += data.len() as u64;
        let mut buf = std::mem::take(&mut self.rest);
        buf.extend_from_slice(data);
        let mut start = 0;
        while let Some(nl) = buf[start..].iter().position(|&b| b == b'\n') {
            let line = String::from_utf8_lossy(&buf[start..start + nl]).into_owned();
            self.handle_line(&line);
            start += nl + 1;
        }
        self.rest = buf[start..].to_vec();
    }
}

fn mtime_secs(meta: &std::fs::Metadata) -> Option<u64> {
    meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs())
}

/// Transcript önbelleği; aynı dosya tekrar okunduğunda yalnız eklenen kısım işlenir.
#[derive(Debug, Default)]
pub struct SessionReader {
    cache: HashMap<PathBuf, TranscriptState>,
}

impl SessionReader {
    pub fn new() -> Self {
        Self::default()
    }

    /// Transcript'in son okumadan bu yana eklenen kısmını okur (tek çağrıda en fazla [`MAX_CHUNK`]).
    /// Dosya küçüldüyse (yeniden yazıldıysa) baştan okunur.
    pub fn read(&mut self, path: &Path) -> &TranscriptState {
        let st = self.cache.entry(path.to_path_buf()).or_default();
        if let Ok(mut f) = File::open(path)
            && let Ok(meta) = f.metadata()
        {
            let size = meta.len();
            if size < st.offset {
                *st = TranscriptState::default();
            }
            if size > st.offset && f.seek(SeekFrom::Start(st.offset)).is_ok() {
                let mut data = Vec::new();
                let _ = (&mut f).take((size - st.offset).min(MAX_CHUNK)).read_to_end(&mut data);
                st.consume(&data);
            }
            st.mtime = mtime_secs(&meta).or(st.mtime);
        }
        st
    }

    /// Alt ajan transcript'lerinin (`<id>/subagents/*.jsonl`) token toplamı ve sayısı.
    fn subagents(&mut self, transcript: &Path) -> (u64, usize) {
        let dir = transcript.with_extension("").join("subagents");
        let Ok(entries) = std::fs::read_dir(&dir) else { return (0, 0) };
        let mut files: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "jsonl"))
            .collect();
        files.sort();
        let mut total = 0;
        for f in &files {
            total += self.read(f).usage.total();
        }
        (total, files.len())
    }

    /// Claude Code sürecinin anlık durumu; kayıt yoksa (süreç henüz yazmadıysa) `None`.
    pub fn snapshot(&mut self, pid: u32, config_dir: &Path) -> Option<Snapshot> {
        let info = process_info(pid, config_dir)?;
        let path = transcript_path(&info, config_dir);
        let st = match &path {
            Some(p) => self.read(p).clone(),
            None => TranscriptState::default(),
        };
        let (subagents, subagent_count) = path.as_deref().map(|p| self.subagents(p)).unwrap_or((0, 0));
        let user_name = info.name.clone().filter(|_| info.name_source.as_deref() == Some("user"));
        Some(Snapshot {
            status: info.status,
            waiting_for: info.waiting_for.clone(),
            session_id: info.session_id.clone(),
            cwd: info.cwd.clone(),
            title: st.custom_title.clone().or(user_name).or(st.ai_title.clone()),
            usage: st.usage,
            total: st.usage.total(),
            context: st.context,
            subagents,
            subagent_count,
            last_tool: st.last_tool,
            last_text: st.last_text,
            last_prompt: st.last_prompt,
            mtime: st.mtime,
        })
    }
}

/// `<config>/sessions/<pid>.json` kaydını okur.
pub fn process_info(pid: u32, config_dir: &Path) -> Option<ProcessInfo> {
    let data = std::fs::read(config_dir.join("sessions").join(format!("{pid}.json"))).ok()?;
    serde_json::from_slice(&data).ok()
}

/// cwd'deki harf/rakam dışı her karakter `-` olur (Claude Code'un proje klasörü adı).
pub fn escape_cwd(cwd: &str) -> String {
    cwd.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

/// Oturumun transcript yolu; kaçış kuralı tutmazsa tüm proje klasörlerinde aranır. Henüz mesaj yoksa `None`.
pub fn transcript_path(info: &ProcessInfo, config_dir: &Path) -> Option<PathBuf> {
    let id = info.session_id.as_deref()?;
    let projects = config_dir.join("projects");
    let file = format!("{id}.jsonl");
    if let Some(cwd) = &info.cwd {
        let p = projects.join(escape_cwd(cwd)).join(&file);
        if p.is_file() {
            return Some(p);
        }
    }
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&projects).ok()?.filter_map(Result::ok).map(|e| e.path()).collect();
    dirs.sort();
    dirs.into_iter().map(|d| d.join(&file)).find(|p| p.is_file())
}

/// Token sayısını kısaltır: 950 → "950", 12345 → "12.3K", 2345678 → "2.35M".
pub fn fmt_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.2}M", n as f64 / 1e6)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1e3)
    } else {
        n.to_string()
    }
}
