//! Derlenmiş sürücü ikilisini gerçek bir SQLite dosyasına karşı süren uçtan uca testler (#47, #68, #88).

use less_db::connections::Connection;
use less_db::types::DbType;
use less_db::{DriverClient, DriverError};
use std::path::PathBuf;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_less-driver-sqlite"))
}

fn connected(dir: &std::path::Path) -> DriverClient {
    let client = DriverClient::spawn(&bin()).unwrap();
    let mut cfg = Connection::new("t", DbType::Sqlite);
    cfg.file = Some(dir.join("t.db").display().to_string());
    let r = client.connect(&cfg).unwrap();
    assert!(r.server_version.is_some());
    client
}

fn seed(c: &DriverClient) {
    c.query(
        "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL, note TEXT DEFAULT 'n');
         INSERT INTO users (name, note) VALUES ('ann', NULL), ('bob', 'x'), ('日本', 'y');
         CREATE TABLE pair (a INTEGER, b INTEGER, PRIMARY KEY (b, a));
         CREATE VIEW v AS SELECT name FROM users;",
        false,
        None,
    )
    .unwrap();
}

#[test]
fn query_columns_pk_and_tables() {
    let dir = tempfile::tempdir().unwrap();
    let c = connected(dir.path());
    seed(&c);
    let r = c.query("SELECT id, name, note FROM users ORDER BY id", true, None).unwrap();
    assert_eq!(r.columns, vec!["id", "name", "note"]);
    assert_eq!(r.rows.len(), 3);
    assert_eq!(r.rows[0], vec![Some("1".into()), Some("ann".into()), None]);
    assert_eq!(r.rows[2][1].as_deref(), Some("日本"));

    let cols = c.columns("users").unwrap();
    assert_eq!(cols.len(), 3);
    assert!(cols[0].primary_key && !cols[1].nullable && cols[2].default.as_deref() == Some("'n'"));
    assert_eq!(c.primary_key("users").unwrap(), vec!["id"]);
    assert_eq!(c.primary_key("pair").unwrap(), vec!["b", "a"]);
    assert!(c.columns("nope").is_err());

    let tables = c.list_tables(None).unwrap();
    let names: Vec<_> = tables.iter().map(|t| (t.name.as_str(), t.kind.as_str())).collect();
    assert_eq!(names, vec![("pair", "table"), ("users", "table"), ("v", "view")]);
    assert_eq!(c.list_schemas().unwrap(), vec!["main"]);
}

#[test]
fn limit_and_table_data() {
    let dir = tempfile::tempdir().unwrap();
    let c = connected(dir.path());
    seed(&c);
    let r = c.query("SELECT * FROM users", true, Some(2)).unwrap();
    assert_eq!(r.rows.len(), 2);
    assert!(r.notice.is_some());
    let page = c.table_data("users", 2, 1).unwrap();
    assert_eq!(page.rows.len(), 2);
    assert_eq!(page.rows[0][1].as_deref(), Some("bob"));
}

#[test]
fn read_only_rejects_writes_68() {
    let dir = tempfile::tempdir().unwrap();
    let c = connected(dir.path());
    seed(&c);
    let err = c.query("DELETE FROM users", true, None).unwrap_err();
    assert!(matches!(err, DriverError::Remote { .. }), "{err:?}");
    assert_eq!(c.query("SELECT count(*) FROM users", true, None).unwrap().rows[0][0].as_deref(), Some("3"));
    let w = c.query("DELETE FROM users WHERE id = 3", false, None).unwrap();
    assert_eq!(w.affected, Some(1));
}

#[test]
fn transaction_commits_and_rolls_back_88() {
    let dir = tempfile::tempdir().unwrap();
    let c = connected(dir.path());
    seed(&c);
    let ok = c
        .transaction(&[
            "UPDATE \"users\" SET \"name\" = 'zed' WHERE \"id\" = '1';".to_string(),
            "INSERT INTO \"users\" (\"name\") VALUES ('new');".to_string(),
        ])
        .unwrap();
    assert_eq!(ok.affected, 2);
    let bad = c.transaction(&[
        "UPDATE users SET name = 'lost' WHERE id = 2;".to_string(),
        "INSERT INTO users (id, name) VALUES (1, 'dup');".to_string(),
    ]);
    assert!(bad.is_err());
    let r = c.query("SELECT name FROM users ORDER BY id", true, None).unwrap();
    let names: Vec<_> = r.rows.iter().map(|r| r[0].clone().unwrap()).collect();
    assert_eq!(names, vec!["zed", "bob", "日本", "new"]);
}

#[test]
fn transaction_after_read_only_query_still_writes_88() {
    let dir = tempfile::tempdir().unwrap();
    let c = connected(dir.path());
    seed(&c);
    c.query("SELECT 1", true, None).unwrap();
    assert!(c.transaction(&["DELETE FROM users WHERE id = 1;".to_string()]).is_ok());
}

#[test]
fn sql_errors_are_remote_errors() {
    let dir = tempfile::tempdir().unwrap();
    let c = connected(dir.path());
    let err = c.query("SELEC 1", false, None).unwrap_err();
    assert!(matches!(err, DriverError::Remote { .. }));
    assert!(c.query("SELECT 1", true, None).is_ok());
}

#[test]
fn must_connect_first() {
    let c = DriverClient::spawn(&bin()).unwrap();
    let err = c.list_tables(None).unwrap_err();
    assert!(matches!(err, DriverError::Remote { code, .. } if code == less_db::protocol::ERR_NOT_CONNECTED));
    let err = c.call_raw("bogus", serde_json::Value::Null, std::time::Duration::from_secs(5)).unwrap_err();
    assert!(matches!(err, DriverError::Remote { code, .. } if code == less_db::protocol::ERR_METHOD_NOT_FOUND));
}

#[test]
fn wrong_type_and_missing_file_fail_cleanly() {
    let c = DriverClient::spawn(&bin()).unwrap();
    assert!(c.connect(&Connection::new("x", DbType::Postgresql)).is_err());
    assert!(c.connect(&Connection::new("x", DbType::Sqlite)).is_err());
}

#[test]
fn driver_crash_is_error_not_panic() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("die.sh");
    std::fs::write(&script, "#!/bin/sh\necho oops 1>&2\nexit 3\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let c = DriverClient::spawn(&script).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));
    let err = c.list_schemas().unwrap_err();
    assert!(matches!(err, DriverError::Crashed(_)), "{err:?}");
    assert!(DriverClient::spawn(std::path::Path::new("/nonexistent/driver")).is_err());
}

/// Ölen sürücünün stdout'unu başka bir süreç tutsa da (EOF gelmez) istek `Timeout` beklemeden `Crashed` döner.
#[test]
fn driver_crash_detected_while_stdout_is_held_open() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("die.sh");
    std::fs::write(&script, "#!/bin/sh\nsleep 30 &\nexit 3\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let c = DriverClient::spawn(&script).unwrap();
    let started = std::time::Instant::now();
    let err = c.list_schemas().unwrap_err();
    assert!(matches!(err, DriverError::Crashed(_)), "{err:?}");
    assert!(started.elapsed() < std::time::Duration::from_secs(5), "took {:?}", started.elapsed());
}

#[test]
fn cancel_interrupts_running_query() {
    let dir = tempfile::tempdir().unwrap();
    let c = std::sync::Arc::new(connected(dir.path()));
    let c2 = std::sync::Arc::clone(&c);
    let h = std::thread::spawn(move || {
        c2.query("WITH RECURSIVE r(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM r) SELECT count(*) FROM r", true, None)
    });
    std::thread::sleep(std::time::Duration::from_millis(300));
    c.cancel().unwrap();
    let r = h.join().unwrap();
    assert!(r.is_err());
}
