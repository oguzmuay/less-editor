//! `git` CLI sarmalayıcısı: tüm modüller buradan çalıştırır.

use std::fmt;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// git çalıştırma hatası; mesaj kullanıcıya gösterilebilir (stderr özeti).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitError(pub String);

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for GitError {}

/// Çıktı baytları + çıkış kodu; `ok_codes` dışındaki kod hata sayılır.
pub(crate) fn run_raw(cwd: &Path, args: &[&str], stdin: Option<&[u8]>, ok_codes: &[i32]) -> Result<Vec<u8>, GitError> {
    let mut cmd = Command::new("git");
    cmd.current_dir(cwd)
        .args(["-c", "core.quotepath=off", "-c", "color.ui=never"])
        .args(args)
        .env("LC_ALL", "C")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() });
    let mut child = cmd.spawn().map_err(|e| GitError(format!("cannot run git: {e}")))?;
    let writer = stdin.map(|data| {
        let mut pipe = child.stdin.take();
        let data = data.to_vec();
        std::thread::spawn(move || {
            if let Some(p) = pipe.as_mut() {
                let _ = p.write_all(&data);
            }
        })
    });
    let out = child.wait_with_output().map_err(|e| GitError(e.to_string()))?;
    if let Some(w) = writer {
        let _ = w.join();
    }
    let code = out.status.code().unwrap_or(-1);
    if code == 0 || ok_codes.contains(&code) {
        Ok(out.stdout)
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        Err(GitError(if err.is_empty() { format!("git {} failed", args.first().unwrap_or(&"")) } else { err }))
    }
}

/// Başarılı çıktıyı metin olarak döndürür.
pub(crate) fn run(cwd: &Path, args: &[&str]) -> Result<String, GitError> {
    run_raw(cwd, args, None, &[]).map(|b| String::from_utf8_lossy(&b).into_owned())
}

#[cfg(test)]
pub(crate) mod testutil {
    use super::run;
    use std::path::Path;
    use tempfile::TempDir;

    pub fn git(dir: &Path, args: &[&str]) -> String {
        run(dir, args).unwrap_or_else(|e| panic!("git {args:?}: {e}"))
    }

    pub fn init_repo() -> TempDir {
        let d = tempfile::tempdir().unwrap();
        git(d.path(), &["init", "-q", "-b", "main"]);
        config(d.path());
        d
    }

    pub fn config(dir: &Path) {
        git(dir, &["config", "user.name", "Test User"]);
        git(dir, &["config", "user.email", "t@example.com"]);
        git(dir, &["config", "commit.gpgsign", "false"]);
    }

    pub fn write(dir: &Path, name: &str, text: &str) {
        let p = dir.join(name);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(p, text).unwrap();
    }

    pub fn commit_file(dir: &Path, name: &str, text: &str, msg: &str) {
        write(dir, name, text);
        git(dir, &["add", "-A"]);
        git(dir, &["commit", "-q", "-m", msg]);
    }
}
