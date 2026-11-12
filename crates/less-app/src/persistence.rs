//! Proje başına kalıcı durumun host tarafı: düzen (#99), oturum ve pinler (#30).
//! Her `ProjectView` kendi anahtarını ve depolarını taşır; böylece proje geçişinde eski düzen yeni anahtara yazılmaz.

use std::path::{Path, PathBuf};

use less_workbench::persist::{LayoutStore, PinStore, Snapshot};
use less_workspace::{autorestore, paths, root, session::Session};

/// Bir proje için açılan depolar ve açılışta uygulanacak kayıtlar.
pub struct ProjectState {
    key: PathBuf,
    enabled: bool,
    layouts: LayoutStore,
    pins: PinStore,
    sessions: PathBuf,
    pending_layout: Snapshot,
    pending_session: Option<Session>,
    allow_ai_autostart: bool,
    #[cfg(test)]
    cleanup: Option<PathBuf>,
}

impl ProjectState {
    /// LESS-rs'in normal kayıt klasörlerini kullanır. Kayda uygun olmayan klasörlerde hiçbir kullanıcı dosyası yazılmaz.
    #[cfg(not(test))]
    pub fn new(key: &Path, workspace: bool) -> Self {
        let enabled = root::recordable(key, &paths::home(), workspace, &root::IGNORED_PREFIXES);
        Self::open(
            key,
            paths::state_dir().join("layouts.json"),
            paths::state_dir().join("pins.json"),
            paths::state_dir().join("sessions"),
            enabled,
        )
    }

    /// Depo yolları açıkça verilir; testler ve izole açılışlar bunu kullanır.
    pub fn open(key: &Path, layouts: PathBuf, pins: PathBuf, sessions: PathBuf, enabled: bool) -> Self {
        let stores = LayoutStore::open(&layouts);
        let pending_layout = stores.get_or_default(&key.to_string_lossy());
        let allow_ai_autostart = pending_layout.panels.iter().any(|p| p == "claude");
        let pending_session = enabled.then(|| Session::load_in(&sessions, key)).flatten();
        Self {
            key: key.to_path_buf(),
            enabled,
            layouts: stores,
            pins: PinStore::open(&pins),
            sessions,
            pending_layout,
            pending_session,
            allow_ai_autostart,
            #[cfg(test)]
            cleanup: None,
        }
    }

    /// Testlerde gerçek kullanıcı kayıtları yerine her projeye ayrılmış geçici klasör.
    #[cfg(test)]
    pub fn isolated(key: &Path) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join("less-app-tests").join(format!(
            "{}-{}-{}",
            std::process::id(),
            less_workspace::session::key(key),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut state = Self::open(key, root.join("layouts.json"), root.join("pins.json"), root.join("sessions"), true);
        state.cleanup = Some(root);
        state
    }

    pub fn key(&self) -> &Path {
        &self.key
    }

    pub fn take_layout(&mut self) -> Snapshot {
        std::mem::take(&mut self.pending_layout)
    }

    pub fn take_session(&mut self) -> Option<Session> {
        self.pending_session.take()
    }

    pub fn allow_ai_autostart(&self) -> bool {
        self.allow_ai_autostart
    }

    pub fn pins(&self) -> &[PathBuf] {
        self.pins.pins(&self.key.to_string_lossy())
    }

    pub fn toggle_pin(&mut self, path: &Path) -> std::io::Result<bool> {
        if !self.enabled {
            return Ok(self.pins().iter().any(|p| p == path));
        }
        self.pins.toggle(&self.key.to_string_lossy(), path)
    }

    pub fn save_layout(&mut self, snapshot: Snapshot) -> std::io::Result<bool> {
        if !self.enabled {
            return Ok(false);
        }
        self.layouts.save(&self.key.to_string_lossy(), snapshot)
    }

    pub fn save_session(&self, session: &Session) -> std::io::Result<()> {
        if self.enabled {
            session.save_in(&self.sessions)?;
        }
        Ok(())
    }
}

#[cfg(test)]
impl Drop for ProjectState {
    fn drop(&mut self) {
        if let Some(dir) = &self.cleanup {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

/// Otomatik açma ve "son proje" kaydı ortak anahtarla kapatılır (#32).
pub fn autorestore_enabled() -> bool {
    std::env::var_os("LESS_RS_AUTORESTORE").is_none_or(|v| v != "0") && std::env::var_os("LESS_RS_NO_RESTORE").is_none()
}

/// Açılan hedefi son kayda yazar; geçici ve işaretçisiz klasörler kayda girmez.
pub fn remember_project(key: &Path, workspace: Option<&str>, now: i64) -> std::io::Result<()> {
    if !autorestore_enabled() || !root::recordable(key, &paths::home(), workspace.is_some(), &root::IGNORED_PREFIXES) {
        return Ok(());
    }
    let record = match workspace {
        Some(name) => autorestore::Record::workspace(name, key, now),
        None => autorestore::Record::project(key, now),
    };
    autorestore::write(&record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use less_workbench::layout::{Layout, Panel};

    fn paths(name: &str) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("less-c2-state-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        (dir.clone(), dir.join("layouts.json"), dir.join("pins.json"), dir.join("sessions"))
    }

    #[test]
    fn explicit_stores_roundtrip_layout_session_and_pins_30_99() {
        let (dir, layouts, pins, sessions) = paths("roundtrip");
        let key = dir.join("project");
        let mut state = ProjectState::open(&key, layouts.clone(), pins.clone(), sessions.clone(), true);
        let mut layout = Layout::default();
        layout.open(Panel::Terminal);
        state.save_layout(Snapshot::capture(&layout, 7)).unwrap();
        assert!(state.toggle_pin(&key.join("a.rs")).unwrap());
        Session::new(&key).save_in(&sessions).unwrap();

        let state = ProjectState::open(&key, layouts, pins, sessions, true);
        assert_eq!(state.pending_layout.panels, ["terminal"]);
        assert_eq!(state.pins(), [key.join("a.rs")]);
        assert!(state.pending_session.is_some());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn disabled_project_never_writes_records_32_99() {
        let (dir, layouts, pins, sessions) = paths("disabled");
        let key = dir.join("project");
        let mut state = ProjectState::open(&key, layouts.clone(), pins.clone(), sessions.clone(), false);
        state.save_layout(Snapshot::default()).unwrap();
        state.toggle_pin(Path::new("a.rs")).unwrap();
        state.save_session(&Session::new(&key)).unwrap();
        assert!(!layouts.exists() && !pins.exists() && !sessions.exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
