//! Run paneli testleri: gerçek pty (sahte `npm` script'i, `PATH` öğe ayarıyla verilir) ve sahte DAP adaptörü
//! (TcpListener). GUI açılmaz; `tick` başsız `egui::Context::run_ui` içinde çağrılır.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui;
use less_input::{EditMode, Key, KeyEvent, Mods};
use less_run::config::ConfigStore;
use less_run::dap::node_attach_config;
use less_run::providers::default_providers;
use less_run::{RunConfig, Runner, Status};

use super::popup::{InputFor, Outcome, Popup, Then};
use super::tree::{self, Action};
use super::*;
use crate::editor::EditorTab;
use crate::panels::Request;
use crate::panels::testing::Harness;

const FAKE_NPM: &str = r#"#!/bin/sh
case "$2" in
ask) printf 'Something is already running on port 3000.\n? Run the app on another port instead? (Y/n) '; read a; echo "got:$a"; exit 3 ;;
ok) printf '\033[32mhello\033[0m\n'; echo "args:$4 $5"; echo "env:$FOO"; exit 0 ;;
trap) trap 'echo bye; exit 0' INT; echo ready; while :; do sleep 0.1; done ;;
stubborn) trap '' INT; echo ready; while :; do sleep 0.1; done ;;
esac
"#;

/// `tempfile` olmadan geçici klasör (düşürülünce silinir).
struct Tmp(PathBuf);

impl Tmp {
    fn new() -> Tmp {
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("less-b2-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Tmp(dir.canonicalize().unwrap())
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Env {
    tmp: Tmp,
    root: PathBuf,
    panel: RunPanel,
    h: Harness,
}

fn setup() -> Env {
    let tmp = Tmp::new();
    let root = tmp.0.join("app");
    let bin = tmp.0.join("bin");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&bin).unwrap();
    fs::write(root.join("package.json"), r#"{"scripts":{"ask":"x","ok":"x","trap":"x","stubborn":"x"}}"#).unwrap();
    fs::write(root.join("package-lock.json"), "{}").unwrap();
    let npm = bin.join("npm");
    fs::write(&npm, FAKE_NPM).unwrap();
    fs::set_permissions(&npm, fs::Permissions::from_mode(0o755)).unwrap();
    let data = tmp.0.join("data");
    let mut runner =
        Runner::with(default_providers(), ConfigStore::open(data.join("run_configs.json"), &[]), data.clone());
    runner.scan(std::slice::from_ref(&root));
    let path = format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", bin.display());
    for s in ["ask", "ok", "trap", "stubborn"] {
        let cfg = RunConfig { env: Some([("PATH".to_string(), path.clone())].into()), ..Default::default() };
        runner.set_config(&id(&root, s), &cfg, false).unwrap();
    }
    let mut panel = RunPanel::with(runner, Some(data.join("breakpoints.json")));
    panel.scanned = true;
    let h = Harness::new(vec![root.clone()]);
    Env { tmp, root, panel, h }
}

fn id(root: &Path, script: &str) -> String {
    format!("node:{}:{}", root.display(), script)
}

/// Bir kare: `tick` başsız egui geçişinde.
fn frame(panel: &mut RunPanel, h: &mut Harness) {
    let ctx = h.ctx.clone();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 800.0))),
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |_ui| {
        let mut cx = h.cx("run");
        panel.tick(&mut cx);
    });
    out.textures_delta.clear();
}

/// `done` doğru olana kadar (en çok 10 sn) kare çizer.
fn until(e: &mut Env, mut done: impl FnMut(&mut RunPanel, &Harness) -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        frame(&mut e.panel, &mut e.h);
        if done(&mut e.panel, &e.h) {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    false
}

/// Üç bölgeyi başsız çizer (boyama kodu panik vermesin; buton genişliği eşiğinin üstünde).
fn render(e: &mut Env) {
    let ctx = e.h.ctx.clone();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 800.0))),
        ..Default::default()
    };
    let (panel, h) = (&mut e.panel, &mut e.h);
    let mut out = ctx.run_ui(input, |ui| {
        for (n, region) in REGIONS.into_iter().enumerate() {
            let rect = egui::Rect::from_min_size(egui::pos2(n as f32 * 400.0, 0.0), egui::vec2(400.0, 300.0));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            let mut cx = h.cx("run");
            panel.ui(region, &mut child, &mut cx);
        }
    });
    out.textures_delta.clear();
}

fn key(e: &mut Env, k: KeyEvent) -> bool {
    let mut cx = e.h.cx("run");
    e.panel.event(&Event::Key(k), &mut cx)
}

fn log_text(p: &RunPanel, id: &str) -> String {
    p.logs.get(id).map(|t| t.snapshot().text()).unwrap_or_default()
}

fn status(p: &RunPanel, id: &str) -> Status {
    p.runner.item(id).unwrap().status
}

#[test]
fn tree_rows_collapse_and_navigate_16() {
    let mut e = setup();
    let rows = e.panel.rows();
    assert!(rows[0].is_group());
    let items: Vec<_> = rows.iter().filter(|r| !r.is_group()).map(|r| r.label.as_str()).collect();
    assert_eq!(items, ["ask", "ok", "trap", "stubborn"], "script order as in package.json");
    let first_item = rows.iter().position(|r| !r.is_group()).unwrap();
    assert!(rows[first_item].parent.is_some());
    e.panel.cursor = Some(rows[0].id.clone());
    assert!(key(&mut e, KeyEvent::char(' ')));
    assert_eq!(e.panel.rows().len(), 1, "Space collapses the group");
    assert!(key(&mut e, KeyEvent::plain(Key::Right)));
    assert_eq!(e.panel.rows().len(), rows.len());
    e.panel.cursor = Some(rows[first_item].id.clone());
    assert!(key(&mut e, KeyEvent::plain(Key::Left)));
    assert_eq!(e.panel.cursor.as_deref(), rows[first_item].parent.map(|p| rows[p].id.as_str()), "Left goes to parent");
    assert!(key(&mut e, KeyEvent::char('q')));
    assert_eq!(e.h.requests.last(), Some(&Request::Layout(LayoutOp::Close(Area::Run))));
}

#[test]
fn item_actions_and_buttons_follow_state_34() {
    let mut e = setup();
    let trap = id(&e.root, "trap");
    let s = e.panel.runner.item(&trap).unwrap();
    assert_eq!(tree::actions(s, false), [Action::Run, Action::Debug, Action::EditConfig]);
    assert_eq!(tree::buttons(s)[0], Action::Run);
    e.panel.cursor = Some(trap.clone());
    assert!(key(&mut e, KeyEvent::plain(Key::Enter)));
    assert!(until(&mut e, |p, _| log_text(p, &trap).contains("ready")));
    let s = e.panel.runner.item(&trap).unwrap();
    assert_eq!(
        tree::actions(s, false),
        [Action::Stop, Action::Rerun, Action::Debug, Action::ShowLog, Action::EditConfig]
    );
    assert_eq!(tree::action_label(Action::Debug, s, false), "Restart in debug mode");
    assert_eq!(tree::buttons(s)[0], Action::Stop);
    assert!(key(&mut e, KeyEvent::char('m')));
    match &e.panel.popup {
        Some(Popup::Menu { entries, .. }) => assert_eq!(entries[0].then, Then::Do(trap.clone(), Action::Stop)),
        other => panic!("expected the action menu, got {other:?}"),
    }
    e.panel.popup = None;
    assert!(key(&mut e, KeyEvent::char('s')));
    assert!(until(&mut e, |p, _| status(p, &trap) == Status::Stopped));
}

#[test]
fn run_output_goes_to_log_tabs_and_cmd_shift_arrows_cycle_101() {
    let mut e = setup();
    let ok = id(&e.root, "ok");
    let trap = id(&e.root, "trap");
    e.panel.cursor = Some(ok.clone());
    key(&mut e, KeyEvent::plain(Key::Enter));
    assert!(until(&mut e, |p, _| status(p, &ok) == Status::Done && log_text(p, &ok).contains("args:")));
    assert_eq!(e.panel.log.as_deref(), Some(ok.as_str()), "started item's log is shown");
    let text = log_text(&e.panel, &ok);
    assert!(text.contains("hello"), "ANSI colors are parsed, not printed: {text:?}");
    assert!(!text.contains("[32m"));
    render(&mut e);
    assert_ne!(e.panel.logs.size().cols, 120, "log grid follows the region size");
    e.panel.cursor = Some(trap.clone());
    key(&mut e, KeyEvent::plain(Key::Enter));
    assert!(until(&mut e, |p, _| log_text(p, &trap).contains("ready")));
    assert_eq!(e.panel.log.as_deref(), Some(trap.as_str()));
    let cmd_shift = Mods { cmd: true, shift: true, ..Mods::NONE };
    assert!(key(&mut e, KeyEvent::new(Key::Right, cmd_shift)));
    assert_eq!(e.panel.log.as_deref(), Some(ok.as_str()), "wraps to the first tab");
    assert!(key(&mut e, KeyEvent::new(Key::Left, cmd_shift)));
    assert_eq!(e.panel.log.as_deref(), Some(trap.as_str()));
    e.panel.runner.stop(&trap);
    assert!(until(&mut e, |p, _| status(p, &trap) == Status::Stopped));
}

#[test]
fn yes_no_prompt_answers_the_process_77() {
    let mut e = setup();
    let ask = id(&e.root, "ask");
    e.panel.runner.start(&ask, Mode::Run);
    assert!(until(&mut e, |p, _| matches!(p.popup, Some(Popup::Prompt { .. }))));
    let out = e.panel.popup.as_mut().unwrap().key(&KeyEvent::char('y'));
    assert_eq!(out, Outcome::Answer { id: ask.clone(), yes: true });
    let mut cx = e.h.cx("run");
    e.panel.apply(out, &mut cx);
    assert!(e.panel.popup.is_none());
    assert!(until(&mut e, |p, _| status(p, &ask) == Status::Failed));
    assert!(log_text(&e.panel, &ask).contains("got:y"));
    assert!(e.panel.popup.is_none(), "the question is asked once");
}

#[test]
fn config_form_validates_and_saves_69() {
    let mut e = setup();
    let ok = id(&e.root, "ok");
    e.panel.cursor = Some(ok.clone());
    assert!(key(&mut e, KeyEvent::char('e')));
    let Some(Popup::Form(f)) = &mut e.panel.popup else { panic!("form expected") };
    assert!(!f.profiles, "node scripts have no profile field");
    assert!(f.env.starts_with("PATH="));
    let path_env = f.env.clone();
    f.env = "not valid".into();
    let out = e.panel.popup.as_mut().unwrap().key(&KeyEvent::plain(Key::Enter));
    assert_eq!(out, Outcome::Keep);
    let Some(Popup::Form(f)) = &mut e.panel.popup else { panic!() };
    assert!(f.error.as_deref().unwrap().starts_with("Environment must look like"));
    f.env = format!("{path_env}; FOO=bar");
    f.args = "one two".into();
    let out = e.panel.popup.as_mut().unwrap().key(&KeyEvent::plain(Key::Enter));
    assert!(matches!(out, Outcome::Save(ref i, _) if *i == ok));
    let mut cx = e.h.cx("run");
    e.panel.apply(out, &mut cx);
    assert!(e.panel.popup.is_none());
    let saved = fs::read_to_string(e.tmp.0.join("data/run_configs.json")).unwrap();
    assert!(saved.contains("\"args\":\"one two\""), "{saved}");
    e.panel.runner.start(&ok, Mode::Run);
    assert!(until(&mut e, |p, _| status(p, &ok) == Status::Done));
    let text = log_text(&e.panel, &ok);
    assert!(text.contains("args:one two") && text.contains("env:bar"), "{text}");
}

#[test]
fn quit_interrupts_processes_then_quits_80() {
    let mut e = setup();
    let trap = id(&e.root, "trap");
    let stubborn = id(&e.root, "stubborn");
    e.panel.runner.start(&trap, Mode::Run);
    e.panel.runner.start(&stubborn, Mode::Run);
    assert!(until(&mut e, |p, _| log_text(p, &trap).contains("ready") && log_text(p, &stubborn).contains("ready")));
    assert!(!e.panel.can_quit(&mut e.h.cx("run")), "running processes delay the quit");
    assert!(until(&mut e, |p, _| status(p, &trap) == Status::Stopped));
    assert!(log_text(&e.panel, &trap).contains("bye"), "Ctrl+C reached the trap handler");
    assert!(!e.h.requests.contains(&Request::Quit), "stubborn one ignores SIGINT");
    let sd = e.panel.shutdown.as_mut().unwrap();
    assert_eq!(sd.key(&KeyEvent::char('K'), &mut e.panel.runner), ShutdownKey::Keep);
    assert!(until(&mut e, |_, h| h.requests.contains(&Request::Quit)));
    assert!(e.panel.shutdown.is_none());
    assert!(e.panel.can_quit(&mut e.h.cx("run")), "nothing left to stop");
}

#[test]
fn quit_can_be_cancelled_80() {
    let mut e = setup();
    let stubborn = id(&e.root, "stubborn");
    e.panel.runner.start(&stubborn, Mode::Run);
    assert!(until(&mut e, |p, _| log_text(p, &stubborn).contains("ready")));
    assert!(!e.panel.can_quit(&mut e.h.cx("run")));
    let sd = e.panel.shutdown.as_mut().unwrap();
    assert_eq!(sd.key(&KeyEvent::char('q'), &mut e.panel.runner), ShutdownKey::Cancel);
    let mut cx = e.h.cx("run");
    e.panel.shutdown_step(true, &mut cx);
    assert!(e.panel.shutdown.is_none());
    assert!(e.h.requests.contains(&Request::Notify { text: "Quit cancelled".into(), error: false }));
    e.panel.runner.force_kill(&stubborn);
    assert!(until(&mut e, |p, _| status(p, &stubborn) == Status::Stopped));
}

#[test]
fn cmd_f8_and_gutter_message_toggle_breakpoints_92_95() {
    let mut e = setup();
    let file = e.root.join("app.js");
    fs::write(&file, "a\nb\n").unwrap();
    let mut tab = EditorTab::from_text("a\nb\n", None, false);
    tab.path = Some(file.clone());
    let mut cx = Cx {
        ctx: &e.h.ctx,
        palette: &e.h.palette,
        roots: &e.h.roots,
        edit_mode: EditMode::Vim,
        focused: false,
        active: Some(&tab),
        owner: "run",
        requests: &mut e.h.requests,
    };
    let cmd_f8 = KeyEvent::new(Key::F(8), Mods { cmd: true, ..Mods::NONE });
    assert!(e.panel.event(&Event::Shortcut(cmd_f8), &mut cx));
    let sent = |reqs: &[Request]| {
        reqs.iter()
            .filter_map(|r| match r {
                Request::Send { topic, data } if topic == "run.breakpoints" => Some(data.clone()),
                _ => None,
            })
            .next_back()
    };
    assert_eq!(sent(&e.h.requests), Some(format!("{}:1", file.display())));
    let saved = fs::read_to_string(e.tmp.0.join("data/breakpoints.json")).unwrap();
    assert!(saved.contains("\"line\": 1"), "{saved}");
    let data = format!("{}:2", file.display());
    let mut cx = e.h.cx("run");
    assert!(e.panel.event(&Event::Message { topic: "run.toggle_breakpoint", data: &data }, &mut cx));
    assert_eq!(sent(&e.h.requests), Some(format!("{}:1,2", file.display())));
    let mut cx = e.h.cx("run");
    e.panel.event(&Event::Message { topic: "run.toggle_breakpoint", data: &data }, &mut cx);
    assert_eq!(sent(&e.h.requests), Some(format!("{}:1", file.display())), "second toggle removes");
    let mut cx = e.h.cx("run");
    assert!(!e.panel.event(&Event::Shortcut(KeyEvent::plain(Key::F(8))), &mut cx), "F8 without a session is free");
    let data = e.tmp.0.join("data");
    let runner = Runner::with(Vec::new(), ConfigStore::open(data.join("run_configs.json"), &[]), data.clone());
    let mut again = RunPanel::with(runner, Some(data.join("breakpoints.json")));
    e.h.requests.clear();
    frame(&mut again, &mut e.h);
    assert_eq!(sent(&e.h.requests), Some(format!("{}:1", file.display())), "saved breakpoints are announced on load");
}

#[test]
fn opened_run_without_session_hides_variables_94() {
    let mut e = setup();
    let mut cx = e.h.cx("run");
    assert!(e.panel.event(&Event::Leader("du"), &mut cx));
    assert_eq!(e.h.requests.last(), Some(&Request::Layout(LayoutOp::ShowVariables)));
    let mut cx = e.h.cx("run");
    e.panel.event(&Event::Layout(Effect::Opened(Area::Run)), &mut cx);
    assert_eq!(e.h.requests.last(), Some(&Request::Layout(LayoutOp::HideVariables)), "no debug session left");
    assert!(!e.panel.vars_shown);
}

#[test]
fn space_j_picks_item_then_action_34() {
    let mut e = setup();
    let ok = id(&e.root, "ok");
    let mut cx = e.h.cx("run");
    assert!(e.panel.event(&Event::Leader("j"), &mut cx));
    let Some(Popup::Menu { entries, .. }) = &e.panel.popup else { panic!("picker expected") };
    let n = entries.iter().position(|en| en.then == Then::Pick(ok.clone())).unwrap();
    assert!(entries[n].label.ends_with("› ok"), "{}", entries[n].label);
    for _ in 0..n {
        e.panel.popup.as_mut().unwrap().key(&KeyEvent::plain(Key::Down));
    }
    let out = e.panel.popup.as_mut().unwrap().key(&KeyEvent::plain(Key::Enter));
    let mut cx = e.h.cx("run");
    e.panel.apply(out, &mut cx);
    let Some(Popup::Menu { entries, title, .. }) = &e.panel.popup else { panic!("action menu expected") };
    assert_eq!(title, "ok");
    assert_eq!(entries[0].then, Then::Do(ok.clone(), Action::Run));
    let out = e.panel.popup.as_mut().unwrap().key(&KeyEvent::plain(Key::Enter));
    let mut cx = e.h.cx("run");
    e.panel.apply(out, &mut cx);
    assert!(until(&mut e, |p, _| status(p, &ok) == Status::Done));
}

// ---- sahte DAP adaptörü ----

type Log = Arc<Mutex<Vec<String>>>;

fn read_msg(r: &mut BufReader<TcpStream>) -> Option<String> {
    let mut len = 0;
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let l = line.trim().to_ascii_lowercase();
        if l.is_empty() {
            break;
        }
        if let Some(v) = l.strip_prefix("content-length:") {
            len = v.trim().parse().ok()?;
        }
    }
    let mut buf = vec![0; len];
    r.read_exact(&mut buf).ok()?;
    String::from_utf8(buf).ok()
}

/// `"key":"değer"` alanı (kaba ayrıştırma; sahte adaptör için yeter).
fn str_field(msg: &str, key: &str) -> String {
    let pat = format!("\"{key}\":\"");
    msg.find(&pat).map(|i| msg[i + pat.len()..].split('"').next().unwrap_or("").to_string()).unwrap_or_default()
}

fn num_field(msg: &str, key: &str) -> i64 {
    let pat = format!("\"{key}\":");
    let Some(i) = msg.rfind(&pat) else { return 0 };
    msg[i + pat.len()..].chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0)
}

fn fake_adapter(source: PathBuf) -> (SocketAddr, Log) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let log: Log = Arc::default();
    let shared = log.clone();
    thread::spawn(move || {
        let Ok((stream, _)) = listener.accept() else { return };
        serve(stream, &shared, &source);
    });
    (addr, log)
}

fn serve(stream: TcpStream, log: &Log, source: &Path) {
    let mut w = stream.try_clone().unwrap();
    let mut r = BufReader::new(stream);
    let mut seq = 1000;
    let mut send = |w: &mut TcpStream, body: String| {
        seq += 1;
        let body = format!("{{\"seq\":{seq},{}", &body[1..]);
        let _ = write!(w, "Content-Length: {}\r\n\r\n{body}", body.len());
    };
    let resp = |req: i64, cmd: &str, body: &str| {
        format!(
            "{{\"type\":\"response\",\"request_seq\":{req},\"command\":\"{cmd}\",\"success\":true,\"body\":{body}}}"
        )
    };
    let mut attach_seq = 0;
    while let Some(msg) = read_msg(&mut r) {
        let cmd = str_field(&msg, "command");
        let req = num_field(&msg, "seq");
        let entry = if cmd == "evaluate" { format!("evaluate:{}", str_field(&msg, "expression")) } else { cmd.clone() };
        log.lock().unwrap().push(entry);
        let body = match cmd.as_str() {
            "initialize" => r#"{"supportsConfigurationDoneRequest":true,"supportsSetVariable":true}"#.to_string(),
            "attach" => {
                attach_seq = req;
                send(&mut w, r#"{"type":"event","event":"initialized"}"#.into());
                continue;
            }
            "configurationDone" => {
                send(&mut w, resp(req, &cmd, "{}"));
                send(&mut w, resp(attach_seq, "attach", "{}"));
                send(&mut w, r#"{"type":"event","event":"stopped","body":{"reason":"breakpoint","threadId":1}}"#.into());
                continue;
            }
            "stackTrace" => format!(
                r#"{{"stackFrames":[{{"id":7,"name":"main","source":{{"path":"{}"}},"line":3,"column":1}}]}}"#,
                source.display()
            ),
            "scopes" => r#"{"scopes":[{"name":"Local","variablesReference":10,"expensive":false}]}"#.into(),
            "variables" => match num_field(&msg, "variablesReference") {
                10 => r#"{"variables":[{"name":"count","value":"1","type":"number","variablesReference":0,"evaluateName":"count"},{"name":"list","value":"Array(2)","type":"object","variablesReference":11,"evaluateName":"list"}]}"#.into(),
                11 => r#"{"variables":[{"name":"0","value":"'a'","type":"string","evaluateName":"list[0]"},{"name":"1","value":"'b'","type":"string","evaluateName":"list[1]"}]}"#.into(),
                _ => r#"{"variables":[]}"#.into(),
            },
            "setVariable" => format!(r#"{{"value":"{}","type":"number"}}"#, str_field(&msg, "value")),
            "evaluate" => r#"{"result":"1","variablesReference":0}"#.into(),
            "continue" => {
                send(&mut w, resp(req, &cmd, r#"{"allThreadsContinued":true}"#));
                send(&mut w, r#"{"type":"event","event":"continued","body":{"threadId":1}}"#.into());
                continue;
            }
            "disconnect" => {
                send(&mut w, resp(req, &cmd, "{}"));
                return;
            }
            _ => "{}".into(),
        };
        send(&mut w, resp(req, &cmd, &body));
    }
}

fn value_of(p: &RunPanel, name: &str) -> Option<String> {
    p.vars.rows().into_iter().find(|r| r.name_cell.trim_start().ends_with(name)).map(|r| r.value)
}

#[test]
fn debug_stop_shows_variables_and_edits_them_91_93_97() {
    let mut e = setup();
    let source = e.root.join("app.js");
    let (addr, log) = fake_adapter(source.clone());
    let ok = id(&e.root, "ok");
    e.panel.dbg.attach_addr(&ok, addr, "pwa-node", node_attach_config("ok", 1, addr.port(), &e.root));
    assert!(until(&mut e, |p, _| value_of(p, "count").is_some()));
    assert!(e.h.requests.contains(&Request::Layout(LayoutOp::ShowVariables)), "breakpoint stop shows Variables");
    assert!(e.h.requests.contains(&Request::OpenFile { path: source.clone(), line: Some(2) }), "jumps to the frame");
    assert!(e.panel.attached(&ok));
    e.panel.part = StripPart::Variables;
    e.panel.vars.cursor = e.panel.vars.rows().into_iter().find(|r| r.name_cell.ends_with("count")).map(|r| r.path);
    assert!(key(&mut e, KeyEvent::char('e')));
    let Some(Popup::Input { value, purpose: InputFor::VarSet(path), .. }) = e.panel.popup.clone() else {
        panic!("value input expected")
    };
    assert_eq!(value, "1", "input is prefilled");
    let mut cx = e.h.cx("run");
    e.panel.apply(Outcome::Input(InputFor::VarSet(path), "5".into()), &mut cx);
    assert!(until(&mut e, |p, _| value_of(p, "count").as_deref() == Some("5")));
    assert!(log.lock().unwrap().iter().any(|l| l == "setVariable"));
    render(&mut e);
    let list = e.panel.vars.rows().into_iter().find(|r| r.name_cell.ends_with("list")).unwrap();
    e.panel.vars.cursor = Some(list.path.clone());
    assert!(key(&mut e, KeyEvent::plain(Key::Right)));
    assert!(until(&mut e, |p, _| p.vars.rows().iter().any(|r| r.name_cell.ends_with("[0]"))));
    key(&mut e, KeyEvent::plain(Key::Down));
    assert!(key(&mut e, KeyEvent::char('d')));
    assert!(until(&mut e, |_, _| log.lock().unwrap().iter().any(|l| l == "evaluate:list.splice(0, 1)")));
    let mut cx = e.h.cx("run");
    assert!(e.panel.event(&Event::Shortcut(KeyEvent::plain(Key::F(9))), &mut cx), "F9 continues");
    assert!(until(&mut e, |p, _| p.vars.is_running()));
    assert!(log.lock().unwrap().iter().any(|l| l == "continue"));
    e.panel.dbg.detach(&ok);
    assert!(until(&mut e, |_, h| h.requests.contains(&Request::Layout(LayoutOp::HideVariables))));
    assert!(e.panel.vars.rows().is_empty(), "last session ended");
}

/// Verilen bölgeleri verilen girdiyle başsız çizer; çıktı (şekiller, platform komutları) döner.
fn draw(e: &mut Env, regions: &[(Region, egui::Rect)], events: Vec<egui::Event>) -> egui::FullOutput {
    let ctx = e.h.ctx.clone();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 800.0))),
        events,
        ..Default::default()
    };
    let (panel, h) = (&mut e.panel, &mut e.h);
    let mut out = ctx.run_ui(input, |ui| {
        for &(region, rect) in regions {
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            let mut cx = h.cx("run");
            panel.ui(region, &mut child, &mut cx);
        }
    });
    out.textures_delta.clear();
    out
}

/// Çıktıdaki metin şekilleri: (metin, ekran dikdörtgeni, kırpma).
fn texts(out: &egui::FullOutput) -> Vec<(String, egui::Rect, egui::Rect)> {
    out.shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Text(t) => {
                let text = format!("{}{}", t.galley.text(), if t.galley.elided { "…" } else { "" });
                Some((text, t.galley.rect.translate(t.pos.to_vec2()), c.clip_rect))
            }
            _ => None,
        })
        .collect()
}

fn rescan(e: &mut Env, scripts: &str) {
    fs::write(e.root.join("package.json"), format!(r#"{{"scripts":{{{scripts}}}}}"#)).unwrap();
    let root = e.root.clone();
    e.panel.runner.scan(std::slice::from_ref(&root));
}

#[test]
fn row_text_never_overlaps_buttons_and_hint_is_visible_16_34() {
    let mut e = setup();
    rescan(
        &mut e,
        r#""dev-server-with-a-really-long-script-name":"vite --host 0.0.0.0 --port 3010 --strictPort --mode development","ok":"x""#,
    );
    let icons_ = [icons::PLAY, icons::STOP, icons::BUG, icons::RERUN, icons::LOG];
    for w in [220.0, 280.0, 330.0, 420.0, 900.0] {
        let region = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w, 300.0));
        let out = draw(&mut e, &[(Region::Strip(StripPart::RunTree), region)], vec![]);
        let all = texts(&out);
        let visible: Vec<_> = all.iter().filter(|(_, r, clip)| clip.intersects(*r)).collect();
        let buttons: Vec<_> = visible
            .iter()
            .filter(|(t, r, _)| icons_.contains(&t.as_str()) && r.center().x > w - 8.0 - 4.0 * BUTTON_W - 2.0)
            .collect();
        assert_eq!(!buttons.is_empty(), w >= BUTTONS_MIN_W, "buttons only when wide enough (w = {w})");
        let edge = buttons.iter().map(|(_, r, _)| r.left()).fold(w - 8.0, f32::min);
        let rows_bottom = 300.0 - HINT_H;
        for (t, r, _) in &visible {
            if buttons.iter().any(|(_, b, _)| b == r) || r.top() >= rows_bottom {
                continue;
            }
            assert!(r.right() <= edge + 0.5, "{t:?} ends at {} past {edge} (w = {w})", r.right());
        }
        assert!(w > 420.0 || visible.iter().any(|(t, ..)| t.contains('…')), "long label is truncated (w = {w})");
        let hint = visible.iter().find(|(t, ..)| t.starts_with("⏎ run/stop")).expect("hint line drawn");
        assert!(hint.1.top() >= rows_bottom && hint.1.bottom() <= 300.0 && hint.1.right() <= w, "hint inside footer");
        for (t, r, clip) in &visible {
            if !t.starts_with("⏎ run/stop") && r.top() >= HEAD_H && r.top() < rows_bottom {
                assert!(clip.bottom() <= rows_bottom + 0.5, "{t:?} row may not cover the hint line");
            }
        }
    }
    let region = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 300.0));
    let out = draw(&mut e, &[(Region::Strip(StripPart::RunTree), region)], vec![]);
    assert!(texts(&out).iter().any(|(t, ..)| t == TREE_HINT), "hint is complete at full width");
}

#[test]
fn arrow_keys_keep_the_selected_row_in_view_16() {
    let mut e = setup();
    let scripts: Vec<String> = (0..40).map(|i| format!(r#""s{i:02}":"x""#)).collect();
    rescan(&mut e, &scripts.join(","));
    let region = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 240.0));
    let tree = [(Region::Strip(StripPart::RunTree), region)];
    draw(&mut e, &tree, vec![]);
    let stride = ROW_H + egui::Style::default().spacing.item_spacing.y;
    let check = |e: &RunPanel| {
        let rows = e.rows();
        let top = e.cursor_index(&rows) as f32 * stride;
        let s = &e.tree_scroll;
        assert!(s.height > 0.0 && s.height < rows.len() as f32 * stride, "list is taller than the viewport");
        assert!(
            top >= s.offset - 0.5 && top + ROW_H <= s.offset + s.height + 0.5,
            "row {top} outside {} + {}",
            s.offset,
            s.height
        );
    };
    for _ in 0..40 {
        assert!(key(&mut e, KeyEvent::plain(Key::Down)));
        draw(&mut e, &tree, vec![]);
        check(&e.panel);
    }
    assert!(e.panel.tree_scroll.offset > 0.0, "scrolled down");
    for _ in 0..40 {
        assert!(key(&mut e, KeyEvent::plain(Key::Up)));
        draw(&mut e, &tree, vec![]);
        check(&e.panel);
    }
    assert!(e.panel.tree_scroll.offset < 0.5, "back at the top");
}

#[test]
fn log_links_are_found_in_colored_and_wrapped_output_101() {
    let t = less_ai::term::Terminal::feed_only(less_ai::term::TermSize::new(40, 6));
    t.feed(b"\x1b[32m  \xe2\x9e\x9c  Local:   \x1b[1;36mhttp://localhost:\x1b[22m3010/\x1b[0m\r\n");
    t.feed(b"see https://example.com/a-very-long/path/that/wraps.\r\nno link here http:/ x\r\n");
    let found = log::links(&t.snapshot());
    let urls: Vec<_> = found.iter().map(|l| l.url.as_str()).collect();
    assert_eq!(urls, ["http://localhost:3010/", "https://example.com/a-very-long/path/that/wraps"]);
    assert_eq!(found[0].spans, [(0, 14, 36)]);
    assert_eq!(found[1].spans, [(1, 4, 40), (2, 0, 11)], "wrapped URL continues on the next row");
    assert!(found[1].contains(2, 3) && !found[1].contains(2, 11));
}

#[test]
fn clicking_a_log_url_opens_it_in_the_browser_101() {
    let mut e = setup();
    let region = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(700.0, 300.0));
    let log_r = [(Region::Strip(StripPart::RunLog), region)];
    draw(&mut e, &log_r, vec![]);
    let item = id(&e.root, "ok");
    e.panel.logs.feed(&item, b"\x1b[36m  Local:   http://localhost:3010/\x1b[0m\r\n");
    e.panel.log = Some(item);
    let cw = e.h.ctx.fonts_mut(|f| f.glyph_width(&egui::FontId::monospace(log::LOG_FONT), 'M'));
    let ch = (log::LOG_FONT * 1.4).round();
    let at = egui::pos2(6.0 + 20.5 * cw, HEAD_H + 4.0 + 0.5 * ch);
    draw(&mut e, &log_r, vec![egui::Event::PointerMoved(at)]);
    let out = draw(&mut e, &log_r, vec![]);
    assert_eq!(out.platform_output.cursor_icon, egui::CursorIcon::PointingHand, "pointer cursor over the URL");
    let button = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    draw(&mut e, &log_r, vec![button(true)]);
    let out = draw(&mut e, &log_r, vec![button(false)]);
    let opened: Vec<_> = out
        .platform_output
        .commands
        .iter()
        .filter_map(|c| match c {
            egui::OutputCommand::OpenUrl(u) => Some(u.url.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(opened, ["http://localhost:3010/"]);
    let away =
        draw(&mut e, &log_r, vec![egui::Event::PointerMoved(egui::pos2(6.0 + 2.5 * cw, HEAD_H + 4.0 + 0.5 * ch))]);
    assert_ne!(away.platform_output.cursor_icon, egui::CursorIcon::PointingHand);
}
