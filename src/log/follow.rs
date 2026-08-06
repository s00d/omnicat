//! Live tail -f with log rendering (shared by `omnicat log --follow` and inspect `-F`).

use std::io::{self, Write};
use std::path::Path;

use anyhow::Result;

use crate::io::follow_loop;
use crate::log::filter::LogFilter;
use crate::log::parse;
use crate::log::record::LogFormat;
use crate::log::render::{is_stack_continuation, line_matches_level, render_and_sanitize};

/// Tail -f with optional level filter and ANSI-safe coloring.
pub fn follow_path(
    path: &Path,
    level: Option<&str>,
    allow_unsafe: bool,
    filter: &LogFilter,
) -> Result<()> {
    let level_filter = level.map(|s| s.to_ascii_lowercase());
    let mut in_matching_block = level_filter.is_none();
    let mut messages: u64 = 0;
    let mut errors: u64 = 0;
    let mut sample = Vec::new();
    let mut format = LogFormat::Unknown;

    follow_loop(path, |line| {
        let text = line.as_str().unwrap_or("");
        if sample.len() < 4 {
            sample.push(text.to_string());
            if sample.len() == 4 {
                format = parse::detect_from_lines(
                    &sample.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                );
            }
        }
        let rec = parse::parse_line_with_format(text, format);
        if !filter.matches(&rec) {
            return Ok(());
        }
        messages += 1;
        let cont = is_stack_continuation(text);
        if let Some(ref lvl) = level_filter {
            if cont {
                if !in_matching_block {
                    return Ok(());
                }
            } else if line_matches_level(text, lvl) {
                in_matching_block = true;
            } else {
                in_matching_block = false;
                return Ok(());
            }
        }
        if text.to_ascii_lowercase().contains("error") {
            errors += 1;
        }
        let out = render_and_sanitize(text, allow_unsafe);
        let mut stdout = io::stdout().lock();
        write!(stdout, "{out}")?;
        if !out.ends_with('\n') {
            writeln!(stdout)?;
        }
        if is_terminal::IsTerminal::is_terminal(&io::stdout()) {
            write!(
                stdout,
                "\x1b[sLIVE  messages={messages} errors={errors}\x1b[u"
            )?;
        }
        stdout.flush()?;
        Ok(())
    })
    .map_err(|e| anyhow::anyhow!("{e}"))
}
