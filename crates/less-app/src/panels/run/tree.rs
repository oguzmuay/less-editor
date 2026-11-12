//! Run ağacının UI dışı mantığı: düzleştirilmiş satırlar, öğe eylemleri (#34), durum ikonu ve rozet.

use std::collections::HashSet;

use less_run::dap::DapState;
use less_run::{Child, Group, GroupKind, ItemSpec, ItemState, Mode, Status};

use super::icons;

/// Ağaçta görünen satır.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// Grup ya da öğe kimliği.
    pub id: String,
    pub label: String,
    pub detail: Option<String>,
    pub depth: usize,
    /// Grup satırıysa türü ve açık mı.
    pub group: Option<(GroupKind, bool)>,
    /// Üst grubun satır dizini (`Left` ile oraya gidilir).
    pub parent: Option<usize>,
}

impl Row {
    pub fn is_group(&self) -> bool {
        self.group.is_some()
    }
}

/// Ağacı satırlara çevirir; `collapsed` gruplarının altı gizlenir (#16).
pub fn rows(tree: &[Group], collapsed: &HashSet<String>) -> Vec<Row> {
    /// Grubu ve (açıksa) altını ekler.
    fn add(g: &Group, depth: usize, parent: Option<usize>, collapsed: &HashSet<String>, out: &mut Vec<Row>) {
        let open = !collapsed.contains(&g.id);
        let me = out.len();
        out.push(Row {
            id: g.id.clone(),
            label: g.label.clone(),
            detail: g.detail.clone(),
            depth,
            group: Some((g.kind, open)),
            parent,
        });
        if !open {
            return;
        }
        for c in &g.children {
            match c {
                Child::Group(sub) => add(sub, depth + 1, Some(me), collapsed, out),
                Child::Item(i) => out.push(item_row(i, depth + 1, Some(me))),
            }
        }
    }
    let mut out = Vec::new();
    for g in tree {
        add(g, 0, None, collapsed, &mut out);
    }
    out
}

fn item_row(i: &ItemSpec, depth: usize, parent: Option<usize>) -> Row {
    Row { id: i.id.clone(), label: i.label.clone(), detail: i.detail.clone(), depth, group: None, parent }
}

/// Öğe eylemi (tuş, satır sonu butonu, sağ tık menüsü, `Space j`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Run,
    Stop,
    Debug,
    Rerun,
    ShowLog,
    EditConfig,
    SetProfile,
}

/// Menüde eylemin etiketi ve ikonu, öğenin durumuna göre (`Rerun (debug)`, `Attach debugger`…).
pub fn action_label(action: Action, s: &ItemState, attached: bool) -> String {
    match action {
        Action::Run => "Run".into(),
        Action::Stop => "Stop".into(),
        Action::Debug if s.is_running() && s.mode != Some(Mode::Debug) => "Restart in debug mode".into(),
        Action::Debug if s.is_running() && !attached => "Attach debugger".into(),
        Action::Debug => "Debug".into(),
        Action::Rerun if s.is_running() && s.mode == Some(Mode::Debug) => "Rerun (debug)".into(),
        Action::Rerun if !s.is_running() => match s.mode {
            Some(Mode::Debug) => "Rerun (debug)".into(),
            _ => "Rerun (run)".into(),
        },
        Action::Rerun => "Rerun".into(),
        Action::ShowLog => "Show log".into(),
        Action::EditConfig => "Edit configuration…".into(),
        Action::SetProfile => match s.profile() {
            Some(p) => format!("Set profile… [{p}]"),
            None => "Set profile…".into(),
        },
    }
}

/// Eylem ikonu.
pub fn action_icon(action: Action) -> &'static str {
    match action {
        Action::Run => icons::PLAY,
        Action::Stop => icons::STOP,
        Action::Debug => icons::BUG,
        Action::Rerun => icons::RERUN,
        Action::ShowLog => icons::LOG,
        Action::EditConfig | Action::SetProfile => icons::GEAR,
    }
}

/// Öğenin o anki durumuna uyan eylemler (LESS-nvim `actions()`); `attached` = DAP oturumu var ya da bağlanıyor.
pub fn actions(s: &ItemState, attached: bool) -> Vec<Action> {
    let mut list = Vec::new();
    if s.is_running() {
        list.push(Action::Stop);
        list.push(Action::Rerun);
        if s.mode != Some(Mode::Debug) || !attached {
            list.push(Action::Debug);
        }
    } else {
        list.push(Action::Run);
        list.push(Action::Debug);
        if s.mode.is_some() {
            list.push(Action::Rerun);
        }
    }
    if s.seq.is_some() {
        list.push(Action::ShowLog);
    }
    list.push(Action::EditConfig);
    if s.spec.supports_profiles() {
        list.push(Action::SetProfile);
    }
    list
}

/// Satır sonu butonları (#34): Run/Stop, Debug, Rerun, Log.
pub fn buttons(s: &ItemState) -> [Action; 4] {
    [if s.is_running() { Action::Stop } else { Action::Run }, Action::Debug, Action::Rerun, Action::ShowLog]
}

/// Durum ikonunun rengi.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Dim,
    Ok,
    Busy,
    Error,
    Debug,
}

/// Öğenin durum ikonu ve tonu (log tab'ında da kullanılır).
pub fn status_icon(s: &ItemState) -> (&'static str, Tone) {
    if s.is_debugging() {
        return if s.status == Status::Starting { (icons::SPINNER, Tone::Debug) } else { (icons::BUG, Tone::Debug) };
    }
    match s.status {
        Status::Idle => (icons::CIRCLE, Tone::Dim),
        Status::Starting => (icons::SPINNER, Tone::Busy),
        Status::Running => (icons::PLAY, Tone::Ok),
        Status::Done => (icons::CHECK_CIRCLE, Tone::Dim),
        Status::Failed => (icons::X_CIRCLE, Tone::Error),
        Status::Stopped => (icons::STOP, Tone::Dim),
    }
}

/// Debug rozeti (`attached ×2`, `waiting for jdtls…`, `attaching…`, `detached`); debug modunda değilse `None`.
pub fn debug_badge(s: &ItemState, state: Option<DapState>, sessions: usize) -> Option<(String, Tone)> {
    if !s.is_debugging() {
        return None;
    }
    let port = s.debug_port.map(|p| format!(":{p} ")).unwrap_or_default();
    Some(match state {
        _ if sessions > 0 => {
            let n = if sessions > 1 { format!(" ×{sessions}") } else { String::new() };
            (format!("{} {port}attached{n}", icons::LINK), Tone::Debug)
        }
        Some(DapState::Waiting) => (format!("{} {port}waiting for jdtls…", icons::SPINNER), Tone::Busy),
        Some(DapState::Attaching) => (format!("{} {port}attaching…", icons::SPINNER), Tone::Busy),
        None => (format!("{} {port}detached", icons::LINK_BREAK), Tone::Dim),
    })
}

/// Satırdaki ek bilgi: ayrıntı, port, profil.
pub fn item_detail(s: &ItemState) -> String {
    let mut parts = Vec::new();
    if let Some(d) = &s.spec.detail {
        parts.push(d.clone());
    }
    if let Some(p) = s.profile() {
        parts.push(format!("[{p}]"));
    }
    if let Some(port) = &s.port {
        parts.push(format!(":{port}"));
    }
    parts.join("  ")
}
