//! Open a file with the OS default application or a chosen program.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

/// Open `path` with the OS default app, or with `editor` when set.
///
/// `editor` is a shell-like command string (`subl`, `code -n`, …). The launcher
/// is started detached and not waited on — GUI CLI shims (`subl`, `code`) often
/// exit non-zero or hang if the parent waits on them.
pub fn open_path(path: &Path, editor: Option<&str>) -> Result<()> {
    if !path.exists() {
        bail!("path not found: {}", path.display());
    }
    let path = resolve_path(path);
    match editor.map(str::trim).filter(|s| !s.is_empty()) {
        None => open::that_detached(&path).with_context(|| {
            format!(
                "failed to open {} with the system default application",
                path.display()
            )
        }),
        Some(cmd) => open_with_command(&path, cmd),
    }
}

fn resolve_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn open_with_command(path: &Path, cmd: &str) -> Result<()> {
    let parts =
        shell_words::split(cmd).with_context(|| format!("invalid --editor value: {cmd}"))?;
    let (program, args) = parts
        .split_first()
        .ok_or_else(|| anyhow::anyhow!("--editor must name a program"))?;

    let mut command = build_editor_command(program, args, path)?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("failed to run `{cmd}` for {}", path.display()))?;
    Ok(())
}

fn build_editor_command(program: &str, args: &[String], path: &Path) -> Result<Command> {
    // On macOS, CLI shims like `subl` live inside `Foo.app` and talk to the
    // running app over a socket that is often stuck. Prefer `open -a Foo.app`
    // which hands the document to LaunchServices instead.
    #[cfg(target_os = "macos")]
    if let Some(app) = macos_app_bundle_for(program) {
        let mut command = Command::new("/usr/bin/open");
        command.arg("-a").arg(&app).arg(path);
        if !args.is_empty() {
            command.arg("--args").args(args);
        }
        return Ok(command);
    }

    let mut command = Command::new(program);
    command.args(args).arg(path);
    Ok(command)
}

#[cfg(target_os = "macos")]
fn macos_app_bundle_for(program: &str) -> Option<PathBuf> {
    let resolved = which::which(program).ok()?;
    let resolved = resolved.canonicalize().ok()?;
    resolved.ancestors().find_map(|ancestor| {
        (ancestor.extension().and_then(|e| e.to_str()) == Some("app"))
            .then(|| ancestor.to_path_buf())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_open_missing_path_errors() {
        let err = open_path(Path::new("/tmp/omnicat-no-such-file-xyz"), None).unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn editor_open_missing_path_errors() {
        let err = open_path(Path::new("/tmp/omnicat-no-such-file-xyz"), Some("subl")).unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn empty_editor_falls_back_to_system_path_check() {
        let err = open_path(Path::new("/tmp/omnicat-no-such-file-xyz"), Some("  ")).unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_resolves_subl_into_sublime_app() {
        if which::which("subl").is_err() {
            return;
        }
        let app = macos_app_bundle_for("subl").expect("subl should live inside Sublime Text.app");
        assert!(
            app.ends_with("Sublime Text.app"),
            "unexpected app bundle: {}",
            app.display()
        );
    }
}
