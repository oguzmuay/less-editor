//! `Driver` uygulaması (rusqlite, gömülü SQLite).

use less_db::connections::Connection as Config;
use less_db::protocol::{ColumnInfo, ConnectResult, QueryResult, TableInfo, TransactionResult};
use less_db::serve::{CancelFn, Driver, DriverFailure, DriverResult};
use less_db::types::{DbType, quote_ident};
use rusqlite::types::ValueRef;
use rusqlite::{Batch, Connection, OpenFlags};

#[derive(Default)]
pub struct SqliteDriver {
    conn: Option<Connection>,
}

impl SqliteDriver {
    fn conn(&self) -> DriverResult<&Connection> {
        self.conn.as_ref().ok_or_else(|| DriverFailure("Not connected".into()))
    }
}

fn cell(v: ValueRef<'_>) -> Option<String> {
    match v {
        ValueRef::Null => None,
        ValueRef::Integer(i) => Some(i.to_string()),
        ValueRef::Real(f) => Some(f.to_string()),
        ValueRef::Text(t) => Some(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(b) => Some(format!("<blob {} bytes>", b.len())),
    }
}

/// Bağlantı dosyası: `file`, yoksa `database`, yoksa `url` (`sqlite:` öneki atılır).
fn path_of(cfg: &Config) -> Option<String> {
    cfg.file
        .clone()
        .or_else(|| cfg.database.clone())
        .or_else(|| {
            cfg.url.as_ref().map(|u| u.trim_start_matches("sqlite3:").trim_start_matches("sqlite:").to_string())
        })
        .filter(|p| !p.is_empty())
}

fn expand_home(p: &str) -> String {
    match (p.strip_prefix("~/"), std::env::var("HOME")) {
        (Some(rest), Ok(h)) => format!("{h}/{rest}"),
        _ => p.to_string(),
    }
}

/// Tek ifadeyi çalıştırır; satır döndürmeyen ifadede `affected` dolu gelir.
fn run_stmt(conn: &Connection, stmt: &mut rusqlite::Statement<'_>, limit: Option<usize>) -> DriverResult<QueryResult> {
    let ncol = stmt.column_count();
    if ncol == 0 {
        let n = stmt.execute([])?;
        return Ok(QueryResult { affected: Some(n as u64), ..Default::default() });
    }
    let columns: Vec<String> = stmt.column_names().into_iter().map(String::from).collect();
    let mut rows = Vec::new();
    let mut q = stmt.query([])?;
    let mut truncated = false;
    while let Some(row) = q.next()? {
        if limit.is_some_and(|l| rows.len() >= l) {
            truncated = true;
            break;
        }
        rows.push((0..ncol).map(|i| row.get_ref(i).map(cell)).collect::<Result<Vec<_>, _>>()?);
    }
    let _ = conn;
    let notice = truncated.then(|| format!("Showing the first {} rows", rows.len()));
    Ok(QueryResult { columns, rows, affected: None, notice })
}

impl Driver for SqliteDriver {
    fn connect(&mut self, config: &Config) -> DriverResult<ConnectResult> {
        if config.ty != DbType::Sqlite {
            return Err(DriverFailure(format!("This driver handles SQLite, not {}", config.ty.display_name())));
        }
        let path = path_of(config).ok_or_else(|| DriverFailure("No database file given".into()))?;
        let path = expand_home(&path);
        let conn = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE | OpenFlags::SQLITE_OPEN_URI,
        )
        .map_err(|e| DriverFailure(format!("Could not open {path}: {e}")))?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let version: String = conn.query_row("select sqlite_version()", [], |r| r.get(0))?;
        self.conn = Some(conn);
        Ok(ConnectResult { server_version: Some(version) })
    }

    fn cancel_handle(&self) -> Option<CancelFn> {
        let h = self.conn.as_ref()?.get_interrupt_handle();
        Some(Box::new(move || h.interrupt()))
    }

    fn list_schemas(&mut self) -> DriverResult<Vec<String>> {
        Ok(vec!["main".into()])
    }

    fn list_tables(&mut self, _schema: Option<&str>) -> DriverResult<Vec<TableInfo>> {
        let conn = self.conn()?;
        let mut st = conn.prepare(
            "SELECT name, type FROM sqlite_master WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' ORDER BY 1",
        )?;
        let rows = st.query_map([], |r| Ok(TableInfo { schema: None, name: r.get(0)?, kind: r.get(1)? }))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    fn columns(&mut self, table: &str) -> DriverResult<Vec<ColumnInfo>> {
        let conn = self.conn()?;
        let mut st = conn.prepare(&format!("PRAGMA table_info({})", quote_ident(DbType::Sqlite, table)))?;
        let rows = st.query_map([], |r| {
            Ok(ColumnInfo {
                name: r.get(1)?,
                data_type: r.get(2)?,
                nullable: r.get::<_, i64>(3)? == 0,
                default: r.get(4)?,
                primary_key: r.get::<_, i64>(5)? > 0,
            })
        })?;
        let cols: Vec<ColumnInfo> = rows.collect::<Result<_, _>>()?;
        if cols.is_empty() {
            return Err(DriverFailure(format!("No such table: {table}")));
        }
        Ok(cols)
    }

    fn primary_key(&mut self, table: &str) -> DriverResult<Vec<String>> {
        let conn = self.conn()?;
        let mut st = conn.prepare(&format!("PRAGMA table_info({})", quote_ident(DbType::Sqlite, table)))?;
        let mut keys: Vec<(i64, String)> = st
            .query_map([], |r| Ok((r.get::<_, i64>(5)?, r.get::<_, String>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|(pk, _)| *pk > 0)
            .collect();
        keys.sort();
        Ok(keys.into_iter().map(|(_, n)| n).collect())
    }

    fn query(&mut self, sql: &str, read_only: bool, limit: Option<usize>) -> DriverResult<QueryResult> {
        let conn = self.conn()?;
        conn.pragma_update(None, "query_only", read_only)?;
        let mut batch = Batch::new(conn, sql);
        let mut last = QueryResult::default();
        let mut affected = 0u64;
        let mut any_affected = false;
        while let Some(mut stmt) = batch.next()? {
            let r = run_stmt(conn, &mut stmt, limit)?;
            if let Some(a) = r.affected {
                affected += a;
                any_affected = true;
            } else {
                last = r;
            }
        }
        if any_affected {
            last.affected = Some(affected);
        }
        Ok(last)
    }

    fn table_data(&mut self, table: &str, limit: usize, offset: usize) -> DriverResult<QueryResult> {
        let conn = self.conn()?;
        conn.pragma_update(None, "query_only", true)?;
        let sql = format!("SELECT * FROM {} LIMIT {limit} OFFSET {offset}", quote_ident(DbType::Sqlite, table));
        let mut stmt = conn.prepare(&sql)?;
        run_stmt(conn, &mut stmt, None)
    }

    fn transaction(&mut self, statements: &[String]) -> DriverResult<TransactionResult> {
        let conn = self.conn.as_mut().ok_or_else(|| DriverFailure("Not connected".into()))?;
        conn.pragma_update(None, "query_only", false)?;
        let tx = conn.transaction()?;
        let mut affected = 0u64;
        for s in statements {
            let mut batch = Batch::new(&tx, s);
            while let Some(mut stmt) = batch.next()? {
                let r = run_stmt(&tx, &mut stmt, None)?;
                affected += r.affected.unwrap_or(0);
            }
        }
        tx.commit()?;
        Ok(TransactionResult { affected })
    }
}
