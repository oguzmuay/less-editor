//! Database paneli testleri; sahte sürücü ve geçici klasörler kullanır, ağ ya da gerçek kullanıcı veritabanı yok.

use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use backend::{Driver, DriverProvider};
use less_db::protocol::{ColumnInfo, ConnectResult, QueryResult, TableInfo};
use less_input::Mods;

use super::*;
use crate::panels::Request;
use crate::panels::testing::Harness;

struct Tmp(PathBuf);

impl Tmp {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "less-b4-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct FakeDriver {
    queries: Mutex<Vec<(String, bool)>>,
    transactions: Mutex<Vec<Vec<String>>>,
    connect_error: Mutex<Option<String>>,
    primary_key: Mutex<Vec<String>>,
}

impl FakeDriver {
    fn new() -> Self {
        Self {
            queries: Mutex::new(Vec::new()),
            transactions: Mutex::new(Vec::new()),
            connect_error: Mutex::new(None),
            primary_key: Mutex::new(vec!["id".into()]),
        }
    }

    fn data() -> QueryResult {
        QueryResult {
            columns: vec!["id".into(), "name".into(), "note".into()],
            rows: vec![
                vec![Some("1".into()), Some("ann".into()), None],
                vec![Some("2".into()), Some("bob".into()), Some("x".into())],
            ],
            ..QueryResult::default()
        }
    }
}

impl Driver for FakeDriver {
    fn connect(&self, _config: &Connection) -> Result<ConnectResult, String> {
        match self.connect_error.lock().unwrap().clone() {
            Some(error) => Err(error),
            None => Ok(ConnectResult { server_version: Some("fake".into()) }),
        }
    }

    fn list_schemas(&self) -> Result<Vec<String>, String> {
        Ok(vec!["main".into()])
    }

    fn list_tables(&self, _schema: Option<&str>) -> Result<Vec<TableInfo>, String> {
        Ok(vec![TableInfo { schema: Some("main".into()), name: "users".into(), kind: "table".into() }])
    }

    fn columns(&self, _table: &str) -> Result<Vec<ColumnInfo>, String> {
        Ok(vec![
            ColumnInfo {
                name: "id".into(),
                data_type: "INTEGER".into(),
                nullable: false,
                default: None,
                primary_key: true,
            },
            ColumnInfo {
                name: "name".into(),
                data_type: "TEXT".into(),
                nullable: true,
                default: None,
                primary_key: false,
            },
        ])
    }

    fn primary_key(&self, _table: &str) -> Result<Vec<String>, String> {
        Ok(self.primary_key.lock().unwrap().clone())
    }

    fn query(&self, sql: &str, read_only: bool, _limit: Option<usize>) -> Result<QueryResult, String> {
        self.queries.lock().unwrap().push((sql.to_string(), read_only));
        if sql.contains("fail") {
            return Err("fake query error".into());
        }
        Ok(QueryResult {
            columns: vec!["answer".into()],
            rows: vec![vec![Some("42".into())]],
            notice: Some("Showing the first 500 rows".into()),
            ..QueryResult::default()
        })
    }

    fn table_data(&self, _table: &str, _limit: usize, _offset: usize) -> Result<QueryResult, String> {
        Ok(Self::data())
    }

    fn transaction(&self, statements: &[String]) -> Result<u64, String> {
        self.transactions.lock().unwrap().push(statements.to_vec());
        Ok(statements.len() as u64)
    }
}

struct FakeProvider {
    installed: AtomicBool,
    installs: AtomicUsize,
    spawns: AtomicUsize,
    driver: Arc<FakeDriver>,
    path: PathBuf,
}

impl FakeProvider {
    fn new(installed: bool, driver: Arc<FakeDriver>, path: PathBuf) -> Self {
        Self {
            installed: AtomicBool::new(installed),
            installs: AtomicUsize::new(0),
            spawns: AtomicUsize::new(0),
            driver,
            path,
        }
    }
}

impl DriverProvider for FakeProvider {
    fn find(&self, _ty: DbType) -> Option<PathBuf> {
        self.installed.load(Ordering::SeqCst).then(|| self.path.clone())
    }

    fn spawn(&self, _binary: &Path) -> Result<Arc<dyn Driver>, String> {
        self.spawns.fetch_add(1, Ordering::SeqCst);
        Ok(self.driver.clone())
    }

    fn install(&self, _ty: DbType) -> Result<PathBuf, String> {
        self.installs.fetch_add(1, Ordering::SeqCst);
        self.installed.store(true, Ordering::SeqCst);
        Ok(self.path.clone())
    }
}

struct Env {
    tmp: Tmp,
    panel: DbPanel,
    h: Harness,
    driver: Arc<FakeDriver>,
    provider: Arc<FakeProvider>,
}

fn setup(installed: bool) -> Env {
    let tmp = Tmp::new();
    let driver = Arc::new(FakeDriver::new());
    let provider = Arc::new(FakeProvider::new(installed, driver.clone(), tmp.0.join("drivers/fake")));
    let mut store = ConnectionStore::default();
    let mut connection = Connection::new("local", DbType::Sqlite);
    connection.file = Some(tmp.0.join("test.db").display().to_string());
    connection.password = Some("secret".into());
    store.upsert(connection);
    let panel = DbPanel::with_parts(
        store,
        tmp.0.join("data/datasource/connections.json"),
        tmp.0.join("data/datasource/consoles"),
        provider.clone(),
        true,
    );
    Env { tmp, panel, h: Harness::new(Vec::new()), driver, provider }
}

fn frame(env: &mut Env) {
    let ctx = env.h.ctx.clone();
    let mut output = ctx.run_ui(egui::RawInput::default(), |_ui| {
        env.panel.tick(&mut env.h.cx("db"));
    });
    output.textures_delta.clear();
}

fn until(env: &mut Env, mut done: impl FnMut(&mut Env) -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(4);
    while Instant::now() < deadline {
        frame(env);
        if done(env) {
            return true;
        }
        thread::sleep(Duration::from_millis(15));
    }
    false
}

fn panel_key(env: &mut Env, key: KeyEvent) -> bool {
    env.panel.event(&Event::Key(key), &mut env.h.cx("db"))
}

fn tab_key(env: &mut Env, key: &str, event: KeyEvent) -> bool {
    env.panel.tab_event(key, &Event::Key(event), &mut env.h.cx("db"))
}

fn approve(env: &mut Env, yes: bool) {
    env.panel.apply_popup(Outcome::Approve(yes), &mut env.h.cx("db"));
}

fn open_loaded_data(env: &mut Env) -> String {
    env.panel.open_data("local", "main.users", &mut env.h.cx("db"));
    let key = DbPanel::data_key("local", "main.users");
    assert!(until(env, |env| env.panel.tabs.get(&key).is_some_and(|tab| !tab.loading && tab.rows().len() == 2)));
    key
}

fn render_tab(env: &mut Env, key: &str) {
    let ctx = env.h.ctx.clone();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 700.0))),
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        env.panel.tab_ui(key, ui, &mut env.h.cx("db"));
    });
    output.textures_delta.clear();
}

#[test]
fn registration_and_headless_tree_render_45() {
    let registered = panels();
    assert_eq!(registered.len(), 1);
    assert_eq!(registered[0].id(), "db");
    assert_eq!(registered[0].regions(), &[Region::Area(Area::Database)]);
    let mut env = setup(true);
    env.panel.states.entry("local".into()).or_default().expanded = true;
    let ctx = env.h.ctx.clone();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(420.0, 600.0))),
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        env.panel.ui(Region::Area(Area::Database), ui, &mut env.h.cx("db"));
    });
    output.textures_delta.clear();
    panel_key(&mut env, KeyEvent::char('a'));
    frame(&mut env);
    assert!(matches!(env.panel.popup, Some(Popup::Types { .. })));
    env.panel.apply_popup(Outcome::Type(DbType::Sqlite), &mut env.h.cx("db"));
    frame(&mut env);
    assert!(matches!(env.panel.popup, Some(Popup::Form(_))));
}

#[test]
fn connection_form_validates_and_persists_46() {
    let mut env = setup(true);
    let mut form = ConnectionForm::new(DbType::Postgresql);
    assert_eq!(form.submit(&env.panel.store).unwrap_err(), "Name is required");
    form.name = "local".into();
    assert!(form.submit(&env.panel.store).unwrap_err().contains("already exists"));
    form.name = "pg".into();
    form.port = "no".into();
    assert!(form.submit(&env.panel.store).unwrap_err().contains("Port must be a number"));
    form.port = "5433".into();
    form.user = "user".into();
    form.password = "new-secret".into();
    env.panel.save_form(form, &mut env.h.cx("db"));
    let saved = env.panel.store.get("pg").unwrap();
    assert_eq!(saved.port, Some(5433));
    assert_eq!(saved.password.as_deref(), Some("new-secret"));
    assert!(env.tmp.0.join("data/datasource/connections.json").exists());
    assert!(env.h.requests.iter().all(|request| match request {
        Request::Notify { text, .. } => !text.contains("new-secret"),
        _ => true,
    }));
    let sqlite = ConnectionForm::new(DbType::Sqlite);
    assert_eq!(sqlite.submit(&env.panel.store).unwrap_err(), "Name is required");
}

#[test]
fn tree_opens_console_data_and_structure_45_47() {
    let mut env = setup(true);
    assert!(panel_key(&mut env, KeyEvent::plain(Key::Enter)));
    let console = DbPanel::console_tab_key("local");
    assert!(env.h.requests.contains(&Request::OpenTab {
        owner: "db",
        key: console.clone(),
        title: "local · Console".into()
    }));
    assert!(env.tmp.0.join("data/datasource/consoles/local.sql").exists());
    assert!(until(&mut env, |env| env.panel.states.get("local").is_some_and(|s| s.loaded)));
    assert_eq!(env.panel.rows().len(), 2);
    assert!(panel_key(&mut env, KeyEvent::plain(Key::Down)));
    assert!(panel_key(&mut env, KeyEvent::plain(Key::Enter)));
    let data = DbPanel::data_key("local", "main.users");
    assert!(until(&mut env, |env| env.panel.tabs.get(&data).is_some_and(|tab| tab.rows().len() == 2)));
    assert!(panel_key(&mut env, KeyEvent::char('e')));
    let structure = DbPanel::structure_key("local", "main.users");
    assert!(until(&mut env, |env| { env.panel.tabs.get(&structure).is_some_and(|tab| tab.rows().len() == 2) }));
    assert!(panel_key(&mut env, KeyEvent::char('q')));
    assert_eq!(env.h.requests.last(), Some(&Request::Layout(LayoutOp::Close(Area::Database))));
}

#[test]
fn delete_connection_is_confirmed_and_keeps_console_45() {
    let mut env = setup(true);
    env.panel.open_console("local", &mut env.h.cx("db"));
    let console_path = env.tmp.0.join("data/datasource/consoles/local.sql");
    panel_key(&mut env, KeyEvent::char('d'));
    approve(&mut env, false);
    assert!(env.panel.store.get("local").is_some());
    panel_key(&mut env, KeyEvent::char('d'));
    approve(&mut env, true);
    assert!(env.panel.store.get("local").is_none());
    assert!(console_path.exists());
    assert_eq!(ConnectionStore::load_from(&env.panel.store_file).connections.len(), 0);
}

#[test]
fn console_autosaves_and_runs_block_or_selection_47_89() {
    let mut env = setup(true);
    env.panel.open_console("local", &mut env.h.cx("db"));
    let key = DbPanel::console_tab_key("local");
    let Some(DbTab { kind: TabKind::Console(console), .. }) = env.panel.tabs.get_mut(&key) else {
        panic!("console expected")
    };
    console.replace_text("select 1;\nselect 2;\n\nselect 3;".into());
    console.set_cursor_char(12);
    assert!(tab_key(&mut env, &key, KeyEvent::new(Key::Enter, Mods::CMD)));
    assert!(until(&mut env, |env| env.driver.queries.lock().unwrap().len() == 1));
    assert_eq!(env.driver.queries.lock().unwrap()[0].0, "select 1;\nselect 2;");
    let Some(DbTab { kind: TabKind::Console(console), .. }) = env.panel.tabs.get_mut(&key) else { panic!() };
    let blank = console.text.find("\n\n").unwrap() + 1;
    console.set_selection(blank..blank);
    assert!(console.query_text().is_err(), "blank line has no statement");
    console.set_selection(0..9);
    tab_key(&mut env, &key, KeyEvent::new(Key::Enter, Mods::CMD));
    assert!(until(&mut env, |env| env.driver.queries.lock().unwrap().len() == 2));
    assert_eq!(env.driver.queries.lock().unwrap()[1].0, "select 1;");
    assert!(until(&mut env, |env| {
        fs::read_to_string(env.tmp.0.join("data/datasource/consoles/local.sql"))
            .is_ok_and(|text| text.contains("select 3"))
    }));
    let Some(DbTab { kind: TabKind::Console(console), .. }) = env.panel.tabs.get_mut(&key) else { panic!() };
    assert!(!console.dirty, "autosave leaves no unsaved state");
}

#[test]
fn read_is_read_only_and_write_needs_approval_68() {
    let mut env = setup(true);
    env.panel.open_console("local", &mut env.h.cx("db"));
    let key = DbPanel::console_tab_key("local");
    let Some(DbTab { kind: TabKind::Console(console), .. }) = env.panel.tabs.get_mut(&key) else { panic!() };
    console.replace_text("update users set name = 'x'".into());
    console.set_cursor_char(0);
    tab_key(&mut env, &key, KeyEvent::new(Key::Enter, Mods::CMD));
    assert!(matches!(env.panel.popup, Some(Popup::Approval { kind: ApprovalKind::Query { .. }, .. })));
    assert!(env.driver.queries.lock().unwrap().is_empty(), "no driver write before approval");
    approve(&mut env, false);
    thread::sleep(Duration::from_millis(30));
    frame(&mut env);
    assert!(env.driver.queries.lock().unwrap().is_empty(), "cancel does not run");
    tab_key(&mut env, &key, KeyEvent::new(Key::Enter, Mods::CMD));
    approve(&mut env, true);
    assert!(until(&mut env, |env| env.driver.queries.lock().unwrap().len() == 1));
    assert!(!env.driver.queries.lock().unwrap()[0].1);
    let result = DbPanel::result_key("local");
    assert!(until(&mut env, |env| env
        .panel
        .tabs
        .get(&result)
        .is_some_and(|tab| !tab.loading && tab.rows().len() == 1)));
    let Some(DbTab { kind: TabKind::Console(console), .. }) = env.panel.tabs.get_mut(&key) else { panic!() };
    console.replace_text("select * from users".into());
    console.set_cursor_char(0);
    tab_key(&mut env, &key, KeyEvent::new(Key::Enter, Mods::CMD));
    assert!(until(&mut env, |env| env.driver.queries.lock().unwrap().len() == 2));
    assert!(env.driver.queries.lock().unwrap()[1].1, "read uses read-only session");
    assert!(env.panel.tabs.get(&result).is_some_and(|tab| tab.rows().len() == 1));
}

#[test]
fn missing_driver_waits_for_install_approval_45() {
    let mut env = setup(false);
    env.panel.start("local", Operation::Load, &mut env.h.cx("db"));
    frame(&mut env);
    assert_eq!(env.provider.installs.load(Ordering::SeqCst), 0);
    assert_eq!(env.provider.spawns.load(Ordering::SeqCst), 0);
    assert!(matches!(
        env.panel.popup,
        Some(Popup::Approval { kind: ApprovalKind::Install { ty: DbType::Sqlite }, .. })
    ));
    approve(&mut env, false);
    assert_eq!(env.provider.installs.load(Ordering::SeqCst), 0);
    env.panel.start("local", Operation::Load, &mut env.h.cx("db"));
    frame(&mut env);
    approve(&mut env, true);
    assert!(until(&mut env, |env| env.panel.states.get("local").is_some_and(|s| s.loaded)));
    assert_eq!(env.provider.installs.load(Ordering::SeqCst,), 1);
    assert_eq!(env.provider.spawns.load(Ordering::SeqCst), 1);
}

#[test]
fn data_edit_preview_and_commit_transaction_88() {
    let mut env = setup(true);
    let key = open_loaded_data(&mut env);
    assert!(tab_key(&mut env, &key, KeyEvent::char('e')));
    assert!(matches!(env.panel.popup, Some(Popup::CellEdit { row: 0, col: 0, .. })));
    env.panel.apply_popup(Outcome::CellApply("7".into()), &mut env.h.cx("db"));
    assert_eq!(env.panel.tabs.get(&key).unwrap().pending(), 1);
    assert!(tab_key(&mut env, &key, KeyEvent::new(Key::Char('d'), Mods::CMD)));
    assert_eq!(env.panel.tabs.get(&key).unwrap().pending(), 2);
    assert!(tab_key(&mut env, &key, KeyEvent::char('u')));
    assert_eq!(env.panel.tabs.get(&key).unwrap().pending(), 1, "undo removes cloned row");
    assert!(tab_key(&mut env, &key, KeyEvent::new(Key::Char('s'), Mods::CMD)));
    assert!(until(&mut env, |env| matches!(
        env.panel.popup,
        Some(Popup::Approval { kind: ApprovalKind::Commit { .. }, .. })
    )));
    assert!(env.driver.transactions.lock().unwrap().is_empty());
    let Some(Popup::Approval { preview, .. }) = &env.panel.popup else { panic!() };
    assert!(preview.contains("BEGIN;"));
    assert!(preview.contains("UPDATE \"main.users\" SET \"id\" = '7' WHERE \"id\" = '1';"));
    approve(&mut env, false);
    assert!(env.driver.transactions.lock().unwrap().is_empty());
    tab_key(&mut env, &key, KeyEvent::new(Key::Char('s'), Mods::CMD));
    assert!(until(&mut env, |env| matches!(env.panel.popup, Some(Popup::Approval { .. }))));
    approve(&mut env, true);
    assert!(until(&mut env, |env| env.driver.transactions.lock().unwrap().len() == 1));
    assert_eq!(env.driver.transactions.lock().unwrap()[0].len(), 1);
    assert!(until(&mut env, |env| env.panel.tabs.get(&key).is_some_and(|tab| tab.pending() == 0 && !tab.loading)));
    render_tab(&mut env, &key);
}

#[test]
fn ex_write_uses_same_commit_approval_88() {
    let mut env = setup(true);
    let key = open_loaded_data(&mut env);
    env.panel.grid_action(&key, GridAction::SetNull, &mut env.h.cx("db"));
    for event in [KeyEvent::char(':'), KeyEvent::char('w'), KeyEvent::plain(Key::Enter)] {
        tab_key(&mut env, &key, event);
    }
    assert!(until(&mut env, |env| matches!(
        env.panel.popup,
        Some(Popup::Approval { kind: ApprovalKind::Commit { .. }, .. })
    )));
    assert!(env.driver.transactions.lock().unwrap().is_empty());
}

#[test]
fn no_primary_key_blocks_update_but_allows_insert_88() {
    let mut env = setup(true);
    env.driver.primary_key.lock().unwrap().clear();
    let key = open_loaded_data(&mut env);
    env.panel.grid_action(&key, GridAction::Edit, &mut env.h.cx("db"));
    env.panel.apply_popup(Outcome::CellApply("9".into()), &mut env.h.cx("db"));
    env.panel.prepare_commit(&key, &mut env.h.cx("db"));
    assert!(until(&mut env, |env| env.panel.tabs.get(&key).unwrap().error.is_some()));
    assert!(!matches!(env.panel.popup, Some(Popup::Approval { .. })));
    env.panel.grid_action(&key, GridAction::CloneRow, &mut env.h.cx("db"));
    env.panel.grid_action(&key, GridAction::SetNull, &mut env.h.cx("db"));
    let Some(DbTab { kind: TabKind::Data(data), .. }) = env.panel.tabs.get_mut(&key) else { panic!() };
    data.edit.as_mut().unwrap().revert_at(0, 0);
    env.panel.prepare_commit(&key, &mut env.h.cx("db"));
    assert!(until(&mut env, |env| matches!(
        env.panel.popup,
        Some(Popup::Approval { kind: ApprovalKind::Commit { .. }, .. })
    )));
}

#[test]
fn driver_errors_redact_password_and_url_67() {
    let mut env = setup(true);
    env.panel.store.connections[0].url = Some("url-secret".into());
    *env.driver.connect_error.lock().unwrap() = Some("bad secret from url-secret".into());
    env.panel.open_data("local", "main.users", &mut env.h.cx("db"));
    let key = DbPanel::data_key("local", "main.users");
    assert!(until(&mut env, |env| env.panel.tabs.get(&key).is_some_and(|tab| tab.error.is_some())));
    let error = env.panel.tabs.get(&key).unwrap().error.as_deref().unwrap();
    assert_eq!(error, "bad [redacted] from [redacted]");
    assert!(env.h.requests.iter().all(|request| match request {
        Request::Notify { text, .. } => !text.contains("secret"),
        _ => true,
    }));
}

#[test]
fn headless_grid_and_structure_render_50() {
    let mut env = setup(true);
    let data = open_loaded_data(&mut env);
    render_tab(&mut env, &data);
    env.panel.open_structure("local", "main.users", &mut env.h.cx("db"));
    let structure = DbPanel::structure_key("local", "main.users");
    assert!(until(&mut env, |env| env.panel.tabs.get(&structure).is_some_and(|tab| !tab.loading)));
    render_tab(&mut env, &structure);
    env.panel.open_console("local", &mut env.h.cx("db"));
    render_tab(&mut env, &DbPanel::console_tab_key("local"));
}

#[test]
fn space_d_runs_console_in_vim_normal_mode_47() {
    let mut env = setup(true);
    env.panel.open_console("local", &mut env.h.cx("db"));
    let key = DbPanel::console_tab_key("local");
    let Some(DbTab { kind: TabKind::Console(console), .. }) = env.panel.tabs.get_mut(&key) else { panic!() };
    console.replace_text("select 1".into());
    console.set_cursor_char(0);
    assert!(tab_key(&mut env, &key, KeyEvent::char(' ')));
    assert!(tab_key(&mut env, &key, KeyEvent::char('D')));
    assert!(until(&mut env, |env| env.driver.queries.lock().unwrap().len() == 1));
}

#[test]
fn close_with_pending_edits_needs_discard_approval_88() {
    let mut env = setup(true);
    let key = open_loaded_data(&mut env);
    env.panel.grid_action(&key, GridAction::SetNull, &mut env.h.cx("db"));
    tab_key(&mut env, &key, KeyEvent::new(Key::Char('w'), Mods::CMD));
    assert!(matches!(env.panel.popup, Some(Popup::Approval { kind: ApprovalKind::CloseTab { .. }, .. })));
    approve(&mut env, false);
    assert!(env.panel.tabs.contains_key(&key));
    tab_key(&mut env, &key, KeyEvent::new(Key::Char('w'), Mods::CMD));
    approve(&mut env, true);
    assert!(!env.panel.tabs.contains_key(&key));
    assert!(env.h.requests.contains(&Request::CloseTab { owner: "db", key: key.clone() }));
}

#[test]
fn quit_requires_discard_for_pending_edits_88() {
    let mut env = setup(true);
    let key = open_loaded_data(&mut env);
    env.panel.grid_action(&key, GridAction::SetNull, &mut env.h.cx("db"));
    assert!(!env.panel.can_quit(&mut env.h.cx("db")));
    approve(&mut env, true);
    assert!(env.h.requests.contains(&Request::Quit));
    assert!(env.panel.can_quit(&mut env.h.cx("db")));
}
