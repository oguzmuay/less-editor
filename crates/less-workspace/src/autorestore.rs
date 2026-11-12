//! Açılışta son proje / workspace (#32) ve çoklu pencere kuralı (#66).
//!
//! "Son" kaydı `<state>/autorestore.json` → `{version, kind, dir, name?, time}` (LESS-nvim ile aynı). Pencere kayıtları
//! `<state>/instances/<pid>`: yalnız ilk pencere son projeyi açar, sonrakiler karşılama ekranını.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::paths;
use crate::recent::Kind;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Record {
    #[serde(default = "one")]
    pub version: u32,
    pub kind: Kind,
    pub dir: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub time: i64,
}

fn one() -> u32 {
    1
}

impl Record {
    pub fn project(dir: &Path, time: i64) -> Self {
        Self { version: 1, kind: Kind::Project, dir: dir.to_path_buf(), name: None, time }
    }

    pub fn workspace(name: &str, dir: &Path, time: i64) -> Self {
        Self { version: 1, kind: Kind::Workspace, dir: dir.to_path_buf(), name: Some(name.to_owned()), time }
    }
}

fn default_file() -> PathBuf {
    paths::state_dir().join("autorestore.json")
}

/// Son kaydı okur; yoksa ya da bozuksa `None`.
pub fn read_from(file: &Path) -> Option<Record> {
    serde_json::from_slice(&std::fs::read(file).ok()?).ok()
}

pub fn read() -> Option<Record> {
    read_from(&default_file())
}

pub fn write_to(file: &Path, rec: &Record) -> std::io::Result<()> {
    paths::write_atomic(file, &serde_json::to_vec(rec).map_err(std::io::Error::other)?)
}

pub fn write(rec: &Record) -> std::io::Result<()> {
    write_to(&default_file(), rec)
}

/// Açılışta ne yapılacağı.
#[derive(Clone, Debug, PartialEq)]
pub enum Startup {
    /// Verilen proje / workspace'i aç.
    Open(Record),
    /// Karşılama ekranı.
    Welcome,
}

/// Açılış kararı (#32, #66). `explicit` komut satırı / menüden verilen klasör (her zaman aç), `other_windows` başka
/// canlı LESS penceresi var mı, `last` son kayıt (klasörü hâlâ duruyorsa).
pub fn decide(explicit: Option<&Path>, other_windows: bool, last: Option<Record>) -> Startup {
    if let Some(dir) = explicit {
        return Startup::Open(Record::project(dir, 0));
    }
    if other_windows {
        return Startup::Welcome;
    }
    match last {
        Some(r) if r.dir.exists() || r.kind == Kind::Workspace => Startup::Open(r),
        _ => Startup::Welcome,
    }
}

/// Süreç yaşıyor mu (`kill -0`).
pub fn pid_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Pencere kaydı: oluşturulunca dosya yazılır, düşünce (`Drop`) silinir.
pub struct Instance {
    file: PathBuf,
    /// Kayıt sırasında başka canlı pencere var mıydı.
    pub others: bool,
}

impl Instance {
    pub fn register() -> Self {
        Self::register_in(&paths::state_dir().join("instances"), std::process::id(), pid_alive)
    }

    /// Ölü kayıtları siler, kendini yazar. `alive` test için enjekte edilir.
    pub fn register_in(dir: &Path, me: u32, alive: impl Fn(u32) -> bool) -> Self {
        let _ = std::fs::create_dir_all(dir);
        let mut others = false;
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let Some(pid) = e.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else { continue };
                if pid == me {
                    continue;
                }
                if alive(pid) {
                    others = true;
                } else {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        let file = dir.join(me.to_string());
        let _ = std::fs::write(&file, b"");
        Self { file, others }
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("less-rs-ar-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn record_roundtrip_32() {
        let d = tmp("rt");
        let f = d.join("autorestore.json");
        write_to(&f, &Record::workspace("ws", Path::new("/x"), 7)).unwrap();
        let r = read_from(&f).unwrap();
        assert_eq!((r.kind, r.name.as_deref(), r.time), (Kind::Workspace, Some("ws"), 7));
        assert!(read_from(&d.join("missing.json")).is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn first_window_opens_last_later_welcome_66() {
        let d = tmp("dec");
        std::fs::create_dir_all(&d).unwrap();
        let rec = Record::project(&d, 1);
        assert_eq!(decide(None, false, Some(rec.clone())), Startup::Open(rec.clone()));
        assert_eq!(decide(None, true, Some(rec)), Startup::Welcome);
        assert!(matches!(decide(Some(&d), true, None), Startup::Open(_)));
        assert_eq!(decide(None, false, None), Startup::Welcome);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn missing_project_dir_falls_back_to_welcome_32() {
        let rec = Record::project(Path::new("/nonexistent/zzz"), 1);
        assert_eq!(decide(None, false, Some(rec)), Startup::Welcome);
    }

    #[test]
    fn instances_detect_live_and_prune_dead_66() {
        let d = tmp("inst");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("111"), "").unwrap();
        std::fs::write(d.join("222"), "").unwrap();
        let a = Instance::register_in(&d, 1, |pid| pid == 111);
        assert!(a.others);
        assert!(!d.join("222").exists());
        drop(a);
        assert!(!d.join("1").exists());
        let b = Instance::register_in(&d, 2, |_| false);
        assert!(!b.others);
        let _ = std::fs::remove_dir_all(&d);
    }
}
