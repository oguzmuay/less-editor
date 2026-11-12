//! Canlı PostgreSQL testi: yalnız `LESS_TEST_PG_URL` (ör. `postgresql://user:pw@localhost/db`) verilirse çalışır.

use less_db::DriverClient;
use less_db::connections::Connection;
use less_db::types::DbType;
use std::path::PathBuf;

#[test]
fn live_roundtrip() {
    let Ok(url) = std::env::var("LESS_TEST_PG_URL") else {
        eprintln!("LESS_TEST_PG_URL not set; skipping");
        return;
    };
    let c = DriverClient::spawn(&PathBuf::from(env!("CARGO_BIN_EXE_less-driver-postgres"))).unwrap();
    let mut cfg = Connection::new("pg", DbType::Postgresql);
    cfg.url = Some(url);
    c.connect(&cfg).unwrap();
    c.query("DROP TABLE IF EXISTS less_rs_test", false, None).unwrap();
    c.query("CREATE TABLE less_rs_test (id serial PRIMARY KEY, name text)", false, None).unwrap();
    c.query("INSERT INTO less_rs_test (name) VALUES ('a'), (NULL)", false, None).unwrap();
    let r = c.query("SELECT id, name FROM less_rs_test ORDER BY id", true, None).unwrap();
    assert_eq!(r.rows.len(), 2);
    assert_eq!(r.rows[1][1], None);
    assert_eq!(c.primary_key("public.less_rs_test").unwrap(), vec!["id"]);
    assert!(c.query("DELETE FROM less_rs_test", true, None).is_err());
    assert!(c.transaction(&["INSERT INTO less_rs_test (name) VALUES ('t');".into(), "BAD SQL;".into()]).is_err());
    assert_eq!(c.table_data("public.less_rs_test", 10, 0).unwrap().rows.len(), 2);
    c.query("DROP TABLE less_rs_test", false, None).unwrap();
}
