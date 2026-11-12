//! Kurucu testleri: ağ yok — sahte `npm` betiği ve `file://` arşivleri.

use super::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

fn script(path: &Path, body: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn wait(inst: &mut Installer, id: &str) -> (Vec<InstallEvent>, Result<(), String>) {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut seen = Vec::new();
    while Instant::now() < deadline {
        for e in inst.poll() {
            if let InstallEvent::Finished { id: i, result, .. } = &e
                && *i == id
            {
                assert!(!inst.is_running(id));
                return (seen, result.clone());
            }
            seen.push(e);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("installer did not finish");
}

fn sys() -> Vec<PathBuf> {
    vec!["/usr/bin".into(), "/bin".into()]
}

#[test]
fn npm_install_links_bins_and_uninstall_removes_them() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = tmp.path().join("fakebin");
    script(&fake.join("node"), "exit 0");
    // Sahte npm: `install --prefix <dir> …` → <dir>/node_modules/.bin/vtsls; çağrıyı kaydeder.
    script(
        &fake.join("npm"),
        &format!(
            "echo \"$@\" >> {log}\nprefix=$3\nmkdir -p \"$prefix/node_modules/.bin\"\nprintf '#!/bin/sh\\n' > \"$prefix/node_modules/.bin/vtsls\"\nchmod +x \"$prefix/node_modules/.bin/vtsls\"",
            log = tmp.path().join("npm.log").display()
        ),
    );
    let dir = tmp.path().join("servers");
    let mut path = vec![fake];
    path.extend(sys());
    let mut inst = Installer::new(dir.clone(), path);
    let entry = catalog().iter().find(|e| e.id == "vtsls").unwrap();
    assert_eq!(status(entry, &dir, &inst.config()), InstallStatus::NotInstalled);

    inst.start(entry, Job::Install).unwrap();
    assert!(inst.is_running("vtsls"));
    assert!(inst.start(entry, Job::Install).is_err(), "one job per entry");
    let (events, result) = wait(&mut inst, "vtsls");
    assert_eq!(result, Ok(()));
    assert!(events.iter().any(|e| matches!(e, InstallEvent::Progress { percent: Some(100), .. })));
    let log = fs::read_to_string(tmp.path().join("npm.log")).unwrap();
    assert!(log.contains("--prefix") && log.contains("@vtsls/language-server"), "{log}");
    assert!(is_executable(&dir.join("bin/vtsls")), "symlink in servers/bin");
    assert!(matches!(status(entry, &dir, &inst.config()), InstallStatus::Installed { managed: true, .. }));

    inst.start(entry, Job::Update).unwrap();
    assert_eq!(wait(&mut inst, "vtsls").1, Ok(()));
    assert!(fs::read_to_string(tmp.path().join("npm.log")).unwrap().contains("@vtsls/language-server@latest"));

    inst.start(entry, Job::Uninstall).unwrap();
    assert_eq!(wait(&mut inst, "vtsls").1, Ok(()));
    assert!(dir.join("bin/vtsls").symlink_metadata().is_err());
    assert!(!dir.join("vtsls").exists());
    assert_eq!(status(entry, &dir, &inst.config()), InstallStatus::NotInstalled);
}

#[test]
fn missing_prerequisite_is_a_clear_error_and_starts_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let mut inst = Installer::new(tmp.path().join("servers"), vec![tmp.path().join("empty")]);
    for (id, needle) in [("vtsls", "Node.js"), ("gopls", "Go"), ("rust-analyzer", "rustup"), ("jdtls", "Java")] {
        let entry = catalog().iter().find(|e| e.id == id).unwrap();
        let err = inst.start(entry, Job::Install).unwrap_err();
        assert!(err.contains(needle), "{id}: {err}");
        assert!(!inst.is_running(id));
    }
    assert!(inst.poll().is_empty());
}

fn tarball(tmp: &Path) -> (PathBuf, String) {
    let src = tmp.join("src");
    script(&src.join("bin/jdtls"), "echo jdtls");
    let tar = tmp.join("x.tar.gz");
    let ok = Command::new("tar").arg("-czf").arg(&tar).arg("-C").arg(&src).arg("bin").status().unwrap();
    assert!(ok.success());
    let sha = format!("{:x}", Sha256::digest(fs::read(&tar).unwrap()));
    (tar, sha)
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

#[test]
fn archive_install_verifies_sha256_and_extracts() {
    let tmp = tempfile::tempdir().unwrap();
    let (tar, sha) = tarball(tmp.path());
    let url = leak(format!("file://{}", tar.display()));
    let dir = tmp.path().join("servers");
    let mut inst = Installer::new(dir.clone(), sys());
    let base = catalog().iter().find(|e| e.id == "jdtls").unwrap().clone();
    let entry = CatalogEntry {
        requires: &[],
        method: Method::Archive { url, sha256: Some(leak(sha)), kind: ArchiveKind::TarGz },
        ..base.clone()
    };
    inst.start(&entry, Job::Install).unwrap();
    let (events, result) = wait(&mut inst, "jdtls");
    assert_eq!(result, Ok(()));
    assert!(events.iter().any(|e| matches!(e, InstallEvent::Progress { message, .. } if message == "Extracting…")));
    assert!(is_executable(&dir.join("bin/jdtls")));
    assert!(matches!(status(&entry, &dir, &inst.config()), InstallStatus::Installed { managed: true, .. }));

    let bad = CatalogEntry {
        method: Method::Archive { url, sha256: Some("00ff"), kind: ArchiveKind::TarGz },
        ..entry.clone()
    };
    inst.start(&bad, Job::Update).unwrap();
    let err = wait(&mut inst, "jdtls").1.unwrap_err();
    assert!(err.contains("Checksum mismatch"), "{err}");
    assert!(is_executable(&dir.join("bin/jdtls")), "failed update keeps the old install");

    let missing = CatalogEntry {
        method: Method::Archive {
            url: leak(format!("file://{}/nope.tgz", tmp.path().display())),
            sha256: None,
            kind: ArchiveKind::TarGz,
        },
        ..entry
    };
    inst.start(&missing, Job::Update).unwrap();
    let err = wait(&mut inst, "jdtls").1.unwrap_err();
    assert!(err.starts_with("Download failed"), "{err}");
}

#[test]
fn binaries_on_path_count_as_installed_but_not_managed() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("bin");
    script(&path.join("gopls"), "exit 0");
    let config = LspConfig { path_dirs: vec![path.clone()], extra_bin_dirs: vec![], ..LspConfig::default() };
    let entry = catalog().iter().find(|e| e.id == "gopls").unwrap();
    assert_eq!(
        status(entry, &tmp.path().join("servers"), &config),
        InstallStatus::Installed { path: path.join("gopls"), managed: false }
    );
    assert_eq!(entry_for_server("eslint").unwrap().id, "vscode-langservers");
    for e in catalog() {
        for s in e.servers {
            let langs_ok = e.languages.iter().any(|l| {
                crate::lsp::registry::server_for(*l).is_some_and(|spec| spec.id == *s)
                    || crate::lsp::registry::linters_for(*l).iter().any(|spec| spec.id == *s)
            });
            assert!(langs_ok, "{s} of {} is registered for one of its languages", e.id);
        }
    }
}

#[test]
fn release_json_and_curl_progress_parse() {
    let json = r#"{"assets":[{"name":"a.zip","browser_download_url":"https://x/a.zip","digest":"sha256:abc"},
        {"name":"server.zip","browser_download_url":"https://x/s.zip","digest":null}]}"#;
    assert_eq!(release_asset(json, "a.zip"), Some(("https://x/a.zip".into(), Some("abc".into()))));
    assert_eq!(release_asset(json, "server.zip"), Some(("https://x/s.zip".into(), None)));
    assert_eq!(release_asset(json, "nope"), None);
    assert_eq!(curl_percent("\r###      12.5%\r########   45.0%"), Some(45));
    assert_eq!(curl_percent("\r#=#=#"), None);
}

#[test]
fn spring_launcher_runs_the_exec_jar_and_open_vsx_json_parses() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(spring_launcher(tmp.path()).is_none(), "no jar yet");
    let dir = tmp.path().join("extension/language-server");
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    std::fs::write(dir.join("spring-boot-language-server-2.5.0-SNAPSHOT-exec.jar"), "").unwrap();
    let script = spring_launcher(tmp.path()).unwrap();
    assert!(script.starts_with("#!/bin/sh\n"));
    assert!(script.contains("-Dsts.lsp.client=vscode"));
    assert!(
        script
            .contains(&format!("-jar '{}'", dir.join("spring-boot-language-server-2.5.0-SNAPSHOT-exec.jar").display()))
    );
    let ok = Command::new("sh").arg("-n").arg("-c").arg(&script).status().unwrap();
    assert!(ok.success(), "valid shell:\n{script}");
    let json = r#"{"version":"2.5","files":{"download":"https://x/a.vsix","sha256":"https://x/a.sha256"}}"#;
    assert_eq!(open_vsx_files(json), Some(("https://x/a.vsix".into(), "https://x/a.sha256".into())));
}

/// Gerçek kurulum (Options'taki Install düğmesiyle aynı): `LESS_REAL_INSTALL=<katalog kimliği>`; ağ kullanır.
#[test]
#[ignore]
fn real_install() {
    let id = std::env::var("LESS_REAL_INSTALL").expect("set LESS_REAL_INSTALL to a catalog id");
    let entry = catalog().iter().find(|e| e.id == id).expect("unknown catalog id");
    let mut inst = Installer::from_env();
    inst.start(entry, Job::Install).unwrap();
    let deadline = Instant::now() + Duration::from_secs(900);
    loop {
        for ev in inst.poll() {
            match ev {
                InstallEvent::Progress { message, .. } => eprintln!("{message}"),
                InstallEvent::Finished { result, .. } => {
                    result.unwrap();
                    eprintln!("{:?}", status(entry, inst.dir(), &inst.config()));
                    return;
                }
            }
        }
        assert!(Instant::now() < deadline, "install timed out");
        std::thread::sleep(Duration::from_millis(200));
    }
}
