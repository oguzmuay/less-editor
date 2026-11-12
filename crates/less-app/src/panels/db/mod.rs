//! Database paneli (B4): sağ sütunda bağlantı ağacı (#45), bağlantı formları (#46), kalıcı konsollar (#47 #89),
//! asenkron sürücü işleri, sonuç/veri/yapı tabloları (#50) ve açık onaylı sorgu/transaction yazma (#68 #88).

mod backend;
mod console;
mod grid_view;
mod popup;
mod tabs;
mod tree;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use backend::{Backend, DriverProvider, Operation, Output, RealProvider, Start};
use console::Console;
use eframe::egui::{self, Align2, Rect, Sense, Ui, pos2, vec2};
use grid_view::GridAction;
use less_db::connections::{data_dir, store_path};
use less_db::types::transaction_sql;
use less_db::{Connection, ConnectionStore, DbType, classifier};
use less_input::{EditMode, Key, KeyEvent};
use popup::{ApprovalKind, ConnectionForm, Outcome, Popup};
use tabs::{DbTab, Grid, TabKind};
use tree::{Action, ConnectionState, Row, RowKind};

use super::{Area, Cx, Effect, Event, LayoutOp, Panel, Region};
use crate::fonts::icon;
use crate::widgets::{color, ui_font};

const REGIONS: [Region; 1] = [Region::Area(Area::Database)];
const ROW_H: f32 = 25.0;
const HEAD_H: f32 = 30.0;

/// Bu klasörün panelleri; host `panels::all()` ile toplar.
pub fn panels() -> Vec<Box<dyn Panel>> {
    vec![Box::new(DbPanel::new())]
}

/// Database sağ sütun paneli.
pub struct DbPanel {
    store: ConnectionStore,
    store_file: PathBuf,
    consoles_dir: PathBuf,
    loaded: bool,
    states: HashMap<String, ConnectionState>,
    cursor: usize,
    tabs: HashMap<String, DbTab>,
    visible_tab: Option<String>,
    backend: Backend,
    popup: Option<Popup>,
    install_wait: HashMap<DbType, Vec<(String, Operation)>>,
    installing: HashSet<DbType>,
    space_console: Option<String>,
    command: Option<String>,
    quit_approved: bool,
}

impl DbPanel {
    pub fn new() -> Self {
        Self::with_parts(
            ConnectionStore::default(),
            store_path(),
            data_dir().join("datasource/consoles"),
            Arc::new(RealProvider),
            false,
        )
    }

    /// Kayıt/konsol konumları ve sürücü sağlayıcısı verilebilen kurucu (testler gerçek kullanıcı dosyasına yazmaz).
    pub(crate) fn with_parts(
        store: ConnectionStore,
        store_file: PathBuf,
        consoles_dir: PathBuf,
        provider: Arc<dyn DriverProvider>,
        loaded: bool,
    ) -> Self {
        Self {
            store,
            store_file,
            consoles_dir,
            loaded,
            states: HashMap::new(),
            cursor: 0,
            tabs: HashMap::new(),
            visible_tab: None,
            backend: Backend::new(provider),
            popup: None,
            install_wait: HashMap::new(),
            installing: HashSet::new(),
            space_console: None,
            command: None,
            quit_approved: false,
        }
    }

    fn ensure_loaded(&mut self) {
        if !self.loaded {
            self.store = ConnectionStore::load_from(&self.store_file);
            self.loaded = true;
            self.cursor = 0;
        }
    }

    fn connection(&self, name: &str) -> Option<&Connection> {
        self.store.get(name)
    }

    fn rows(&self) -> Vec<Row> {
        tree::rows(&self.store.connections, &self.states)
    }

    fn console_tab_key(name: &str) -> String {
        format!("console:{name}")
    }

    fn result_key(name: &str) -> String {
        format!("result:{name}")
    }

    fn data_key(name: &str, table: &str) -> String {
        format!("data:{name}:{table}")
    }

    fn structure_key(name: &str, table: &str) -> String {
        format!("structure:{name}:{table}")
    }

    /// Sürücü/veritabanı hatası kullanıcıya gösterilmeden önce kayıttaki parola ve ham URL maskelenir.
    fn safe_error(connection: Option<&Connection>, error: impl Into<String>) -> String {
        let mut text = error.into();
        if let Some(connection) = connection {
            let mut secrets: Vec<&str> = [&connection.password, &connection.url]
                .into_iter()
                .flatten()
                .map(String::as_str)
                .filter(|s| !s.is_empty())
                .collect();
            secrets.sort_by_key(|secret| std::cmp::Reverse(secret.len()));
            for secret in secrets {
                text = text.replace(secret, "[redacted]");
            }
        }
        text
    }

    fn open_console(&mut self, name: &str, cx: &mut Cx) {
        let Some(connection) = self.connection(name).cloned() else { return };
        let key = Self::console_tab_key(name);
        if !self.tabs.contains_key(&key) {
            match Console::open(&self.consoles_dir, name, connection.ty, cx.edit_mode == EditMode::Modeless) {
                Ok(console) => {
                    self.tabs.insert(key.clone(), DbTab::console(name, console));
                }
                Err(e) => {
                    cx.error(e);
                    return;
                }
            }
        }
        cx.open_tab(key, format!("{name} · Console"));
        let state = self.states.entry(name.to_string()).or_default();
        state.expanded = true;
        if !state.loaded && !state.loading {
            self.start(name, Operation::Load, cx);
        }
    }

    fn open_data(&mut self, name: &str, table: &str, cx: &mut Cx) {
        let Some(connection) = self.connection(name).cloned() else { return };
        let key = Self::data_key(name, table);
        self.tabs.entry(key.clone()).or_insert_with(|| DbTab::data(&connection, table.to_string()));
        cx.open_tab(key.clone(), format!("{name} · {table}"));
        if self.tabs.get(&key).is_some_and(|tab| tab.pending() > 0) {
            return;
        }
        if let Some(tab) = self.tabs.get_mut(&key) {
            tab.loading = true;
            tab.error = None;
        }
        self.start(name, Operation::Data { key, table: table.to_string() }, cx);
    }

    fn open_structure(&mut self, name: &str, table: &str, cx: &mut Cx) {
        if self.connection(name).is_none() {
            return;
        }
        let key = Self::structure_key(name, table);
        self.tabs.entry(key.clone()).or_insert_with(|| DbTab {
            connection: name.to_string(),
            kind: TabKind::Structure(Grid::default()),
            loading: true,
            error: None,
        });
        if let Some(tab) = self.tabs.get_mut(&key) {
            tab.loading = true;
            tab.error = None;
        }
        cx.open_tab(key.clone(), format!("{name} · {table} · Structure"));
        self.start(name, Operation::Structure { key, table: table.to_string() }, cx);
    }

    fn ensure_result(&mut self, name: &str, cx: &mut Cx) -> String {
        let key = Self::result_key(name);
        self.tabs.entry(key.clone()).or_insert_with(|| DbTab::result(name));
        cx.open_tab(key.clone(), format!("{name} · Result"));
        key
    }

    /// Sürücü işini başlatır; eksik sürücüde ağ isteği yapılmadan Install/Cancel kuyruğuna girer.
    fn start(&mut self, name: &str, operation: Operation, cx: &mut Cx) {
        let Some(connection) = self.connection(name).cloned() else {
            cx.error("The data source no longer exists");
            return;
        };
        if let Operation::Load = operation {
            let state = self.states.entry(name.to_string()).or_default();
            state.loading = true;
            state.error = None;
        }
        if let Some(tab) = operation.key().and_then(|key| self.tabs.get_mut(key)) {
            tab.loading = true;
            tab.error = None;
        }
        match self.backend.start(name, &connection, operation.clone()) {
            Start::Started | Start::AlreadyRunning => {}
            Start::MissingDriver => {
                if let Operation::Load = operation
                    && let Some(state) = self.states.get_mut(name)
                {
                    state.loading = false;
                }
                if let Some(tab) = operation.key().and_then(|key| self.tabs.get_mut(key)) {
                    tab.loading = false;
                }
                let waiting = self.install_wait.entry(connection.ty).or_default();
                if !waiting.iter().any(|(n, op)| n == name && *op == operation) {
                    waiting.push((name.to_string(), operation));
                }
            }
        }
    }

    fn next_install_prompt(&mut self) {
        if self.popup.is_some() {
            return;
        }
        let Some(&ty) = self
            .install_wait
            .iter()
            .find(|(ty, jobs)| !self.installing.contains(ty) && !jobs.is_empty())
            .map(|(ty, _)| ty)
        else {
            return;
        };
        let title = format!("Install {} driver?", ty.display_name());
        let preview = format!(
            "{} is not installed. Install downloads the release driver and verifies its SHA-256 checksum before use.",
            ty.driver_binary()
        );
        self.popup = Some(Popup::approval(ApprovalKind::Install { ty }, title, preview, "Install"));
    }

    fn run_console(&mut self, key: &str, cx: &mut Cx) {
        let Some(tab) = self.tabs.get(key) else { return };
        let TabKind::Console(console) = &tab.kind else { return };
        let name = tab.connection.clone();
        let query = match console.query_text() {
            Ok(query) if !query.trim().is_empty() => query,
            Ok(_) => {
                cx.info("Nothing to run");
                return;
            }
            Err(e) => {
                cx.info(e);
                return;
            }
        };
        let Some(connection) = self.connection(&name).cloned() else {
            cx.error("This console is not connected to a data source");
            return;
        };
        let result = self.ensure_result(&name, cx);
        let operation = Operation::Query { key: result, sql: query.clone(), read_only: true };
        match classifier::classify(connection.ty.kind(), &query) {
            classifier::QueryKind::Read => self.start(&name, operation, cx),
            classifier::QueryKind::NeedsApproval => {
                self.popup = Some(Popup::approval(
                    ApprovalKind::Query { connection: name.clone(), sql: query.clone() },
                    format!("Run a writing or uncertain query on {name}?"),
                    query,
                    "Run query",
                ));
            }
        }
    }

    fn save_console(&mut self, key: &str, cx: &mut Cx) {
        if let Some(DbTab { kind: TabKind::Console(console), .. }) = self.tabs.get_mut(key) {
            match console.save() {
                Ok(()) => cx.info("Query console saved"),
                Err(e) => cx.error(e),
            }
        }
    }

    fn prepare_commit(&mut self, key: &str, cx: &mut Cx) {
        let Some(tab) = self.tabs.get(key) else { return };
        let TabKind::Data(data) = &tab.kind else { return };
        if data.pending() == 0 {
            cx.info("No pending database changes");
            return;
        }
        if data.edit.is_none() {
            cx.error("Editing is available only for SQL tables");
            return;
        }
        let name = tab.connection.clone();
        self.start(&name, Operation::PrimaryKey { key: key.to_string(), table: data.table.clone() }, cx);
    }

    /// Birincil anahtar commit anında okunduktan sonra SQL üretir; yazma yalnız sonraki onayla başlar.
    fn show_commit_approval(&mut self, key: &str, pk: Vec<String>, cx: &mut Cx) {
        let Some(tab) = self.tabs.get_mut(key) else { return };
        tab.loading = false;
        let TabKind::Data(data) = &tab.kind else { return };
        let Some(edit) = &data.edit else { return };
        match edit.statements(&pk) {
            Ok(statements) if statements.is_empty() => cx.info("No pending database changes"),
            Ok(statements) => {
                let preview = transaction_sql(edit.db_type, &statements);
                self.popup = Some(Popup::approval(
                    ApprovalKind::Commit { key: key.to_string(), statements },
                    format!("Commit changes to {}?", data.table),
                    preview,
                    "Commit transaction",
                ));
            }
            Err(e) => {
                tab.error = Some(e.clone());
                cx.error(e);
            }
        }
    }

    fn grid_action(&mut self, key: &str, action: GridAction, cx: &mut Cx) {
        let Some(tab) = self.tabs.get_mut(key) else { return };
        match action {
            GridAction::Select(row, col) => {
                if let Some(grid) = tab.grid_mut() {
                    grid.cursor = (row, col);
                }
            }
            GridAction::ViewAt(row, col) => {
                if let Some(grid) = tab.grid_mut() {
                    grid.cursor = (row, col);
                }
                let column = tab.grid().and_then(|g| g.columns.get(col)).cloned().unwrap_or_else(|| "Value".into());
                let value = tab.value(row, col).unwrap_or_else(|| "NULL".into());
                self.popup = Some(Popup::Value { title: column, value });
            }
            GridAction::View => {
                let (row, col) = tab.grid().map(|g| g.cursor).unwrap_or_default();
                let Some(column) = tab.grid().and_then(|g| g.columns.get(col)).cloned() else { return };
                if row >= tab.rows().len() {
                    return;
                }
                let value = tab.value(row, col).unwrap_or_else(|| "NULL".into());
                self.popup = Some(Popup::Value { title: column, value });
            }
            GridAction::Copy => {
                let (row, col) = tab.grid().map(|g| g.cursor).unwrap_or_default();
                cx.ctx.copy_text(tab.value(row, col).unwrap_or_else(|| "NULL".into()));
                cx.info("Cell value copied");
            }
            GridAction::Edit => {
                let TabKind::Data(data) = &tab.kind else { return };
                if data.edit.is_none() {
                    cx.error("Editing is available only for SQL tables");
                    return;
                }
                let (row, col) = data.grid.cursor;
                if row >= data.rows().len() || col >= data.grid.columns.len() {
                    return;
                }
                let column = data.grid.columns.get(col).cloned().unwrap_or_else(|| "Value".into());
                let value = data.value(row, col).unwrap_or_default();
                self.popup = Some(Popup::CellEdit { key: key.to_string(), row, col, title: column, value });
            }
            GridAction::SetNull => {
                if let TabKind::Data(data) = &mut tab.kind
                    && let Some(edit) = &mut data.edit
                {
                    let (row, col) = data.grid.cursor;
                    edit.set_null(row, col);
                }
            }
            GridAction::CloneRow => {
                if let TabKind::Data(data) = &mut tab.kind
                    && let Some(edit) = &mut data.edit
                {
                    let row = data.grid.cursor.0;
                    if let Some(new_row) = edit.copy_row(row) {
                        data.grid.cursor.0 = new_row;
                    }
                }
            }
            GridAction::Commit => self.prepare_commit(key, cx),
            GridAction::Refresh => self.refresh_tab(key, cx),
        }
    }

    fn refresh_tab(&mut self, key: &str, cx: &mut Cx) {
        let Some(tab) = self.tabs.get(key) else { return };
        let name = tab.connection.clone();
        match &tab.kind {
            TabKind::Data(data) => {
                if data.pending() > 0 {
                    cx.error("Commit or discard pending changes before refreshing");
                    return;
                }
                self.start(&name, Operation::Data { key: key.to_string(), table: data.table.clone() }, cx);
            }
            TabKind::Structure(_) => {
                let table = key.strip_prefix(&format!("structure:{name}:")).unwrap_or_default().to_string();
                self.start(&name, Operation::Structure { key: key.to_string(), table }, cx);
            }
            TabKind::Result(_) | TabKind::Console(_) => {}
        }
    }

    fn close_custom_tab(&mut self, key: &str, cx: &mut Cx) {
        if let Some(mut tab) = self.tabs.remove(key)
            && let TabKind::Console(console) = &mut tab.kind
            && let Err(error) = console.save()
        {
            cx.error(error);
        }
        if self.visible_tab.as_deref() == Some(key) {
            self.visible_tab = None;
        }
        cx.close_tab(key.to_string());
    }

    fn request_close_tab(&mut self, key: &str, cx: &mut Cx) {
        if self.tabs.get(key).is_some_and(|tab| tab.pending() > 0) {
            self.popup = Some(Popup::approval(
                ApprovalKind::CloseTab { key: key.to_string() },
                "Discard pending database changes?",
                "The edits have not been sent to the database.",
                "Discard and close",
            ));
        } else {
            self.close_custom_tab(key, cx);
        }
    }

    fn close_connection_tabs(&mut self, name: &str, cx: &mut Cx) {
        let keys: Vec<String> =
            self.tabs.iter().filter(|(_, tab)| tab.connection == name).map(|(key, _)| key.clone()).collect();
        for key in keys {
            if let Some(DbTab { kind: TabKind::Console(console), .. }) = self.tabs.get_mut(&key) {
                let _ = console.save();
            }
            self.tabs.remove(&key);
            cx.close_tab(key);
        }
    }

    fn save_form(&mut self, mut form: ConnectionForm, cx: &mut Cx) {
        let (old, connection) = match form.submit(&self.store) {
            Ok(value) => value,
            Err(e) => {
                form.error = Some(e);
                self.popup = Some(Popup::Form(form));
                return;
            }
        };
        let mut next = self.store.clone();
        if let Some(old_name) = &old
            && old_name != &connection.name
        {
            next.remove(old_name);
        }
        next.upsert(connection.clone());
        if let Err(e) = next.save_to(&self.store_file) {
            form.error = Some(format!("Could not save data sources: {e}"));
            self.popup = Some(Popup::Form(form));
            return;
        }
        if let Some(old_name) = old {
            self.backend.remove(&old_name);
            self.states.remove(&old_name);
            if old_name != connection.name {
                self.close_connection_tabs(&old_name, cx);
            }
        } else {
            self.backend.remove(&connection.name);
        }
        self.store = next;
        self.states.entry(connection.name.clone()).or_default().expanded = true;
        self.popup = None;
        cx.info(format!("{} connection saved", connection.name));
        self.start(&connection.name, Operation::Load, cx);
    }

    fn delete_connection(&mut self, name: &str, cx: &mut Cx) {
        let mut next = self.store.clone();
        if !next.remove(name) {
            self.popup = None;
            return;
        }
        if let Err(e) = next.save_to(&self.store_file) {
            cx.error(format!("Could not save data sources: {e}"));
            return;
        }
        self.store = next;
        self.states.remove(name);
        self.backend.remove(name);
        self.close_connection_tabs(name, cx);
        self.cursor = self.cursor.min(self.store.connections.len().saturating_sub(1));
        self.popup = None;
        cx.info("Connection deleted; its query console was kept");
    }

    fn apply_popup(&mut self, outcome: Outcome, cx: &mut Cx) {
        if outcome == Outcome::Keep {
            return;
        }
        if outcome == Outcome::Close {
            if let Some(Popup::Approval { kind: ApprovalKind::Install { ty }, .. }) = self.popup.take() {
                self.install_wait.remove(&ty);
                cx.info("Driver installation cancelled");
            } else {
                self.popup = None;
            }
            return;
        }
        match outcome {
            Outcome::Type(ty) => self.popup = Some(Popup::Form(ConnectionForm::new(ty))),
            Outcome::Form => {
                if let Some(Popup::Form(form)) = self.popup.take() {
                    self.save_form(form, cx);
                }
            }
            Outcome::Action(action) => {
                self.popup = None;
                self.tree_action(action, cx);
            }
            Outcome::CellApply(value) => {
                if let Some(Popup::CellEdit { key, row, col, .. }) = self.popup.take()
                    && let Some(DbTab { kind: TabKind::Data(data), .. }) = self.tabs.get_mut(&key)
                    && let Some(edit) = &mut data.edit
                {
                    edit.set_cell(row, col, Some(value));
                }
            }
            Outcome::Approve(approved) => {
                let Some(Popup::Approval { kind, .. }) = self.popup.take() else { return };
                match kind {
                    ApprovalKind::Install { ty } => {
                        if !approved {
                            self.install_wait.remove(&ty);
                            cx.info("Driver installation cancelled");
                            return;
                        }
                        let Some((name, operation)) = self.install_wait.get(&ty).and_then(|jobs| jobs.first()).cloned()
                        else {
                            return;
                        };
                        let Some(connection) = self.connection(&name).cloned() else { return };
                        self.installing.insert(ty);
                        self.backend.install(&name, &connection, operation);
                        cx.info(format!("Installing {} driver…", ty.display_name()));
                    }
                    ApprovalKind::Delete { name } if approved => self.delete_connection(&name, cx),
                    ApprovalKind::Query { connection, sql } if approved => {
                        let key = self.ensure_result(&connection, cx);
                        self.start(&connection, Operation::Query { key, sql, read_only: false }, cx);
                    }
                    ApprovalKind::Commit { key, statements } if approved => {
                        let Some(tab) = self.tabs.get(&key) else { return };
                        let name = tab.connection.clone();
                        let TabKind::Data(data) = &tab.kind else { return };
                        self.start(&name, Operation::Commit { key, table: data.table.clone(), statements }, cx);
                    }
                    ApprovalKind::CloseTab { key } if approved => self.close_custom_tab(&key, cx),
                    ApprovalKind::Quit if approved => {
                        self.quit_approved = true;
                        cx.quit();
                    }
                    _ => {}
                }
            }
            Outcome::Keep | Outcome::Close => {}
        }
    }

    fn overlays(&mut self, cx: &mut Cx) {
        self.next_install_prompt();
        let Some(popup) = &mut self.popup else { return };
        let mut outcome = popup.show(cx.ctx, cx.palette);
        for key in take_keys(cx.ctx) {
            if outcome != Outcome::Keep {
                break;
            }
            if let Some(popup) = &mut self.popup {
                outcome = popup.key(&key);
            }
        }
        self.apply_popup(outcome, cx);
    }

    fn poll(&mut self, cx: &mut Cx) {
        for done in self.backend.poll() {
            let connection = self.connection(&done.name).cloned();
            if self.installing.remove(&done.ty) {
                let jobs = self.install_wait.remove(&done.ty).unwrap_or_default();
                match done.result {
                    Ok(Output::Installed(path)) => {
                        cx.info(format!("{} driver installed at {}", done.ty.display_name(), path.display()));
                        for (name, operation) in jobs {
                            self.start(&name, operation, cx);
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        let message = Self::safe_error(connection.as_ref(), e);
                        for (name, _) in jobs {
                            self.states.entry(name).or_default().error = Some(message.clone());
                        }
                        cx.error(message);
                    }
                }
                continue;
            }
            let key = done.operation.key().map(str::to_string);
            if let Some(tab) = key.as_deref().and_then(|key| self.tabs.get_mut(key)) {
                tab.loading = false;
            }
            match done.result {
                Err(e) => {
                    let message = Self::safe_error(connection.as_ref(), e);
                    if matches!(done.operation, Operation::Load) {
                        let state = self.states.entry(done.name.clone()).or_default();
                        state.loading = false;
                        state.error = Some(message.clone());
                    } else if let Some(tab) = key.as_deref().and_then(|key| self.tabs.get_mut(key)) {
                        tab.error = Some(message.clone());
                    }
                    cx.error(message);
                }
                Ok(Output::Tables(tables)) => {
                    let state = self.states.entry(done.name).or_default();
                    state.loading = false;
                    state.loaded = true;
                    state.expanded = true;
                    state.error = None;
                    state.tables = tables;
                }
                Ok(Output::Data(result)) => {
                    if let Some(DbTab { kind: TabKind::Data(data), .. }) =
                        key.as_deref().and_then(|key| self.tabs.get_mut(key))
                    {
                        data.apply(result);
                    }
                }
                Ok(Output::Columns(columns)) => {
                    if let Some(tab) = key.as_deref().and_then(|key| self.tabs.get_mut(key)) {
                        tab.kind = DbTab::structure(&done.name, columns).kind;
                    }
                }
                Ok(Output::Query(result)) => {
                    if let Some(tab) = key.as_deref().and_then(|key| self.tabs.get_mut(key)) {
                        tab.kind = TabKind::Result(Grid::from_result(result));
                    }
                }
                Ok(Output::PrimaryKey(pk)) => {
                    if let Some(key) = key {
                        self.show_commit_approval(&key, pk, cx);
                    }
                }
                Ok(Output::Committed(affected)) => {
                    if let Some(DbTab { kind: TabKind::Data(data), .. }) =
                        key.as_deref().and_then(|key| self.tabs.get_mut(key))
                        && let Some(edit) = &mut data.edit
                    {
                        edit.revert_all();
                    }
                    cx.info(format!(
                        "Transaction committed · {affected} row{} affected",
                        if affected == 1 { "" } else { "s" }
                    ));
                    if let Some(key) = key {
                        self.refresh_tab(&key, cx);
                    }
                }
                Ok(Output::Installed(_)) => {}
            }
        }
    }

    fn activate(&mut self, row: &Row, cx: &mut Cx) {
        match &row.kind {
            RowKind::Connection(name) => self.open_console(name, cx),
            RowKind::Table { connection, table } => self.open_data(connection, table, cx),
        }
    }

    fn tree_action(&mut self, action: Action, cx: &mut Cx) {
        match action {
            Action::Add => self.popup = Some(Popup::Types { sel: 0 }),
            Action::OpenConsole(name) => self.open_console(&name, cx),
            Action::EditConnection(name) => {
                if let Some(connection) = self.connection(&name).cloned() {
                    self.popup = Some(Popup::Form(ConnectionForm::edit(&connection)));
                }
            }
            Action::DeleteConnection(name) => {
                self.popup = Some(Popup::approval(
                    ApprovalKind::Delete { name: name.clone() },
                    format!("Delete connection “{name}”?"),
                    "The query console file will be kept.",
                    "Delete",
                ));
            }
            Action::Refresh(name) => self.start(&name, Operation::Load, cx),
            Action::OpenData(name, table) => self.open_data(&name, &table, cx),
            Action::Structure(name, table) => self.open_structure(&name, &table, cx),
        }
    }

    fn panel_key(&mut self, key: KeyEvent, cx: &mut Cx) -> bool {
        if self.popup.is_some() {
            return true;
        }
        let rows = self.rows();
        let index = tree::cursor_index(&rows, self.cursor);
        let row = rows.get(index).cloned();
        let plain = !key.mods.ctrl && !key.mods.alt && !key.mods.cmd;
        match key.key {
            Key::Up => self.cursor = index.saturating_sub(1),
            Key::Down => self.cursor = (index + 1).min(rows.len().saturating_sub(1)),
            Key::Home | Key::PageUp => self.cursor = 0,
            Key::End | Key::PageDown => self.cursor = rows.len().saturating_sub(1),
            Key::Esc => cx.layout(LayoutOp::Focus(Area::Code)),
            Key::Enter | Key::Char(' ') if plain => {
                if let Some(row) = row {
                    self.activate(&row, cx);
                }
            }
            Key::Right if plain => {
                if let Some(Row { kind: RowKind::Connection(name), .. }) = row {
                    let state = self.states.entry(name.clone()).or_default();
                    state.expanded = true;
                    if !state.loaded {
                        self.start(&name, Operation::Load, cx);
                    }
                }
            }
            Key::Left if plain => match row {
                Some(Row { kind: RowKind::Connection(name), .. }) => {
                    self.states.entry(name).or_default().expanded = false;
                }
                Some(row) => {
                    if let Some(parent) = row.parent {
                        self.cursor = parent;
                    }
                }
                None => {}
            },
            Key::Char('a') if plain => self.tree_action(Action::Add, cx),
            Key::Char('e') if plain => match row {
                Some(Row { kind: RowKind::Connection(name), .. }) => self.tree_action(Action::EditConnection(name), cx),
                Some(Row { kind: RowKind::Table { connection, table }, .. }) => {
                    self.tree_action(Action::Structure(connection, table), cx)
                }
                None => {}
            },
            Key::Char('d') if plain => {
                if let Some(Row { kind: RowKind::Connection(name), .. }) = row {
                    self.tree_action(Action::DeleteConnection(name), cx);
                }
            }
            Key::Char('R') if plain => match row {
                Some(Row { kind: RowKind::Table { connection, .. } | RowKind::Connection(connection), .. }) => {
                    self.tree_action(Action::Refresh(connection), cx)
                }
                None => {
                    let names: Vec<String> = self.store.connections.iter().map(|c| c.name.clone()).collect();
                    for name in names {
                        self.start(&name, Operation::Load, cx);
                    }
                }
            },
            Key::Char('m') if plain => self.popup = Some(Popup::menu(row.as_ref())),
            Key::Char('q') if plain => cx.layout(LayoutOp::Close(Area::Database)),
            _ => return false,
        }
        true
    }

    fn console_tab_event(&mut self, key: &str, event: KeyEvent, cx: &mut Cx) -> bool {
        if event.key == Key::Enter && event.mods.cmd {
            self.run_console(key, cx);
            return true;
        }
        if event.key == Key::Char('s') && event.mods.cmd {
            self.save_console(key, cx);
            return true;
        }
        let modeless = cx.edit_mode == EditMode::Modeless;
        let editing =
            self.tabs.get(key).is_some_and(|tab| matches!(&tab.kind, TabKind::Console(console) if console.editing));
        if event.key == Key::Esc && editing && !modeless {
            if let Some(DbTab { kind: TabKind::Console(console), .. }) = self.tabs.get_mut(key)
                && let Err(error) = console.leave_insert()
            {
                cx.error(error);
            }
            return true;
        }
        if modeless || editing {
            return false;
        }
        if self.space_console.as_deref() == Some(key) {
            self.space_console = None;
            if event.key == Key::Char('D') {
                self.run_console(key, cx);
            }
            return true;
        }
        match event.key {
            Key::Char(' ') => {
                self.space_console = Some(key.to_string());
            }
            Key::Char('i') => {
                if let Some(DbTab { kind: TabKind::Console(console), .. }) = self.tabs.get_mut(key) {
                    console.edit();
                }
            }
            Key::Esc => {
                self.space_console = None;
            }
            Key::Up | Key::Down | Key::Home | Key::End => {
                if let Some(DbTab { kind: TabKind::Console(console), .. }) = self.tabs.get_mut(key) {
                    move_console_cursor(console, event.key);
                }
            }
            _ => return false,
        }
        true
    }

    fn grid_key(&mut self, key: &str, event: KeyEvent, cx: &mut Cx) -> bool {
        if self.command.is_some() {
            match event.key {
                Key::Esc => self.command = None,
                Key::Enter => {
                    let command = self.command.take().unwrap_or_default();
                    if command == ":w" {
                        self.prepare_commit(key, cx);
                    } else {
                        cx.error(format!("Not an editor command: {}", command.trim_start_matches(':')));
                    }
                }
                Key::Backspace => {
                    if let Some(command) = &mut self.command {
                        command.pop();
                        if command.is_empty() {
                            self.command = None;
                        }
                    }
                }
                Key::Char(c) if !event.mods.cmd && !event.mods.ctrl && !event.mods.alt => {
                    if let Some(command) = &mut self.command {
                        command.push(c);
                    }
                }
                _ => {}
            }
            return true;
        }
        let plain = !event.mods.ctrl && !event.mods.alt && !event.mods.cmd;
        if event.key == Key::Char(':') && plain {
            self.command = Some(":".into());
            return true;
        }
        if event.key == Key::Char('w') && event.mods.cmd {
            self.request_close_tab(key, cx);
            return true;
        }
        if event.key == Key::Char('s') && event.mods.cmd {
            self.prepare_commit(key, cx);
            return true;
        }
        if event.key == Key::Char('d') && event.mods.cmd {
            self.grid_action(key, GridAction::CloneRow, cx);
            return true;
        }
        if event.key == Key::Char('c') && event.mods.cmd {
            self.grid_action(key, GridAction::Copy, cx);
            return true;
        }
        match event.key {
            Key::Up => self.move_grid(key, -1, 0),
            Key::Down => self.move_grid(key, 1, 0),
            Key::Left => self.move_grid(key, 0, -1),
            Key::Right => self.move_grid(key, 0, 1),
            Key::PageUp => self.move_grid(key, -20, 0),
            Key::PageDown => self.move_grid(key, 20, 0),
            Key::Home => self.move_grid(key, isize::MIN / 2, 0),
            Key::End => self.move_grid(key, isize::MAX / 2, 0),
            Key::Enter => self.grid_action(key, GridAction::View, cx),
            Key::Char('y') if plain => self.grid_action(key, GridAction::Copy, cx),
            Key::Char('e') if plain => self.grid_action(key, GridAction::Edit, cx),
            Key::Delete if plain => self.grid_action(key, GridAction::SetNull, cx),
            Key::Char('u') if plain => {
                if let Some(DbTab { kind: TabKind::Data(data), .. }) = self.tabs.get_mut(key)
                    && let Some(edit) = &mut data.edit
                {
                    edit.undo();
                }
            }
            Key::Char('U') if plain => {
                if let Some(DbTab { kind: TabKind::Data(data), .. }) = self.tabs.get_mut(key)
                    && let Some(edit) = &mut data.edit
                {
                    edit.revert_all();
                    cx.info("Discarded pending changes");
                }
            }
            Key::Char('R') if plain => self.refresh_tab(key, cx),
            Key::Char('q') | Key::Esc if plain => self.request_close_tab(key, cx),
            _ => return false,
        }
        true
    }

    fn move_grid(&mut self, key: &str, dr: isize, dc: isize) {
        if let Some(tab) = self.tabs.get_mut(key) {
            let rows = tab.rows().len();
            if let Some(grid) = tab.grid_mut() {
                grid.move_cursor(dr, dc, rows);
            }
        }
    }

    fn tab_key(&mut self, key: &str, event: KeyEvent, cx: &mut Cx) -> bool {
        if self.popup.is_some() {
            return true;
        }
        match self.tabs.get(key).map(|tab| &tab.kind) {
            Some(TabKind::Console(_)) => self.console_tab_event(key, event, cx),
            Some(TabKind::Result(_) | TabKind::Data(_) | TabKind::Structure(_)) => self.grid_key(key, event, cx),
            None => false,
        }
    }

    fn tree_ui(&mut self, ui: &mut Ui, cx: &mut Cx) {
        self.ensure_loaded();
        let p = cx.palette;
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0.0, color(p.surface));
        let (head, _) = ui.allocate_exact_size(vec2(rect.width(), HEAD_H), Sense::hover());
        let tint = if cx.focused { color(p.accent) } else { color(p.comment) };
        ui.painter().text(head.left_center() + vec2(12.0, 0.0), Align2::LEFT_CENTER, "DATABASE", ui_font(11.0), tint);
        let add_rect = Rect::from_center_size(head.right_center() - vec2(18.0, 0.0), vec2(24.0, 24.0));
        let add = ui.interact(add_rect, ui.scope_id().with("db-add"), Sense::click());
        ui.painter().text(add_rect.center(), Align2::CENTER_CENTER, icon::PLUS, ui_font(15.0), color(p.accent));
        let mut action = add.clicked().then_some(Action::Add);
        let rows = self.rows();
        if rows.is_empty() {
            ui.painter().text(
                rect.left_top() + vec2(14.0, HEAD_H + 22.0),
                Align2::LEFT_TOP,
                "No data sources\n\na / +  Add a connection",
                ui_font(12.5),
                color(p.comment),
            );
        }
        let current = tree::cursor_index(&rows, self.cursor);
        let mut clicked = None;
        egui::ScrollArea::vertical().id_salt("db-tree").auto_shrink([false, false]).show_rows(
            ui,
            ROW_H,
            rows.len(),
            |ui, range| {
                for (index, row) in rows.iter().enumerate().take(range.end).skip(range.start) {
                    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
                    if index == current {
                        let fill =
                            if cx.focused { color(p.accent).gamma_multiply(0.25) } else { color(p.bg_highlight) };
                        ui.painter().rect_filled(rect.shrink2(vec2(3.0, 1.0)), 4.0, fill);
                    } else if response.hovered() {
                        ui.painter().rect_filled(rect, 0.0, color(p.bg_highlight).gamma_multiply(0.5));
                    }
                    self.paint_tree_row(ui, p, row, rect, index == current);
                    if response.clicked() {
                        clicked = Some(index);
                    }
                    response.context_menu(|ui| {
                        for (label, item) in tree::menu(Some(row)) {
                            if ui.button(label).clicked() {
                                action = Some(item);
                                ui.close();
                            }
                        }
                    });
                }
            },
        );
        if let Some(index) = clicked {
            self.cursor = index;
            if let Some(row) = rows.get(index).cloned() {
                self.activate(&row, cx);
            }
        }
        if let Some(action) = action {
            self.tree_action(action, cx);
        }
    }

    fn paint_tree_row(&self, ui: &Ui, p: &less_theme::Palette, row: &Row, rect: Rect, current: bool) {
        let connection = match &row.kind {
            RowKind::Connection(name) => self.connection(name),
            RowKind::Table { connection, .. } => self.connection(connection),
        };
        let glyph = match &row.kind {
            RowKind::Connection(_) => type_icon(connection.map(|c| c.ty).unwrap_or(DbType::Sqlite)),
            RowKind::Table { .. } => icon::SQUARES_FOUR,
        };
        let x = rect.left() + 9.0 + row.depth as f32 * 16.0;
        let caret = match &row.kind {
            RowKind::Connection(name) => {
                if self.states.get(name).is_some_and(|s| s.expanded) {
                    icon::CARET_DOWN
                } else {
                    icon::CARET_RIGHT
                }
            }
            RowKind::Table { .. } => "",
        };
        let fg = if row.error { color(p.error) } else { color(p.fg) };
        ui.painter().text(pos2(x, rect.center().y), Align2::LEFT_CENTER, caret, ui_font(10.0), color(p.comment));
        ui.painter().text(pos2(x + 16.0, rect.center().y), Align2::LEFT_CENTER, glyph, ui_font(13.0), color(p.accent));
        ui.painter().text(pos2(x + 38.0, rect.center().y), Align2::LEFT_CENTER, &row.label, ui_font(13.0), fg);
        let detail_width = ui.painter().layout_no_wrap(row.detail.clone(), ui_font(10.5), fg).size().x;
        if rect.width() > 210.0 && detail_width > 0.0 {
            ui.painter().text(
                pos2(rect.right() - detail_width - 8.0, rect.center().y),
                Align2::LEFT_CENTER,
                &row.detail,
                ui_font(10.5),
                color(if current { p.fg_dim } else { p.comment }),
            );
        }
    }

    fn tab_ui_inner(&mut self, key: &str, ui: &mut Ui, cx: &mut Cx) {
        self.visible_tab = cx.focused.then(|| key.to_string());
        let Some(tab) = self.tabs.get(key) else { return };
        let heading = match &tab.kind {
            TabKind::Console(console) => {
                let state = if cx.edit_mode == EditMode::Modeless {
                    "Cmd+Enter run · autosave"
                } else if console.editing {
                    "INSERT · Esc normal · Cmd+Enter run"
                } else {
                    "NORMAL · i edit · Space D run · Cmd+Enter run"
                };
                format!("{} · {state}", tab.connection)
            }
            TabKind::Result(grid) => format!("{} · {} rows", tab.connection, grid.rows.len()),
            TabKind::Data(data) => {
                let pending = data.pending();
                if pending == 0 {
                    format!("{} · {} · {} rows · e edit · Cmd+D clone", tab.connection, data.table, data.rows().len())
                } else {
                    format!("{} · {} · {pending} pending · Cmd+S commit · u undo", tab.connection, data.table)
                }
            }
            TabKind::Structure(grid) => format!("{} · Structure · {} columns", tab.connection, grid.rows.len()),
        };
        let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
        ui.painter().rect_filled(head, 0.0, color(cx.palette.surface));
        ui.painter().text(
            head.left_center() + vec2(10.0, 0.0),
            Align2::LEFT_CENTER,
            heading,
            ui_font(11.5),
            color(cx.palette.comment),
        );
        if let Some(error) = &tab.error {
            ui.label(egui::RichText::new(error).color(color(cx.palette.error)).font(ui_font(12.5)));
        } else if tab.loading {
            ui.label(egui::RichText::new("Loading…").color(color(cx.palette.comment)).font(ui_font(12.0)));
        }
        if let Some(notice) = tab.grid().and_then(|g| g.notice.as_ref()) {
            ui.label(egui::RichText::new(notice).color(color(cx.palette.extra.warn)).font(ui_font(12.0)));
        }
        if let Some(command) = &self.command {
            ui.label(egui::RichText::new(command).font(egui::FontId::monospace(13.0)).color(color(cx.palette.accent)));
        }
        let actions = match &mut self.tabs.get_mut(key).expect("tab checked").kind {
            TabKind::Console(console) => {
                console.show(ui, cx.palette, egui::Id::unique("db-console").with(key), cx.edit_mode == EditMode::Vim);
                Vec::new()
            }
            _ => grid_view::show(ui, cx.palette, self.tabs.get(key).expect("tab checked"), cx.focused),
        };
        for action in actions {
            self.grid_action(key, action, cx);
        }
    }
}

impl Default for DbPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl Panel for DbPanel {
    fn id(&self) -> &'static str {
        "db"
    }

    fn regions(&self) -> &[Region] {
        &REGIONS
    }

    fn tick(&mut self, cx: &mut Cx) {
        self.ensure_loaded();
        self.backend.set_waker(cx.ctx.clone());
        self.poll(cx);
        for tab in self.tabs.values_mut() {
            if let TabKind::Console(console) = &mut tab.kind
                && let Some(error) = console.tick()
            {
                tab.error = Some(error);
            }
        }
        self.overlays(cx);
    }

    fn ui(&mut self, region: Region, ui: &mut Ui, cx: &mut Cx) {
        if region == Region::Area(Area::Database) {
            self.tree_ui(ui, cx);
        }
    }

    fn event(&mut self, event: &Event, cx: &mut Cx) -> bool {
        match *event {
            Event::Key(key) => self.panel_key(key, cx),
            Event::Shortcut(key) if cx.active.is_none() => {
                self.visible_tab.clone().is_some_and(|tab| self.tab_key(&tab, key, cx))
            }
            Event::Leader("D") if cx.active.is_none() => self.visible_tab.clone().is_some_and(|key| {
                if self.tabs.get(&key).is_some_and(|tab| matches!(&tab.kind, TabKind::Console(_))) {
                    self.run_console(&key, cx);
                    true
                } else {
                    false
                }
            }),
            Event::Layout(Effect::Focused(area)) => {
                if area != Area::Code {
                    self.visible_tab = None;
                }
                false
            }
            Event::Layout(Effect::Opened(Area::Database)) => {
                self.ensure_loaded();
                false
            }
            _ => false,
        }
    }

    fn tab_ui(&mut self, key: &str, ui: &mut Ui, cx: &mut Cx) {
        self.tab_ui_inner(key, ui, cx);
    }

    fn tab_event(&mut self, key: &str, event: &Event, cx: &mut Cx) -> bool {
        match *event {
            Event::Key(k) | Event::Shortcut(k) => self.tab_key(key, k, cx),
            Event::Leader("D") if self.tabs.get(key).is_some_and(|tab| matches!(&tab.kind, TabKind::Console(_))) => {
                self.run_console(key, cx);
                true
            }
            _ => false,
        }
    }

    fn tab_closed(&mut self, key: &str, cx: &mut Cx) {
        if let Some(mut tab) = self.tabs.remove(key)
            && let TabKind::Console(console) = &mut tab.kind
            && let Err(error) = console.save()
        {
            cx.error(error);
        }
        if self.visible_tab.as_deref() == Some(key) {
            self.visible_tab = None;
        }
    }

    fn can_quit(&mut self, _cx: &mut Cx) -> bool {
        for tab in self.tabs.values_mut() {
            if let TabKind::Console(console) = &mut tab.kind {
                let _ = console.save();
            }
        }
        if self.quit_approved || self.tabs.values().all(|tab| tab.pending() == 0) {
            return true;
        }
        if !matches!(self.popup, Some(Popup::Approval { kind: ApprovalKind::Quit, .. })) {
            self.popup = Some(Popup::approval(
                ApprovalKind::Quit,
                "Discard pending database changes?",
                "One or more data tabs have uncommitted edits. They have not been sent to the database.",
                "Discard and quit",
            ));
        }
        false
    }
}

/// Ağaç satırında türü ayırt eden ikon.
fn type_icon(ty: DbType) -> &'static str {
    match ty {
        DbType::Postgresql | DbType::Sqlserver => icon::SQUARES_FOUR,
        DbType::Mysql | DbType::Mariadb => icon::FOLDER,
        DbType::Sqlite => icon::FILE,
        DbType::Redis => icon::KEYBOARD,
        DbType::Mongodb => icon::FILE_TEXT,
    }
}

/// Normal mod konsol imlecini satır başları arasında taşır.
fn move_console_cursor(console: &mut Console, key: Key) {
    let chars: Vec<char> = console.text.chars().collect();
    let position = console.cursor_char().min(chars.len());
    let line_start = chars[..position].iter().rposition(|c| *c == '\n').map(|i| i + 1).unwrap_or(0);
    let line_end = chars[position..].iter().position(|c| *c == '\n').map(|i| position + i).unwrap_or(chars.len());
    let col = position - line_start;
    let target = match key {
        Key::Home => Some(line_start),
        Key::End => Some(line_end),
        Key::Up if line_start > 0 => {
            let prev_end = line_start - 1;
            let prev_start = chars[..prev_end].iter().rposition(|c| *c == '\n').map(|i| i + 1).unwrap_or(0);
            Some(prev_start + col.min(prev_end - prev_start))
        }
        Key::Down if line_end < chars.len() => {
            let next_start = line_end + 1;
            let next_end =
                chars[next_start..].iter().position(|c| *c == '\n').map(|i| next_start + i).unwrap_or(chars.len());
            Some(next_start + col.min(next_end - next_start))
        }
        _ => None,
    };
    if let Some(target) = target {
        console.set_cursor_char(target);
    }
}

/// Pencere açıkken klavye olaylarını egui girdisinden alır; popup bunları host'a bırakmaz.
fn take_keys(ctx: &egui::Context) -> Vec<KeyEvent> {
    let events = ctx.input(|i| i.events.clone());
    ctx.input_mut(|i| {
        i.events.retain(|event| {
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
