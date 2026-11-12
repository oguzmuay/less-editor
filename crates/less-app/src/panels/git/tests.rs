//! B1 Git paneli testleri; mutasyonlar yalnız burada oluşturulan geçici repolarda çalışır.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use eframe::egui;
use less_input::{EditMode, Key, KeyEvent};

use super::history::BranchGesture;
use super::status_view::StatusRow;
use super::tabs::{MergeView, TabState};
use super::*;
use crate::editor::EditorTab;
use crate::panels::Request;
use crate::panels::testing::Harness;

struct Repo {
    root: PathBuf,
}

impl Repo {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("less-b1-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        git(&root, &["init", "-q", "-b", "main"]);
        git(&root, &["config", "user.name", "Test User"]);
        git(&root, &["config", "user.email", "test@example.com"]);
        git(&root, &["config", "commit.gpgsign", "false"]);
        Self { root: root.canonicalize().unwrap() }
    }

    fn commit(&self, path: &str, text: &str, message: &str) {
        let file = self.root.join(path);
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(file, text).unwrap();
        git(&self.root, &["add", "-A"]);
        git(&self.root, &["commit", "-q", "-m", message]);
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git").current_dir(root).args(args).output().unwrap();
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn setup(repo: &Repo) -> (GitPanel, Harness) {
    let mut panel = GitPanel::without_watch();
    let mut harness = Harness::new(vec![repo.root.clone()]);
    panel.refresh_all(&mut harness.cx("git"));
    panel.git_open = true;
    panel.history_open = true;
    panel.focused_area = Area::Git;
    (panel, harness)
}

fn key(panel: &mut GitPanel, harness: &mut Harness, key: KeyEvent) -> bool {
    panel.event(&Event::Key(key), &mut harness.cx("git"))
}

fn select_file(panel: &mut GitPanel, section: Section, path: &str) {
    let row = panel
        .status_rows()
        .into_iter()
        .find(|row| matches!(row, StatusRow::File { entry, .. } if entry.section == section && entry.path == path))
        .unwrap();
    panel.status_cursor = Some(row.id());
}

#[test]
fn status_stage_unstage_all_and_cancel_first_discard_35() {
    let repo = Repo::new();
    repo.commit("a.txt", "one\n", "initial");
    fs::write(repo.root.join("a.txt"), "two\n").unwrap();
    fs::write(repo.root.join("new.txt"), "new\n").unwrap();
    let (mut panel, mut h) = setup(&repo);

    select_file(&mut panel, Section::Changes, "a.txt");
    assert!(key(&mut panel, &mut h, KeyEvent::char('s')));
    assert_eq!(status::status(&repo.root).unwrap().staged.len(), 1);
    select_file(&mut panel, Section::Staged, "a.txt");
    assert!(key(&mut panel, &mut h, KeyEvent::char('u')));
    assert_eq!(status::status(&repo.root).unwrap().changes.len(), 1);

    select_file(&mut panel, Section::Changes, "a.txt");
    key(&mut panel, &mut h, KeyEvent::char('X'));
    let Some(Prompt::Confirm { confirmed, action, .. }) = &panel.prompt else { panic!("discard confirmation") };
    assert!(!confirmed, "Cancel is the first/default choice");
    assert_eq!(
        action,
        &Action::Status(
            StatusCommand::DiscardConfirmed,
            StatusRow::File { repo: repo.root.clone(), entry: status::status(&repo.root).unwrap().changes[0].clone() },
        )
    );
    let outcome = panel.prompt.as_mut().unwrap().key(&KeyEvent::plain(Key::Enter));
    assert_eq!(outcome, Outcome::Close);
    assert_eq!(fs::read_to_string(repo.root.join("a.txt")).unwrap(), "two\n");
    panel.prompt = None;

    key(&mut panel, &mut h, KeyEvent::char('X'));
    let outcome = panel.prompt.as_mut().unwrap().key(&KeyEvent::plain(Key::Right));
    assert_eq!(outcome, Outcome::Keep);
    let outcome = panel.prompt.as_mut().unwrap().key(&KeyEvent::plain(Key::Enter));
    let Outcome::Run(action) = outcome else { panic!("confirmed discard") };
    panel.apply_action(action, &mut h.cx("git"));
    panel.prompt = None;
    assert_eq!(fs::read_to_string(repo.root.join("a.txt")).unwrap(), "one\n");

    fs::write(repo.root.join("a.txt"), "three\n").unwrap();
    panel.refresh_status(&mut h.cx("git"));
    select_file(&mut panel, Section::Changes, "a.txt");
    key(&mut panel, &mut h, KeyEvent::char('S'));
    let staged = status::status(&repo.root).unwrap();
    assert_eq!(staged.staged.len(), 2, "S stages the whole repository, not only the selected file");
    assert!(staged.untracked.is_empty());
    key(&mut panel, &mut h, KeyEvent::char('U'));
    let unstaged = status::status(&repo.root).unwrap();
    assert!(unstaged.staged.is_empty());
    assert_eq!(unstaged.untracked.len(), 1);
}

#[test]
fn saved_event_refreshes_status_and_history_35_58() {
    let repo = Repo::new();
    repo.commit("a.txt", "one\n", "initial");
    let (mut panel, mut h) = setup(&repo);
    assert!(panel.status_rows().iter().all(|row| !matches!(row, StatusRow::File { .. })));
    fs::write(repo.root.join("a.txt"), "two\n").unwrap();
    let path = repo.root.join("a.txt");
    assert!(panel.event(&Event::Saved(&path), &mut h.cx("git")));
    assert!(
        panel.status_rows().iter().any(|row| matches!(row, StatusRow::File { entry, .. } if entry.path == "a.txt"))
    );
    assert!(!panel.status_dirty);
    assert!(!panel.history_dirty);
}

#[test]
fn unified_diff_enter_jumps_to_real_file_line_17() {
    let repo = Repo::new();
    repo.commit("a.txt", "one\ntwo\nthree\n", "initial");
    fs::write(repo.root.join("a.txt"), "one\nTWO\nthree\n").unwrap();
    let (mut panel, mut h) = setup(&repo);
    assert!(panel.open_diff(repo.root.clone(), Source::Working { path: Some("a.txt".into()) }, &mut h.cx("git")));
    let key = h
        .requests
        .iter()
        .find_map(|request| match request {
            Request::OpenTab { key, .. } if key.starts_with("diff:") => Some(key.clone()),
            _ => None,
        })
        .unwrap();
    let line = match panel.tabs.get(&key) {
        Some(TabState::Diff(tab)) => tab.doc.lines.iter().position(|line| line.text == "+TWO").unwrap(),
        _ => panic!("diff tab"),
    };
    if let Some(TabState::Diff(tab)) = panel.tabs.get_mut(&key) {
        tab.cursor = line;
    }
    assert!(panel.tab_event(&key, &Event::Key(KeyEvent::plain(Key::Enter)), &mut h.cx("git")));
    assert!(h.requests.contains(&Request::OpenFile { path: repo.root.join("a.txt"), line: Some(1) }));
}

#[test]
fn history_branch_preview_does_not_checkout_and_checkout_is_confirmed_58_72() {
    let repo = Repo::new();
    repo.commit("a.txt", "main\n", "main commit");
    git(&repo.root, &["checkout", "-q", "-b", "feature"]);
    repo.commit("a.txt", "feature\n", "feature commit");
    git(&repo.root, &["checkout", "-q", "main"]);
    let (mut panel, mut h) = setup(&repo);
    assert!(!panel.history.graph.is_empty());
    let feature = panel.history.branches.locals.iter().find(|branch| branch.name == "feature").unwrap().clone();
    panel.preview_branch(feature.clone(), &mut h.cx("git"));
    assert_eq!(git(&repo.root, &["branch", "--show-current"]), "main", "preview never changes HEAD");
    assert_eq!(panel.history.branch.as_deref(), Some("feature"));
    assert!(panel.history.commits.iter().any(|commit| commit.subject == "feature commit"));

    panel.ask_checkout(feature);
    let outcome = panel.prompt.as_mut().unwrap().key(&KeyEvent::plain(Key::Enter));
    assert_eq!(outcome, Outcome::Close, "checkout also defaults to Cancel");
    assert_eq!(git(&repo.root, &["branch", "--show-current"]), "main");
    panel.prompt = None;
    panel.ask_checkout(panel.history.branches.locals.iter().find(|branch| branch.name == "feature").unwrap().clone());
    panel.prompt.as_mut().unwrap().key(&KeyEvent::plain(Key::Right));
    let Outcome::Run(action) = panel.prompt.as_mut().unwrap().key(&KeyEvent::plain(Key::Enter)) else {
        panic!("confirmed checkout")
    };
    panel.apply_action(action, &mut h.cx("git"));
    panel.prompt = None;
    assert_eq!(git(&repo.root, &["branch", "--show-current"]), "feature");
    assert!(panel.history.branch.is_none());
}

#[test]
fn history_tab_file_all_copy_and_repo_choices_17_58_72() {
    let repo = Repo::new();
    repo.commit("a.txt", "one\n", "initial");
    let (mut panel, mut h) = setup(&repo);
    panel.focused_area = Area::GitHistory;
    panel.history_part = StripPart::History;
    panel.last_file = Some(repo.root.join("a.txt"));
    assert!(key(&mut panel, &mut h, KeyEvent::char('f')));
    assert_eq!(panel.history.file.as_deref(), Some("a.txt"));
    assert!(key(&mut panel, &mut h, KeyEvent::char('a')));
    assert!(panel.history.all);
    assert!(key(&mut panel, &mut h, KeyEvent::char('y')));
    assert!(
        h.requests
            .iter()
            .any(|request| matches!(request, Request::Notify { text, .. } if text.starts_with("Copied: ")))
    );
    assert!(key(&mut panel, &mut h, KeyEvent::plain(Key::Tab)));
    assert_eq!(panel.history_part, StripPart::Branches);
    assert!(key(&mut panel, &mut h, KeyEvent::char('q')));
    assert!(
        h.requests.contains(&Request::Layout(LayoutOp::Close(Area::GitHistory))),
        "one close closes both strip parts"
    );
}

#[test]
fn compare_rollback_undo_navigation_and_save_85() {
    let repo = Repo::new();
    repo.commit("a.txt", "a\nb\nc\n", "initial");
    fs::write(repo.root.join("a.txt"), "a\nB\nc\nnew\n").unwrap();
    let (mut panel, mut h) = setup(&repo);
    panel.open_compare(repo.root.clone(), "a.txt".into(), &mut h.cx("git"));
    let key = panel.visible_tab.clone().unwrap();
    panel.compare_rollback(&key, Some(0), &mut h.cx("git"));
    let Some(TabState::Compare(tab)) = panel.tabs.get(&key) else { panic!("compare") };
    assert_eq!(tab.current, "a\nb\nc\nnew\n");
    assert!(tab.dirty);
    panel.compare_undo(&key);
    let Some(TabState::Compare(tab)) = panel.tabs.get(&key) else { panic!("compare") };
    assert_eq!(tab.current, "a\nB\nc\nnew\n");
    panel.compare_jump(&key, 1);
    assert_eq!(
        panel.tabs.get(&key).map(|tab| match tab {
            TabState::Compare(tab) => tab.cursor,
            _ => 0,
        }),
        Some(2)
    );
    panel.compare_jump(&key, 1);
    assert_eq!(
        panel.tabs.get(&key).map(|tab| match tab {
            TabState::Compare(tab) => tab.cursor,
            _ => 0,
        }),
        Some(4)
    );
    panel.compare_save(&key, &mut h.cx("git"));
    assert_eq!(fs::read_to_string(repo.root.join("a.txt")).unwrap(), "a\nB\nc\nnew\n");
    let Some(TabState::Compare(tab)) = panel.tabs.get(&key) else { panic!("compare") };
    assert!(!tab.dirty);
}

#[test]
fn merge_left_right_both_accept_all_and_mark_resolved_86() {
    let repo = Repo::new();
    repo.commit("a.txt", "top\nbase\nend\n", "base");
    git(&repo.root, &["checkout", "-q", "-b", "other"]);
    repo.commit("a.txt", "top\nright\nend\n", "right");
    git(&repo.root, &["checkout", "-q", "main"]);
    repo.commit("a.txt", "top\nleft\nend\n", "left");
    let output = Command::new("git").current_dir(&repo.root).args(["merge", "other"]).output().unwrap();
    assert!(!output.status.success());
    let (mut panel, mut h) = setup(&repo);
    panel.open_merge(repo.root.clone(), "a.txt".into(), &mut h.cx("git"));
    let key = panel.visible_tab.clone().unwrap();
    panel.merge_choice(&key, Some(0), Choice::Both, &mut h.cx("git"));
    let Some(TabState::Merge(tab)) = panel.tabs.get(&key) else { panic!("merge") };
    assert_eq!(tab.result, "top\nleft\nright\nend\n");
    panel.merge_resolve(&key, &mut h.cx("git"));
    assert!(status::status(&repo.root).unwrap().conflicts.is_empty());
    assert_eq!(fs::read_to_string(repo.root.join("a.txt")).unwrap(), "top\nleft\nright\nend\n");
    assert!(h.requests.iter().any(|request| matches!(request, Request::CloseTab { key: k, .. } if k == &key)));
}

#[test]
fn blame_command_and_leader_open_view_then_commit_diff_22() {
    let repo = Repo::new();
    repo.commit("a.txt", "one\ntwo\n", "initial");
    let (mut panel, mut h) = setup(&repo);
    let tab = EditorTab::load(&repo.root.join("a.txt")).unwrap();
    let mut cx = Cx {
        ctx: &h.ctx,
        palette: &h.palette,
        roots: &h.roots,
        edit_mode: EditMode::Vim,
        focused: false,
        active: Some(&tab),
        owner: "git",
        requests: &mut h.requests,
    };
    assert!(panel.event(&Event::Ex("GitBlame"), &mut cx));
    let blame_key = panel.visible_tab.clone().unwrap();
    assert!(blame_key.starts_with("blame:"));
    assert!(panel.tab_event(&blame_key, &Event::Key(KeyEvent::plain(Key::Enter)), &mut h.cx("git")));
    assert!(
        h.requests.iter().any(|request| matches!(request, Request::OpenTab { key, .. } if key.starts_with("diff:")))
    );
    panel.visible_tab = Some(blame_key.clone());
    assert!(panel.event(&Event::Leader("ga"), &mut h.cx("git")));
    assert!(h.requests.contains(&Request::CloseTab { owner: "git", key: blame_key }));
}

#[test]
fn all_regions_and_custom_tabs_render_headless_17_35_58_85_86_22() {
    let repo = Repo::new();
    repo.commit("a.txt", "one\n", "initial");
    fs::write(repo.root.join("a.txt"), "two\n").unwrap();
    let (mut panel, mut h) = setup(&repo);
    panel.open_diff(repo.root.clone(), Source::Working { path: Some("a.txt".into()) }, &mut h.cx("git"));
    panel.open_compare(repo.root.clone(), "a.txt".into(), &mut h.cx("git"));
    panel.tabs.insert(
        "merge:test".into(),
        TabState::Merge(MergeView {
            repo: repo.root.clone(),
            path: "a.txt".into(),
            ours: "left\n".into(),
            theirs: "right\n".into(),
            result: "<<<<<<< HEAD\nleft\n=======\nright\n>>>>>>> other\n".into(),
            selected: 0,
            undo: Vec::new(),
        }),
    );
    panel.tabs.insert(
        "blame:test".into(),
        TabState::Blame(BlameView {
            repo: repo.root.clone(),
            file: repo.root.join("a.txt"),
            source: "two\n".into(),
            lines: vec![None],
            cursor: 0,
        }),
    );
    let tab_keys: Vec<String> = panel.tabs.keys().cloned().collect();
    let ctx = h.ctx.clone();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0))),
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        for (index, region) in REGIONS.into_iter().enumerate() {
            let rect = egui::Rect::from_min_size(egui::pos2(index as f32 * 410.0, 0.0), egui::vec2(400.0, 240.0));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            panel.ui(region, &mut child, &mut h.cx("git"));
        }
        for key in &tab_keys {
            panel.tab_ui(key, ui, &mut h.cx("git"));
        }
    });
    output.textures_delta.clear();
}

#[test]
fn section_collapse_and_right_click_menu_actions_35() {
    let repo = Repo::new();
    repo.commit("a.txt", "one\n", "initial");
    fs::write(repo.root.join("a.txt"), "two\n").unwrap();
    let (mut panel, mut h) = setup(&repo);
    let section = panel.status_rows().into_iter().find(|row| matches!(row, StatusRow::Section { .. })).unwrap();
    panel.status_cursor = Some(section.id());
    key(&mut panel, &mut h, KeyEvent::plain(Key::Left));
    assert!(panel.collapsed.contains(&section.id()));
    assert!(!panel.status_rows().iter().any(|row| matches!(row, StatusRow::File { .. })));
    key(&mut panel, &mut h, KeyEvent::plain(Key::Right));
    assert!(panel.status_rows().iter().any(|row| matches!(row, StatusRow::File { .. })));
    key(&mut panel, &mut h, KeyEvent::char('m'));
    assert!(matches!(panel.prompt, Some(Prompt::Menu { .. })));
}

#[test]
fn branch_group_gesture_only_collapses_without_checkout_72() {
    let repo = Repo::new();
    repo.commit("a.txt", "one\n", "initial");
    let (mut panel, mut h) = setup(&repo);
    panel.branch_gesture(BranchRow::Group { name: "Local" }, BranchGesture::Preview, &mut h.cx("git"));
    assert!(panel.history.collapsed.contains("group:Local"));
    assert_eq!(git(&repo.root, &["branch", "--show-current"]), "main");
}

#[test]
fn multiple_repositories_are_listed_and_mutated_independently_35() {
    let first = Repo::new();
    let second = Repo::new();
    first.commit("a.txt", "one\n", "first");
    second.commit("b.txt", "one\n", "second");
    fs::write(first.root.join("a.txt"), "changed\n").unwrap();
    fs::write(second.root.join("b.txt"), "changed\n").unwrap();
    let mut panel = GitPanel::without_watch();
    let mut h = Harness::new(vec![first.root.clone(), second.root.clone()]);
    panel.refresh_all(&mut h.cx("git"));
    let repos: Vec<_> = panel
        .status_rows()
        .into_iter()
        .filter_map(|row| match row {
            StatusRow::Repo(repo) => Some(repo),
            _ => None,
        })
        .collect();
    assert_eq!(repos, vec![first.root.clone(), second.root.clone()]);
    select_file(&mut panel, Section::Changes, "a.txt");
    panel.status_action(
        StatusCommand::Stage,
        panel
            .status_rows()
            .into_iter()
            .find(|row| matches!(row, StatusRow::File { entry, .. } if entry.path == "a.txt"))
            .unwrap(),
        &mut h.cx("git"),
    );
    assert_eq!(status::status(&first.root).unwrap().staged.len(), 1);
    assert_eq!(status::status(&second.root).unwrap().changes.len(), 1);
}

#[test]
fn untracked_file_is_deleted_only_after_confirmation_35() {
    let repo = Repo::new();
    repo.commit("a.txt", "one\n", "initial");
    fs::write(repo.root.join("new.txt"), "new\n").unwrap();
    let (mut panel, mut h) = setup(&repo);
    select_file(&mut panel, Section::Untracked, "new.txt");
    key(&mut panel, &mut h, KeyEvent::char('X'));
    assert!(repo.root.join("new.txt").exists());
    let outcome = panel.prompt.as_mut().unwrap().key(&KeyEvent::char('y'));
    let Outcome::Run(action) = outcome else { panic!("y confirms") };
    panel.apply_action(action, &mut h.cx("git"));
    assert!(!repo.root.join("new.txt").exists());
}

#[test]
fn commit_stat_enter_jumps_to_file_section_17() {
    let repo = Repo::new();
    repo.commit("a.txt", "one\n", "initial");
    repo.commit("b.txt", "two\n", "second file");
    let (mut panel, mut h) = setup(&repo);
    panel.open_commit(repo.root.clone(), "HEAD".into(), None, &mut h.cx("git"));
    let key = panel.visible_tab.clone().unwrap();
    let Some(TabState::Diff(tab)) = panel.tabs.get(&key) else { panic!("diff") };
    let stat = tab.doc.lines.iter().position(|line| line.kind == diff::LineKind::Stat).unwrap();
    panel.diff_jump(&key, stat, &mut h.cx("git"));
    let Some(TabState::Diff(tab)) = panel.tabs.get(&key) else { panic!("diff") };
    assert_eq!(tab.doc.lines[tab.cursor].kind, diff::LineKind::FileHeader);
    assert!(tab.doc.lines[tab.cursor].text.contains("b.txt"));
}

#[test]
fn merge_accept_all_preserves_manually_resolved_blocks_86() {
    let repo = Repo::new();
    let (mut panel, mut h) = setup(&repo);
    let key = "merge:manual".to_owned();
    let text = "<<<<<<< HEAD\nleft1\n=======\nright1\n>>>>>>> other\nmid\n<<<<<<< HEAD\nleft2\n=======\nright2\n>>>>>>> other\n";
    panel.tabs.insert(
        key.clone(),
        TabState::Merge(MergeView {
            repo: repo.root.clone(),
            path: "a.txt".into(),
            ours: String::new(),
            theirs: String::new(),
            result: text.into(),
            selected: 0,
            undo: Vec::new(),
        }),
    );
    panel.merge_choice(&key, Some(0), Choice::Left, &mut h.cx("git"));
    panel.merge_accept_all(&key, Choice::Right);
    let Some(TabState::Merge(tab)) = panel.tabs.get(&key) else { panic!("merge") };
    assert_eq!(tab.result, "left1\nmid\nright2\n");
    assert!(!merge::has_conflicts(&tab.result));
}

#[test]
fn compare_edit_mode_accepts_text_and_blame_message_is_routed_85_22() {
    let repo = Repo::new();
    repo.commit("a.txt", "one\n", "initial");
    fs::write(repo.root.join("a.txt"), "two\n").unwrap();
    let (mut panel, mut h) = setup(&repo);
    panel.open_compare(repo.root.clone(), "a.txt".into(), &mut h.cx("git"));
    let key = panel.visible_tab.clone().unwrap();
    assert!(panel.tab_event(&key, &Event::Key(KeyEvent::char('e')), &mut h.cx("git")));
    let Some(TabState::Compare(tab)) = panel.tabs.get_mut(&key) else { panic!("compare") };
    assert!(tab.editing);
    tab.current = "edited\n".into();
    tab.dirty = true;
    panel.compare_save(&key, &mut h.cx("git"));
    assert_eq!(fs::read_to_string(repo.root.join("a.txt")).unwrap(), "edited\n");

    let tab = EditorTab::load(&repo.root.join("a.txt")).unwrap();
    let mut cx = Cx {
        ctx: &h.ctx,
        palette: &h.palette,
        roots: &h.roots,
        edit_mode: EditMode::Vim,
        focused: false,
        active: Some(&tab),
        owner: "git",
        requests: &mut h.requests,
    };
    assert!(panel.event(&Event::Message { topic: "git.blame", data: "a.txt" }, &mut cx));
    assert!(panel.visible_tab.as_deref().is_some_and(|key| key.starts_with("blame:")));
}
