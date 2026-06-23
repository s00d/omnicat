use std::process::Command;

use assert_cmd::assert::OutputAssertExt;
use assert_cmd::cargo::cargo_bin;
use omnicat::preview::gui_available;

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

#[test]
fn native_flag_still_in_help() {
    omnicat()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("-native"));
}
