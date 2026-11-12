//! Veri kaynağı türleri (ikon adı, varsayılan port, lehçe tırnaklama) ve değer sabitleri.

use serde::{Deserialize, Serialize};

/// Desteklenen veri kaynağı türü. Seri biçimi LESS-nvim `connections.json` ile aynıdır (`postgresql`, `mssql` yok).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DbType {
    #[serde(alias = "postgres")]
    Postgresql,
    Mysql,
    Mariadb,
    #[serde(alias = "mssql")]
    Sqlserver,
    #[serde(alias = "sqlite3")]
    Sqlite,
    #[serde(alias = "rediss")]
    Redis,
    #[serde(alias = "mongo")]
    Mongodb,
}

/// Sorgu dili ailesi (konsol uzantısı ve sınıflandırıcı seçimi).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Sql,
    Redis,
    Mongo,
}

impl DbType {
    pub const ALL: [DbType; 7] = [
        DbType::Postgresql,
        DbType::Mysql,
        DbType::Mariadb,
        DbType::Sqlserver,
        DbType::Sqlite,
        DbType::Redis,
        DbType::Mongodb,
    ];

    /// Kayıt dosyasındaki kimlik.
    pub fn id(self) -> &'static str {
        match self {
            DbType::Postgresql => "postgresql",
            DbType::Mysql => "mysql",
            DbType::Mariadb => "mariadb",
            DbType::Sqlserver => "sqlserver",
            DbType::Sqlite => "sqlite",
            DbType::Redis => "redis",
            DbType::Mongodb => "mongodb",
        }
    }

    /// Kimlik ya da URL şeması takma adından tür.
    pub fn from_id(id: &str) -> Option<DbType> {
        let id = id.to_ascii_lowercase();
        match id.as_str() {
            "postgres" => Some(DbType::Postgresql),
            "mssql" => Some(DbType::Sqlserver),
            "sqlite3" => Some(DbType::Sqlite),
            "rediss" => Some(DbType::Redis),
            "mongo" => Some(DbType::Mongodb),
            other => DbType::ALL.into_iter().find(|t| t.id() == other),
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            DbType::Postgresql => "PostgreSQL",
            DbType::Mysql => "MySQL",
            DbType::Mariadb => "MariaDB",
            DbType::Sqlserver => "SQL Server",
            DbType::Sqlite => "SQLite",
            DbType::Redis => "Redis",
            DbType::Mongodb => "MongoDB",
        }
    }

    /// UI'nın ikon takımından seçeceği genel ad.
    pub fn icon_name(self) -> &'static str {
        match self {
            DbType::Postgresql => "postgresql",
            DbType::Mysql | DbType::Mariadb => "mysql",
            DbType::Sqlserver => "sqlserver",
            DbType::Sqlite => "sqlite",
            DbType::Redis => "redis",
            DbType::Mongodb => "mongodb",
        }
    }

    /// Dosya tabanlı türlerde (SQLite) yok.
    pub fn default_port(self) -> Option<u16> {
        match self {
            DbType::Postgresql => Some(5432),
            DbType::Mysql | DbType::Mariadb => Some(3306),
            DbType::Sqlserver => Some(1433),
            DbType::Sqlite => None,
            DbType::Redis => Some(6379),
            DbType::Mongodb => Some(27017),
        }
    }

    pub fn is_file_based(self) -> bool {
        self == DbType::Sqlite
    }

    pub fn kind(self) -> Kind {
        match self {
            DbType::Redis => Kind::Redis,
            DbType::Mongodb => Kind::Mongo,
            _ => Kind::Sql,
        }
    }

    /// Konsol dosyası uzantısı.
    pub fn console_ext(self) -> &'static str {
        match self.kind() {
            Kind::Sql => "sql",
            Kind::Redis => "redis",
            Kind::Mongo => "js",
        }
    }

    /// Sürücü ikilisinin adı (mongodb/redis/mysql/sqlserver sürücüleri henüz yok, bkz. DRIVERS.md).
    pub fn driver_binary(self) -> String {
        let name = match self {
            DbType::Mariadb => "mysql",
            DbType::Postgresql => "postgres",
            other => other.id(),
        };
        format!("less-driver-{name}")
    }
}

/// Tek bir tanımlayıcıyı türün lehçesinde tırnaklar (nokta bölünmez).
pub fn quote_ident(ty: DbType, s: &str) -> String {
    match ty {
        DbType::Mysql | DbType::Mariadb => format!("`{}`", s.replace('`', "``")),
        DbType::Sqlserver => format!("[{}]", s.replace(']', "]]")),
        _ => format!("\"{}\"", s.replace('"', "\"\"")),
    }
}

/// `şema.tablo` adını tırnaklar; SQLite ve şemasız adlarda tüm ad tek tanımlayıcıdır.
pub fn quote_table(ty: DbType, name: &str) -> String {
    match name.split_once('.') {
        Some((schema, table)) if ty != DbType::Sqlite => {
            format!("{}.{}", quote_ident(ty, schema), quote_ident(ty, table))
        }
        _ => quote_ident(ty, name),
    }
}

/// Değer sabiti: hep dize sabiti (MySQL'de `\` kaçışı, SQL Server `N'…'`); `None` → `NULL`.
pub fn value_literal(ty: DbType, v: Option<&str>) -> String {
    let Some(v) = v else { return "NULL".into() };
    let mut s = v.replace('\'', "''");
    if matches!(ty, DbType::Mysql | DbType::Mariadb) {
        s = s.replace('\\', "\\\\");
    }
    if ty == DbType::Sqlserver { format!("N'{s}'") } else { format!("'{s}'") }
}

/// İfadeleri tek transaction'a sarar (#88).
pub fn transaction_sql(ty: DbType, statements: &[String]) -> String {
    let body = statements.join("\n");
    match ty {
        DbType::Sqlserver => format!("SET XACT_ABORT ON;\nBEGIN TRANSACTION;\n{body}\nCOMMIT TRANSACTION;"),
        DbType::Mysql | DbType::Mariadb => format!("START TRANSACTION;\n{body}\nCOMMIT;"),
        _ => format!("BEGIN;\n{body}\nCOMMIT;"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_88() {
        assert_eq!(value_literal(DbType::Postgresql, Some("it's")), "'it''s'");
        assert_eq!(value_literal(DbType::Mysql, Some(r"a\b'")), r"'a\\b'''");
        assert_eq!(value_literal(DbType::Sqlserver, Some("x")), "N'x'");
        assert_eq!(value_literal(DbType::Sqlite, None), "NULL");
    }

    #[test]
    fn quoting() {
        assert_eq!(quote_table(DbType::Postgresql, "public.users"), "\"public\".\"users\"");
        assert_eq!(quote_table(DbType::Sqlite, "a.b"), "\"a.b\"");
        assert_eq!(quote_table(DbType::Mysql, "db.t"), "`db`.`t`");
        assert_eq!(quote_ident(DbType::Sqlserver, "a]b"), "[a]]b]");
    }

    #[test]
    fn ids_and_aliases() {
        assert_eq!(DbType::from_id("postgres"), Some(DbType::Postgresql));
        assert_eq!(DbType::from_id("MSSQL"), Some(DbType::Sqlserver));
        assert_eq!(DbType::from_id("nope"), None);
        assert_eq!(DbType::Mariadb.default_port(), Some(3306));
        assert_eq!(DbType::Sqlite.default_port(), None);
        assert_eq!(DbType::Postgresql.driver_binary(), "less-driver-postgres");
    }

    #[test]
    fn serde_alias() {
        let t: DbType = serde_json::from_str("\"postgres\"").unwrap();
        assert_eq!(t, DbType::Postgresql);
        assert_eq!(serde_json::to_string(&DbType::Sqlserver).unwrap(), "\"sqlserver\"");
    }

    #[test]
    fn transaction_wrap_88() {
        let s = vec!["X;".to_string()];
        assert_eq!(transaction_sql(DbType::Sqlite, &s), "BEGIN;\nX;\nCOMMIT;");
        assert!(transaction_sql(DbType::Mysql, &s).starts_with("START TRANSACTION;"));
    }
}
