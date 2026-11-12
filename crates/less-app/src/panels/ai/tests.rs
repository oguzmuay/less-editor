//! AI / Terminal paneli testleri: `Harness` + `/bin/sh` pty'leri (gerçek AI CLI'ları başlatılmaz).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui;
use less_ai::agents::AgentKind;
use less_ai::tabs::{Group, Item};
use less_ai::term::{CellStyle, GridCell, TermColor, TermSize};
use less_input::{Key, KeyEvent, Mods};

use super::view::{self, cell_at, grid_size, indexed, spans};
use super::*;
use crate::panels::Request;
use crate::panels::testing::Harness;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("less-b3-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn setup(name: &str) -> (AiPanel, Harness, PathBuf) {
    let dir = temp(name);
    (AiPanel::with_launch(Launch::Fake(vec!["/bin/sh".into()]), false), Harness::new(vec![dir.clone()]), dir)
}

fn ev(p: &mut AiPanel, h: &mut Harness, e: Event) -> bool {
    let mut cx = h.cx("ai");
    p.event(&e, &mut cx)
}

fn tick(p: &mut AiPanel, h: &mut Harness) {
    let mut cx = h.cx("ai");
    p.tick(&mut cx);
}

fn typ(p: &mut AiPanel, h: &mut Harness, s: &str) {
    for c in s.chars() {
        let k = if c == '\r' { KeyEvent::plain(Key::Enter) } else { KeyEvent::char(c) };
        assert!(ev(p, h, Event::Key(k)));
    }
}

fn cmd(c: char) -> KeyEvent {
    KeyEvent::new(Key::Char(c), Mods::CMD)
}

fn tabs(p: &AiPanel, g: Group) -> Vec<TabId> {
    p.tabs.as_ref().map(|t| t.model.tabs(g).iter().map(|t| t.id).collect()).unwrap_or_default()
}

fn screen(p: &AiPanel, id: TabId) -> String {
    p.tabs.as_ref().unwrap().terminal(id).unwrap().snapshot().text()
}

fn wait(mut f: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn term_key_maps_input_keys_27() {
    assert_eq!(term_key(Key::Esc), aterm::Key::Escape);
    assert_eq!(term_key(Key::Char('ş')), aterm::Key::Char('ş'));
    assert_eq!(term_key(Key::F(5)), aterm::Key::F(5));
    assert_eq!(term_key(Key::PageDown), aterm::Key::PageDown);
}

#[test]
fn opened_terminal_spawns_shell_and_keys_reach_it_20_27() {
    let (mut p, mut h, dir) = setup("shell");
    ev(&mut p, &mut h, Event::Layout(Effect::Opened(Area::Terminal)));
    let ids = tabs(&p, Group::Shell);
    assert_eq!(ids.len(), 1);
    assert!(tabs(&p, Group::Ai).is_empty(), "Terminal does not start an AI");
    typ(&mut p, &mut h, "echo $((40+2))\r");
    assert!(wait(|| screen(&p, ids[0]).lines().any(|l| l.trim() == "42")), "{}", screen(&p, ids[0]));
    ev(&mut p, &mut h, Event::Layout(Effect::Opened(Area::Terminal)));
    assert_eq!(tabs(&p, Group::Shell).len(), 1, "reopening shows the existing tab");
    drop(p);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn empty_ai_panel_starts_default_kind_and_cmd_t_picks_kind_60_64() {
    let (mut p, mut h, dir) = setup("kinds");
    ev(&mut p, &mut h, Event::Layout(Effect::Opened(Area::Ai)));
    let first = tabs(&p, Group::Ai);
    assert_eq!(first.len(), 1);
    let t = p.tabs.as_ref().unwrap().model.tab(first[0]).unwrap().clone();
    assert_eq!((t.agent, t.ai_name.as_deref()), (Some(AgentKind::Claude), Some("claude-1")));

    assert!(ev(&mut p, &mut h, Event::Key(cmd('t'))));
    assert_eq!(p.prompt, Some((Group::Ai, Prompt::Kinds { sel: 0 })));
    ev(&mut p, &mut h, Event::Key(KeyEvent::plain(Key::Down)));
    ev(&mut p, &mut h, Event::Key(KeyEvent::plain(Key::Enter)));
    assert_eq!(p.prompt, None);
    let ids = tabs(&p, Group::Ai);
    assert_eq!(ids.len(), 2);
    let t = p.tabs.as_ref().unwrap().model.tab(ids[1]).unwrap().clone();
    assert_eq!(
        (t.agent, t.ai_name.as_deref(), t.label().as_str()),
        (Some(AgentKind::Codex), Some("codex-1"), "Codex 1")
    );
    let items = p.tabs.as_ref().unwrap().model.items(Group::Ai);
    assert_eq!(items[0], Item::Summary, "summary tab first once there are two sessions");
    drop(p);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn ollama_asks_for_model_64() {
    let (mut p, mut h, dir) = setup("ollama");
    ev(&mut p, &mut h, Event::Layout(Effect::Opened(Area::Ai)));
    ev(&mut p, &mut h, Event::Key(cmd('t')));
    ev(&mut p, &mut h, Event::Key(KeyEvent::char('4')));
    assert_eq!(
        p.prompt,
        Some((Group::Ai, Prompt::Text { purpose: Purpose::Ollama, value: "llama3.2".into(), focused: false }))
    );
    ev(&mut p, &mut h, Event::Key(KeyEvent::plain(Key::Enter)));
    let ids = tabs(&p, Group::Ai);
    assert_eq!(p.tabs.as_ref().unwrap().model.tab(ids[1]).unwrap().agent, Some(AgentKind::Ollama));
    drop(p);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn cmd_shift_arrows_cycle_tabs_in_focused_panel_27() {
    let (mut p, mut h, dir) = setup("cycle");
    ev(&mut p, &mut h, Event::Layout(Effect::Opened(Area::Terminal)));
    ev(&mut p, &mut h, Event::Key(cmd('t')));
    let ids = tabs(&p, Group::Shell);
    assert_eq!(ids.len(), 2);
    assert_eq!(p.current(Group::Shell), Some(Item::Tab(ids[1])));
    let right = KeyEvent::new(Key::Right, Mods { shift: true, cmd: true, ..Mods::NONE });
    assert!(ev(&mut p, &mut h, Event::Key(right)));
    assert_eq!(p.current(Group::Shell), Some(Item::Tab(ids[0])), "wraps around");
    let left = KeyEvent::new(Key::Left, Mods { alt: true, cmd: true, ..Mods::NONE });
    assert!(ev(&mut p, &mut h, Event::Key(left)));
    assert_eq!(p.current(Group::Shell), Some(Item::Tab(ids[1])));
    assert!(!ev(&mut p, &mut h, Event::Key(cmd('1'))), "app shortcuts go back to the host");
    drop(p);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn closing_ai_session_asks_and_last_exit_closes_panel_27() {
    let (mut p, mut h, dir) = setup("close");
    ev(&mut p, &mut h, Event::Layout(Effect::Opened(Area::Ai)));
    let id = tabs(&p, Group::Ai)[0];
    ev(&mut p, &mut h, Event::Key(cmd('w')));
    assert_eq!(p.prompt, Some((Group::Ai, Prompt::Confirm { tab: id, yes: false })));
    ev(&mut p, &mut h, Event::Key(KeyEvent::char('n')));
    assert_eq!(p.prompt, None);
    assert!(p.tabs.as_ref().unwrap().terminal(id).unwrap().is_alive());
    ev(&mut p, &mut h, Event::Key(cmd('w')));
    ev(&mut p, &mut h, Event::Key(KeyEvent::char('y')));
    assert!(wait(|| {
        tick(&mut p, &mut h);
        tabs(&p, Group::Ai).is_empty()
    }));
    assert!(h.requests.contains(&Request::Layout(LayoutOp::Close(Area::Ai))));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn shell_exit_removes_tab_without_confirmation_27() {
    let (mut p, mut h, dir) = setup("exit");
    ev(&mut p, &mut h, Event::Layout(Effect::Opened(Area::Terminal)));
    typ(&mut p, &mut h, "exit\r");
    assert!(wait(|| {
        tick(&mut p, &mut h);
        tabs(&p, Group::Shell).is_empty()
    }));
    assert!(h.requests.contains(&Request::Layout(LayoutOp::Close(Area::Terminal))));
    assert_eq!(p.prompt, None);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn ex_r_renames_tab_27() {
    let (mut p, mut h, dir) = setup("rename");
    ev(&mut p, &mut h, Event::Layout(Effect::Opened(Area::Terminal)));
    assert!(ev(&mut p, &mut h, Event::Ex("R build logs")));
    let id = tabs(&p, Group::Shell)[0];
    assert_eq!(p.tabs.as_ref().unwrap().model.tab(id).unwrap().label(), "build logs");
    assert!(h.requests.contains(&Request::Notify { text: "Tab name: build logs".into(), error: false }));
    assert!(ev(&mut p, &mut h, Event::Ex("R")));
    assert!(matches!(p.prompt, Some((Group::Shell, Prompt::Text { purpose: Purpose::Rename(_), .. }))));
    assert!(!ev(&mut p, &mut h, Event::Ex("GitBlame")));
    drop(p);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn first_tick_opens_ai_then_returns_to_code_81() {
    let dir = temp("autostart");
    let mut p = AiPanel::with_launch(Launch::Fake(vec!["/bin/sh".into()]), true);
    let mut h = Harness::new(vec![dir.clone()]);
    tick(&mut p, &mut h);
    tick(&mut p, &mut h);
    assert_eq!(
        h.requests,
        vec![Request::Layout(LayoutOp::Open(Area::Ai)), Request::Layout(LayoutOp::Focus(Area::Code))]
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn database_hides_ai_without_killing_it_45() {
    let (mut p, mut h, dir) = setup("hidden");
    ev(&mut p, &mut h, Event::Layout(Effect::Opened(Area::Ai)));
    ev(&mut p, &mut h, Event::Layout(Effect::Hidden(Area::Ai)));
    assert_eq!(p.open, [false, false]);
    let id = tabs(&p, Group::Ai)[0];
    assert!(p.tabs.as_ref().unwrap().terminal(id).unwrap().is_alive());
    ev(&mut p, &mut h, Event::Layout(Effect::Restored(Area::Ai)));
    assert_eq!(p.open, [true, false]);
    assert_eq!(tabs(&p, Group::Ai), vec![id], "restoring does not start a new session");
    drop(p);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn send_to_ai_pastes_into_active_session_13() {
    let (mut p, mut h, dir) = setup("send");
    ev(&mut p, &mut h, Event::Message { topic: "ai.send", data: "x" });
    assert!(h.requests.iter().any(|r| matches!(r, Request::Notify { error: true, .. })));
    ev(&mut p, &mut h, Event::Layout(Effect::Opened(Area::Ai)));
    let id = tabs(&p, Group::Ai)[0];
    ev(&mut p, &mut h, Event::Message { topic: "ai.send", data: "echo $((6*7))\n" });
    assert!(wait(|| screen(&p, id).lines().any(|l| l.trim() == "42")), "{}", screen(&p, id));
    assert!(h.requests.contains(&Request::Layout(LayoutOp::Open(Area::Ai))));
    drop(p);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn claude_account_opens_account_picker_63() {
    let (mut p, mut h, dir) = setup("accounts");
    assert!(ev(&mut p, &mut h, Event::Ex("Claude account")));
    assert_eq!(p.prompt, Some((Group::Ai, Prompt::Accounts { sel: 0 })));
    ev(&mut p, &mut h, Event::Layout(Effect::Opened(Area::Ai)));
    assert!(tabs(&p, Group::Ai).is_empty(), "the picker does not start a session");
    ev(&mut p, &mut h, Event::Key(KeyEvent::plain(Key::Down)));
    ev(&mut p, &mut h, Event::Key(KeyEvent::plain(Key::Enter)));
    assert!(matches!(p.prompt, Some((_, Prompt::Text { purpose: Purpose::AccountName, .. }))));
    ev(&mut p, &mut h, Event::Key(KeyEvent::plain(Key::Esc)));
    assert_eq!(p.prompt, None);
    let _ = std::fs::remove_dir_all(dir);
}

fn cell(ch: char, fg: TermColor) -> GridCell {
    GridCell { ch, fg, ..GridCell::default() }
}

#[test]
fn row_spans_split_on_style_and_non_ascii_27() {
    let pal = less_theme::TOKYONIGHT_NIGHT;
    let mut row =
        vec![cell('a', TermColor::DefaultFg), cell('b', TermColor::DefaultFg), cell('│', TermColor::DefaultFg)];
    row.push(cell('c', TermColor::Indexed(1)));
    row.push(GridCell { ch: ' ', bg: TermColor::Indexed(4), ..GridCell::default() });
    row.push(GridCell { ch: 'x', style: CellStyle { inverse: true, ..CellStyle::default() }, ..GridCell::default() });
    let (bgs, texts) = spans(&row, &pal);
    let t: Vec<(usize, &str)> = texts.iter().map(|t| (t.col, t.text.as_str())).collect();
    assert_eq!(t, vec![(0, "ab"), (2, "│"), (3, "c"), (5, "x")]);
    assert_eq!(bgs.len(), 2);
    assert_eq!((bgs[0].from, bgs[0].to, bgs[0].color), (4, 5, indexed(4, true)));
    assert_eq!(bgs[1].color, crate::widgets::color(pal.fg), "inverse swaps fg and bg");
}

#[test]
fn xterm_256_color_table_27() {
    assert_eq!(indexed(16, true), egui::Color32::from_rgb(0, 0, 0));
    assert_eq!(indexed(231, true), egui::Color32::from_rgb(255, 255, 255));
    assert_eq!(indexed(196, false), egui::Color32::from_rgb(255, 0, 0));
    assert_eq!(indexed(232, true), egui::Color32::from_rgb(8, 8, 8));
    assert_ne!(indexed(1, true), indexed(1, false));
}

#[test]
fn mouse_cell_and_grid_size_27() {
    let size = TermSize::new(10, 5);
    let o = egui::pos2(100.0, 50.0);
    assert_eq!(cell_at(o, (8.0, 16.0), egui::pos2(117.0, 83.0), size), (2, 2));
    assert_eq!(cell_at(o, (8.0, 16.0), egui::pos2(10.0, 900.0), size), (0, 4));
    let r = egui::Rect::from_min_size(o, egui::vec2(85.0, 50.0));
    assert_eq!(grid_size(r, (8.0, 16.0)), TermSize::new(10, 3));
    assert_eq!(truncate("abcdefgh", 5), "abcd…");
    assert_eq!(truncate("abc", 5), "abc");
}

fn draw(p: &mut AiPanel, h: &mut Harness, region: Region) {
    let ctx = h.ctx.clone();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 420.0))),
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| {
        let mut cx = h.cx("ai");
        p.ui(region, ui, &mut cx);
    });
    out.textures_delta.clear();
}

#[test]
fn renders_terminal_resizes_it_and_draws_summary_60() {
    let (mut p, mut h, dir) = setup("render");
    ev(&mut p, &mut h, Event::Layout(Effect::Opened(Area::Ai)));
    let id = tabs(&p, Group::Ai)[0];
    typ(&mut p, &mut h, "printf '\\033[31mred\\033[0m ok\\n'\r");
    assert!(wait(|| screen(&p, id).contains("red ok")));
    draw(&mut p, &mut h, Region::Area(Area::Ai));
    let term = p.tabs.as_ref().unwrap().terminal(id).unwrap().clone();
    assert_ne!(term.size(), TermSize::new(80, 24), "terminal follows the panel size");
    assert_eq!(p.tabs.as_ref().unwrap().default_size, term.size());
    draw(&mut p, &mut h, Region::Area(Area::Ai));
    assert!(p.views.contains_key(&id));

    ev(&mut p, &mut h, Event::Key(cmd('t')));
    ev(&mut p, &mut h, Event::Key(KeyEvent::char('2')));
    p.tabs.as_mut().unwrap().model.select(Group::Ai, Item::Summary);
    draw(&mut p, &mut h, Region::Area(Area::Ai));
    let view = p.tabs.as_ref().unwrap().summary(0);
    assert_eq!(view.blocks.len(), 2);
    assert_eq!(view::FONT_SIZE, 13.0);
    assert!(ev(&mut p, &mut h, Event::Key(KeyEvent::plain(Key::Down))));
    assert!(ev(&mut p, &mut h, Event::Key(KeyEvent::plain(Key::Enter))));
    assert_eq!(p.current(Group::Ai), Some(Item::Tab(tabs(&p, Group::Ai)[1])), "Enter goes to the session");
    drop(p);
    let _ = std::fs::remove_dir_all(dir);
}
