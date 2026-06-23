use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result};

use crate::config::HandlerConfig;
use crate::content::PreviewContent;

pub fn substitute_template(template: &str, path: &Path) -> String {
    let file = path.to_string_lossy();
    let path_str = path.display().to_string();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    template
        .replace("{file}", &file)
        .replace("{path}", &path_str)
        .replace("{name}", &name)
}

pub fn command_available(program: &str) -> bool {
    which::which(program).is_ok()
}

pub fn command_status_label(program: &str) -> &'static str {
    if command_available(program) {
        "(+)"
    } else {
        "(-)"
    }
}

pub fn first_available_command(commands: &[String]) -> Option<&str> {
    for template in commands {
        let program = template.split_whitespace().next()?;
        if command_available(program) {
            return Some(template.as_str());
        }
    }
    None
}

pub fn external_status(commands: &[String]) -> String {
    commands
        .iter()
        .filter_map(|t| t.split_whitespace().next())
        .map(|p| format!("{p}{}", command_status_label(p)))
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn try_render_terminal(
    handler: &HandlerConfig,
    path: &Path,
    timeout: Duration,
    out: &mut dyn Write,
) -> Result<bool> {
    for template in &handler.commands {
        let program = match template.split_whitespace().next() {
            Some(p) => p,
            None => continue,
        };
        if !command_available(program) {
            continue;
        }
        if let Ok(output) = run_template_capture(template, path, timeout) {
            if output.status.success() || !output.stdout.is_empty() {
                out.write_all(&output.stdout)?;
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub fn try_build_content(
    handler: &HandlerConfig,
    path: &Path,
    timeout: Duration,
) -> Result<Option<PreviewContent>> {
    for template in &handler.commands {
        let program = match template.split_whitespace().next() {
            Some(p) => p,
            None => continue,
        };
        if !command_available(program) {
            continue;
        }
        if let Ok(output) = run_template_capture(template, path, timeout) {
            if output.status.success() || !output.stdout.is_empty() {
                let text = String::from_utf8_lossy(&output.stdout).into_owned();
                return Ok(Some(PreviewContent::Text(text)));
            }
        }
    }
    Ok(None)
}

struct CommandOutput {
    status: std::process::ExitStatus,
    stdout: Vec<u8>,
}

fn run_template_capture(template: &str, path: &Path, timeout: Duration) -> Result<CommandOutput> {
    let resolved = substitute_template(template, path);
    let args = shell_words::split(&resolved).context("invalid command template")?;
    let (program, args) = args
        .split_first()
        .context("empty command template")?;

    let mut child = Command::new(program);
    child.args(args);
    child.stdin(Stdio::null());
    child.stdout(Stdio::piped());
    child.stderr(Stdio::null());
    child.env("NO_COLOR", "1");
    child.env("CLICOLOR_FORCE", "0");

    let mut child = child.spawn().context("spawn external renderer")?;
    let status = wait_timeout::ChildExt::wait_timeout(&mut child, timeout)
        .context("wait external renderer")?
        .context("external renderer timed out")?;
    let mut stdout = Vec::new();
    if let Some(mut pipe) = child.stdout.take() {
        use std::io::Read;
        pipe.read_to_end(&mut stdout)?;
    }
    Ok(CommandOutput { status, stdout })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn substitutes_file_placeholder() {
        let p = Path::new("/tmp/hello.md");
        let s = substitute_template("glow {file}", p);
        assert_eq!(s, "glow /tmp/hello.md");
    }

    #[test]
    fn substitutes_path_and_name() {
        let p = Path::new("/tmp/docs/readme.md");
        assert_eq!(
            substitute_template("bat {path} {name}", p),
            "bat /tmp/docs/readme.md readme.md"
        );
    }

    #[test]
    fn first_available_skips_missing() {
        let cmds = vec![
            "missing-tool-xyz {file}".into(),
            "cat {file}".into(),
        ];
        assert_eq!(first_available_command(&cmds), Some("cat {file}"));
    }

    #[test]
    fn external_status_marks_availability() {
        let cmds = vec!["definitely-not-a-real-binary-xyz {file}".into()];
        let status = external_status(&cmds);
        assert!(status.contains("(-)"));
    }
}
