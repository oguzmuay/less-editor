//! Kullanıcının login + interaktif shell ortamı (LESS launcher'ındaki `$SHELL -lic` önbelleği): Dock'tan açılan
//! uygulama da terminaldeki `PATH`'i ve export'ları görsün (LSP, Run, AI süreçleri). Temiz bir taban ortamdan bir kez
//! `$SHELL -lic` çalıştırılır, yalnız rc dosyalarının eklediği / değiştirdiği değişkenler `$TMPDIR/less-rs-env-<uid>`
//! dosyasına (0600) yazılır; rc dosyası daha yeniyse ya da 24 saati geçtiyse yenilenir.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

/// Ortam yüklendikten sonra yeniden başlatılan sürece konan işaret (döngü olmasın).
pub const MARKER: &str = "LESS_RS_SHELL_ENV";

const RC_FILES: [&str; 8] =
    [".zshenv", ".zprofile", ".zshrc", ".zlogin", ".profile", ".bash_profile", ".bashrc", ".config/fish/config.fish"];
const MAX_AGE: Duration = Duration::from_secs(24 * 3600);
const TIMEOUT: Duration = Duration::from_secs(5);
const BASE_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";

/// Pencereye / sürece özgü, shell ortamıyla asla ezilmeyen değişkenler (launcher'daki liste).
pub fn skipped(key: &str) -> bool {
    matches!(key, "PWD" | "OLDPWD" | "SHLVL" | "_" | "NVIM_APPNAME" | "TERM" | "TERMINFO" | "COLORTERM" | MARKER)
        || ["LESS_", "GHOSTTY_", "TERM_PROGRAM"].iter().any(|p| key.starts_with(p))
}

/// `env -0` çıktısı → `(ad, değer)` listesi.
pub fn parse(bytes: &[u8]) -> Vec<(String, String)> {
    String::from_utf8_lossy(bytes)
        .split('\0')
        .filter_map(|kv| kv.split_once('='))
        .filter(|(k, _)| !k.is_empty())
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

/// Önbellek, hiçbir rc dosyasından eski değilse ve 24 saati geçmediyse taze.
pub fn fresh(cache: SystemTime, rc: &[SystemTime], now: SystemTime) -> bool {
    now.duration_since(cache).is_ok_and(|age| age < MAX_AGE) && rc.iter().all(|m| *m <= cache)
}

/// Yüklenen ortamdan şimdikinden farklı ve ezilebilir olanlar.
pub fn changes(loaded: &[(String, String)], current: impl Fn(&str) -> Option<String>) -> Vec<(String, String)> {
    loaded.iter().filter(|(k, v)| !skipped(k) && current(k).as_deref() != Some(v.as_str())).cloned().collect()
}

fn base(home: &Path, shell: &str) -> Vec<(String, String)> {
    let var = |k: &str| std::env::var(k).unwrap_or_default();
    vec![
        ("HOME".into(), home.to_string_lossy().into_owned()),
        ("USER".into(), var("USER")),
        ("LOGNAME".into(), var("LOGNAME")),
        ("SHELL".into(), shell.to_owned()),
        ("TMPDIR".into(), var("TMPDIR")),
        ("PATH".into(), BASE_PATH.into()),
    ]
}

/// Shell'i çalıştırıp ortamı alır; tabanla aynı olan değişkenler atılır. Zaman aşımında süreç öldürülür.
fn capture(shell: &str, home: &Path, out: &Path) -> Option<Vec<(String, String)>> {
    let base = base(home, shell);
    let mut child = Command::new(shell)
        .env_clear()
        .envs(base.iter().cloned())
        .env("LESS_ENV_OUT", out)
        .args(["-lic", "exec /usr/bin/env -0 > \"$LESS_ENV_OUT\""])
        .current_dir(home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let start = Instant::now();
    loop {
        if child.try_wait().ok()?.is_some() {
            break;
        }
        if start.elapsed() > TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let bytes = std::fs::read(out).ok()?;
    let _ = std::fs::remove_file(out);
    let vars = parse(&bytes);
    (!vars.is_empty()).then(|| vars.into_iter().filter(|kv| !base.contains(kv) && kv.0 != "LESS_ENV_OUT").collect())
}

fn serialize(vars: &[(String, String)]) -> Vec<u8> {
    vars.iter().flat_map(|(k, v)| format!("{k}={v}\0").into_bytes()).collect()
}

#[cfg(unix)]
fn private_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let tmp = path.with_extension(format!("{}", std::process::id()));
    let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
    f.write_all(bytes)?;
    std::fs::rename(tmp, path)
}

/// Önbellek dosyası bu kullanıcının (ev klasörünün sahibi) ve yalnız ona açık mı.
#[cfg(unix)]
fn owned_private(path: &Path, uid: u32) -> bool {
    use std::os::unix::fs::MetadataExt;
    std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file() && m.uid() == uid && m.mode() & 0o077 == 0)
}

/// Önbellekten ya da (bayatsa) shell'den login ortamı. Shell bulunamazsa / zaman aşımında boş liste.
/// `dir` önbellek klasörü (`$TMPDIR`), testte geçici klasör.
#[cfg(unix)]
pub fn load_in(dir: &Path, home: &Path, shell: &str) -> Vec<(String, String)> {
    use std::os::unix::fs::MetadataExt;
    let Ok(uid) = std::fs::metadata(home).map(|m| m.uid()) else { return Vec::new() };
    let cache: PathBuf = dir.join(format!("less-rs-env-{uid}"));
    let rc: Vec<SystemTime> =
        RC_FILES.iter().filter_map(|f| std::fs::metadata(home.join(f)).and_then(|m| m.modified()).ok()).collect();
    let mtime = std::fs::metadata(&cache).and_then(|m| m.modified()).ok();
    if owned_private(&cache, uid)
        && mtime.is_some_and(|m| fresh(m, &rc, SystemTime::now()))
        && let Ok(bytes) = std::fs::read(&cache)
    {
        return parse(&bytes);
    }
    let out = dir.join(format!("less-rs-env-{uid}.out.{}", std::process::id()));
    let Some(vars) = capture(shell, home, &out) else {
        let _ = std::fs::remove_file(&out);
        return Vec::new();
    };
    let _ = private_file(&cache, &serialize(&vars));
    vars
}

/// Gerçek konumlarla [`load_in`]: `$TMPDIR`, `$HOME`, `$SHELL` (yoksa `/bin/zsh`).
#[cfg(unix)]
pub fn load() -> Vec<(String, String)> {
    let shell = std::env::var("SHELL").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "/bin/zsh".into());
    load_in(&std::env::temp_dir(), &crate::paths::home(), &shell)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_skip_window_vars() {
        let vars = parse(b"PATH=/a:/b\0EMPTY=\0X=a=b\0\0");
        assert_eq!(vars, [("PATH".into(), "/a:/b".into()), ("EMPTY".into(), "".into()), ("X".into(), "a=b".into())]);
        for k in ["PWD", "TERM", "TERM_PROGRAM_VERSION", "GHOSTTY_X", "LESS_OPEN", MARKER] {
            assert!(skipped(k), "{k}");
        }
        assert!(!skipped("PATH") && !skipped("JAVA_HOME"));
    }

    #[test]
    fn freshness_follows_rc_files_and_age() {
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        assert!(fresh(t, &[t - Duration::from_secs(1)], t + Duration::from_secs(60)));
        assert!(!fresh(t, &[t + Duration::from_secs(1)], t + Duration::from_secs(60)));
        assert!(!fresh(t, &[], t + MAX_AGE));
    }

    #[test]
    fn changes_drop_equal_and_window_vars() {
        let loaded = vec![("PATH".into(), "/x".into()), ("A".into(), "1".into()), ("PWD".into(), "/p".into())];
        let got = changes(&loaded, |k| (k == "A").then(|| "1".to_owned()));
        assert_eq!(got, [("PATH".to_owned(), "/x".to_owned())]);
    }

    #[cfg(unix)]
    #[test]
    fn load_runs_shell_once_then_uses_private_cache() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let dir = std::env::temp_dir().join(format!("less-rs-shellenv-{}", std::process::id()));
        let home = dir.join("home");
        std::fs::create_dir_all(&home).unwrap();
        let shell = dir.join("fake-shell");
        let script = "#!/bin/sh\nFOO=bar /usr/bin/env -0 > \"$LESS_ENV_OUT\"\n";
        std::fs::write(&shell, script).unwrap();
        std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755)).unwrap();
        let first = load_in(&dir, &home, shell.to_str().unwrap());
        assert!(first.contains(&("FOO".into(), "bar".into())));
        assert!(!first.iter().any(|(k, _)| k == "PATH"), "base PATH is not a change");
        let uid = std::fs::metadata(&home).unwrap().uid();
        let cache = dir.join(format!("less-rs-env-{uid}"));
        assert_eq!(std::fs::metadata(&cache).unwrap().mode() & 0o777, 0o600);
        std::fs::remove_file(&shell).unwrap();
        assert_eq!(load_in(&dir, &home, shell.to_str().unwrap()), first, "served from cache");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
