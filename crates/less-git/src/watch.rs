//! Repo izleyici: kök klasörlerde özyinelemeli `notify`, 300 ms birleştirme; `.git` altında yalnız anlamlı dosyalar.

use notify::{RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

/// Birleştirme süresi.
pub const DEBOUNCE: Duration = Duration::from_millis(300);

/// `.git` içinde yenilemeyi tetikleyen yollar mı (`rel` = `.git` sonrası göreli yol).
fn git_internal_relevant(rel: &Path) -> bool {
    let s = rel.to_string_lossy();
    if s.ends_with(".lock") {
        return false;
    }
    matches!(s.as_ref(), "index" | "HEAD" | "packed-refs" | "FETCH_HEAD" | "logs/HEAD") || s.starts_with("refs/")
}

/// `path` (repo `root` altında) değişikliği yenileme gerektirir mi.
pub fn relevant(root: &Path, path: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(root) else { return false };
    let comps: Vec<_> = rel.components().collect();
    match comps.iter().position(|c| c.as_os_str() == ".git") {
        Some(i) => git_internal_relevant(&comps[i + 1..].iter().collect::<PathBuf>()),
        None => true,
    }
}

/// Çalışma ağacı dosyası mı (`.git` içi hariç); Files ağacını yenilemek için.
pub fn worktree_file(root: &Path, path: &Path) -> bool {
    path.strip_prefix(root).is_ok_and(|rel| !rel.components().any(|c| c.as_os_str() == ".git"))
}

/// Canlı izleyici; düşürülünce izleme biter. `rx` değişen repo kökünü (verilen biçimiyle) yollar.
pub struct RepoWatcher {
    _watcher: notify::RecommendedWatcher,
    pub rx: Receiver<PathBuf>,
}

impl RepoWatcher {
    /// Kökleri izlemeye başlar; UI `rx.try_recv()` ile yoklar.
    pub fn new(roots: &[PathBuf]) -> Result<Self, notify::Error> {
        Self::with_filter(roots, relevant, || {})
    }

    /// `filter(kök, yol)` doğru dönen değişiklikler için kökü yollar; her birleştirilmiş partiden sonra `wake` çağrılır.
    pub fn with_filter(
        roots: &[PathBuf],
        filter: fn(&Path, &Path) -> bool,
        wake: impl Fn() + Send + 'static,
    ) -> Result<Self, notify::Error> {
        let pairs: Vec<(PathBuf, PathBuf)> =
            roots.iter().map(|r| (r.canonicalize().unwrap_or_else(|_| r.clone()), r.clone())).collect();
        let (raw_tx, raw_rx) = mpsc::channel::<PathBuf>();
        let (out_tx, out_rx) = mpsc::channel::<PathBuf>();
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(ev) = res {
                for p in ev.paths {
                    let _ = raw_tx.send(p);
                }
            }
        })?;
        for (canon, _) in &pairs {
            watcher.watch(canon, RecursiveMode::Recursive)?;
        }
        std::thread::spawn(move || {
            let owner = |p: &Path| {
                pairs
                    .iter()
                    .filter(|(c, _)| filter(c, p))
                    .max_by_key(|(c, _)| c.components().count())
                    .map(|(_, o)| o.clone())
            };
            let mut pending: Vec<PathBuf> = Vec::new();
            loop {
                let first = if pending.is_empty() {
                    raw_rx.recv().map_err(|_| RecvTimeoutError::Disconnected)
                } else {
                    raw_rx.recv_timeout(DEBOUNCE)
                };
                match first {
                    Ok(p) => {
                        if let Some(o) = owner(&p)
                            && !pending.contains(&o)
                        {
                            pending.push(o);
                        }
                        loop {
                            match raw_rx.recv_timeout(DEBOUNCE) {
                                Ok(p) => {
                                    if let Some(o) = owner(&p)
                                        && !pending.contains(&o)
                                    {
                                        pending.push(o);
                                    }
                                }
                                Err(RecvTimeoutError::Timeout) => break,
                                Err(RecvTimeoutError::Disconnected) => return,
                            }
                        }
                        for o in pending.drain(..) {
                            if out_tx.send(o).is_err() {
                                return;
                            }
                        }
                        wake();
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
        });
        Ok(Self { _watcher: watcher, rx: out_rx })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::testutil::*;

    #[test]
    fn git_internals_filtered_35() {
        let r = Path::new("/r");
        assert!(relevant(r, Path::new("/r/src/a.rs")));
        assert!(relevant(r, Path::new("/r/.git/index")));
        assert!(relevant(r, Path::new("/r/.git/refs/heads/main")));
        assert!(relevant(r, Path::new("/r/.git/logs/HEAD")));
        assert!(!relevant(r, Path::new("/r/.git/objects/ab/cd")));
        assert!(!relevant(r, Path::new("/r/.git/index.lock")));
        assert!(!relevant(r, Path::new("/other/a")));
        assert!(worktree_file(r, Path::new("/r/src/a.rs")));
        assert!(!worktree_file(r, Path::new("/r/.git/index")));
        assert!(!worktree_file(r, Path::new("/other/a")));
    }

    #[test]
    fn file_change_is_reported_once_35() {
        let d = init_repo();
        let w = RepoWatcher::new(&[d.path().to_path_buf()]).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        write(d.path(), "x.txt", "1");
        let got = w.rx.recv_timeout(Duration::from_secs(8)).expect("event");
        assert_eq!(got, d.path().to_path_buf());
    }
}
