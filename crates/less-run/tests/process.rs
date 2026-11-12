use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use less_run::config::{ConfigStore, RunConfig};
use less_run::providers::default_providers;
use less_run::shutdown::{Shutdown, ShutdownState};
use less_run::{AttachTarget, Mode, RunEvent, Runner, Status};

const FAKE_NPM: &str = r#"#!/bin/sh
case "$2" in
ask)
  printf '\033[32mLocal: http://localhost:3123/\033[0m\r\n'
  printf 'Something is already running on port 3000.\n'
  printf '? Run the app on another port instead? (Y/n) '
  read a
  echo "got:$a"
  exit 3 ;;
ok) echo "args:$4 $5"; echo "env:$FOO"; exit 0 ;;
trap) trap 'echo bye; exit 0' INT; echo ready; while :; do sleep 0.1; done ;;
stubborn) trap '' INT; echo ready; while :; do sleep 0.1; done ;;
inspect)
  python3 -c 'import socket,time; s=socket.socket(); s.bind(("127.0.0.1",0)); s.listen(); print("Debugger listening on ws://127.0.0.1:%d/abc" % s.getsockname()[1], flush=True); time.sleep(5)' &
  wait ;;
esac
"#;

struct Env {
    _dir: tempfile::TempDir,
    root: PathBuf,
    runner: Runner,
}

fn setup() -> Env {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("app");
    let bin = dir.path().join("bin");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&bin).unwrap();
    fs::write(root.join("package.json"), r#"{"scripts":{"ask":"x","ok":"x","trap":"x","stubborn":"x","inspect":"x"}}"#)
        .unwrap();
    fs::write(root.join("package-lock.json"), "{}").unwrap();
    let npm = bin.join("npm");
    fs::write(&npm, FAKE_NPM).unwrap();
    fs::set_permissions(&npm, fs::Permissions::from_mode(0o755)).unwrap();
    let configs = ConfigStore::open(dir.path().join("data/run_configs.json"), &[]);
    let mut runner = Runner::with(default_providers(), configs, dir.path().join("data"));
    runner.scan(std::slice::from_ref(&root));
    let path = format!("{}:/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin", bin.display());
    for s in ["ask", "ok", "trap", "stubborn", "inspect"] {
        let id = id(&root, s);
        let cfg = RunConfig { env: Some([("PATH".to_string(), path.clone())].into()), ..Default::default() };
        runner.set_config(&id, &cfg, false).unwrap();
    }
    Env { _dir: dir, root, runner }
}

fn id(root: &Path, script: &str) -> String {
    format!("node:{}:{}", root.display(), script)
}

fn wait_status(r: &mut Runner, id: &str, done: impl Fn(Status) -> bool) -> Vec<RunEvent> {
    r.poll_until(Duration::from_secs(10), |r| r.item(id).is_some_and(|s| done(s.status)))
}

#[test]
fn prompt_answer_port_capture_and_exit_77() {
    let mut e = setup();
    let id = id(&e.root, "ask");
    assert!(e.runner.start(&id, Mode::Run));
    let mut events = e.runner.poll_until(Duration::from_secs(10), |r| r.item(&id).is_some_and(|s| s.prompt.is_some()));
    let prompt = events.iter().find_map(|ev| match ev {
        RunEvent::Prompt { question, .. } => Some(question.clone()),
        _ => None,
    });
    assert_eq!(
        prompt.as_deref(),
        Some("ask: Something is already running on port 3000. Run the app on another port instead?")
    );
    assert_eq!(e.runner.item(&id).unwrap().port.as_deref(), Some("3123"));
    assert!(events.iter().any(|ev| matches!(ev, RunEvent::Port { port, .. } if port == "3123")));
    assert!(e.runner.answer(&id, true));
    assert!(!e.runner.answer(&id, true), "answered once");
    events.extend(wait_status(&mut e.runner, &id, |s| s == Status::Failed));
    let item = e.runner.item(&id).unwrap();
    assert_eq!(item.exit.as_ref().unwrap().code, 3);
    assert!(item.log.lines.iter().any(|l| l.contains("got:y")), "{:?}", item.log.lines);
    assert!(item.log.raw.windows(5).any(|w| w == b"\x1b[32m"), "raw bytes keep colors");
    assert!(events.iter().any(|ev| matches!(ev, RunEvent::Exited { status: Status::Failed, .. })));
    assert_eq!(e.runner.log_items().len(), 1);
}

#[test]
fn config_args_and_env_reach_process_69() {
    let mut e = setup();
    let id = id(&e.root, "ok");
    let mut cfg = e.runner.config(&id);
    cfg.env.as_mut().unwrap().insert("FOO".into(), "bar".into());
    cfg.args = Some("one two".into());
    e.runner.set_config(&id, &cfg, false).unwrap();
    e.runner.start(&id, Mode::Run);
    wait_status(&mut e.runner, &id, |s| s == Status::Done);
    let lines = &e.runner.item(&id).unwrap().log.lines;
    assert!(lines.iter().any(|l| l == "args:one two"), "{lines:?}");
    assert!(lines.iter().any(|l| l == "env:bar"));
}

#[test]
fn rerun_restarts_with_same_mode_and_log_tabs_cycle_101() {
    let mut e = setup();
    let trap = id(&e.root, "trap");
    let ok = id(&e.root, "ok");
    e.runner.start(&trap, Mode::Run);
    e.runner.start(&ok, Mode::Run);
    e.runner.poll_until(Duration::from_secs(5), |r| r.item(&trap).unwrap().log.lines.iter().any(|l| l == "ready"));
    let pid = e.runner.item(&trap).unwrap().pid();
    e.runner.rerun(&trap, None);
    let events =
        e.runner.poll_until(Duration::from_secs(10), |r| r.item(&trap).unwrap().pid().is_some_and(|p| Some(p) != pid));
    assert!(events.iter().any(|ev| matches!(ev, RunEvent::Exited { status: Status::Stopped, .. })));
    assert!(events.iter().any(|ev| matches!(ev, RunEvent::Started { mode: Mode::Run, .. })));
    assert_eq!(e.runner.cycle_log(Some(&trap), 1).as_deref(), Some(ok.as_str()));
    assert_eq!(e.runner.cycle_log(Some(&ok), 1).as_deref(), Some(trap.as_str()), "wraps around");
    assert_eq!(e.runner.cycle_log(None, -1).as_deref(), Some(ok.as_str()));
    e.runner.stop(&trap);
    wait_status(&mut e.runner, &trap, |s| s == Status::Stopped);
}

#[test]
fn shutdown_interrupts_then_force_kills_80() {
    let mut e = setup();
    let trap = id(&e.root, "trap");
    let stubborn = id(&e.root, "stubborn");
    e.runner.start(&trap, Mode::Run);
    e.runner.start(&stubborn, Mode::Run);
    e.runner.poll_until(Duration::from_secs(5), |r| {
        [&trap, &stubborn].iter().all(|i| r.item(i).unwrap().log.lines.iter().any(|l| l == "ready"))
    });
    let mut sd = Shutdown::begin(&mut e.runner).expect("two processes running");
    e.runner.poll_until(Duration::from_secs(5), |r| !r.item(&trap).unwrap().is_running());
    let rows = sd.rows(&e.runner);
    let state = |id: &str| rows.iter().find(|r| r.id == id).unwrap().state;
    assert_eq!(state(&trap), ShutdownState::Stopped);
    assert!(matches!(state(&stubborn), ShutdownState::Stopping { .. }), "SIGINT is ignored by the stubborn one");
    assert!(e.runner.item(&trap).unwrap().log.lines.iter().any(|l| l == "bye"), "trap handler ran");
    assert!(!sd.finished(&e.runner));
    sd.kill(&mut e.runner, &stubborn);
    assert_eq!(sd.rows(&e.runner).iter().find(|r| r.id == stubborn).unwrap().state, ShutdownState::Killing);
    e.runner.poll_until(Duration::from_secs(5), |r| sd.finished(r));
    assert!(sd.finished(&e.runner));
    assert_eq!(e.runner.item(&stubborn).unwrap().status, Status::Stopped);
    assert!(Shutdown::begin(&mut e.runner).is_none());
}

#[test]
fn node_debug_emits_attach_for_child_process_34() {
    if std::process::Command::new("python3").arg("-V").output().is_err() {
        return;
    }
    let mut e = setup();
    let id = id(&e.root, "inspect");
    e.runner.start(&id, Mode::Debug);
    let events = e.runner.poll_until(Duration::from_secs(10), |r| !r.item(&id).unwrap().debug_targets.is_empty());
    let more = e.runner.poll_until(Duration::from_secs(5), |_| false);
    let attach = events.iter().chain(more.iter()).find_map(|ev| match ev {
        RunEvent::Attach { target, .. } => Some(target.clone()),
        _ => None,
    });
    let item = e.runner.item(&id).unwrap();
    let port = item.debug_targets[0];
    assert!(item.port.is_none(), "inspector ws:// line is not an app port");
    match attach {
        Some(AttachTarget::Node { port: p, pid, .. }) => {
            assert_eq!(p, port);
            assert_ne!(Some(pid), item.pid(), "the job's own process is skipped");
        }
        other => panic!("expected node attach target, got {other:?}"),
    }
    e.runner.force_kill(&id);
}
