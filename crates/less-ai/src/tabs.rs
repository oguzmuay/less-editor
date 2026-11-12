//! AI ve Terminal panellerinin sekmeleri (karar #27, #64).
//!
//! [`TabsModel`] saf mantıktır (süreç yok): gruplar `ai` ve `shell`, her biri `{ tabs, current, active, seq }`.
//! `current` panelde görünen öğe (AI grubunda özet olabilir), `active` son seçilen gerçek sekme.
//! [`TermTabs`] modeli süreçlerle ([`Terminal`]) birleştirir: başlatma, kapatma, 2 sn'lik yenileme (otomatik adlar,
//! Claude oturum bilgisi), biten süreçlerin sekmesini kaldırma ve posta kutusu bildirimleri.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::accounts::{Account, Accounts};
use crate::agents::{self, AgentKind, AiState, LaunchRequest, McpLaunch};
use crate::mailbox::{self, Mailbox, Recipient};
use crate::sessions::{SessionReader, Snapshot, Status};
use crate::summary::{self, SummaryView};
use crate::term::{ExitInfo, Key, Modifiers, SpawnOptions, TermSize, Terminal};

pub type TabId = u64;

/// Sekme grubu: sağ sütunun üstündeki AI paneli ya da altındaki Terminal paneli.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Group {
    Ai,
    Shell,
}

impl Group {
    fn idx(self) -> usize {
        match self {
            Group::Ai => 0,
            Group::Shell => 1,
        }
    }

    fn fallback(self, n: u32) -> String {
        match self {
            Group::Ai => format!("Claude {n}"),
            Group::Shell => format!("Terminal {n}"),
        }
    }
}

/// AI sekmesinin durumu. Claude'da oturum kaydından (`snapshot`), diğerlerinde terminal çıktısı değişti mi tahmini.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabMeta {
    pub status: Option<Status>,
    /// Yalnız Claude.
    pub snapshot: Option<Snapshot>,
}

impl TabMeta {
    pub fn simple(&self) -> bool {
        self.snapshot.is_none()
    }
}

/// Bir sekme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    pub id: TabId,
    pub group: Group,
    /// Grup içindeki sıra numarası (yedek ad, maskot rengi).
    pub n: u32,
    pub cwd: PathBuf,
    /// Kullanıcının verdiği ad (`:R` / sağ tık).
    pub name: Option<String>,
    /// Otomatik ad (shell: başlık/süreç/klasör; Claude: oturum başlığı).
    pub auto: Option<String>,
    pub agent: Option<AgentKind>,
    /// Posta kutusu adı (`codex-1`).
    pub ai_name: Option<String>,
    pub account: Option<Account>,
    pub pid: Option<u32>,
    pub meta: Option<TabMeta>,
}

impl Tab {
    /// Sekme adı: kullanıcı adı > otomatik ad > "Codex 1" > "Claude N" / "Terminal N".
    pub fn label(&self) -> String {
        if let Some(n) = self.name.as_ref().or(self.auto.as_ref()) {
            return n.clone();
        }
        let num = self
            .ai_name
            .as_deref()
            .and_then(|a| a.rsplit_once('-'))
            .map(|(_, n)| n)
            .filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
        match (self.agent, num) {
            (Some(kind), Some(num)) => format!("{} {num}", kind.label()),
            _ => self.group.fallback(self.n),
        }
    }

    /// Durum simgesi: ● çalışıyor, ◐ onay/girdi bekliyor, ○ boşta, ◌ bilinmiyor.
    pub fn status_icon(&self) -> &'static str {
        match self.meta.as_ref().and_then(|m| m.status) {
            Some(Status::Busy) => "●",
            Some(Status::Waiting) => "◐",
            Some(Status::Idle) => "○",
            None => "◌",
        }
    }
}

/// Tab çubuğu öğesi.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Item {
    /// AI grubunda birden çok sekme varken en soldaki "≡ Summary".
    Summary,
    Tab(TabId),
}

#[derive(Debug, Clone, Default)]
struct GroupState {
    tabs: Vec<Tab>,
    current: Option<Item>,
    active: Option<TabId>,
    seq: u32,
}

/// Yeni sekmenin bilgileri.
#[derive(Debug, Clone, Default)]
pub struct NewTab {
    pub cwd: PathBuf,
    pub agent: Option<AgentKind>,
    pub ai_name: Option<String>,
    pub account: Option<Account>,
    pub pid: Option<u32>,
}

/// Saf sekme modeli.
#[derive(Debug, Clone, Default)]
pub struct TabsModel {
    groups: [GroupState; 2],
    next_id: TabId,
}

impl TabsModel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Gruba sekme ekler; yeni sekme hem görünen hem aktif olur.
    pub fn add(&mut self, group: Group, init: NewTab) -> TabId {
        self.next_id += 1;
        let id = self.next_id;
        let g = &mut self.groups[group.idx()];
        g.seq += 1;
        let agent = init.agent.or((group == Group::Ai).then_some(AgentKind::Claude));
        g.tabs.push(Tab {
            id,
            group,
            n: g.seq,
            cwd: init.cwd,
            name: None,
            auto: None,
            agent,
            ai_name: init.ai_name,
            account: init.account,
            pid: init.pid,
            meta: None,
        });
        g.current = Some(Item::Tab(id));
        g.active = Some(id);
        id
    }

    pub fn tabs(&self, group: Group) -> &[Tab] {
        &self.groups[group.idx()].tabs
    }

    pub fn all_tabs(&self) -> impl Iterator<Item = &Tab> {
        self.groups.iter().flat_map(|g| g.tabs.iter())
    }

    pub fn tab(&self, id: TabId) -> Option<&Tab> {
        self.all_tabs().find(|t| t.id == id)
    }

    pub fn tab_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        self.groups.iter_mut().flat_map(|g| g.tabs.iter_mut()).find(|t| t.id == id)
    }

    pub fn group_of(&self, id: TabId) -> Option<Group> {
        self.tab(id).map(|t| t.group)
    }

    /// Tab çubuğundaki öğeler: AI grubunda birden çok sekme varken önce özet.
    pub fn items(&self, group: Group) -> Vec<Item> {
        let g = &self.groups[group.idx()];
        let mut items = Vec::with_capacity(g.tabs.len() + 1);
        if group == Group::Ai && g.tabs.len() > 1 {
            items.push(Item::Summary);
        }
        items.extend(g.tabs.iter().map(|t| Item::Tab(t.id)));
        items
    }

    /// Son seçilen gerçek sekme (yoksa son sekme).
    pub fn active(&mut self, group: Group) -> Option<TabId> {
        let g = &mut self.groups[group.idx()];
        if let Some(a) = g.active.filter(|a| g.tabs.iter().any(|t| t.id == *a)) {
            return Some(a);
        }
        g.active = g.tabs.last().map(|t| t.id);
        g.active
    }

    /// Panelde gösterilecek öğe: seçili öğe hâlâ geçerliyse o, değilse aktif sekme.
    pub fn current(&mut self, group: Group) -> Option<Item> {
        let items = self.items(group);
        if let Some(c) = self.groups[group.idx()].current.filter(|c| items.contains(c)) {
            return Some(c);
        }
        let cur = self.active(group).map(Item::Tab);
        self.groups[group.idx()].current = cur;
        cur
    }

    /// Öğeyi seçer (görünen yapar; gerçek sekmeyse aktif de olur).
    pub fn select(&mut self, group: Group, item: Item) {
        if !self.items(group).contains(&item) {
            return;
        }
        let g = &mut self.groups[group.idx()];
        g.current = Some(item);
        if let Item::Tab(id) = item {
            g.active = Some(id);
        }
    }

    /// Görünen öğeden `delta` kadar döner (sona gelince başa sarar). Seçilen öğeyi döndürür.
    pub fn cycle(&mut self, group: Group, delta: i32) -> Option<Item> {
        let items = self.items(group);
        if items.is_empty() {
            return None;
        }
        let cur = self.current(group);
        let idx = cur.and_then(|c| items.iter().position(|i| *i == c)).unwrap_or(0) as i64;
        let next = items[(idx + delta as i64).rem_euclid(items.len() as i64) as usize];
        self.select(group, next);
        Some(next)
    }

    /// Sekmeyi kaldırır: aktif/görünen sekmeyse komşusu (sağdaki, yoksa soldaki) geçer.
    pub fn remove(&mut self, id: TabId) -> Option<Tab> {
        let group = self.group_of(id)?;
        let g = &mut self.groups[group.idx()];
        let idx = g.tabs.iter().position(|t| t.id == id)?;
        let tab = g.tabs.remove(idx);
        let neighbor = g.tabs.get(idx).or_else(|| idx.checked_sub(1).and_then(|i| g.tabs.get(i))).map(|t| t.id);
        if g.active == Some(id) {
            g.active = neighbor;
        }
        if g.current == Some(Item::Tab(id)) {
            g.current = g.active.map(Item::Tab);
        }
        if g.current == Some(Item::Summary) && g.tabs.len() < 2 {
            g.current = g.active.map(Item::Tab);
        }
        Some(tab)
    }

    /// Sekmeyi yeniden adlandırır; boş ad otomatik ada döner.
    pub fn rename(&mut self, id: TabId, name: &str) -> bool {
        let Some(tab) = self.tab_mut(id) else { return false };
        let name = name.trim();
        tab.name = (!name.is_empty()).then(|| name.to_string());
        true
    }
}

/// Shell sekmesinin otomatik adı: OSC başlığı (`user@host: yol` → yolun son parçası; kabuğun kendi adı ve `term://`
/// sayılmaz), yoksa ön plandaki süreç, o da yoksa başlangıç klasörü.
pub fn shell_title(osc_title: Option<&str>, shell: &str, foreground: Option<&str>, cwd: &Path) -> String {
    if let Some(title) = osc_title.map(str::trim).filter(|t| {
        !t.is_empty()
            && !t.starts_with("term://")
            && *t != shell
            && Some(*t) != Path::new(shell).file_name().and_then(|f| f.to_str())
    }) {
        if let Some((user_host, path)) = title.split_once(':')
            && user_host.contains('@')
            && !user_host.contains(char::is_whitespace)
        {
            let path = path.trim();
            if path == "~" {
                return "~".into();
            }
            let base = path.trim_end_matches('/').rsplit('/').next().unwrap_or(path);
            return if base.is_empty() { path.to_string() } else { base.to_string() };
        }
        return title.to_string();
    }
    if let Some(fg) = foreground.filter(|f| !f.is_empty()) {
        return fg.to_string();
    }
    cwd.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| cwd.display().to_string())
}

/// Varsayılan kabuk (`$SHELL`, yoksa `/bin/zsh`).
pub fn default_shell() -> String {
    std::env::var("SHELL").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "/bin/zsh".into())
}

/// Yenileme aralığı (Lua zamanlayıcısı gibi 2 sn).
pub const REFRESH_EVERY: Duration = Duration::from_secs(2);

/// [`TermTabs::poll`] sonucu.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PollResult {
    /// Sekme listesi, adlar ya da durumlar değişti (yeniden çiz).
    pub changed: bool,
    /// Süreci bittiği için kaldırılan sekmeler.
    pub removed: Vec<(TabId, Option<ExitInfo>)>,
    /// Kullanıcıya gösterilecek uyarılar (ör. "Claude Code exited with code 1").
    pub notices: Vec<String>,
}

type Hook = std::sync::Arc<dyn Fn() + Send + Sync>;

/// Sekmeler + süreçler + Claude oturum okuyucu + posta kutusu.
pub struct TermTabs {
    pub model: TabsModel,
    terminals: HashMap<TabId, Terminal>,
    pub accounts: Accounts,
    pub ai_state: AiState,
    pub mailbox: Mailbox,
    reader: SessionReader,
    /// LESS MCP sunucusu (varsa AI'lara bağlanır).
    pub mcp: Option<McpLaunch>,
    /// Yeni terminallerin başlangıç boyutu (UI panel boyutunu bilince `resize` eder).
    pub default_size: TermSize,
    last_refresh: Option<Instant>,
    generations: HashMap<TabId, u64>,
    hook: Option<Hook>,
}

impl Default for TermTabs {
    fn default() -> Self {
        Self::new()
    }
}

impl TermTabs {
    /// Kayıtları varsayılan konumlardan yükler.
    pub fn new() -> Self {
        Self::with_parts(Accounts::load(), AiState::load())
    }

    pub fn with_parts(accounts: Accounts, ai_state: AiState) -> Self {
        Self {
            model: TabsModel::new(),
            terminals: HashMap::new(),
            accounts,
            ai_state,
            mailbox: Mailbox::new(),
            reader: SessionReader::new(),
            mcp: None,
            default_size: TermSize::new(80, 24),
            last_refresh: None,
            generations: HashMap::new(),
            hook: None,
        }
    }

    /// Terminal içeriği değişince ya da posta gelince çağrılır (UI `request_repaint`). Mevcut terminallere de uygulanır.
    pub fn set_on_change(&mut self, hook: impl Fn() + Send + Sync + 'static) {
        let hook: Hook = std::sync::Arc::new(hook);
        for t in self.terminals.values() {
            let h = hook.clone();
            t.set_on_change(move || h());
        }
        let h = hook.clone();
        self.mailbox.set_on_mail(move || h());
        self.hook = Some(hook);
    }

    pub fn terminal(&self, id: TabId) -> Option<&Terminal> {
        self.terminals.get(&id)
    }

    /// Gruba yeni bir terminal sekmesi ekler ve komutu başlatır.
    pub fn spawn(&mut self, group: Group, opts: SpawnOptions, init: NewTab) -> std::io::Result<TabId> {
        let mut opts = opts;
        opts.size.get_or_insert(self.default_size);
        if opts.cwd.is_none() {
            opts.cwd = Some(init.cwd.clone());
        }
        let term = Terminal::spawn(opts)?;
        if let Some(h) = &self.hook {
            let h = h.clone();
            term.set_on_change(move || h());
        }
        let id = self.model.add(group, NewTab { pid: term.pid(), ..init });
        self.generations.insert(id, term.generation());
        self.terminals.insert(id, term);
        self.refresh_tab(id);
        Ok(id)
    }

    /// Yeni shell sekmesi (Terminal paneli, Cmd+T).
    pub fn spawn_shell(&mut self, cwd: &Path) -> std::io::Result<TabId> {
        let opts = SpawnOptions { argv: vec![default_shell()], cwd: Some(cwd.to_path_buf()), ..Default::default() };
        self.spawn(Group::Shell, opts, NewTab { cwd: cwd.to_path_buf(), ..Default::default() })
    }

    /// AI türünü varsayılan yapar ve yeni sekmede başlatır (karar #64). Claude seçili hesapla açılır (#63);
    /// `extra_env` IDE bağlantısı gibi ek ortam. Posta kutusu hazırlanır ve izlenir (#65).
    pub fn open_agent(
        &mut self,
        kind: AgentKind,
        cwd: &Path,
        ollama_model: Option<&str>,
        extra_env: Vec<(String, String)>,
    ) -> std::io::Result<TabId> {
        let _ = self.ai_state.choose(kind, ollama_model);
        let account = (kind == AgentKind::Claude).then(|| self.accounts.current());
        let mut env = account.as_ref().map(|a| self.accounts.env(a)).unwrap_or_default();
        env.extend(extra_env);
        let used: Vec<String> = self.model.tabs(Group::Ai).iter().filter_map(|t| t.ai_name.clone()).collect();
        let model =
            (kind == AgentKind::Ollama).then(|| ollama_model.unwrap_or(self.ai_state.ollama_model()).to_string());
        let launch = agents::launch(&LaunchRequest {
            kind,
            cwd,
            used_names: used.iter().map(String::as_str).collect(),
            model: model.as_deref(),
            mcp: self.mcp.as_ref(),
            claude_cmd: None,
            extra_env: env,
        });
        let opts = SpawnOptions {
            argv: launch.argv.clone(),
            cwd: Some(cwd.to_path_buf()),
            env: launch.env.clone(),
            size: None,
        };
        let init = NewTab {
            cwd: cwd.to_path_buf(),
            agent: Some(kind),
            ai_name: Some(launch.name.clone()),
            account,
            pid: None,
        };
        let id = self.spawn(Group::Ai, opts, init)?;
        self.prepare_mailbox(&launch.dir);
        Ok(id)
    }

    /// Posta kutusunu bu klasördeki açık AI'larla yeniden yazar ve izlemeye alır.
    pub fn prepare_mailbox(&mut self, dir: &Path) {
        let list: Vec<(String, AgentKind)> = self
            .model
            .tabs(Group::Ai)
            .iter()
            .filter(|t| agents::mailbox_dir(&t.cwd) == dir)
            .filter_map(|t| Some((t.ai_name.clone()?, t.agent?)))
            .collect();
        if mailbox::prepare(dir, &list).is_ok() {
            let _ = self.mailbox.watch(dir);
        }
    }

    /// Sekmenin sürecini sonlandırır; sekme süreç bitince [`TermTabs::poll`] ile kalkar.
    /// AI sekmesi kapatmadan önce UI onay sorar ("Close session '<ad>'?").
    pub fn close(&mut self, id: TabId) {
        match self.terminals.get(&id) {
            Some(t) if t.is_alive() => t.kill(),
            _ => {
                self.remove(id);
            }
        }
    }

    /// AI sekmeleri kapatılmadan önce onay ister.
    pub fn needs_confirm(&self, id: TabId) -> bool {
        self.model.group_of(id) == Some(Group::Ai)
    }

    fn remove(&mut self, id: TabId) -> Option<Tab> {
        self.terminals.remove(&id);
        self.generations.remove(&id);
        self.model.remove(id)
    }

    /// Tuşu sekmenin terminaline gönderir (Claude'da Shift+Enter yeni satır).
    pub fn send_key(&self, id: TabId, key: Key, mods: Modifiers) {
        let claude = self.model.tab(id).and_then(|t| t.agent) == Some(AgentKind::Claude);
        if let Some(t) = self.terminals.get(&id) {
            t.send_key(key, mods, claude);
        }
    }

    /// Sekmenin otomatik adını ve (AI'da) durumunu tazeler.
    pub fn refresh_tab(&mut self, id: TabId) {
        let Some(tab) = self.model.tab(id) else { return };
        let term = self.terminals.get(&id);
        match (tab.group, tab.agent) {
            (Group::Shell, _) => {
                let title = term.and_then(Terminal::title);
                let fg = term.and_then(Terminal::foreground_process_name);
                let auto = shell_title(title.as_deref(), &default_shell(), fg.as_deref(), &tab.cwd);
                if let Some(t) = self.model.tab_mut(id) {
                    t.auto = Some(auto);
                }
            }
            (Group::Ai, Some(AgentKind::Claude)) => {
                let dir = tab.account.as_ref().map(|a| a.dir.clone()).unwrap_or_else(|| self.accounts.current().dir);
                let snap = tab.pid.and_then(|pid| self.reader.snapshot(pid, &dir));
                if let Some(t) = self.model.tab_mut(id) {
                    t.auto = snap.as_ref().and_then(|s| s.title.clone());
                    t.meta = snap.map(|s| TabMeta { status: s.status, snapshot: Some(s) });
                }
            }
            (Group::Ai, _) => {
                let generation = term.map(Terminal::generation).unwrap_or(0);
                let prev = self.generations.insert(id, generation);
                let busy = prev.is_some_and(|p| p != generation) && tab.meta.is_some();
                if let Some(t) = self.model.tab_mut(id) {
                    let status = if busy { Status::Busy } else { Status::Idle };
                    t.meta = Some(TabMeta { status: Some(status), snapshot: None });
                }
            }
        }
    }

    /// Zamanlayıcı adımı (UI her karede ya da kısa aralıkla çağırır). Biten süreçlerin sekmelerini her çağrıda
    /// kaldırır; görünen grupların adlarını/durumlarını [`REFRESH_EVERY`]'de bir tazeler; bekleyen posta varsa
    /// panel gizli olsa da AI durumlarını tazeleyip bildirimleri yazar.
    pub fn poll(&mut self, visible_ai: bool, visible_shell: bool) -> PollResult {
        let mut res = PollResult::default();
        let dead: Vec<TabId> = self
            .terminals
            .iter()
            .filter(|(_, t)| !t.is_alive() && t.exit_status().is_some())
            .map(|(id, _)| *id)
            .collect();
        for id in dead {
            let exit = self.terminals.get(&id).and_then(Terminal::exit_status);
            if let Some(tab) = self.remove(id) {
                if tab.agent == Some(AgentKind::Claude)
                    && let Some(code) = exit.as_ref().map(|e| e.code).filter(|c| ![0, 129, 143].contains(c))
                {
                    res.notices.push(format!("Claude Code exited with code {code}"));
                }
                if let Some(dir) = tab.ai_name.as_ref().map(|_| agents::mailbox_dir(&tab.cwd)) {
                    self.prepare_mailbox(&dir);
                }
                res.removed.push((id, exit));
                res.changed = true;
            }
        }
        let due = self.last_refresh.is_none_or(|t| t.elapsed() >= REFRESH_EVERY);
        if !due {
            return res;
        }
        self.last_refresh = Some(Instant::now());
        let pending = self.mailbox.any_pending();
        let ids: Vec<TabId> = self
            .model
            .all_tabs()
            .filter(|t| match t.group {
                Group::Ai => visible_ai || (pending && t.ai_name.as_ref().is_some_and(|n| self.mailbox.has_mail(n))),
                Group::Shell => visible_shell,
            })
            .map(|t| t.id)
            .collect();
        for id in ids {
            let before = self.model.tab(id).cloned();
            self.refresh_tab(id);
            if self.model.tab(id) != before.as_ref() {
                res.changed = true;
            }
        }
        if pending {
            self.deliver_mail();
        }
        res
    }

    /// Bekleyen postası olan boştaki AI'lara bildirimi yazar (metin, 80 ms sonra `\r`).
    pub fn deliver_mail(&mut self) {
        let recipients: Vec<(TabId, Recipient)> = self
            .model
            .tabs(Group::Ai)
            .iter()
            .filter(|t| self.terminals.get(&t.id).is_some_and(Terminal::is_alive))
            .filter_map(|t| {
                Some((
                    t.id,
                    Recipient {
                        name: t.ai_name.clone()?,
                        kind: t.agent?,
                        cwd: t.cwd.clone(),
                        idle: t.meta.as_ref().and_then(|m| m.status) == Some(Status::Idle),
                    },
                ))
            })
            .collect();
        let list: Vec<Recipient> = recipients.iter().map(|(_, r)| r.clone()).collect();
        let now = summary::unix_now();
        for d in self.mailbox.deliver(&list, now) {
            let Some((id, _)) = recipients.iter().find(|(_, r)| r.name == d.name) else { continue };
            if let Some(t) = self.terminals.get(id) {
                t.write_chunks(vec![d.text.into_bytes(), b"\r".to_vec()], Duration::from_millis(80));
            }
        }
    }

    /// Özet tab'ının modeli (karar #60).
    pub fn summary(&self, now: u64) -> SummaryView {
        summary::build(self.model.tabs(Group::Ai), self.accounts.is_multi(), |n| self.mailbox.has_mail(n), now)
    }
}
