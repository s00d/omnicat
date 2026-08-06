//! Integration tests for `omnicat db`.

use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf {
    std::env::var("OMNICAT_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/omnicat"))
}

fn demo(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)
}

#[test]
fn db_mysql_dump_query() {
    let sql = demo("demo/db/sample.sql");
    let out = Command::new(bin())
        .args([
            "db",
            sql.to_str().unwrap(),
            "--query",
            "SELECT id, email FROM users LIMIT 2",
        ])
        .output()
        .expect("run db query");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("a@example.com"));
    assert!(stdout.contains("id"));
}

#[test]
fn db_mysql_dump_query_where_single_quotes() {
    let sql = demo("demo/db/sample.sql");
    let out = Command::new(bin())
        .args([
            "db",
            sql.to_str().unwrap(),
            "--query",
            "SELECT email FROM users WHERE status = 'failed' LIMIT 1",
        ])
        .output()
        .expect("run db query filter");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("b@example.com"));
}

#[test]
fn db_mysql_dump_schema() {
    let sql = demo("demo/db/sample.sql");
    let out = Command::new(bin())
        .args(["db", sql.to_str().unwrap(), "--schema", "--json"])
        .output()
        .expect("run db schema");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("users"));
    assert!(stdout.contains("email"));
}

#[test]
fn db_mysql_dump_stats_vs_overview() {
    let sql = demo("demo/db/sample.sql");
    let stats = Command::new(bin())
        .args(["db", sql.to_str().unwrap(), "--stats"])
        .output()
        .expect("stats");
    assert!(stats.status.success());
    let stats_out = String::from_utf8_lossy(&stats.stdout);
    assert!(stats_out.contains("MySQL dump stats"));

    let overview = Command::new(bin())
        .arg("db")
        .arg(sql.to_str().unwrap())
        .output()
        .expect("overview");
    assert!(overview.status.success());
    let ov = String::from_utf8_lossy(&overview.stdout);
    assert!(ov.contains("MySQL dump:"));
    assert!(ov.contains("INSERT stmts"));
}

#[test]
fn db_mysql_dump_table_filter() {
    let sql = demo("demo/db/sample.sql");
    let out = Command::new(bin())
        .args([
            "db",
            sql.to_str().unwrap(),
            "--tables",
            "--table",
            "users",
            "--json",
        ])
        .output()
        .expect("table filter");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("users"));
    assert!(!stdout.contains("orders"));
}

#[test]
fn db_mysql_dump_find() {
    let sql = demo("demo/db/sample.sql");
    let out = Command::new(bin())
        .args(["db", sql.to_str().unwrap(), "--find", "CREATE TABLE"])
        .output()
        .expect("find");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("CREATE TABLE"));
}

#[test]
fn db_mysql_gz_query_if_present() {
    let sql = demo("demo/db/sample.sql.gz");
    if !sql.is_file() {
        return;
    }
    let out = Command::new(bin())
        .args([
            "db",
            sql.to_str().unwrap(),
            "--query",
            "SELECT COUNT(*) AS n FROM users",
        ])
        .output()
        .expect("gz query");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains('3'));
}

#[test]
fn db_mysql_zst_schema_if_present() {
    let sql = demo("demo/db/sample.sql.zst");
    if !sql.is_file() {
        return;
    }
    let out = Command::new(bin())
        .args(["db", sql.to_str().unwrap(), "--schema"])
        .output()
        .expect("zst schema");
    assert!(out.status.success());
}

#[test]
fn db_redis_aof_stats() {
    let aof = demo("demo/db/sample.aof");
    let out = Command::new(bin())
        .args(["db", aof.to_str().unwrap(), "--stats"])
        .output()
        .expect("run aof stats");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("SET"));
}

#[test]
fn db_redis_aof_find() {
    let aof = demo("demo/db/sample.aof");
    let out = Command::new(bin())
        .args(["db", aof.to_str().unwrap(), "--find", "user:"])
        .output()
        .expect("aof find");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("user:"));
}

#[test]
fn db_mysql_datadir_warning() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("ibdata1"), b"x").unwrap();
    std::fs::write(dir.path().join("users.ibd"), b"x").unwrap();
    let out = Command::new(bin())
        .arg("db")
        .arg(dir.path())
        .output()
        .expect("run datadir");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("WARNING"));
    assert!(stdout.contains(".ibd"));
}

#[test]
fn db_redis_rdb_stats_if_fixture_present() {
    let rdb = demo("demo/db/sample.rdb");
    if !rdb.is_file() {
        return;
    }
    let out = Command::new(bin())
        .args(["db", rdb.to_str().unwrap(), "--stats"])
        .output()
        .expect("run rdb stats");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Keys"));
}

#[test]
fn db_print_query_emits_runnable_sql() {
    let sql = demo("demo/db/sample.sql");
    let out = Command::new(bin())
        .args([
            "db",
            sql.to_str().unwrap(),
            "--query",
            "SELECT email FROM users LIMIT 1",
            "--print-query",
        ])
        .output()
        .expect("print-query");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("INSERT INTO `users` (`email`) VALUES"));
    assert!(stdout.contains("'a@example.com'"));
    assert!(!stdout.contains('┌'), "table output with --print-query");
}

#[test]
fn db_mongo_dump_query_if_fixture_present() {
    let dir = demo("demo/db/mongo-dump/sample");
    if !dir.is_dir() {
        return;
    }
    let out = Command::new(bin())
        .args([
            "db",
            dir.to_str().unwrap(),
            "--table",
            "users",
            "--query",
            r#"{"status":"failed"}"#,
        ])
        .output()
        .expect("mongo query");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("b@example.com"));
}

#[test]
fn db_mongo_dump_stats_if_fixture_present() {
    let dir = demo("demo/db/mongo-dump/sample");
    if !dir.is_dir() {
        return;
    }
    let out = Command::new(bin())
        .args(["db", dir.to_str().unwrap(), "--stats"])
        .output()
        .expect("mongo stats");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("users") || stdout.contains("Mongo"));
}

#[test]
fn db_sqlite_query_if_fixture_present() {
    let db = demo("demo/db/sample.sqlite");
    if !db.is_file() {
        return;
    }
    let out = Command::new(bin())
        .args([
            "db",
            db.to_str().unwrap(),
            "--query",
            "SELECT email FROM users WHERE status = 'failed' LIMIT 1",
            "--print-query",
        ])
        .output()
        .expect("sqlite query");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("INSERT INTO"));
    assert!(stdout.contains("b@example.com"));
}

#[test]
fn db_query_bad_column_hint() {
    let sql = demo("demo/db/sample.sql");
    let out = Command::new(bin())
        .args([
            "db",
            sql.to_str().unwrap(),
            "--query",
            "SELECT emial FROM users LIMIT 1",
        ])
        .output()
        .expect("bad column");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Hint:") || stderr.contains("email"),
        "expected column hint, got: {stderr}"
    );
}
