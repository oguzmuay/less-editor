//! `Driver` uygulaması (`postgres` eşzamanlı istemci). Tüm sorgular basit sorgu protokolüyle (metin) çalışır:
//! değerler sunucunun metin biçiminde gelir, tür dönüştürme gerekmez. TLS yok (yalnız `NoTls`).

use less_db::connections::Connection as Config;
use less_db::protocol::{ColumnInfo, ConnectResult, QueryResult, TableInfo, TransactionResult};
use less_db::serve::{CancelFn, Driver, DriverFailure, DriverResult};
use less_db::types::{DbType, quote_ident};
use postgres::{Client, NoTls, SimpleQueryMessage};

#[derive(Default)]
pub struct PostgresDriver {
    client: Option<Client>,
}

impl PostgresDriver {
    fn client(&mut self) -> DriverResult<&mut Client> {
        self.client.as_mut().ok_or_else(|| DriverFailure("Not connected".into()))
    }
}

/// SQL dize sabiti.
pub fn lit(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// `şema.tablo` → (şema, tablo); şemasız adda şema `public`.
pub fn split_table(name: &str) -> (&str, &str) {
    name.split_once('.').unwrap_or(("public", name))
}

/// Bağlantı dizesi: `url` varsa o, yoksa alanlardan (`key=value`, boşluk/tırnak kaçışlı).
pub fn conn_string(cfg: &Config) -> String {
    if let Some(u) = cfg.url.as_ref().filter(|u| !u.is_empty()) {
        return u.clone();
    }
    let esc = |v: &str| format!("'{}'", v.replace('\\', "\\\\").replace('\'', "\\'"));
    let mut parts = vec![
        format!("host={}", esc(cfg.host.as_deref().filter(|h| !h.is_empty()).unwrap_or("localhost"))),
        format!("port={}", cfg.port.unwrap_or(5432)),
    ];
    if let Some(u) = cfg.user.as_deref().filter(|u| !u.is_empty()) {
        parts.push(format!("user={}", esc(u)));
    }
    if let Some(p) = cfg.password.as_deref().filter(|p| !p.is_empty()) {
        parts.push(format!("password={}", esc(p)));
    }
    if let Some(d) = cfg.database.as_deref().filter(|d| !d.is_empty()) {
        parts.push(format!("dbname={}", esc(d)));
    }
    parts.push("connect_timeout=10".into());
    parts.join(" ")
}

/// Basit sorgu sonucu (son sonuç kümesi + toplam etkilenen satır).
fn collect(msgs: Vec<SimpleQueryMessage>, limit: Option<usize>) -> QueryResult {
    let mut last = QueryResult::default();
    let mut affected = 0u64;
    let mut any_affected = false;
    let mut in_rows = false;
    let mut truncated = false;
    for m in msgs {
        match m {
            SimpleQueryMessage::RowDescription(cols) => {
                last =
                    QueryResult { columns: cols.iter().map(|c| c.name().to_string()).collect(), ..Default::default() };
                in_rows = true;
                truncated = false;
            }
            SimpleQueryMessage::Row(row) => {
                if !in_rows {
                    last.columns = row.columns().iter().map(|c| c.name().to_string()).collect();
                    in_rows = true;
                }
                if limit.is_some_and(|l| last.rows.len() >= l) {
                    truncated = true;
                    continue;
                }
                last.rows.push((0..row.len()).map(|i| row.get(i).map(String::from)).collect());
            }
            SimpleQueryMessage::CommandComplete(n) => {
                if !in_rows {
                    affected += n;
                    any_affected = true;
                }
                in_rows = false;
            }
            _ => {}
        }
    }
    if truncated {
        last.notice = Some(format!("Showing the first {} rows", last.rows.len()));
    }
    if any_affected && last.columns.is_empty() {
        last.affected = Some(affected);
    }
    last
}

fn set_mode(client: &mut Client, read_only: bool) -> DriverResult<()> {
    let sql =
        if read_only { "SET default_transaction_read_only = on" } else { "SET default_transaction_read_only = off" };
    client.batch_execute(sql)?;
    Ok(())
}

impl Driver for PostgresDriver {
    fn connect(&mut self, config: &Config) -> DriverResult<ConnectResult> {
        if config.ty != DbType::Postgresql {
            return Err(DriverFailure(format!("This driver handles PostgreSQL, not {}", config.ty.display_name())));
        }
        let mut client = Client::connect(&conn_string(config), NoTls)?;
        set_mode(&mut client, true)?;
        let version = client.simple_query("SHOW server_version")?.into_iter().find_map(|m| match m {
            SimpleQueryMessage::Row(r) => r.get(0).map(String::from),
            _ => None,
        });
        self.client = Some(client);
        Ok(ConnectResult { server_version: version })
    }

    fn cancel_handle(&self) -> Option<CancelFn> {
        let token = self.client.as_ref()?.cancel_token();
        Some(Box::new(move || {
            let _ = token.cancel_query(NoTls);
        }))
    }

    fn list_schemas(&mut self) -> DriverResult<Vec<String>> {
        let c = self.client()?;
        let sql = "SELECT schema_name FROM information_schema.schemata \
                   WHERE schema_name NOT IN ('pg_catalog','information_schema') AND schema_name NOT LIKE 'pg_toast%' \
                   AND schema_name NOT LIKE 'pg_temp%' ORDER BY 1";
        Ok(collect(c.simple_query(sql)?, None).rows.into_iter().filter_map(|mut r| r.remove(0)).collect())
    }

    fn list_tables(&mut self, schema: Option<&str>) -> DriverResult<Vec<TableInfo>> {
        let c = self.client()?;
        let filter = match schema {
            Some(s) => format!("table_schema = {}", lit(s)),
            None => "table_schema NOT IN ('pg_catalog','information_schema')".into(),
        };
        let sql = format!(
            "SELECT table_schema, table_name, CASE table_type WHEN 'VIEW' THEN 'view' ELSE 'table' END \
             FROM information_schema.tables WHERE {filter} ORDER BY 1, 2"
        );
        let rows = collect(c.simple_query(&sql)?, None).rows;
        Ok(rows
            .into_iter()
            .map(|r| TableInfo {
                schema: r[0].clone(),
                name: r[1].clone().unwrap_or_default(),
                kind: r[2].clone().unwrap_or_default(),
            })
            .collect())
    }

    fn columns(&mut self, table: &str) -> DriverResult<Vec<ColumnInfo>> {
        let pk = self.primary_key(table)?;
        let (schema, name) = split_table(table);
        let sql = format!(
            "SELECT column_name, data_type, is_nullable, column_default FROM information_schema.columns \
             WHERE table_schema = {} AND table_name = {} ORDER BY ordinal_position",
            lit(schema),
            lit(name)
        );
        let rows = collect(self.client()?.simple_query(&sql)?, None).rows;
        if rows.is_empty() {
            return Err(DriverFailure(format!("No such table: {table}")));
        }
        Ok(rows
            .into_iter()
            .map(|r| {
                let col = r[0].clone().unwrap_or_default();
                ColumnInfo {
                    primary_key: pk.contains(&col),
                    name: col,
                    data_type: r[1].clone().unwrap_or_default(),
                    nullable: r[2].as_deref() == Some("YES"),
                    default: r[3].clone(),
                }
            })
            .collect())
    }

    fn primary_key(&mut self, table: &str) -> DriverResult<Vec<String>> {
        let (schema, name) = split_table(table);
        let sql = format!(
            "SELECT kcu.column_name FROM information_schema.table_constraints tc \
             JOIN information_schema.key_column_usage kcu ON tc.constraint_name = kcu.constraint_name \
             AND tc.constraint_schema = kcu.constraint_schema AND tc.table_name = kcu.table_name \
             WHERE tc.constraint_type = 'PRIMARY KEY' AND tc.table_schema = {} AND tc.table_name = {} \
             ORDER BY kcu.ordinal_position",
            lit(schema),
            lit(name)
        );
        let rows = collect(self.client()?.simple_query(&sql)?, None).rows;
        Ok(rows.into_iter().filter_map(|mut r| r.remove(0)).collect())
    }

    fn query(&mut self, sql: &str, read_only: bool, limit: Option<usize>) -> DriverResult<QueryResult> {
        let c = self.client()?;
        set_mode(c, read_only)?;
        Ok(collect(c.simple_query(sql)?, limit))
    }

    fn table_data(&mut self, table: &str, limit: usize, offset: usize) -> DriverResult<QueryResult> {
        let c = self.client()?;
        set_mode(c, true)?;
        let (schema, name) = split_table(table);
        let sql = format!(
            "SELECT * FROM {}.{} LIMIT {limit} OFFSET {offset}",
            quote_ident(DbType::Postgresql, schema),
            quote_ident(DbType::Postgresql, name)
        );
        Ok(collect(c.simple_query(&sql)?, None))
    }

    fn transaction(&mut self, statements: &[String]) -> DriverResult<TransactionResult> {
        let c = self.client()?;
        set_mode(c, false)?;
        let body = statements.join("\n");
        let msgs = c.simple_query(&format!("BEGIN;\n{body}\nCOMMIT;"));
        match msgs {
            Ok(m) => {
                let affected = m
                    .iter()
                    .filter_map(|m| match m {
                        SimpleQueryMessage::CommandComplete(n) => Some(*n),
                        _ => None,
                    })
                    .sum::<u64>();
                Ok(TransactionResult { affected })
            }
            Err(e) => {
                let _ = c.batch_execute("ROLLBACK");
                Err(e.into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_escaping() {
        assert_eq!(lit("o'x"), "'o''x'");
    }

    #[test]
    fn table_splitting() {
        assert_eq!(split_table("a.b"), ("a", "b"));
        assert_eq!(split_table("b"), ("public", "b"));
    }

    #[test]
    fn conn_string_from_fields() {
        let mut c = Config::new("x", DbType::Postgresql);
        c.host = Some("db".into());
        c.user = Some("me".into());
        c.password = Some("p'w d".into());
        c.database = Some("app".into());
        let s = conn_string(&c);
        assert!(s.contains("host='db'") && s.contains("port=5432") && s.contains("user='me'"));
        assert!(s.contains(r"password='p\'w d'") && s.contains("dbname='app'"));
    }

    #[test]
    fn conn_string_prefers_url() {
        let mut c = Config::new("x", DbType::Postgresql);
        c.url = Some("postgresql://u@h/d".into());
        assert_eq!(conn_string(&c), "postgresql://u@h/d");
    }

    #[test]
    fn rejects_other_types_and_requires_connect() {
        let mut d = PostgresDriver::default();
        assert!(d.connect(&Config::new("x", DbType::Mysql)).is_err());
        assert!(d.list_schemas().is_err());
        assert!(d.cancel_handle().is_none());
    }
}
