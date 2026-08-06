//! Open a file in an external editor, auto-detecting a sensible one.
//!
//! Detection order: an explicitly requested editor, then `$VISUAL`, then
//! `$EDITOR`, then the first known editor found on `PATH`. It stays lightweight:
//! it only does `PATH` lookups (via the `which` crate) and never spawns a probe.

use std::path::Path;
use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};

/// Editors probed on `PATH` when nothing is configured, in order of preference.
/// Terminal editors come first since omnicat runs in a terminal.
const CANDIDATES: &[&str] = &[
    "nvim", "vim", "hx", "micro", "nano", "vi", "code", "subl", "zed", "emacs", "gedit", "kate",
    "mate",
];

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

/// First whitespace-separated token of a command string (the program name).
fn program_token(cmd: &str) -> Result<String> {
    shell_words::split(cmd)
        .ok()
        .and_then(|mut parts| {
            if parts.is_empty() {
                None
            } else {
                Some(parts.remove(0))
            }
        })
        .ok_or_else(|| anyhow!("could not parse editor command: {cmd:?}"))
}

/// Pick the first candidate that `is_available` accepts. Extracted for testing.
fn pick_available<F>(candidates: &[&str], is_available: F) -> Option<String>
where
    F: Fn(&str) -> bool,
{
    candidates
        .iter()
        .find(|c| is_available(c))
        .map(|c| c.to_string())
}

fn on_path(program: &str) -> bool {
    which::which(program).is_ok()
}

/// Resolve which editor command to use.
pub fn resolve_editor(explicit: Option<&str>) -> Result<String> {
    if let Some(requested) = explicit {
        let requested = requested.trim();
        if requested.is_empty() {
            bail!("no editor specified");
        }
        let cmd = normalize_editor(requested);
        let program = program_token(&cmd)?;
        if !on_path(&program) {
            bail!(
                "editor '{program}' was not found on PATH — install it or pass a different one \
                 (e.g. `omnicat edit <file> --with code`)"
            );
        }
        return Ok(cmd);
    }

    for var in ["VISUAL", "EDITOR"] {
        if let Ok(value) = std::env::var(var) {
            let value = value.trim().to_string();
            if value.is_empty() {
                continue;
            }
            if let Ok(program) = program_token(&value) {
                if on_path(&program) {
                    return Ok(value);
                }
            }
        }
    }

    if let Some(found) = pick_available(CANDIDATES, on_path) {
        return Ok(found);
    }

    bail!(
        "no editor found — set $EDITOR or $VISUAL, or choose one explicitly \
         (e.g. `omnicat edit <file> --with code`)"
    )
}

/// Launch `editor_cmd` with `file` appended as the final argument.
pub fn open_in_editor(editor_cmd: &str, file: &Path) -> Result<()> {
    let mut parts = shell_words::split(editor_cmd)
        .with_context(|| format!("invalid editor command: {editor_cmd:?}"))?;
    if parts.is_empty() {
        bail!("empty editor command");
    }
    let program = parts.remove(0);
    let resolved =
        which::which(&program).map_err(|_| anyhow!("editor '{program}' was not found on PATH"))?;

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

/// Resolve an editor and open `file` in it.
pub fn open(file: &Path, explicit: Option<&str>) -> Result<()> {
    let editor = resolve_editor(explicit)?;
    open_in_editor(&editor, file)
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
    fn program_token_extracts_first_word() {
        assert_eq!(program_token("code --wait").unwrap(), "code");
        assert_eq!(program_token("subl").unwrap(), "subl");
    }

    #[test]
    fn pick_available_returns_first_match() {
        let picked = pick_available(&["aaa", "bbb", "ccc"], |c| c == "bbb");
        assert_eq!(picked.as_deref(), Some("bbb"));
        assert_eq!(pick_available(&["aaa", "bbb"], |_| false), None);
    }

    #[test]
    fn explicit_missing_editor_errors() {
        let err = resolve_editor(Some("definitely-not-a-real-editor-xyz")).unwrap_err();
        assert!(err.to_string().contains("not found"));
    }
}
