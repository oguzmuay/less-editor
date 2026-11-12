//! Sürücü ikilileri için ortak stdio sunucusu: `Driver` trait'ini uygulayan tür `run_stdio` ile protokolü konuşur.

use crate::connections::Connection;
use crate::protocol::*;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::io::{BufRead, Write};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

/// Sürücü hatası (mesaj kullanıcıya olduğu gibi gösterilir).
#[derive(Debug, Clone, PartialEq)]
pub struct DriverFailure(pub String);

impl<E: std::fmt::Display> From<E> for DriverFailure {
    fn from(e: E) -> Self {
        DriverFailure(e.to_string())
    }
}

pub type DriverResult<T> = Result<T, DriverFailure>;

/// Çalışan sorguyu başka iş parçacığından kesen işlev.
pub type CancelFn = Box<dyn Fn() + Send + Sync>;

/// Bir veritabanı sürücüsünün yapması gerekenler. Tüm çağrılar tek iş parçacığında sırayla gelir; `cancel` hariç.
pub trait Driver {
    fn connect(&mut self, config: &Connection) -> DriverResult<ConnectResult>;
    /// `connect` sonrası; sorguyu kesmek için kullanılır (desteklenmiyorsa `None`).
    fn cancel_handle(&self) -> Option<CancelFn>;
    fn list_schemas(&mut self) -> DriverResult<Vec<String>>;
    fn list_tables(&mut self, schema: Option<&str>) -> DriverResult<Vec<TableInfo>>;
    fn columns(&mut self, table: &str) -> DriverResult<Vec<ColumnInfo>>;
    fn primary_key(&mut self, table: &str) -> DriverResult<Vec<String>>;
    fn query(&mut self, sql: &str, read_only: bool, limit: Option<usize>) -> DriverResult<QueryResult>;
    fn table_data(&mut self, table: &str, limit: usize, offset: usize) -> DriverResult<QueryResult>;
    fn transaction(&mut self, statements: &[String]) -> DriverResult<TransactionResult>;
}

fn params<T: DeserializeOwned>(v: Value) -> Result<T, RpcError> {
    serde_json::from_value(v)
        .map_err(|e| RpcError { code: ERR_INVALID_PARAMS, message: format!("Invalid params: {e}") })
}

fn to_value<T: serde::Serialize>(v: T) -> Result<Value, RpcError> {
    serde_json::to_value(v).map_err(|e| RpcError { code: ERR_DB, message: e.to_string() })
}

fn db_err(f: DriverFailure) -> RpcError {
    RpcError { code: ERR_DB, message: f.0 }
}

/// Tek isteği sürücüye yönlendirir (`close` ve `cancel` çağıran tarafından işlenir).
pub fn dispatch(driver: &mut dyn Driver, connected: &mut bool, req: Request) -> Result<Value, RpcError> {
    let method = req.method.as_str();
    let known = [
        METHOD_CONNECT,
        METHOD_LIST_SCHEMAS,
        METHOD_LIST_TABLES,
        METHOD_COLUMNS,
        METHOD_PRIMARY_KEY,
        METHOD_QUERY,
        METHOD_TABLE_DATA,
        METHOD_TRANSACTION,
    ];
    if !known.contains(&method) {
        return Err(RpcError { code: ERR_METHOD_NOT_FOUND, message: format!("Unknown method: {method}") });
    }
    if method != METHOD_CONNECT && !*connected {
        return Err(RpcError { code: ERR_NOT_CONNECTED, message: "Not connected: call connect first".into() });
    }
    match method {
        METHOD_CONNECT => {
            let p: ConnectParams = params(req.params)?;
            let r = driver.connect(&p.config).map_err(db_err)?;
            *connected = true;
            to_value(r)
        }
        METHOD_LIST_SCHEMAS => to_value(driver.list_schemas().map_err(db_err)?),
        METHOD_LIST_TABLES => {
            let p: ListTablesParams = params(req.params)?;
            to_value(driver.list_tables(p.schema.as_deref()).map_err(db_err)?)
        }
        METHOD_COLUMNS => {
            let p: TableParams = params(req.params)?;
            to_value(driver.columns(&p.table).map_err(db_err)?)
        }
        METHOD_PRIMARY_KEY => {
            let p: TableParams = params(req.params)?;
            to_value(driver.primary_key(&p.table).map_err(db_err)?)
        }
        METHOD_QUERY => {
            let p: QueryParams = params(req.params)?;
            to_value(driver.query(&p.sql, p.read_only, p.limit).map_err(db_err)?)
        }
        METHOD_TABLE_DATA => {
            let p: TableDataParams = params(req.params)?;
            to_value(driver.table_data(&p.table, p.limit, p.offset).map_err(db_err)?)
        }
        METHOD_TRANSACTION => {
            let p: TransactionParams = params(req.params)?;
            to_value(driver.transaction(&p.statements).map_err(db_err)?)
        }
        other => Err(RpcError { code: ERR_METHOD_NOT_FOUND, message: format!("Unknown method: {other}") }),
    }
}

fn send(out: &Arc<Mutex<Box<dyn Write + Send>>>, resp: &Response) {
    if let Ok(mut w) = out.lock()
        && let Ok(line) = serde_json::to_string(resp)
    {
        let _ = writeln!(w, "{line}");
        let _ = w.flush();
    }
}

/// Protokol döngüsü. Okuyucu iş parçacığı `cancel`'ı sorgu sürerken de yanıtlar; diğer istekler sırayla çalışır.
pub fn serve<R, W>(mut driver: Box<dyn Driver + Send>, input: R, output: W)
where
    R: BufRead + Send + 'static,
    W: Write + Send + 'static,
{
    let out: Arc<Mutex<Box<dyn Write + Send>>> = Arc::new(Mutex::new(Box::new(output)));
    let cancel: Arc<Mutex<Option<CancelFn>>> = Arc::new(Mutex::new(None));
    let (tx, rx) = mpsc::channel::<Request>();

    let reader_out = Arc::clone(&out);
    let reader_cancel = Arc::clone(&cancel);
    std::thread::spawn(move || {
        for line in input.lines() {
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<Request>(&line) {
                Err(e) => send(&reader_out, &Response::err(None, ERR_PARSE, format!("Parse error: {e}"))),
                Ok(req) if req.method == METHOD_CANCEL => {
                    if let Ok(g) = reader_cancel.lock()
                        && let Some(f) = g.as_ref()
                    {
                        f();
                    }
                    send(&reader_out, &Response::ok(req.id, Value::Null));
                }
                Ok(req) => {
                    if tx.send(req).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let mut connected = false;
    while let Ok(req) = rx.recv() {
        let id = req.id;
        if req.method == METHOD_CLOSE {
            send(&out, &Response::ok(id, Value::Null));
            return;
        }
        let is_connect = req.method == METHOD_CONNECT;
        let resp = match dispatch(driver.as_mut(), &mut connected, req) {
            Ok(v) => Response::ok(id, v),
            Err(e) => Response::err(Some(id), e.code, e.message),
        };
        if is_connect
            && connected
            && let Ok(mut g) = cancel.lock()
        {
            *g = driver.cancel_handle();
        }
        send(&out, &resp);
    }
}

/// `main` için kısayol: stdin/stdout üzerinde `serve`.
pub fn run_stdio(driver: Box<dyn Driver + Send>) {
    serve(driver, std::io::BufReader::new(std::io::stdin()), std::io::stdout());
}
