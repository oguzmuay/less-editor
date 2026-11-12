use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use less_ai::accounts::Accounts;
use less_ai::agents::{AgentKind, AiState};
use less_ai::sessions::{Snapshot, Status};
use less_ai::summary::{self, COLORS, frame};
use less_ai::tabs::{Group, Item, NewTab, TabMeta, TabsModel, TermTabs, shell_title};
use less_ai::term::SpawnOptions;

fn tab(cwd: &str) -> NewTab {
    NewTab { cwd: PathBuf::from(cwd), ..Default::default() }
}

#[test]
fn items_current_active_and_summary_27() {
    let mut m = TabsModel::new();
    let a = m.add(Group::Ai, tab("/p"));
    assert_eq!(m.items(Group::Ai), [Item::Tab(a)]);
    let b = m.add(Group::Ai, tab("/p"));
    assert_eq!(m.items(Group::Ai), [Item::Summary, Item::Tab(a), Item::Tab(b)]);
    assert_eq!(m.current(Group::Ai), Some(Item::Tab(b)));
    m.select(Group::Ai, Item::Summary);
    assert_eq!(m.current(Group::Ai), Some(Item::Summary));
    assert_eq!(m.active(Group::Ai), Some(b), "summary does not change the active session");
    assert_eq!(m.cycle(Group::Ai, 1), Some(Item::Tab(a)));
    assert_eq!(m.cycle(Group::Ai, -1), Some(Item::Summary));
    assert_eq!(m.cycle(Group::Ai, -1), Some(Item::Tab(b)), "wraps around");
    m.remove(a);
    assert_eq!(m.items(Group::Ai), [Item::Tab(b)]);
    assert_eq!(m.current(Group::Ai), Some(Item::Tab(b)));
}

#[test]
fn remove_picks_right_then_left_neighbor_27() {
    let mut m = TabsModel::new();
    let a = m.add(Group::Shell, tab("/p"));
    let b = m.add(Group::Shell, tab("/p"));
    let c = m.add(Group::Shell, tab("/p"));
    m.select(Group::Shell, Item::Tab(b));
    m.remove(b);
    assert_eq!(m.active(Group::Shell), Some(c));
    m.remove(c);
    assert_eq!(m.active(Group::Shell), Some(a));
    m.remove(a);
    assert_eq!(m.current(Group::Shell), None);
    let d = m.add(Group::Shell, tab("/p"));
    assert_eq!(m.tab(d).unwrap().n, 4, "sequence numbers keep growing");
}

#[test]
fn labels_rename_and_icons_27_64() {
    let mut m = TabsModel::new();
    let s = m.add(Group::Shell, tab("/p"));
    assert_eq!(m.tab(s).unwrap().label(), "Terminal 1");
    let c = m.add(Group::Ai, NewTab { ai_name: Some("codex-2".into()), agent: Some(AgentKind::Codex), ..tab("/p") });
    assert_eq!(m.tab(c).unwrap().label(), "Codex 2");
    let cl = m.add(Group::Ai, tab("/p"));
    assert_eq!(m.tab(cl).unwrap().agent, Some(AgentKind::Claude));
    assert_eq!(m.tab(cl).unwrap().label(), "Claude 2");
    m.tab_mut(cl).unwrap().auto = Some("Session title".into());
    assert_eq!(m.tab(cl).unwrap().label(), "Session title");
    m.rename(cl, "  mine ");
    assert_eq!(m.tab(cl).unwrap().label(), "mine");
    m.rename(cl, "");
    assert_eq!(m.tab(cl).unwrap().label(), "Session title");
    assert_eq!(m.tab(cl).unwrap().status_icon(), "◌");
    m.tab_mut(cl).unwrap().meta = Some(TabMeta { status: Some(Status::Waiting), snapshot: None });
    assert_eq!(m.tab(cl).unwrap().status_icon(), "◐");
}

#[test]
fn shell_auto_name_priority_27() {
    let cwd = Path::new("/Users/me/proj");
    assert_eq!(shell_title(Some("me@host: ~/code/app"), "/bin/zsh", Some("vim"), cwd), "app");
    assert_eq!(shell_title(Some("me@host:~"), "/bin/zsh", None, cwd), "~");
    assert_eq!(shell_title(Some("npm run dev"), "/bin/zsh", None, cwd), "npm run dev");
    assert_eq!(shell_title(Some("zsh"), "/bin/zsh", Some("node"), cwd), "node");
    assert_eq!(shell_title(None, "/bin/zsh", None, cwd), "proj");
}

#[test]
fn mascot_frames_60() {
    let busy: Vec<[String; 4]> = (0..3).map(|t| frame(Some(Status::Busy), t, AgentKind::Claude)).collect();
    assert_eq!(busy[0][0], "        ┏━┻━┓ ");
    assert!(busy[2][1].contains('◆'));
    assert!(busy.iter().all(|f| f[3].ends_with("  ▟█▙")));
    assert_eq!(frame(Some(Status::Idle), 1, AgentKind::Claude)[0], "     z Z      ");
    assert_eq!(frame(Some(Status::Idle), 0, AgentKind::Claude)[1], " ▐█████▌      ");
    assert_eq!(frame(Some(Status::Waiting), 0, AgentKind::Codex)[0], "    !         ");
    assert_eq!(frame(Some(Status::Waiting), 1, AgentKind::Codex)[0].trim(), "");
    assert!(frame(None, 0, AgentKind::Ollama)[2].contains("█• •█"));
    assert_eq!(COLORS[0], (0xD9, 0x77, 0x57));
}

#[test]
fn summary_view_blocks_60() {
    let mut m = TabsModel::new();
    let a = m.add(Group::Ai, tab("/p"));
    let b = m.add(Group::Ai, NewTab { ai_name: Some("gemini-1".into()), agent: Some(AgentKind::Gemini), ..tab("/p") });
    let snap = Snapshot {
        status: Some(Status::Busy),
        total: 1500,
        subagents: 500,
        subagent_count: 1,
        last_tool: Some("Read: a.rs".into()),
        mtime: Some(90),
        ..Default::default()
    };
    m.tab_mut(a).unwrap().meta = Some(TabMeta { status: Some(Status::Busy), snapshot: Some(snap) });
    m.tab_mut(b).unwrap().meta = Some(TabMeta { status: Some(Status::Idle), snapshot: None });
    let v = summary::build(m.tabs(Group::Ai), false, |n| n == "gemini-1", 100);
    assert_eq!(v.header, "AI sessions: 2 open · 2.0K Claude tokens");
    assert_eq!(v.blocks.len(), 2);
    assert_eq!(v.blocks[0].head, "1. ● Claude 1");
    assert_eq!(v.blocks[0].status_text, "  [working]");
    assert!(v.blocks[0].lines.iter().any(|(l, _)| l == "   Now: Read: a.rs"));
    assert!(v.blocks[0].lines.iter().any(|(l, _)| l.starts_with("   Last activity: 10s ago")));
    assert_eq!(v.blocks[1].head, "2. ○ Gemini 1  ✉ new mail");
    assert_eq!(v.blocks[1].lines[0].0, "   Gemini · mailbox name gemini-1");
    assert_eq!(v.blocks[1].mascot_color, COLORS[1]);
    assert_eq!(v.blocks[1].mascot_kind, AgentKind::Gemini);
}

fn manager(tmp: &Path) -> TermTabs {
    let acc = Accounts::load_from(&tmp.join("acc.json"), None, tmp);
    TermTabs::with_parts(acc, AiState::load_from(&tmp.join("ai.json"), None))
}

#[test]
fn manager_removes_exited_tabs_on_poll_27() {
    let tmp = tempfile::tempdir().unwrap();
    let mut tt = manager(tmp.path());
    let opts = SpawnOptions { argv: vec!["sh".into(), "-c".into(), "printf hi".into()], ..Default::default() };
    let id = tt.spawn(Group::Shell, opts, tab(tmp.path().to_str().unwrap())).unwrap();
    assert!(tt.model.tab(id).unwrap().pid.is_some());
    let start = Instant::now();
    let mut res = tt.poll(true, true);
    while res.removed.is_empty() && start.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(30));
        res = tt.poll(true, true);
    }
    assert_eq!(res.removed.len(), 1);
    assert_eq!(res.removed[0].0, id);
    assert!(tt.model.tabs(Group::Shell).is_empty());
    assert!(tt.terminal(id).is_none());
}

#[test]
fn manager_close_kills_process_27() {
    let tmp = tempfile::tempdir().unwrap();
    let mut tt = manager(tmp.path());
    let opts = SpawnOptions { argv: vec!["sh".into(), "-c".into(), "sleep 30".into()], ..Default::default() };
    let id = tt.spawn(Group::Ai, opts, tab(tmp.path().to_str().unwrap())).unwrap();
    assert!(tt.needs_confirm(id));
    tt.close(id);
    let start = Instant::now();
    while tt.model.tab(id).is_some() && start.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(30));
        tt.poll(false, false);
    }
    assert!(tt.model.tab(id).is_none());
}
