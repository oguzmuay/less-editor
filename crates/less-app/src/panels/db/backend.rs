//! Database panelinin asenkron sürücü katmanı. Bloklayıcı `DriverClient` çağrıları ayrı iş parçacıklarında çalışır;
//! sürücü kurulumu ancak `install` açıkça çağrılırsa başlar.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use less_db::manager;
use less_db::protocol::{ColumnInfo, ConnectResult, QueryResult, TableInfo};
use less_db::{Connection, DbType, DriverClient};

/// Panelin isteyebileceği tek sürücü işi.
#[derive(Clone, Debug, PartialEq)]
pub enum Operation {
    Load,
    Data { key: String, table: String },
    Structure { key: String, table: String },
    Query { key: String, sql: String, read_only: bool },
    PrimaryKey { key: String, table: String },
    Commit { key: String, table: String, statements: Vec<String> },
}

impl Operation {
    /// Aynı işin ikinci kez başlatılmasını önleyen kimlik.
    pub fn id(&self) -> String {
        match self {
            Operation::Load => "load".into(),
            Operation::Data { key, .. } => format!("data:{key}"),
            Operation::Structure { key, .. } => format!("structure:{key}"),
            Operation::Query { key, .. } => format!("query:{key}"),
            Operation::PrimaryKey { key, .. } => format!("pk:{key}"),
            Operation::Commit { key, .. } => format!("commit:{key}"),
        }
    }

    pub fn key(&self) -> Option<&str> {
        match self {
            Operation::Load => None,
            Operation::Data { key, .. }
            | Operation::Structure { key, .. }
            | Operation::Query { key, .. }
            | Operation::PrimaryKey { key, .. }
            | Operation::Commit { key, .. } => Some(key),
        }
    }
}

/// Bir işin başarılı sonucu.
#[derive(Debug)]
pub enum Output {
    Tables(Vec<TableInfo>),
    Data(QueryResult),
    Columns(Vec<ColumnInfo>),
    Query(QueryResult),
    PrimaryKey(Vec<String>),
    Committed(u64),
    Installed(PathBuf),
}

/// Biten iş; yeni açılan oturum ana thread'e burada taşınır.
pub struct Done {
    pub name: String,
    pub ty: DbType,
    pub operation: Operation,
    pub result: Result<Output, String>,
    pub client: Option<Arc<dyn Driver>>,
}

/// Sürücü işlemlerinin testte sahtelenebilen arayüzü.
pub trait Driver: Send + Sync {
    fn connect(&self, config: &Connection) -> Result<ConnectResult, String>;
    fn list_schemas(&self) -> Result<Vec<String>, String>;
    fn list_tables(&self, schema: Option<&str>) -> Result<Vec<TableInfo>, String>;
    fn columns(&self, table: &str) -> Result<Vec<ColumnInfo>, String>;
    fn primary_key(&self, table: &str) -> Result<Vec<String>, String>;
    fn query(&self, sql: &str, read_only: bool, limit: Option<usize>) -> Result<QueryResult, String>;
    fn table_data(&self, table: &str, limit: usize, offset: usize) -> Result<QueryResult, String>;
    fn transaction(&self, statements: &[String]) -> Result<u64, String>;
}

struct RealDriver(DriverClient);

impl Driver for RealDriver {
    fn connect(&self, config: &Connection) -> Result<ConnectResult, String> {
        self.0.connect(config).map_err(|e| e.to_string())
    }

    fn list_schemas(&self) -> Result<Vec<String>, String> {
        self.0.list_schemas().map_err(|e| e.to_string())
    }

    fn list_tables(&self, schema: Option<&str>) -> Result<Vec<TableInfo>, String> {
        self.0.list_tables(schema).map_err(|e| e.to_string())
    }

    fn columns(&self, table: &str) -> Result<Vec<ColumnInfo>, String> {
        self.0.columns(table).map_err(|e| e.to_string())
    }

    fn primary_key(&self, table: &str) -> Result<Vec<String>, String> {
        self.0.primary_key(table).map_err(|e| e.to_string())
    }

    fn query(&self, sql: &str, read_only: bool, limit: Option<usize>) -> Result<QueryResult, String> {
        self.0.query(sql, read_only, limit).map_err(|e| e.to_string())
    }

    fn table_data(&self, table: &str, limit: usize, offset: usize) -> Result<QueryResult, String> {
        self.0.table_data(table, limit, offset).map_err(|e| e.to_string())
    }

    fn transaction(&self, statements: &[String]) -> Result<u64, String> {
        self.0.transaction(statements).map(|r| r.affected).map_err(|e| e.to_string())
    }
}

/// Sürücü bulma, başlatma ve kurma sağlayıcısı.
pub trait DriverProvider: Send + Sync {
    fn find(&self, ty: DbType) -> Option<PathBuf>;
    fn spawn(&self, binary: &Path) -> Result<Arc<dyn Driver>, String>;
    /// Bu metot yalnız kullanıcı Install'i onayladıktan sonra çağrılır.
    fn install(&self, ty: DbType) -> Result<PathBuf, String>;
}

pub struct RealProvider;

impl DriverProvider for RealProvider {
    fn find(&self, ty: DbType) -> Option<PathBuf> {
        manager::find_driver(ty)
    }

    fn spawn(&self, binary: &Path) -> Result<Arc<dyn Driver>, String> {
        DriverClient::spawn(binary).map(|c| Arc::new(RealDriver(c)) as Arc<dyn Driver>).map_err(|e| e.to_string())
    }

    fn install(&self, ty: DbType) -> Result<PathBuf, String> {
        let plan = manager::plan_install(ty).map_err(|e| e.to_string())?;
        manager::install(&plan, true).map_err(|e| e.to_string())
    }
}

pub enum Start {
    Started,
    MissingDriver,
    AlreadyRunning,
}

/// Bağlantı oturumları ve biten iş kanalı.
pub struct Backend {
    provider: Arc<dyn DriverProvider>,
    sessions: HashMap<String, Arc<dyn Driver>>,
    running: HashSet<(String, String)>,
    tx: Sender<Done>,
    rx: Receiver<Done>,
    waker: Option<eframe::egui::Context>,
}

impl Backend {
    pub fn new(provider: Arc<dyn DriverProvider>) -> Self {
        let (tx, rx) = mpsc::channel();
        Self { provider, sessions: HashMap::new(), running: HashSet::new(), tx, rx, waker: None }
    }

    pub fn set_waker(&mut self, ctx: eframe::egui::Context) {
        self.waker = Some(ctx);
    }

    pub fn remove(&mut self, name: &str) {
        self.sessions.remove(name);
    }

    /// İşi başlatır; sürücü yoksa hiçbir thread/ağ işi yapmadan `MissingDriver` döner.
    pub fn start(&mut self, name: &str, connection: &Connection, operation: Operation) -> Start {
        let running_key = (name.to_string(), operation.id());
        if self.running.contains(&running_key) {
            return Start::AlreadyRunning;
        }
        let existing = self.sessions.get(name).cloned();
        let binary = if existing.is_none() { self.provider.find(connection.ty) } else { None };
        if existing.is_none() && binary.is_none() {
            return Start::MissingDriver;
        }
        self.running.insert(running_key);
        let name = name.to_string();
        let connection = connection.clone();
        let tx = self.tx.clone();
        let waker = self.waker.clone();
        let provider = self.provider.clone();
        let op = operation.clone();
        let worker_name = name.clone();
        let worker_connection = connection.clone();
        let ty = connection.ty;
        let spawn = thread::Builder::new().name(format!("less-db-{}", connection.ty.id())).spawn(move || {
            let mut new_client = None;
            let client = match existing {
                Some(c) => Ok(c),
                None => match binary {
                    Some(path) => provider.spawn(&path).inspect(|client| {
                        new_client = Some(client.clone());
                    }),
                    None => Err("The database driver is not installed".to_string()),
                },
            };
            let result = match client {
                Ok(client) => {
                    client.connect(&worker_connection).and_then(|_| execute(client.as_ref(), &worker_connection, &op))
                }
                Err(e) => Err(e),
            };
            let _ = tx.send(Done { name: worker_name, ty, operation: op, result, client: new_client });
            if let Some(ctx) = waker {
                ctx.request_repaint();
            }
        });
        if spawn.is_err() {
            self.running.remove(&(name.clone(), operation.id()));
            let _ = self.tx.send(Done {
                name,
                ty,
                operation,
                result: Err("Could not start the database worker".into()),
                client: None,
            });
        }
        Start::Started
    }

    /// Kullanıcı onayından sonra manifesti alır, indirir ve SHA-256 doğrular.
    pub fn install(&mut self, name: &str, connection: &Connection, operation: Operation) {
        let provider = self.provider.clone();
        let tx = self.tx.clone();
        let waker = self.waker.clone();
        let name = name.to_string();
        let connection = connection.clone();
        let _ = thread::Builder::new().name(format!("less-install-{}", connection.ty.id())).spawn(move || {
            let result = provider.install(connection.ty).map(Output::Installed);
            let _ = tx.send(Done { name, ty: connection.ty, operation, result, client: None });
            if let Some(ctx) = waker {
                ctx.request_repaint();
            }
        });
    }

    /// Biten işleri alır ve yeni oturumları bağlantı adına kaydeder.
    pub fn poll(&mut self) -> Vec<Done> {
        let mut out = Vec::new();
        while let Ok(done) = self.rx.try_recv() {
            self.running.remove(&(done.name.clone(), done.operation.id()));
            if let Some(client) = &done.client
                && done.result.is_ok()
            {
                self.sessions.insert(done.name.clone(), client.clone());
            }
            out.push(done);
        }
        out
    }
}

fn execute(client: &dyn Driver, _connection: &Connection, operation: &Operation) -> Result<Output, String> {
    match operation {
        Operation::Load => {
            let schemas = client.list_schemas()?;
            let mut tables = Vec::new();
            if schemas.is_empty() {
                tables.extend(client.list_tables(None)?);
            } else {
                for schema in schemas {
                    tables.extend(client.list_tables(Some(&schema))?);
                }
            }
            tables.sort_by_key(|table| table.qualified());
            tables.dedup_by(|a, b| a.qualified() == b.qualified());
            Ok(Output::Tables(tables))
        }
        Operation::Data { table, .. } => client.table_data(table, 500, 0).map(Output::Data),
        Operation::Structure { table, .. } => client.columns(table).map(Output::Columns),
        Operation::Query { sql, read_only, .. } => client.query(sql, *read_only, Some(500)).map(Output::Query),
        Operation::PrimaryKey { table, .. } => client.primary_key(table).map(Output::PrimaryKey),
        Operation::Commit { statements, .. } => client.transaction(statements).map(Output::Committed),
    }
}
