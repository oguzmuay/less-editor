//! Sürücü protokolü: stdio üzerinde satır ayraçlı JSON-RPC 2.0 (bkz. `docs/DRIVERS.md`).

use crate::connections::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const METHOD_CONNECT: &str = "connect";
pub const METHOD_LIST_SCHEMAS: &str = "list_schemas";
pub const METHOD_LIST_TABLES: &str = "list_tables";
pub const METHOD_COLUMNS: &str = "columns";
pub const METHOD_PRIMARY_KEY: &str = "primary_key";
pub const METHOD_QUERY: &str = "query";
pub const METHOD_TABLE_DATA: &str = "table_data";
pub const METHOD_TRANSACTION: &str = "transaction";
pub const METHOD_CANCEL: &str = "cancel";
pub const METHOD_CLOSE: &str = "close";

/// JSON-RPC hata kodları.
pub const ERR_PARSE: i64 = -32700;
pub const ERR_METHOD_NOT_FOUND: i64 = -32601;
pub const ERR_INVALID_PARAMS: i64 = -32602;
/// Veritabanı hatası (sorgu/bağlantı).
pub const ERR_DB: i64 = -32000;
/// `connect` öncesi çağrı.
pub const ERR_NOT_CONNECTED: i64 = -32001;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub jsonrpc: String,
    /// Ayrıştırılamayan istekte `null`.
    pub id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl Response {
    pub fn ok(id: u64, result: Value) -> Self {
        Response { jsonrpc: "2.0".into(), id: Some(id), result: Some(result), error: None }
    }

    pub fn err(id: Option<u64>, code: i64, message: impl Into<String>) -> Self {
        Response { jsonrpc: "2.0".into(), id, result: None, error: Some(RpcError { code, message: message.into() }) }
    }
}

/// `connect` parametreleri.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectParams {
    pub config: Connection,
}

/// `connect` sonucu.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConnectResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_version: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListTablesParams {
    #[serde(default)]
    pub schema: Option<String>,
}

/// `list_tables` öğesi.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub name: String,
    /// `table` | `view`.
    pub kind: String,
}

impl TableInfo {
    /// Panelde ve sorgularda kullanılan `şema.ad` (şemasızsa yalnız ad).
    pub fn qualified(&self) -> String {
        match &self.schema {
            Some(s) => format!("{s}.{}", self.name),
            None => self.name.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableParams {
    pub table: String,
}

/// `columns` öğesi.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    pub primary_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryParams {
    pub sql: String,
    #[serde(default)]
    pub read_only: bool,
    #[serde(default)]
    pub limit: Option<usize>,
}

/// `query` / `table_data` sonucu; değerler metin, `null` = NULL.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Option<String>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub affected: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableDataParams {
    pub table: String,
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionParams {
    pub statements: Vec<String>,
}

/// `transaction` sonucu.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TransactionResult {
    pub affected: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_shapes() {
        let ok = serde_json::to_string(&Response::ok(1, serde_json::json!({"a": 1}))).unwrap();
        assert!(ok.contains("\"result\"") && !ok.contains("\"error\""));
        let e = serde_json::to_string(&Response::err(Some(2), ERR_DB, "boom")).unwrap();
        assert!(e.contains("\"error\"") && !e.contains("\"result\""));
    }

    #[test]
    fn query_params_defaults() {
        let p: QueryParams = serde_json::from_str(r#"{"sql":"select 1"}"#).unwrap();
        assert!(!p.read_only && p.limit.is_none());
    }

    #[test]
    fn qualified_name() {
        let t = TableInfo { schema: Some("public".into()), name: "u".into(), kind: "table".into() };
        assert_eq!(t.qualified(), "public.u");
    }
}
