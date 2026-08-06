use std::process::Command;

use assert_cmd::assert::OutputAssertExt;
use assert_cmd::cargo::cargo_bin;
use omnicat::preview::gui_available;
use predicates::prelude::PredicateBooleanExt;

fn omnicat() -> Command {
    Command::new(cargo_bin("omnicat"))
}

#[test]
fn version_prints_omnicat() {
    omnicat()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicates::str::contains("omnicat"));
}

#[test]
fn help_mentions_preview() {
    omnicat()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("--preview"));
}

#[test]
fn init_zsh_emits_wrapper() {
    omnicat()
        .args(["init", "zsh"])
        .assert()
        .success()
        .stdout(predicates::str::contains("command omnicat"));
}

#[test]
fn init_unknown_shell_errors() {
    omnicat()
        .args(["init", "fish"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("unsupported shell"));
}

#[test]
fn passthrough_single_file_piped() {
    let tmp = tempfile::NamedTempFile::with_suffix(".md").unwrap();
    std::fs::write(tmp.path(), "# Title\n\nSENTINEL-MD\n").unwrap();
    let expected = std::fs::read_to_string(tmp.path()).unwrap();

    let output = omnicat()
        .arg(tmp.path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .unwrap();

    assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
}

#[test]
fn passthrough_multi_file() {
    let a = tempfile::NamedTempFile::new().unwrap();
    let b = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(a.path(), "aaa\n").unwrap();
    std::fs::write(b.path(), "bbb\n").unwrap();

    let expected = format!(
        "{}{}",
        std::fs::read_to_string(a.path()).unwrap(),
        std::fs::read_to_string(b.path()).unwrap()
    );

    let output = omnicat()
        .args([a.path(), b.path()])
        .stdout(std::process::Stdio::piped())
        .output()
        .unwrap();

    assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
}

#[test]
fn status_lists_handlers() {
    omnicat()
        .arg("-status")
        .assert()
        .success()
        .stdout(predicates::str::contains("markdown"))
        .stdout(predicates::str::contains("BUILTIN"))
        .stdout(predicates::str::contains("EXTERNAL"))
        .stdout(predicates::str::contains("GUI SETTINGS"));
}

#[test]
fn preview_headless_returns_false() {
    std::env::set_var("OMNICAT_NO_GUI", "1");
    assert!(!gui_available());
    std::env::remove_var("OMNICAT_NO_GUI");
}

#[cfg(unix)]
#[test]
fn open_missing_file_errors() {
    omnicat()
        .args(["open", "/tmp/omnicat-definitely-missing-xyz"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("not found"));
}

#[test]
fn help_mentions_system_open() {
    omnicat()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("system default"));
}

#[test]
fn native_flag_still_in_help() {
    omnicat()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("-native"));
}

#[test]
fn help_mentions_inspect() {
    omnicat()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("--info"))
        .stdout(predicates::str::contains("--capabilities"));
}

#[test]
fn info_json_works_when_piped() {
    let tmp = tempfile::NamedTempFile::with_suffix(".txt").unwrap();
    std::fs::write(tmp.path(), "hello world\n").unwrap();
    omnicat()
        .args(["--info", "--json"])
        .arg(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains(r#""report": "info""#))
        .stdout(predicates::str::contains("encoding"));
}

#[test]
fn capabilities_global() {
    omnicat()
        .arg("--capabilities")
        .assert()
        .success()
        .stdout(predicates::str::contains("database"))
        .stdout(predicates::str::contains("preview"));
}

#[test]
fn schema_csv() {
    let tmp = tempfile::NamedTempFile::with_suffix(".csv").unwrap();
    std::fs::write(tmp.path(), "id,name\n1,a\n2,b\n").unwrap();
    omnicat()
        .arg("--schema")
        .arg(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("id"))
        .stdout(predicates::str::contains("name"));
}

#[test]
fn find_in_text() {
    let tmp = tempfile::NamedTempFile::with_suffix(".md").unwrap();
    std::fs::write(tmp.path(), "# Title\nTODO: fix me\n").unwrap();
    omnicat()
        .args(["--find", "TODO"])
        .arg(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("TODO"));
}

#[test]
fn find_jsonl_field_aware() {
    let tmp = tempfile::NamedTempFile::with_suffix(".jsonl").unwrap();
    std::fs::write(
        tmp.path(),
        r#"{"level":"info","message":"ok"}
{"level":"error","message":"boom"}
"#,
    )
    .unwrap();
    omnicat()
        .args(["--find", "level:error"])
        .arg(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("boom"));
}

#[test]
fn log_context_around() {
    let tmp = tempfile::NamedTempFile::with_suffix(".log").unwrap();
    std::fs::write(
        tmp.path(),
        "2026-08-06T12:42:15Z INFO before\n2026-08-06T12:42:17Z ERROR center\n2026-08-06T12:42:19Z INFO after\n",
    )
    .unwrap();
    omnicat()
        .args(["log", "--around", "12:42:17", "--context", "1"])
        .arg(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("center"))
        .stdout(predicates::str::contains("before"));
}

#[test]
fn query_csv_predicate() {
    let tmp = tempfile::NamedTempFile::with_suffix(".csv").unwrap();
    std::fs::write(tmp.path(), "name,age\nalice,30\nbob,10\n").unwrap();
    omnicat()
        .args(["--query", "age > 18"])
        .arg(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("alice"));
}

#[test]
fn virtual_zip_path_text() {
    let dir = tempfile::tempdir().unwrap();
    let zip_path = dir.path().join("t.zip");
    {
        use std::io::Write;
        let file = std::fs::File::create(&zip_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file("inner.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"virtual-content").unwrap();
        zip.finish().unwrap();
    }
    let path = format!("{}/inner.txt", zip_path.display());
    omnicat()
        .args(["--text", &path])
        .assert()
        .success()
        .stdout(predicates::str::contains("virtual-content"));
}

#[test]
fn hash_json_has_blake3() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), b"hash-me").unwrap();
    omnicat()
        .args(["--hash", "--json"])
        .arg(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains(r#""report": "hash""#))
        .stdout(predicates::str::contains("blake3"));
}

#[test]
fn duplicates_finds_copies() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.bin");
    let b = dir.path().join("b.bin");
    let c = dir.path().join("unique.bin");
    std::fs::write(&a, b"same-bytes-here").unwrap();
    std::fs::write(&b, b"same-bytes-here").unwrap();
    std::fs::write(&c, b"other").unwrap();
    omnicat()
        .arg("--duplicates")
        .arg(dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("Duplicate files"))
        .stdout(predicates::str::contains("reclaimable"));
}

#[test]
fn jsonl_head_uses_log_columns() {
    let tmp = tempfile::NamedTempFile::with_suffix(".jsonl").unwrap();
    std::fs::write(
        tmp.path(),
        r#"{"ts":"12:01","level":"INFO","service":"api","msg":"Started"}
{"ts":"12:02","level":"ERROR","service":"db","msg":"Timeout"}
"#,
    )
    .unwrap();
    omnicat()
        .args(["--head", "5"])
        .arg(tmp.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("TIME"))
        .stdout(predicates::str::contains("LEVEL"))
        .stdout(predicates::str::contains("MESSAGE"))
        .stdout(predicates::str::contains("Timeout"));
}

#[test]
fn sqlite_diff_reports_schema() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("left.sqlite");
    let b = dir.path().join("right.sqlite");
    {
        let conn = rusqlite::Connection::open(&a).unwrap();
        conn.execute_batch("CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT);")
            .unwrap();
    }
    {
        let conn = rusqlite::Connection::open(&b).unwrap();
        conn.execute_batch("CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT NOT NULL);")
            .unwrap();
    }
    omnicat()
        .args(["--diff"])
        .arg(&a)
        .arg(&b)
        .assert()
        .success()
        .stdout(predicates::str::contains("schema:"))
        .stdout(predicates::str::contains("users.email"));
}

#[test]
fn info_image_includes_bit_depth() {
    omnicat()
        .args(["--info", "--json"])
        .arg("demo/files/sample.png")
        .assert()
        .success()
        .stdout(predicates::str::contains("bit_depth"));
}

#[test]
fn log_stats_json() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.log");
    std::fs::write(
        &path,
        r#"{"ts":"2026-01-01T12:00:00Z","level":"error","service":"api","msg":"timeout"}
{"ts":"2026-01-01T12:00:01Z","level":"info","service":"api","msg":"ok"}
"#,
    )
    .unwrap();
    omnicat()
        .args(["log", "--stats", "--json"])
        .arg(&path)
        .assert()
        .success()
        .stdout(predicates::str::contains("messages"))
        .stdout(predicates::str::contains("ERROR"));
}

#[test]
fn log_where_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.log");
    std::fs::write(
        &path,
        r#"{"level":"error","msg":"bad"}
{"level":"info","msg":"good"}
"#,
    )
    .unwrap();
    omnicat()
        .args(["log", "--errors"])
        .arg(&path)
        .assert()
        .success()
        .stdout(predicates::str::contains("bad"))
        .stdout(predicates::str::contains("good").not());
}

#[test]
fn log_subcommand_in_help() {
    omnicat()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("log"));
}
