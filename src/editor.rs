//! Open a file in an external editor.
//!
//! The default-editor path is delegated to the cross-platform, maintained
//! [`edit`](https://crates.io/crates/edit) crate (it honours `$VISUAL`/`$EDITOR`
//! and knows platform fallbacks such as Notepad on Windows). When the user names
//! a specific editor we launch it directly — a thin `PATH` lookup via `which`.

use std::path::Path;
use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};

/// Resolve a friendly editor name to its usual executable name.
/// Multi-word values (e.g. `code --wait`) are treated as full commands and left
/// untouched.
pub fn normalize_editor(name: &str) -> String {
    let name = name.trim();
    if name.contains(char::is_whitespace) {
        return name.to_string();
    }
    match name.to_ascii_lowercase().as_str() {
        "sublime" | "sublimetext" | "sublime-text" | "sublime_text" => "subl",
        "vscode" | "vs-code" | "vs_code" => "code",
        "vscodium" | "codium" => "codium",
        "neovim" => "nvim",
        "helix" => "hx",
        "textmate" => "mate",
        "intellij" | "idea" => "idea",
        _ => name,
    }
    .to_string()
}

/// Open `file` in a specific editor named by the user (possibly with flags,
/// e.g. `code --wait`). The file is appended as the final argument.
fn open_named(editor: &str, file: &Path) -> Result<()> {
    let cmd = normalize_editor(editor);
    let mut parts =
        shell_words::split(&cmd).with_context(|| format!("invalid editor command: {cmd:?}"))?;
    if parts.is_empty() {
        bail!("no editor specified");
    }
    let program = parts.remove(0);
    let resolved = which::which(&program).map_err(|_| {
        anyhow!(
            "editor '{program}' was not found on PATH — install it or pass a different one \
             (e.g. `omnicat edit <file> --with code`)"
        )
    })?;

    let status = Command::new(resolved)
        .args(&parts)
        .arg(file)
        .status()
        .with_context(|| format!("failed to launch editor '{program}'"))?;
    if !status.success() {
        bail!("editor '{program}' exited unsuccessfully ({status})");
    }
    Ok(())
}

/// Open `file` in the user's default editor via the `edit` crate.
fn open_default(file: &Path) -> Result<()> {
    edit::edit_file(file).map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => anyhow!(
            "no default editor found — set $EDITOR or $VISUAL, or name one \
             (e.g. `omnicat edit <file> --with code`)"
        ),
        _ => anyhow!("failed to open the default editor: {err}"),
    })
}

/// Open `file` in an editor: a specific one when `explicit` is given, otherwise
/// the system default.
pub fn open(file: &Path, explicit: Option<&str>) -> Result<()> {
    match explicit {
        Some(name) if !name.trim().is_empty() => open_named(name, file),
        _ => open_default(file),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_friendly_names() {
        assert_eq!(normalize_editor("sublime"), "subl");
        assert_eq!(normalize_editor("Sublime-Text"), "subl");
        assert_eq!(normalize_editor("vscode"), "code");
        assert_eq!(normalize_editor("neovim"), "nvim");
        assert_eq!(normalize_editor("helix"), "hx");
    }

    #[test]
    fn passes_unknown_and_multiword_through() {
        assert_eq!(normalize_editor("subl"), "subl");
        assert_eq!(normalize_editor("code --wait"), "code --wait");
    }

    #[test]
    fn named_missing_editor_errors() {
        let path = Path::new("/tmp/does-not-matter.txt");
        let err = open_named("definitely-not-a-real-editor-xyz", path).unwrap_err();
        assert!(err.to_string().contains("not found"));
    }
}
