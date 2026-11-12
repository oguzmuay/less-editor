//! Git paneli (B1): çoklu repo status (#35), Git History + graph + Branches (#58 #72), unified / yan yana diff,
//! merge (#17 #85 #86) ve host izin verdiği ölçüde blame (#22). Git işlemleri `less-git`'e, çizim bu klasöre aittir.

mod history;
mod popup;
#[path = "status.rs"]
mod status_view;
mod tabs;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

use eframe::egui::{self, Ui};
use less_git::blame;
use less_git::branches::{self, Branch};
use less_git::compare;
use less_git::diff::{self, DiffDoc, Jump, Source};
use less_git::log::{self, LogOptions};
use less_git::merge::{self, Choice};
use less_git::status::{self, Entry, Section};
use less_git::watch::RepoWatcher;
use less_input::{Key, KeyEvent};
use less_workbench::layout::StripPart;

pub use history::History;
use history::{BranchGesture, BranchRow};
pub use popup::{MenuEntry, Outcome, Prompt};
use status_view::{RepoStatus, StatusRow};
use tabs::{BlameView, CompareView, DiffView, Intent, MergeView, TabState};

use super::{Area, Cx, Effect, Event, LayoutOp, Panel, Region};

const REGIONS: [Region; 3] =
    [Region::Area(Area::Git), Region::Strip(StripPart::Branches), Region::Strip(StripPart::History)];

/// Bu klasörün panelleri; host `panels::all()` ile toplar.
pub fn panels() -> Vec<Box<dyn Panel>> {
    vec![Box::new(GitPanel::new())]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusCommand {
    Open,
    Diff,
    Stage,
    Unstage,
    StageAll,
    UnstageAll,
    Discard,
    DiscardConfirmed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryCommand {
    PreviewBranch(Branch),
    Checkout(Branch),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Status(StatusCommand, StatusRow),
    History(HistoryCommand),
    SelectRepo(PathBuf),
    Copy(String),
    RefreshStatus,
}

/// Git ve Git History tek panel nesnesidir; böylece özel diff/merge/blame tab'larının sahibi de aynı kalır.
pub struct GitPanel {
    repos: Vec<PathBuf>,
    statuses: Vec<RepoStatus>,
    pub(super) collapsed: HashSet<String>,
    pub(super) status_cursor: Option<String>,
    pub history: History,
    pub(super) history_part: StripPart,
    pub(super) prompt: Option<Prompt>,
    tabs: HashMap<String, TabState>,
    visible_tab: Option<String>,
    watcher: Option<RepoWatcher>,
    watched_roots: Vec<PathBuf>,
    watch_enabled: bool,
    status_dirty: bool,
    history_dirty: bool,
    git_open: bool,
    history_open: bool,
    focused_area: Area,
    last_file: Option<PathBuf>,
    pending_prefix: Option<char>,
}

impl GitPanel {
    pub fn new() -> Self {
        Self {
            repos: Vec::new(),
            statuses: Vec::new(),
            collapsed: HashSet::new(),
            status_cursor: None,
            history: History::default(),
            history_part: StripPart::History,
            prompt: None,
            tabs: HashMap::new(),
            visible_tab: None,
            watcher: None,
            watched_roots: Vec::new(),
            watch_enabled: true,
            status_dirty: true,
            history_dirty: true,
            git_open: false,
            history_open: false,
            focused_area: Area::Code,
            last_file: None,
            pending_prefix: None,
        }
    }

    #[cfg(test)]
    fn without_watch() -> Self {
        Self { watch_enabled: false, ..Self::new() }
    }

    pub(super) fn status_rows(&self) -> Vec<StatusRow> {
        status_view::rows(&self.statuses, &self.collapsed)
    }

    pub(super) fn status_cursor_index(&self, rows: &[StatusRow]) -> usize {
        self.status_cursor.as_deref().and_then(|id| rows.iter().position(|r| r.id() == id)).unwrap_or(0)
    }

    fn discover(&mut self, cx: &Cx) {
        self.repos = status::repos(cx.roots);
        if self.history.repo.as_ref().is_none_or(|r| !self.repos.contains(r)) {
            self.history.repo = self.repo_for_file(self.last_file.as_deref()).or_else(|| self.repos.first().cloned());
        }
    }

    fn repo_for_file(&self, file: Option<&Path>) -> Option<PathBuf> {
        let file = file?;
        self.repos.iter().filter(|root| file.starts_with(root)).max_by_key(|root| root.components().count()).cloned()
    }

    fn refresh_status(&mut self, cx: &mut Cx) {
        if self.repos.is_empty() {
            self.discover(cx);
        }
        self.statuses = self
            .repos
            .iter()
            .map(|root| RepoStatus {
                root: root.clone(),
                result: status::status(root).map_err(|e| e.to_string()),
                track: branches::list(root)
                    .ok()
                    .and_then(|list| list.locals.into_iter().find(|branch| branch.current))
                    .and_then(|branch| branch.track),
            })
            .collect();
        self.status_dirty = false;
        let rows = self.status_rows();
        if self.status_cursor.as_ref().is_none_or(|id| rows.iter().all(|r| &r.id() != id)) {
            self.status_cursor = rows.first().map(StatusRow::id);
        }
    }

    fn refresh_history(&mut self, cx: &mut Cx) {
        if self.repos.is_empty() {
            self.discover(cx);
        }
        if self.history.repo.as_ref().is_none_or(|r| !self.repos.contains(r)) {
            self.history.repo = self.repo_for_file(self.last_file.as_deref()).or_else(|| self.repos.first().cloned());
        }
        let Some(repo) = self.history.repo.clone() else {
            self.history.commits.clear();
            self.history.graph.clear();
            self.history.branches = Default::default();
            self.history.error = None;
            self.history_dirty = false;
            return;
        };
        match load_history(&repo, &self.history) {
            Ok((commits, branch_list)) => {
                self.history.graph = history::graph_for(&commits);
                self.history.commits = commits;
                self.history.branches = branch_list;
                self.history.error = None;
                if self.history.cursor.as_ref().is_none_or(|h| self.history.commits.iter().all(|c| &c.hash != h)) {
                    self.history.cursor = self.history.commits.first().map(|c| c.hash.clone());
                }
                let rows = history::branch_rows(&self.history);
                if self.history.branch_cursor.as_ref().is_none_or(|id| rows.iter().all(|r| &r.id() != id)) {
                    self.history.branch_cursor = rows.first().map(|r| r.id());
                }
            }
            Err(error) => {
                self.history.error = Some(error);
                self.history.commits.clear();
                self.history.graph.clear();
            }
        }
        self.history_dirty = false;
    }

    fn refresh_all(&mut self, cx: &mut Cx) {
        self.repos.clear();
        self.discover(cx);
        self.refresh_status(cx);
        self.refresh_history(cx);
    }

    pub(super) fn status_menu(&self, row: &StatusRow) -> Prompt {
        let title = match row {
            StatusRow::File { entry, .. } => entry.path.clone(),
            StatusRow::Section { section, .. } => status_view::section_name(*section).to_owned(),
            StatusRow::Repo(repo) => repo.display().to_string(),
        };
        Prompt::menu(title, status_view::menu_entries(row))
    }

    pub(super) fn activate_status(&mut self, row: StatusRow, cx: &mut Cx) {
        match &row {
            StatusRow::Section { .. } => self.toggle_section(&row, None),
            StatusRow::File { entry, .. } if entry.section == Section::Conflicts => self.show_diff(&row, cx),
            StatusRow::File { repo, entry } => {
                let path = repo.join(&entry.path);
                if path.exists() {
                    cx.open_file(path, None);
                } else {
                    self.show_diff(&row, cx);
                }
            }
            StatusRow::Repo(_) => {}
        }
    }

    fn toggle_section(&mut self, row: &StatusRow, open: Option<bool>) {
        if !matches!(row, StatusRow::Section { .. }) {
            return;
        }
        let id = row.id();
        let open = open.unwrap_or_else(|| self.collapsed.contains(&id));
        if open {
            self.collapsed.remove(&id);
        } else {
            self.collapsed.insert(id);
        }
    }

    fn status_action(&mut self, command: StatusCommand, row: StatusRow, cx: &mut Cx) {
        match command {
            StatusCommand::Open => self.activate_status(row, cx),
            StatusCommand::Diff => self.show_diff(&row, cx),
            StatusCommand::Discard => {
                let StatusRow::File { repo, entry } = row else { return };
                if matches!(entry.section, Section::Staged | Section::Conflicts) {
                    let message = if entry.section == Section::Staged {
                        "Unstage the file first to discard its changes"
                    } else {
                        "Resolve the conflict first"
                    };
                    cx.error(message);
                    return;
                }
                let verb =
                    if entry.section == Section::Untracked { "Delete untracked file" } else { "Discard changes to" };
                let body = format!("{verb} {}? This cannot be undone.", entry.path);
                self.prompt = Some(Prompt::confirm(
                    "Git",
                    body,
                    Action::Status(StatusCommand::DiscardConfirmed, StatusRow::File { repo, entry }),
                ));
            }
            StatusCommand::DiscardConfirmed => {
                let StatusRow::File { repo, entry } = row else { return };
                match status::discard(&repo, &entry) {
                    Ok(()) => {
                        cx.info(format!("{} discarded", entry.path));
                        self.refresh_status(cx);
                    }
                    Err(error) => cx.error(error.to_string()),
                }
            }
            StatusCommand::Stage | StatusCommand::Unstage | StatusCommand::StageAll | StatusCommand::UnstageAll => {
                let result = self.change_stage(command, &row);
                match result {
                    Ok(()) => self.refresh_status(cx),
                    Err(error) => cx.error(error),
                }
            }
        }
    }

    fn change_stage(&self, command: StatusCommand, row: &StatusRow) -> Result<(), String> {
        let repo = row.repo();
        let status = self
            .statuses
            .iter()
            .find(|s| &s.root == repo)
            .and_then(|s| s.result.as_ref().ok())
            .ok_or_else(|| "Repository status is unavailable".to_owned())?;
        let entries: Vec<Entry> = match command {
            StatusCommand::StageAll | StatusCommand::UnstageAll => {
                status_view::sections(status).into_iter().flat_map(|(_, entries)| entries.to_vec()).collect()
            }
            _ => match row {
                StatusRow::File { entry, .. } => vec![entry.clone()],
                StatusRow::Section { entries, .. } => entries.clone(),
                StatusRow::Repo(_) => {
                    status_view::sections(status).into_iter().flat_map(|(_, entries)| entries.to_vec()).collect()
                }
            },
        };
        let entries: Vec<Entry> = entries
            .into_iter()
            .filter(|entry| match command {
                StatusCommand::Stage | StatusCommand::StageAll => entry.section != Section::Staged,
                StatusCommand::Unstage | StatusCommand::UnstageAll => entry.section == Section::Staged,
                _ => false,
            })
            .collect();
        if entries.is_empty() {
            return Ok(());
        }
        let paths = entry_paths(&entries);
        let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
        match command {
            StatusCommand::Stage | StatusCommand::StageAll => status::stage(repo, &refs).map_err(|e| e.to_string()),
            StatusCommand::Unstage | StatusCommand::UnstageAll => {
                status::unstage(repo, &refs).map_err(|e| e.to_string())
            }
            _ => Ok(()),
        }
    }

    fn show_diff(&mut self, row: &StatusRow, cx: &mut Cx) {
        let StatusRow::File { repo, entry } = row else { return };
        let path = repo.join(&entry.path);
        if entry.section == Section::Conflicts {
            self.open_merge(repo.clone(), entry.path.clone(), cx);
        } else if path.exists() && entry.section != Section::Untracked {
            self.open_compare(repo.clone(), entry.path.clone(), cx);
        } else {
            let source = if entry.section == Section::Staged {
                Source::Staged { path: Some(entry.path.clone()) }
            } else {
                Source::Working { path: Some(entry.path.clone()) }
            };
            self.open_diff(repo.clone(), source, cx);
        }
    }

    fn open_diff(&mut self, repo: PathBuf, source: Source, cx: &mut Cx) -> bool {
        match diff::diff(&repo, &source) {
            Ok(Some(doc)) => {
                self.insert_diff(repo, source, doc, cx);
                true
            }
            Ok(None) => {
                cx.info("No changes to show");
                false
            }
            Err(error) => {
                cx.error(error.to_string());
                false
            }
        }
    }

    fn insert_diff(&mut self, repo: PathBuf, source: Source, doc: DiffDoc, cx: &mut Cx) {
        let key = diff_key(&repo, &source);
        let title = diff_title(&source);
        self.tabs.insert(key.clone(), TabState::Diff(DiffView { repo, source, doc, cursor: 0 }));
        self.visible_tab = Some(key.clone());
        cx.open_tab(key, title);
    }

    fn open_commit(&mut self, repo: PathBuf, rev: String, path: Option<String>, cx: &mut Cx) {
        if let Some(path) = path.clone()
            && self.open_diff(repo.clone(), Source::Commit { rev: rev.clone(), path: Some(path) }, cx)
        {
            return;
        }
        self.open_diff(repo, Source::Commit { rev, path: None }, cx);
    }

    fn open_compare(&mut self, repo: PathBuf, path: String, cx: &mut Cx) {
        let key = stable_key("compare", &repo, &path);
        let preserve = match self.tabs.get(&key) {
            Some(TabState::Compare(tab)) => tab.dirty,
            _ => false,
        };
        if preserve {
            self.visible_tab = Some(key.clone());
            cx.open_tab(key, format!("{path} (compare)"));
            return;
        }
        let absolute = repo.join(&path);
        let current = match cx.active.filter(|tab| tab.path.as_deref() == Some(&absolute)) {
            Some(tab) => tab.editor.text(),
            None => match std::fs::read_to_string(&absolute) {
                Ok(text) => text,
                Err(error) => {
                    cx.error(format!("Could not read {path}: {error}"));
                    return;
                }
            },
        };
        let base = match git_text(&repo, &["show", &format!("HEAD:{path}")]) {
            Ok(base) => base,
            Err(error) if error.contains("HEAD") => String::new(),
            Err(error) => {
                cx.error(format!("Could not read HEAD version of {path}: {error}"));
                return;
            }
        };
        self.tabs.insert(
            key.clone(),
            TabState::Compare(CompareView {
                repo,
                path: path.clone(),
                base,
                current,
                cursor: 1,
                left: false,
                undo: Vec::new(),
                dirty: false,
                editing: false,
            }),
        );
        self.visible_tab = Some(key.clone());
        cx.open_tab(key, format!("{path} (compare)"));
    }

    fn open_merge(&mut self, repo: PathBuf, path: String, cx: &mut Cx) {
        let key = stable_key("merge", &repo, &path);
        if matches!(self.tabs.get(&key), Some(TabState::Merge(_))) {
            self.visible_tab = Some(key.clone());
            cx.open_tab(key, format!("{path} (merge)"));
            return;
        }
        let absolute = repo.join(&path);
        let result = match std::fs::read_to_string(&absolute) {
            Ok(text) => text,
            Err(error) => {
                cx.error(format!("Could not read {path}: {error}"));
                return;
            }
        };
        match merge::sides(&repo, &path) {
            Ok((ours, theirs)) => {
                self.tabs.insert(
                    key.clone(),
                    TabState::Merge(MergeView {
                        repo,
                        path: path.clone(),
                        ours,
                        theirs,
                        result,
                        selected: 0,
                        undo: Vec::new(),
                    }),
                );
                self.visible_tab = Some(key.clone());
                cx.open_tab(key, format!("{path} (merge)"));
            }
            Err(error) => cx.error(error.to_string()),
        }
    }

    fn toggle_blame(&mut self, cx: &mut Cx) {
        if let Some(key) = self.visible_tab.clone().filter(|key| key.starts_with("blame:")) {
            self.visible_tab = None;
            cx.close_tab(key);
            return;
        }
        let Some(tab) = cx.active else {
            cx.error("Open a file to show Git blame");
            return;
        };
        let Some(file) = tab.path.clone() else {
            cx.error("Git blame needs a file on disk");
            return;
        };
        let source = tab.editor.text();
        let repo = self
            .repo_for_file(Some(&file))
            .or_else(|| file.parent().map(|p| status::repos(&[p.to_path_buf()])).and_then(|r| r.first().cloned()));
        let Some(repo) = repo else {
            cx.error("This file is not in a Git repository");
            return;
        };
        match blame::blame(&file, &source) {
            Ok(lines) => {
                let key = format!("blame:{}", stable_hash(&file));
                self.tabs.insert(
                    key.clone(),
                    TabState::Blame(BlameView { repo, file: file.clone(), source, lines, cursor: 0 }),
                );
                self.visible_tab = Some(key.clone());
                let title = file.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
                cx.open_tab(key, format!("{title} (blame)"));
            }
            Err(error) => cx.error(error.to_string()),
        }
    }

    fn refresh_blame(&mut self, path: &Path, cx: &mut Cx) {
        for tab in self.tabs.values_mut() {
            let TabState::Blame(tab) = tab else { continue };
            if tab.file != path {
                continue;
            }
            match std::fs::read_to_string(path) {
                Ok(source) => match blame::blame(path, &source) {
                    Ok(lines) => {
                        tab.source = source;
                        tab.lines = lines;
                        tab.cursor = tab.cursor.min(tab.source.lines().count().saturating_sub(1));
                    }
                    Err(error) => cx.error(error.to_string()),
                },
                Err(error) => cx.error(format!("Could not read {}: {error}", path.display())),
            }
        }
    }

    fn status_key(&mut self, key: KeyEvent, cx: &mut Cx) -> bool {
        let rows = self.status_rows();
        let index = self.status_cursor_index(&rows);
        let plain = !key.mods.ctrl && !key.mods.alt && !key.mods.cmd;
        match key.key {
            Key::Up => self.select_status(&rows, index.saturating_sub(1)),
            Key::Down => self.select_status(&rows, (index + 1).min(rows.len().saturating_sub(1))),
            Key::Home | Key::PageUp => self.select_status(&rows, 0),
            Key::End | Key::PageDown => self.select_status(&rows, rows.len().saturating_sub(1)),
            Key::Right if plain => {
                if let Some(row) = rows.get(index) {
                    self.toggle_section(row, Some(true));
                }
            }
            Key::Left if plain => {
                if let Some(row) = rows.get(index) {
                    self.toggle_section(row, Some(false));
                }
            }
            Key::Enter if plain => {
                if let Some(row) = rows.get(index).cloned() {
                    self.activate_status(row, cx);
                }
            }
            Key::Char(' ') if plain => {
                if let Some(row @ StatusRow::Section { .. }) = rows.get(index).cloned() {
                    self.toggle_section(&row, None);
                }
            }
            Key::Esc if plain => cx.layout(LayoutOp::Focus(Area::Code)),
            Key::Char('q') if plain => cx.layout(LayoutOp::Close(Area::Git)),
            Key::Char('R') if plain => {
                self.repos.clear();
                self.refresh_status(cx);
            }
            Key::Char(c @ ('o' | 'd' | 's' | 'u' | 'S' | 'U' | 'X')) if plain => {
                if let Some(row) = rows.get(index).cloned() {
                    let command = match c {
                        'o' => StatusCommand::Open,
                        'd' => StatusCommand::Diff,
                        's' => StatusCommand::Stage,
                        'u' => StatusCommand::Unstage,
                        'S' => StatusCommand::StageAll,
                        'U' => StatusCommand::UnstageAll,
                        _ => StatusCommand::Discard,
                    };
                    self.status_action(command, row, cx);
                }
            }
            Key::Char('m') if plain => {
                if let Some(row) = rows.get(index) {
                    self.prompt = Some(self.status_menu(row));
                }
            }
            _ => return false,
        }
        true
    }

    fn select_status(&mut self, rows: &[StatusRow], index: usize) {
        self.status_cursor = rows.get(index).map(StatusRow::id);
    }

    fn history_key(&mut self, key: KeyEvent, cx: &mut Cx) -> bool {
        if key.key == Key::Tab && !key.mods.cmd {
            self.history_part =
                if self.history_part == StripPart::Branches { StripPart::History } else { StripPart::Branches };
            return true;
        }
        if self.history_part == StripPart::Branches { self.branch_key(key, cx) } else { self.commit_key(key, cx) }
    }

    fn commit_key(&mut self, key: KeyEvent, cx: &mut Cx) -> bool {
        let plain = !key.mods.ctrl && !key.mods.alt && !key.mods.cmd;
        let count = self.history.commits.len();
        let index = self
            .history
            .cursor
            .as_deref()
            .and_then(|h| self.history.commits.iter().position(|c| c.hash == h))
            .unwrap_or(0);
        match key.key {
            Key::Up => self.select_commit(index.saturating_sub(1)),
            Key::Down => self.select_commit((index + 1).min(count.saturating_sub(1))),
            Key::Home | Key::PageUp => self.select_commit(0),
            Key::End | Key::PageDown => self.select_commit(count.saturating_sub(1)),
            Key::Enter if plain => self.open_history_commit(index, cx),
            Key::Char('f') if plain => self.toggle_file_history(cx),
            Key::Char('a') if plain => {
                self.history.all = !self.history.all;
                self.refresh_history(cx);
            }
            Key::Char('y') if plain => {
                if let Some(commit) = self.history.commits.get(index) {
                    let hash = commit.hash.clone();
                    self.copy(hash, cx);
                }
            }
            Key::Char('g') if plain => self.choose_repo(cx),
            Key::Char('R') if plain => self.refresh_all(cx),
            Key::Char('q') if plain => cx.layout(LayoutOp::Close(Area::GitHistory)),
            Key::Esc if plain => cx.layout(LayoutOp::Focus(Area::Code)),
            _ => return false,
        }
        true
    }

    fn select_commit(&mut self, index: usize) {
        self.history.cursor = self.history.commits.get(index).map(|c| c.hash.clone());
    }

    fn branch_key(&mut self, key: KeyEvent, cx: &mut Cx) -> bool {
        let plain = !key.mods.ctrl && !key.mods.alt && !key.mods.cmd;
        let rows = history::branch_rows(&self.history);
        let index =
            self.history.branch_cursor.as_deref().and_then(|id| rows.iter().position(|r| r.id() == id)).unwrap_or(0);
        let row = rows.get(index).cloned();
        match key.key {
            Key::Up => self.select_branch(&rows, index.saturating_sub(1)),
            Key::Down => self.select_branch(&rows, (index + 1).min(rows.len().saturating_sub(1))),
            Key::Enter if plain => {
                if let Some(row) = row {
                    self.branch_gesture(row, BranchGesture::Preview, cx);
                }
            }
            Key::Char(' ') if plain => {
                if let Some(row @ BranchRow::Group { .. }) = row {
                    self.branch_gesture(row, BranchGesture::Preview, cx);
                }
            }
            Key::Char('c') if plain => {
                if let Some(row) = row {
                    self.branch_gesture(row, BranchGesture::Checkout, cx);
                }
            }
            Key::Char('y') if plain => {
                if let Some(BranchRow::Branch(branch)) = row {
                    self.copy(branch.name, cx);
                }
            }
            Key::Char('m') if plain => {
                if let Some(BranchRow::Branch(branch)) = row {
                    self.prompt = Some(history::branch_menu(&branch));
                }
            }
            Key::Char('a') if plain => {
                self.history.all = !self.history.all;
                self.refresh_history(cx);
            }
            Key::Char('g') if plain => self.choose_repo(cx),
            Key::Char('R') if plain => self.refresh_all(cx),
            Key::Char('q') if plain => cx.layout(LayoutOp::Close(Area::GitHistory)),
            Key::Esc if plain => cx.layout(LayoutOp::Focus(Area::Code)),
            _ => return false,
        }
        true
    }

    fn select_branch(&mut self, rows: &[BranchRow], index: usize) {
        self.history.branch_cursor = rows.get(index).map(BranchRow::id);
    }

    pub(super) fn branch_gesture(&mut self, row: BranchRow, gesture: BranchGesture, cx: &mut Cx) {
        match row {
            BranchRow::Group { .. } if gesture != BranchGesture::Menu => {
                let id = row.id();
                if !self.history.collapsed.remove(&id) {
                    self.history.collapsed.insert(id);
                }
            }
            BranchRow::Branch(branch) => match gesture {
                BranchGesture::Preview => self.preview_branch(branch, cx),
                BranchGesture::Checkout => self.ask_checkout(branch),
                BranchGesture::Menu => self.prompt = Some(history::branch_menu(&branch)),
            },
            BranchRow::Group { .. } => {}
        }
    }

    fn preview_branch(&mut self, branch: Branch, cx: &mut Cx) {
        self.history.branch = (!branch.current).then_some(branch.name);
        self.history.all = false;
        self.refresh_history(cx);
    }

    fn ask_checkout(&mut self, branch: Branch) {
        if branch.current {
            return;
        }
        let target = branch
            .remote
            .as_ref()
            .map(|r| branch.name[r.len() + 1..].to_owned())
            .unwrap_or_else(|| branch.name.clone());
        self.prompt = Some(Prompt::confirm(
            "Switch branch",
            format!("Checkout {target}? Unsaved work may prevent the switch."),
            Action::History(HistoryCommand::Checkout(branch)),
        ));
    }

    fn choose_repo(&mut self, cx: &mut Cx) {
        if self.repos.len() < 2 {
            cx.info("No other Git repository to choose");
            return;
        }
        let entries = self
            .repos
            .iter()
            .map(|repo| MenuEntry { label: repo.display().to_string(), action: Action::SelectRepo(repo.clone()) })
            .collect();
        self.prompt = Some(Prompt::menu("Git repository", entries));
    }

    fn toggle_file_history(&mut self, cx: &mut Cx) {
        if self.history.file.take().is_some() {
            self.refresh_history(cx);
            return;
        }
        let Some(file) = self.last_file.clone().or_else(|| cx.active.and_then(|t| t.path.clone())) else {
            cx.info("Open a file to show its history");
            return;
        };
        if let Some(repo) = self.repo_for_file(Some(&file)) {
            self.history.repo = Some(repo.clone());
            match file.strip_prefix(repo) {
                Ok(path) => self.history.file = Some(path.to_string_lossy().into_owned()),
                Err(_) => self.history.file = None,
            }
            self.refresh_history(cx);
        } else {
            cx.info("The active file is not in a Git repository");
        }
    }

    pub(super) fn open_history_commit(&mut self, index: usize, cx: &mut Cx) {
        let Some(repo) = self.history.repo.clone() else { return };
        if let Some(commit) = self.history.commits.get(index) {
            self.open_commit(repo, commit.hash.clone(), self.history.file.clone(), cx);
        }
    }

    fn copy(&self, text: String, cx: &mut Cx) {
        cx.ctx.copy_text(text.clone());
        cx.info(format!("Copied: {text}"));
    }

    fn apply_action(&mut self, action: Action, cx: &mut Cx) {
        match action {
            Action::Status(command, row) => self.status_action(command, row, cx),
            Action::History(HistoryCommand::PreviewBranch(branch)) => self.preview_branch(branch, cx),
            Action::History(HistoryCommand::Checkout(branch)) => {
                let Some(repo) = self.history.repo.clone() else { return };
                match branches::checkout(&repo, &branch, &self.history.branches.locals) {
                    Ok(message) => {
                        cx.info(if message.is_empty() { format!("Switched to {}", branch.name) } else { message });
                        self.history.branch = None;
                        self.history.all = false;
                        self.refresh_all(cx);
                    }
                    Err(error) => cx.error(error.to_string()),
                }
            }
            Action::SelectRepo(repo) => {
                self.history.repo = Some(repo);
                self.history.file = None;
                self.history.branch = None;
                self.refresh_history(cx);
            }
            Action::Copy(text) => self.copy(text, cx),
            Action::RefreshStatus => {
                self.repos.clear();
                self.refresh_status(cx);
            }
        }
    }

    fn overlays(&mut self, cx: &mut Cx) {
        let Some(prompt) = &mut self.prompt else { return };
        let mut outcome = prompt.show(cx.ctx, cx.palette);
        for key in take_keys(cx.ctx) {
            if outcome != Outcome::Keep {
                break;
            }
            if let Some(prompt) = &mut self.prompt {
                outcome = prompt.key(&key);
            }
        }
        match outcome {
            Outcome::Keep => {}
            Outcome::Close => self.prompt = None,
            Outcome::Run(action) => {
                self.prompt = None;
                self.apply_action(action, cx);
            }
        }
    }

    fn close_tab(&mut self, key: &str, cx: &mut Cx) {
        self.visible_tab = None;
        cx.close_tab(key.to_owned());
    }

    fn tab_intents(&mut self, key: &str, intents: Vec<Intent>, cx: &mut Cx) {
        for intent in intents {
            match intent {
                Intent::Close => self.close_tab(key, cx),
                Intent::DiffJump(index) => self.diff_jump(key, index, cx),
                Intent::CompareRollback(index) => self.compare_rollback(key, Some(index), cx),
                Intent::CompareRollbackAll => self.compare_rollback_all(key),
                Intent::CompareSave => self.compare_save(key, cx),
                Intent::MergeChoice(index, choice) => self.merge_choice(key, Some(index), choice, cx),
                Intent::MergeAll(choice) => self.merge_accept_all(key, choice),
                Intent::MergeResolve => self.merge_resolve(key, cx),
                Intent::BlameCommit(rev) => {
                    if let Some(TabState::Blame(tab)) = self.tabs.get(key) {
                        self.open_commit(tab.repo.clone(), rev, None, cx);
                    }
                }
            }
        }
    }

    fn diff_jump(&mut self, key: &str, index: usize, cx: &mut Cx) {
        let Some(TabState::Diff(tab)) = self.tabs.get_mut(key) else { return };
        tab.cursor = index.min(tab.doc.lines.len().saturating_sub(1));
        match tab.doc.lines.get(tab.cursor).and_then(|line| line.jump.clone()) {
            Some(Jump::Section(section)) => tab.cursor = section,
            Some(Jump::File { path, line }) => cx.open_file(tab.repo.join(path), Some(line.saturating_sub(1))),
            None => {}
        }
    }

    fn refresh_diff(&mut self, key: &str, cx: &mut Cx) {
        let Some(TabState::Diff(tab)) = self.tabs.get(key) else { return };
        match diff::diff(&tab.repo, &tab.source) {
            Ok(Some(doc)) => {
                if let Some(TabState::Diff(tab)) = self.tabs.get_mut(key) {
                    tab.doc = doc;
                    tab.cursor = 0;
                }
            }
            Ok(None) => cx.info("No changes to show"),
            Err(error) => cx.error(error.to_string()),
        }
    }

    fn compare_rollback(&mut self, key: &str, index: Option<usize>, cx: &mut Cx) {
        let Some(TabState::Compare(tab)) = self.tabs.get_mut(key) else { return };
        let blocks = compare::blocks(&tab.base, &tab.current);
        let block = match index {
            Some(index) => blocks.get(index),
            None => compare::block_at(&blocks, tab.cursor, tab.left),
        };
        let Some(block) = block else {
            cx.info("No change under the cursor");
            return;
        };
        tab.undo.push(tab.current.clone());
        tab.current = compare::rollback(&tab.base, &tab.current, block);
        tab.dirty = true;
    }

    fn compare_rollback_all(&mut self, key: &str) {
        if let Some(TabState::Compare(tab)) = self.tabs.get_mut(key) {
            tab.undo.push(tab.current.clone());
            tab.current = compare::rollback_all(&tab.base);
            tab.dirty = true;
        }
    }

    fn compare_undo(&mut self, key: &str) {
        if let Some(TabState::Compare(tab)) = self.tabs.get_mut(key)
            && let Some(previous) = tab.undo.pop()
        {
            tab.current = previous;
            tab.dirty = true;
        }
    }

    fn compare_save(&mut self, key: &str, cx: &mut Cx) {
        let Some(TabState::Compare(tab)) = self.tabs.get_mut(key) else { return };
        let path = tab.repo.join(&tab.path);
        match std::fs::write(&path, &tab.current) {
            Ok(()) => {
                tab.dirty = false;
                cx.info(format!("{} saved", tab.path));
                self.status_dirty = true;
                self.refresh_status(cx);
            }
            Err(error) => cx.error(format!("Could not save {}: {error}", tab.path)),
        }
    }

    fn compare_jump(&mut self, key: &str, direction: i32) {
        if let Some(TabState::Compare(tab)) = self.tabs.get_mut(key) {
            let blocks = compare::blocks(&tab.base, &tab.current);
            tab.cursor = next_block_line(&blocks, tab.cursor, direction, false);
            tab.left = false;
        }
    }

    fn merge_choice(&mut self, key: &str, index: Option<usize>, choice: Choice, cx: &mut Cx) {
        let Some(TabState::Merge(tab)) = self.tabs.get_mut(key) else { return };
        let count = merge::parse(&tab.result).len();
        let index = index.unwrap_or(tab.selected).min(count.saturating_sub(1));
        match merge::resolve(&tab.result, index, choice) {
            Some(result) => {
                tab.undo.push(tab.result.clone());
                tab.result = result;
                tab.selected = index.min(count.saturating_sub(2));
            }
            None => cx.info("No conflict at or below the cursor"),
        }
    }

    fn merge_accept_all(&mut self, key: &str, choice: Choice) {
        if let Some(TabState::Merge(tab)) = self.tabs.get_mut(key) {
            tab.undo.push(tab.result.clone());
            tab.result = merge::accept_all(&tab.result, choice);
        }
    }

    fn merge_jump(&mut self, key: &str, direction: i32) {
        if let Some(TabState::Merge(tab)) = self.tabs.get_mut(key) {
            let count = merge::parse(&tab.result).len();
            if count == 0 {
                return;
            }
            tab.selected =
                if direction > 0 { (tab.selected + 1).min(count - 1) } else { tab.selected.saturating_sub(1) };
        }
    }

    fn merge_resolve(&mut self, key: &str, cx: &mut Cx) {
        let Some(TabState::Merge(tab)) = self.tabs.get_mut(key) else { return };
        let left = merge::parse(&tab.result).len();
        if left > 0 {
            cx.error(format!("{left} conflict(s) left"));
            return;
        }
        let path = tab.repo.join(&tab.path);
        if let Err(error) = std::fs::write(&path, &tab.result) {
            cx.error(format!("Could not save {}: {error}", tab.path));
            return;
        }
        match merge::mark_resolved(&tab.repo, &tab.path) {
            Ok(()) => {
                cx.info(format!("{} marked as resolved", tab.path));
                self.status_dirty = true;
                self.refresh_status(cx);
                self.tabs.remove(key);
                self.close_tab(key, cx);
            }
            Err(error) => cx.error(error.to_string()),
        }
    }

    fn blame_commit(&mut self, key: &str, cx: &mut Cx) {
        let Some(TabState::Blame(tab)) = self.tabs.get(key) else { return };
        if let Some(info) = tab.lines.get(tab.cursor).and_then(Option::as_ref) {
            self.open_commit(tab.repo.clone(), info.sha.clone(), None, cx);
        }
    }

    fn custom_leader(&mut self, sequence: &str, cx: &mut Cx) -> bool {
        let Some(key) = self.visible_tab.clone() else { return false };
        match self.tabs.get(&key) {
            Some(TabState::Compare(_)) if sequence == "r" => self.compare_rollback(&key, None, cx),
            Some(TabState::Merge(_)) => match sequence {
                "ml" => self.merge_choice(&key, None, Choice::Left, cx),
                "mr" => self.merge_choice(&key, None, Choice::Right, cx),
                "mb" => self.merge_choice(&key, None, Choice::Both, cx),
                "mL" => self.merge_accept_all(&key, Choice::Left),
                "mR" => self.merge_accept_all(&key, Choice::Right),
                "mm" => self.merge_resolve(&key, cx),
                _ => return false,
            },
            _ => return false,
        }
        true
    }

    fn tab_key(&mut self, key: &str, event: KeyEvent, cx: &mut Cx) -> bool {
        let plain = !event.mods.ctrl && !event.mods.alt && !event.mods.cmd;
        if plain && matches!(event.key, Key::Char(']' | '[')) {
            self.pending_prefix = match event.key {
                Key::Char(c) => Some(c),
                _ => None,
            };
            return matches!(self.tabs.get(key), Some(TabState::Compare(_) | TabState::Merge(_)));
        }
        if let Some(prefix) = self.pending_prefix.take() {
            let forward = prefix == ']';
            match (self.tabs.get(key), event.key) {
                (Some(TabState::Compare(_)), Key::Char('c')) => {
                    self.compare_jump(key, if forward { 1 } else { -1 });
                    return true;
                }
                (Some(TabState::Merge(_)), Key::Char('x')) => {
                    self.merge_jump(key, if forward { 1 } else { -1 });
                    return true;
                }
                _ => {}
            }
        }
        if plain && event.key == Key::Char('q') {
            self.close_tab(key, cx);
            return true;
        }
        if event.mods.cmd && event.key == Key::Char('s') && matches!(self.tabs.get(key), Some(TabState::Compare(_))) {
            self.compare_save(key, cx);
            return true;
        }
        match self.tabs.get(key) {
            Some(TabState::Diff(_)) => match event.key {
                Key::Up | Key::Down | Key::Home | Key::End | Key::PageUp | Key::PageDown => {
                    let Some(TabState::Diff(tab)) = self.tabs.get_mut(key) else { return false };
                    let last = tab.doc.lines.len().saturating_sub(1);
                    tab.cursor = match event.key {
                        Key::Up => tab.cursor.saturating_sub(1),
                        Key::Down => (tab.cursor + 1).min(last),
                        Key::Home | Key::PageUp => 0,
                        _ => last,
                    };
                    true
                }
                Key::Enter if plain => {
                    self.diff_jump(
                        key,
                        self.tabs
                            .get(key)
                            .and_then(|t| match t {
                                TabState::Diff(t) => Some(t.cursor),
                                _ => None,
                            })
                            .unwrap_or(0),
                        cx,
                    );
                    true
                }
                Key::Char('R') if plain => {
                    self.refresh_diff(key, cx);
                    true
                }
                _ => false,
            },
            Some(TabState::Compare(_)) => match event.key {
                Key::Up | Key::Down | Key::Home | Key::End | Key::PageUp | Key::PageDown => {
                    let Some(TabState::Compare(tab)) = self.tabs.get_mut(key) else { return false };
                    let last = tab.current.lines().count().max(1);
                    tab.cursor = match event.key {
                        Key::Up => tab.cursor.saturating_sub(1).max(1),
                        Key::Down => (tab.cursor + 1).min(last),
                        Key::Home | Key::PageUp => 1,
                        _ => last,
                    };
                    true
                }
                Key::Char('u') if plain => {
                    self.compare_undo(key);
                    true
                }
                Key::Char('e') if plain => {
                    if let Some(TabState::Compare(tab)) = self.tabs.get_mut(key) {
                        tab.editing = !tab.editing;
                    }
                    true
                }
                Key::Esc if plain => {
                    if let Some(TabState::Compare(tab)) = self.tabs.get_mut(key) {
                        tab.editing = false;
                    }
                    true
                }
                _ => false,
            },
            Some(TabState::Merge(_)) => match event.key {
                Key::Char('u') if plain => {
                    if let Some(TabState::Merge(tab)) = self.tabs.get_mut(key)
                        && let Some(previous) = tab.undo.pop()
                    {
                        tab.result = previous;
                    }
                    true
                }
                _ => false,
            },
            Some(TabState::Blame(_)) => match event.key {
                Key::Up | Key::Down | Key::Home | Key::End | Key::PageUp | Key::PageDown => {
                    let Some(TabState::Blame(tab)) = self.tabs.get_mut(key) else { return false };
                    let last = tab.source.lines().count().saturating_sub(1);
                    tab.cursor = match event.key {
                        Key::Up => tab.cursor.saturating_sub(1),
                        Key::Down => (tab.cursor + 1).min(last),
                        Key::Home | Key::PageUp => 0,
                        _ => last,
                    };
                    true
                }
                Key::Enter if plain => {
                    self.blame_commit(key, cx);
                    true
                }
                _ => false,
            },
            None => false,
        }
    }
}

impl Default for GitPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl Panel for GitPanel {
    fn id(&self) -> &'static str {
        "git"
    }

    fn regions(&self) -> &[Region] {
        &REGIONS
    }

    fn tick(&mut self, cx: &mut Cx) {
        if let Some(path) = cx.active.and_then(|tab| tab.path.clone()) {
            self.last_file = Some(path);
        }
        if cx.active.is_some() {
            self.visible_tab = None;
        }
        if self.watch_enabled && self.watched_roots != cx.roots {
            self.watched_roots = cx.roots.to_vec();
            self.watcher = match RepoWatcher::new(cx.roots) {
                Ok(watcher) => Some(watcher),
                Err(error) => {
                    cx.error(format!("Could not watch Git repositories: {error}"));
                    None
                }
            };
        }
        if let Some(watcher) = &self.watcher {
            let mut changed = false;
            for _repo in watcher.rx.try_iter() {
                self.status_dirty = true;
                self.history_dirty = true;
                changed = true;
            }
            if changed {
                cx.ctx.request_repaint();
            }
        }
        if self.status_dirty && self.git_open {
            self.refresh_status(cx);
        }
        if self.history_dirty && self.history_open {
            self.refresh_history(cx);
        }
        self.overlays(cx);
    }

    fn ui(&mut self, region: Region, ui: &mut Ui, cx: &mut Cx) {
        match region {
            Region::Area(Area::Git) => {
                self.git_open = true;
                if self.status_dirty {
                    self.refresh_status(cx);
                }
                status_view::draw(self, ui, cx);
            }
            Region::Strip(StripPart::Branches) => {
                self.history_open = true;
                if self.history_dirty {
                    self.refresh_history(cx);
                }
                history::draw_branches(self, ui, cx);
            }
            Region::Strip(StripPart::History) => {
                self.history_open = true;
                if self.history_dirty {
                    self.refresh_history(cx);
                }
                history::draw_history(self, ui, cx);
            }
            _ => {}
        }
    }

    fn event(&mut self, event: &Event, cx: &mut Cx) -> bool {
        match *event {
            Event::Key(key) if cx.focused && self.focused_area == Area::Git && self.git_open => {
                self.status_key(key, cx)
            }
            Event::Key(key) if cx.focused && self.focused_area == Area::GitHistory && self.history_open => {
                self.history_key(key, cx)
            }
            Event::Saved(path) => {
                self.status_dirty = true;
                self.history_dirty = true;
                self.refresh_all(cx);
                self.refresh_blame(path, cx);
                true
            }
            Event::Ex("GitBlame") | Event::Leader("ga") => {
                self.toggle_blame(cx);
                true
            }
            Event::Ex("e") => {
                let Some(key) = self.visible_tab.clone().filter(|key| key.starts_with("diff:")) else { return false };
                self.refresh_diff(&key, cx);
                true
            }
            Event::Message { topic: "git.blame", .. } => {
                self.toggle_blame(cx);
                true
            }
            Event::Leader(sequence) => self.custom_leader(sequence, cx),
            Event::Unmapped("]c") => {
                if let Some(key) = self.visible_tab.clone() {
                    self.compare_jump(&key, 1);
                    true
                } else {
                    false
                }
            }
            Event::Unmapped("[c") => {
                if let Some(key) = self.visible_tab.clone() {
                    self.compare_jump(&key, -1);
                    true
                } else {
                    false
                }
            }
            Event::Layout(effect) => {
                match effect {
                    Effect::Opened(Area::Git) | Effect::Restored(Area::Git) => {
                        self.git_open = true;
                        self.repos.clear();
                        self.refresh_status(cx);
                    }
                    Effect::Closed(Area::Git) | Effect::Hidden(Area::Git) => self.git_open = false,
                    Effect::Opened(Area::GitHistory) | Effect::Restored(Area::GitHistory) => {
                        self.history_open = true;
                        self.history_part = StripPart::History;
                        self.repos.clear();
                        self.refresh_history(cx);
                    }
                    Effect::Closed(Area::GitHistory) | Effect::Hidden(Area::GitHistory) => self.history_open = false,
                    Effect::Focused(area) => self.focused_area = area,
                    _ => {}
                }
                false
            }
            _ => false,
        }
    }

    fn tab_ui(&mut self, key: &str, ui: &mut Ui, cx: &mut Cx) {
        self.visible_tab = Some(key.to_owned());
        let intents = self.tabs.get_mut(key).map(|tab| tab.show(ui, cx)).unwrap_or_default();
        self.tab_intents(key, intents, cx);
    }

    fn tab_event(&mut self, key: &str, event: &Event, cx: &mut Cx) -> bool {
        match *event {
            Event::Key(key_event) => self.tab_key(key, key_event, cx),
            Event::Leader(sequence) => self.custom_leader(sequence, cx),
            _ => false,
        }
    }

    fn tab_closed(&mut self, key: &str, _cx: &mut Cx) {
        if self.visible_tab.as_deref() == Some(key) {
            self.visible_tab = None;
        }
    }
}

fn entry_paths(entries: &[Entry]) -> Vec<String> {
    let mut paths = Vec::new();
    for entry in entries {
        paths.push(entry.path.clone());
        if let Some(original) = &entry.orig_path {
            paths.push(original.clone());
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

fn stable_hash(value: impl Hash) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn stable_key(kind: &str, repo: &Path, detail: &str) -> String {
    format!("{kind}:{:016x}", stable_hash((repo, detail)))
}

fn diff_key(repo: &Path, source: &Source) -> String {
    let detail = match source {
        Source::Working { path } => format!("working:{path:?}"),
        Source::Staged { path } => format!("staged:{path:?}"),
        Source::Commit { rev, path } => format!("commit:{rev}:{path:?}"),
    };
    stable_key("diff", repo, &detail)
}

fn diff_title(source: &Source) -> String {
    let path = match source {
        Source::Working { path } | Source::Staged { path } => path.clone(),
        Source::Commit { path, .. } => path.clone(),
    };
    let name = path.as_deref().unwrap_or("repository");
    match source {
        Source::Working { .. } => format!("{name} (working tree diff)"),
        Source::Staged { .. } => format!("{name} (staged diff)"),
        Source::Commit { rev, .. } => format!("{name} ({rev} diff)"),
    }
}

fn next_block_line(blocks: &[compare::Block], cursor: usize, direction: i32, left: bool) -> usize {
    let line = |block: &compare::Block| if left { block.base_start + 1 } else { block.cur_start + 1 };
    if direction > 0 {
        blocks.iter().map(line).find(|line| *line > cursor).unwrap_or(cursor)
    } else {
        blocks.iter().rev().map(line).find(|line| *line < cursor).unwrap_or(cursor)
    }
}

fn load_history(repo: &Path, history: &History) -> Result<(Vec<log::Commit>, branches::BranchList), String> {
    let commits = if let Some(branch) = &history.branch {
        branch_log(repo, branch, &history.file)?
    } else {
        log::log(repo, &LogOptions { all: history.all, file: history.file.clone(), limit: None })
            .map_err(|e| e.to_string())?
    };
    let branch_list = branches::list(repo).map_err(|e| e.to_string())?;
    Ok((commits, branch_list))
}

/// `less-git::LogOptions` dal seçimi taşımadığı için Branches önizlemesi salt okunur `git log <dal>` kullanır.
fn branch_log(repo: &Path, branch: &str, file: &Option<String>) -> Result<Vec<log::Commit>, String> {
    let remotes = git_text(repo, &["remote"])?;
    let remotes: Vec<String> = remotes.lines().map(str::to_owned).collect();
    let mut args = vec![
        "log".to_owned(),
        "--topo-order".to_owned(),
        "--date=format:%Y-%m-%d".to_owned(),
        "--decorate=short".to_owned(),
        "--pretty=format:%H%x1f%h%x1f%P%x1f%an%x1f%ad%x1f%s%x1f%D".to_owned(),
        branch.to_owned(),
    ];
    if let Some(file) = file {
        args.extend(["--follow".to_owned(), "--".to_owned(), file.clone()]);
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let raw = git_text(repo, &refs)?;
    Ok(log::parse_log(&raw, &remotes, file.is_some()))
}

fn git_text(repo: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .current_dir(repo)
        .args(["-c", "core.quotepath=off", "-c", "color.ui=never"])
        .args(args)
        .env("LC_ALL", "C")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|error| format!("Could not run git: {error}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(if error.is_empty() { format!("git {} failed", args.first().unwrap_or(&"")) } else { error })
    }
}

fn take_keys(ctx: &egui::Context) -> Vec<KeyEvent> {
    let events = ctx.input(|input| input.events.clone());
    ctx.input_mut(|input| {
        input.events.retain(|event| {
            !matches!(
                event,
                egui::Event::Key { .. }
                    | egui::Event::Text(_)
                    | egui::Event::Paste(_)
                    | egui::Event::Copy
                    | egui::Event::Cut
            )
        })
    });
    crate::keys::translate(&events)
}
