//! Options paneli: `:options` tab'ı açar, sahte `npm` ile kurulum durumu + bildirim + `lang.servers_changed`,
//! başsız egui çizimi. Ağ yok.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

use super::*;
use crate::panels::Request;
use crate::panels::testing::Harness;

fn script(path: &Path, body: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// Geçici klasör (less-app'te tempfile yok); düşünce silinir.
struct Tmp(PathBuf);

impl Tmp {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("less-options-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Tmp(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn options_tab_lists_servers_installs_with_fake_npm_and_notifies_lang() {
    let tmp = Tmp::new();
    let root = tmp.path().join("proj");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/main.py"), "").unwrap();
    fs::write(root.join("src/app.ts"), "").unwrap();
    fs::write(root.join("package.json"), r#"{"devDependencies":{"tailwindcss":"4"}}"#).unwrap();
    let fake = tmp.path().join("fakebin");
    script(&fake.join("node"), "exit 0");
    script(
        &fake.join("npm"),
        "prefix=$3\nmkdir -p \"$prefix/node_modules/.bin\"\nfor b in basedpyright basedpyright-langserver; do printf '#!/bin/sh\\n' > \"$prefix/node_modules/.bin/$b\"; chmod +x \"$prefix/node_modules/.bin/$b\"; done",
    );
    let inst = Installer::new(tmp.path().join("servers"), vec![fake, "/usr/bin".into(), "/bin".into()]);
    let mut panel = OptionsPanel::new(Some(inst));
    let mut h = Harness::new(vec![root]);

    assert!(panel.event(&Event::Ex("options"), &mut h.cx("options")));
    assert!(h.requests.contains(&Request::OpenTab {
        owner: "options",
        key: "options".into(),
        title: "Options".into()
    }));
    let deadline = Instant::now() + Duration::from_secs(5);
    while panel.project.is_none() && Instant::now() < deadline {
        panel.tick(&mut h.cx("options"));
        std::thread::sleep(Duration::from_millis(10));
    }
    let py = servers::catalog().iter().find(|e| e.id == "basedpyright").unwrap();
    let tw = servers::catalog().iter().find(|e| e.id == "tailwindcss").unwrap();
    let go = servers::catalog().iter().find(|e| e.id == "gopls").unwrap();
    assert_eq!(panel.needed_by(py), Some(vec![Language::Python]));
    assert!(panel.needed_by(tw).unwrap().contains(&Language::TypeScript), "tailwindcss dependency detected");
    assert_eq!(panel.needed_by(go), Some(vec![]));
    assert_eq!(panel.rows[py.id], Row::Status(InstallStatus::NotInstalled));

    h.requests.clear();
    panel.start(py, Job::Install, &mut h.cx("options"));
    assert!(matches!(panel.rows[py.id], Row::Working { .. }));
    let deadline = Instant::now() + Duration::from_secs(10);
    while matches!(panel.rows.get(py.id), Some(Row::Working { .. })) && Instant::now() < deadline {
        panel.tick(&mut h.cx("options"));
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(matches!(panel.rows[py.id], Row::Status(InstallStatus::Installed { managed: true, .. })));
    assert!(h.requests.contains(&Request::Send { topic: TOPIC_SERVERS_CHANGED.into(), data: "basedpyright".into() }));
    assert!(
        h.requests.iter().any(|r| matches!(r, Request::Notify { text, error: false } if text.ends_with("installed")))
    );

    let mut no_node = OptionsPanel::new(Some(Installer::new(tmp.path().join("s2"), vec![tmp.path().join("none")])));
    no_node.start(py, Job::Install, &mut h.cx("options"));
    assert!(matches!(&no_node.rows[py.id], Row::Error(e) if e.contains("Node.js")), "clear prerequisite error");

    let ctx = egui::Context::default();
    for _ in 0..2 {
        let mut out = ctx.run_ui(Default::default(), |ui| {
            let mut cx = h.cx("options");
            panel.tab_ui(TAB_KEY, ui, &mut cx);
        });
        out.textures_delta.clear();
    }
}
