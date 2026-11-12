//! Komut satırı ve açılış hedefi (#32 #66). Kararlar UI'dan ve canlı örnek kaydından ayrıdır; testler süreç açmaz.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use less_workspace::{autorestore, paths, root, session::Session, workspaces};

use crate::app::Target;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestedTarget {
    Folder(PathBuf),
    Workspace(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Launch {
    pub target: Option<RequestedTarget>,
    pub dashboard: bool,
}

pub enum ParseOutcome {
    Run(Launch),
    Help,
    Version,
}

/// `less-rs [FOLDER]`, `--workspace NAME` ve `--dashboard` dışındaki argümanları reddeder.
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<ParseOutcome, String> {
    let mut launch = Launch::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--help" | "-h") => return Ok(ParseOutcome::Help),
            Some("--version" | "-V") => return Ok(ParseOutcome::Version),
            Some("--dashboard") => launch.dashboard = true,
            Some("--workspace") => {
                let name = args
                    .next()
                    .ok_or("--workspace needs a name")?
                    .into_string()
                    .map_err(|_| "Workspace name must be valid UTF-8".to_owned())?;
                set_target(&mut launch, RequestedTarget::Workspace(name))?;
            }
            Some(text) if text.starts_with("--workspace=") => {
                let name = text.trim_start_matches("--workspace=").to_owned();
                set_target(&mut launch, RequestedTarget::Workspace(name))?;
            }
            Some(text) if text.starts_with('-') => return Err(format!("Unknown option: {text}")),
            _ => set_target(&mut launch, RequestedTarget::Folder(PathBuf::from(arg)))?,
        }
    }
    Ok(ParseOutcome::Run(launch))
}

fn set_target(launch: &mut Launch, target: RequestedTarget) -> Result<(), String> {
    if launch.target.is_some() {
        return Err("Open only one folder or workspace at a time".to_owned());
    }
    launch.target = Some(target);
    Ok(())
}

/// Açılış hedefini seçer. Açık hedef her zaman kazanır; başka LESS örneği varken argümansız açılış karşılamadır.
/// İlk örnekte cwd projesi, sonra cwd oturumu, en son "son proje" kaydı denenir.
pub fn choose_target(
    launch: &Launch,
    other_windows: bool,
    last: Option<autorestore::Record>,
    cwd: &Path,
    cwd_project: Option<PathBuf>,
    cwd_has_session: bool,
    workspaces: &workspaces::Store,
) -> Option<Target> {
    if launch.dashboard {
        return None;
    }
    if let Some(target) = &launch.target {
        return Some(match target {
            RequestedTarget::Folder(path) => Target::Folder(path.clone()),
            RequestedTarget::Workspace(name) => return workspace_target(name, workspaces),
        });
    }
    if other_windows {
        return None;
    }
    if let Some(ws) = workspaces.items.iter().find(|ws| workspace_dir(&ws.name) == cwd) {
        return Some(Target::Workspace(ws.clone()));
    }
    let explicit = cwd_project.or_else(|| cwd_has_session.then(|| cwd.to_path_buf()));
    match autorestore::decide(explicit.as_deref(), false, last) {
        autorestore::Startup::Welcome => None,
        autorestore::Startup::Open(record) => target_from_record(&record, workspaces),
    }
}

fn workspace_target(name: &str, workspaces: &workspaces::Store) -> Option<Target> {
    workspaces.get(name).cloned().map(Target::Workspace)
}

fn target_from_record(record: &autorestore::Record, workspaces: &workspaces::Store) -> Option<Target> {
    match record.kind {
        less_workspace::recent::Kind::Project => Some(Target::Folder(record.dir.clone())),
        less_workspace::recent::Kind::Workspace => workspace_target(record.name.as_deref()?, workspaces),
    }
}

pub fn workspace_dir(name: &str) -> PathBuf {
    paths::data_dir().join("workspaces").join(name)
}

/// Gerçek açılışta cwd sorguları; saf seçim fonksiyonuna veri olarak verilir.
pub fn current_project(cwd: &Path) -> Option<PathBuf> {
    root::project_root(cwd)
}

pub fn current_has_session(cwd: &Path) -> bool {
    Session::load(cwd).is_some()
}

/// Login shell ortamını (`less_workspace::shellenv`) sürece uygular: Rust 2024'te `set_var` `unsafe` olduğu için
/// değişen değişkenlerle aynı ikili yeniden `exec` edilir (pencere açılmadan, LSP / Run / AI süreçlerinden önce).
/// İşaret değişkeni varsa ya da bir şey değişmiyorsa hiçbir şey yapmaz; `exec` başarısız olursa açılış sürer.
#[cfg(unix)]
pub fn adopt_login_env() {
    use less_workspace::shellenv;
    use std::os::unix::process::CommandExt;
    if cfg!(test) || std::env::var_os(shellenv::MARKER).is_some() {
        return;
    }
    let changes = shellenv::changes(&shellenv::load(), |k| std::env::var(k).ok());
    let Ok(exe) = std::env::current_exe() else { return };
    if changes.is_empty() {
        return;
    }
    let err = std::process::Command::new(exe)
        .args(std::env::args_os().skip(1))
        .envs(changes)
        .env(shellenv::MARKER, "1")
        .exec();
    eprintln!("less-rs: could not apply the login shell environment: {err}");
}

#[cfg(not(unix))]
pub fn adopt_login_env() {}

pub const HELP: &str = "LESS\n\nUsage: less-rs [FOLDER]\n       less-rs --workspace NAME\n       less-rs --dashboard\n\nOptions:\n  -h, --help       Show this help\n  -V, --version    Show the version";

#[cfg(test)]
mod tests {
    use super::*;
    use less_workspace::recent::Kind;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn cli_accepts_folder_workspace_and_dashboard_32_61_66() {
        let ParseOutcome::Run(folder) = parse(args(&["/tmp/project"])).unwrap() else { panic!("run") };
        assert_eq!(folder.target, Some(RequestedTarget::Folder(PathBuf::from("/tmp/project"))));
        let ParseOutcome::Run(workspace) = parse(args(&["--workspace", "work"])).unwrap() else { panic!("run") };
        assert_eq!(workspace.target, Some(RequestedTarget::Workspace("work".into())));
        assert!(parse(args(&["--unknown"])).is_err());
        assert!(parse(args(&["a", "b"])).is_err());
        assert!(matches!(parse(args(&["--help"])).unwrap(), ParseOutcome::Help));
    }

    #[test]
    fn first_window_prefers_cwd_then_last_and_later_windows_welcome_32_66() {
        let dir = std::env::temp_dir().join(format!("less-c2-startup-last-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let current = dir.join("current");
        let last_dir = dir.join("last");
        std::fs::create_dir_all(&current).unwrap();
        std::fs::create_dir_all(&last_dir).unwrap();
        let cwd = current.as_path();
        let last = autorestore::Record { version: 1, kind: Kind::Project, dir: last_dir.clone(), name: None, time: 1 };
        let ws = workspaces::Store::open(Path::new("/nonexistent/less-c2-startup-workspaces.json"));
        let launch = Launch::default();
        assert!(matches!(
            choose_target(&launch, false, Some(last.clone()), cwd, Some(PathBuf::from("/work/project")), false, &ws),
            Some(Target::Folder(path)) if path == Path::new("/work/project")
        ));
        assert!(matches!(
            choose_target(&launch, false, Some(last), cwd, None, false, &ws),
            Some(Target::Folder(path)) if path == last_dir
        ));
        assert!(choose_target(&launch, true, None, cwd, Some(cwd.to_path_buf()), true, &ws).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn explicit_workspace_opens_even_when_another_instance_exists_61_66() {
        let dir = std::env::temp_dir().join(format!("less-c2-startup-{}", std::process::id()));
        let store = workspaces::Store::open(&dir.join("workspaces.json"));
        let mut store = store;
        store.create("work", vec![PathBuf::from("/work/api")], 1).unwrap();
        let launch = Launch { target: Some(RequestedTarget::Workspace("work".into())), dashboard: false };
        assert!(matches!(
            choose_target(&launch, true, None, Path::new("/"), None, false, &store),
            Some(Target::Workspace(ws)) if ws.name == "work"
        ));
        let _ = std::fs::remove_dir_all(dir);
    }
}
