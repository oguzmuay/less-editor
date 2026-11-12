//! Okuma/yazma sorgu sınıflandırıcısı (#68): okuma kesin değilse onay gerekir. `lessmcp/tools/db.lua` portu.
//!
//! Fark: Lua sürümü sırayla `gsub` yapıyordu (`'--'` gibi bir dize sabiti yorum sanılıp satırın kalanını yutabilirdi);
//! burada tek geçişli bir tarayıcı yorum / dize / tırnaklı adı doğru ayırır. Şüphede sonuç hep "okuma değil".

use crate::types::Kind;

/// Sınıflandırma sonucu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryKind {
    /// Onaysız, salt okunur oturumda çalışabilir.
    Read,
    /// Kullanıcı onayı gerekir.
    NeedsApproval,
}

const READ_SQL: &[&str] = &["select", "show", "describe", "desc", "explain", "with", "values", "table", "pragma"];

const WRITE_SQL: &[&str] = &[
    "insert",
    "update",
    "delete",
    "merge",
    "upsert",
    "replace",
    "drop",
    "alter",
    "create",
    "truncate",
    "rename",
    "comment",
    "grant",
    "revoke",
    "call",
    "exec",
    "execute",
    "do",
    "copy",
    "into",
    "load",
    "lock",
    "unlock",
    "vacuum",
    "analyze",
    "reindex",
    "cluster",
    "refresh",
    "attach",
    "detach",
    "set",
    "reset",
    "begin",
    "commit",
    "rollback",
    "savepoint",
    "release",
    "prepare",
    "deallocate",
    "listen",
    "notify",
    "discard",
    "handler",
    "kill",
    "shutdown",
    "pg_terminate_backend",
    "pg_cancel_backend",
    "pg_reload_conf",
    "lo_import",
    "lo_export",
    "dblink",
    "dblink_exec",
    "load_file",
    "sleep",
    "benchmark",
    "xp_cmdshell",
    "sp_configure",
    "openrowset",
    "opendatasource",
    "bulk",
];

const READ_REDIS: &[&str] = &[
    "get",
    "mget",
    "strlen",
    "getrange",
    "exists",
    "type",
    "ttl",
    "pttl",
    "keys",
    "scan",
    "dbsize",
    "info",
    "ping",
    "hget",
    "hmget",
    "hgetall",
    "hkeys",
    "hvals",
    "hlen",
    "hexists",
    "hstrlen",
    "hscan",
    "lrange",
    "llen",
    "lindex",
    "smembers",
    "scard",
    "sismember",
    "smismember",
    "srandmember",
    "sscan",
    "zrange",
    "zrangebyscore",
    "zrevrange",
    "zrevrangebyscore",
    "zcard",
    "zscore",
    "zrank",
    "zrevrank",
    "zcount",
    "zscan",
    "xrange",
    "xrevrange",
    "xlen",
    "bitcount",
    "getbit",
    "pfcount",
    "object",
    "memory",
];

/// Yorumları ve tırnak içlerini (dize, tırnaklı ad, `$tag$` gövdesi, `[ad]`) atar; kapanmamış tırnak/yorum için `None`
/// (yorum kapanmamışsa sorun yok, sonuna kadar atılır).
fn strip(sql: &str) -> Option<String> {
    let c: Vec<char> = sql.chars().collect();
    let mut out = String::with_capacity(sql.len());
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        let next = c.get(i + 1).copied();
        match ch {
            '-' if next == Some('-') => {
                while i < c.len() && c[i] != '\n' {
                    i += 1;
                }
                out.push(' ');
            }
            '/' if next == Some('*') => {
                i += 2;
                while i < c.len() && !(c[i] == '*' && c.get(i + 1) == Some(&'/')) {
                    i += 1;
                }
                i += 2;
                out.push(' ');
            }
            '$' => match dollar_tag(&c, i) {
                Some(tag_len) => {
                    let tag: Vec<char> = c[i..i + tag_len].to_vec();
                    let mut j = i + tag_len;
                    let mut closed = false;
                    while j + tag_len <= c.len() {
                        if c[j..j + tag_len] == tag[..] {
                            closed = true;
                            break;
                        }
                        j += 1;
                    }
                    if !closed {
                        return None;
                    }
                    i = j + tag_len;
                    out.push_str(" '' ");
                }
                None => {
                    out.push(' ');
                    i += 1;
                }
            },
            '\'' | '"' | '`' => {
                let mut j = i + 1;
                while j < c.len() && c[j] != ch {
                    j += 1;
                }
                if j >= c.len() {
                    return None;
                }
                out.push_str(if ch == '\'' { " '' " } else { " x " });
                i = j + 1;
            }
            '[' => {
                match c[i + 1..].iter().position(|&x| x == ']') {
                    Some(p) => i += p + 2,
                    None => i += 1,
                }
                out.push_str(" x ");
            }
            _ => {
                out.push(ch);
                i += 1;
            }
        }
    }
    Some(out)
}

/// `$tag$` açılışının uzunluğu (`$$` dahil); `$1` gibi parametreler eşleşmez.
fn dollar_tag(c: &[char], i: usize) -> Option<usize> {
    let mut j = i + 1;
    while j < c.len() && (c[j].is_ascii_alphanumeric() || c[j] == '_') {
        j += 1;
    }
    (c.get(j) == Some(&'$') && !c[i + 1..j].first().is_some_and(|x| x.is_ascii_digit())).then_some(j + 1 - i)
}

fn words(s: &str) -> impl Iterator<Item = &str> {
    s.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_')).filter(|w| !w.is_empty())
}

fn is_read_sql(text: &str) -> bool {
    let Some(clean) = strip(text) else { return false };
    let clean = clean.to_lowercase();
    let mut any = false;
    for stmt in clean.split(';') {
        let trimmed = stmt.trim_start();
        let first: String = trimmed.chars().take_while(|ch| ch.is_ascii_lowercase() || *ch == '_').collect();
        if first.is_empty() {
            if stmt.trim().is_empty() {
                continue;
            }
            return false;
        }
        any = true;
        if !READ_SQL.contains(&first.as_str()) || (first == "pragma" && stmt.contains('=')) {
            return false;
        }
        if words(stmt).any(|w| WRITE_SQL.contains(&w)) {
            return false;
        }
    }
    any
}

fn is_read_redis(text: &str) -> bool {
    let mut any = false;
    for line in text.lines() {
        if let Some(cmd) = line.split_whitespace().next() {
            any = true;
            if !READ_REDIS.contains(&cmd.to_lowercase().as_str()) {
                return false;
            }
        }
    }
    any
}

/// Sorgu onaysız (salt okunur) çalıştırılabilir mi? Şüphede hayır; MongoDB her zaman onay ister.
pub fn is_read(kind: Kind, text: &str) -> bool {
    match kind {
        Kind::Sql => is_read_sql(text),
        Kind::Redis => is_read_redis(text),
        Kind::Mongo => false,
    }
}

/// `is_read` sonucunu enum olarak verir.
pub fn classify(kind: Kind, text: &str) -> QueryKind {
    if is_read(kind, text) { QueryKind::Read } else { QueryKind::NeedsApproval }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(s: &str) -> bool {
        is_read(Kind::Sql, s)
    }

    #[test]
    fn plain_reads_68() {
        for q in [
            "SELECT 1",
            "select * from users where name = 'delete me';",
            "  SHOW TABLES",
            "EXPLAIN SELECT * FROM t",
            "WITH a AS (SELECT 1) SELECT * FROM a",
            "DESC t",
            "DESCRIBE t",
            "VALUES (1)",
            "TABLE t",
            "PRAGMA table_info(t)",
            "select 1; select 2",
            "select 1;;",
            "-- hi\nselect 1",
            "/* x */ select 1 /* y */",
        ] {
            assert!(r(q), "{q}");
        }
    }

    #[test]
    fn writes_need_approval_68() {
        for q in [
            "INSERT INTO t VALUES (1)",
            "update t set a=1",
            "delete from t",
            "DROP TABLE t",
            "create table x(a int)",
            "truncate t",
            "select * into t2 from t",
            "select 1; drop table t",
            "with x as (delete from t returning *) select * from x",
            "select pg_terminate_backend(1)",
            "select sleep(10)",
            "select load_file('/etc/passwd')",
            "copy t to '/tmp/x'",
            "set role admin",
            "begin",
            "commit",
            "vacuum",
            "grant all on t to u",
            "call proc()",
            "exec xp_cmdshell 'dir'",
            "PRAGMA journal_mode = WAL",
            "analyze t",
            "",
            "   ",
            ";",
            "1",
        ] {
            assert!(!r(q), "{q}");
        }
    }

    #[test]
    fn keyword_inside_string_or_comment_is_ignored_68() {
        assert!(r("select 'drop table x' as s"));
        assert!(r("select \"update\" from t"));
        assert!(r("select `delete` from t"));
        assert!(r("select [insert] from t"));
        assert!(r("select 1 -- drop table t"));
        assert!(r("select $$delete from t$$"));
        assert!(r("select $tag$ insert; drop $tag$"));
    }

    #[test]
    fn hidden_write_is_caught_68() {
        assert!(!r("select '--'; drop table t"));
        assert!(!r("select 1 /* x */; delete from t"));
        assert!(!r("select 1;\n-- c\ndelete from t"));
        assert!(!r("SELECT 1; Drop Table t"));
        assert!(!r("select 'it''s'; drop table t"));
    }

    #[test]
    fn unterminated_quote_rejected_68() {
        assert!(!r("select 'abc"));
        assert!(!r("select \"abc"));
        assert!(!r("select $$abc"));
        assert!(!r("select 'a\\'; drop table t; --'"));
    }

    #[test]
    fn dollar_params_are_not_quotes() {
        assert!(r("select $1, $2"));
    }

    #[test]
    fn redis_68() {
        assert!(is_read(Kind::Redis, "GET a\nHGETALL h\n\nKEYS *"));
        assert!(!is_read(Kind::Redis, "GET a\nSET a 1"));
        assert!(!is_read(Kind::Redis, "FLUSHALL"));
        assert!(!is_read(Kind::Redis, ""));
        assert!(is_read(Kind::Redis, "  ttl k"));
    }

    #[test]
    fn mongo_always_needs_approval_68() {
        assert!(!is_read(Kind::Mongo, "db.users.find({})"));
        assert_eq!(classify(Kind::Mongo, "db.a.find()"), QueryKind::NeedsApproval);
        assert_eq!(classify(Kind::Sql, "select 1"), QueryKind::Read);
    }
}
