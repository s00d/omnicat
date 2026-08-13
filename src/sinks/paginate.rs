//! Static page output for `--paginate` / `--page`.

use std::io::{self, Write};

use anyhow::{bail, Result};
use crossterm::terminal;

use crate::config::PaginateDisplay;
use crate::detect::HandlerKind;
use crate::orchestrator::ResolvedHandler;

type Page = Vec<Vec<u8>>;

/// True when the user asked for static paging via CLI.
pub fn pagination_requested(cli_paginate: bool, page: Option<usize>) -> bool {
    cli_paginate || page.is_some()
}

pub fn skips_pagination(resolved: &ResolvedHandler) -> bool {
    matches!(
        resolved,
        ResolvedHandler::Builtin(HandlerKind::Image | HandlerKind::Media)
    )
}

/// Write one page of rendered output plus a footer with page/total info.
///
/// `page` is 1-based. Invalid page numbers return an error.
pub fn write_page(bytes: &[u8], settings: &PaginateDisplay, page: usize) -> Result<()> {
    write_page_to(bytes, settings, page, &mut io::stdout().lock())
}

fn write_page_to(
    bytes: &[u8],
    settings: &PaginateDisplay,
    page: usize,
    out: &mut dyn Write,
) -> Result<()> {
    let page_size = page_size(settings);
    let lines = split_lines(bytes);
    let pages = paginate_lines(&lines, page_size);
    let total = pages.len().max(1);
    let total_lines = lines.len();

    if page == 0 || page > total {
        bail!("page {page} out of range (valid: 1..={total})");
    }

    let selected = pages.get(page - 1).cloned().unwrap_or_default();
    for line in &selected {
        out.write_all(line)?;
        if !line.last().is_some_and(|&b| b == b'\n') {
            out.write_all(b"\n")?;
        }
    }

    let start_line = if total_lines == 0 {
        0
    } else {
        (page - 1) * page_size + 1
    };
    let end_line = if total_lines == 0 {
        0
    } else {
        ((page - 1) * page_size + selected.len()).min(total_lines)
    };

    writeln!(
        out,
        "--- page {page}/{total} (lines {start_line}–{end_line} of {total_lines}) ---"
    )?;
    out.flush()?;
    Ok(())
}

fn page_size(settings: &PaginateDisplay) -> usize {
    if settings.page_lines > 0 {
        return settings.page_lines as usize;
    }
    let rows = terminal::size()
        .map(|(_, h)| h as usize)
        .unwrap_or(50)
        .saturating_sub(1);
    rows.max(1)
}

fn split_lines(bytes: &[u8]) -> Vec<Vec<u8>> {
    if bytes.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let mut start = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\n' {
            lines.push(bytes[start..=i].to_vec());
            start = i + 1;
        }
    }
    if start < bytes.len() {
        lines.push(bytes[start..].to_vec());
    }
    lines
}

fn paginate_lines(lines: &[Vec<u8>], page_size: usize) -> Vec<Page> {
    if lines.is_empty() {
        return vec![Vec::new()];
    }
    lines
        .chunks(page_size.max(1))
        .map(|chunk| chunk.to_vec())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_lines_keeps_newline_terminated_rows() {
        let lines = split_lines(b"a\nb\n");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], b"a\n");
        assert_eq!(lines[1], b"b\n");
    }

    #[test]
    fn paginate_groups_lines() {
        let lines: Vec<Vec<u8>> = (1..=5).map(|n| vec![b'0' + n]).collect();
        let pages = paginate_lines(&lines, 2);
        assert_eq!(pages.len(), 3);
        assert_eq!(pages[0], vec![vec![b'1'], vec![b'2']]);
    }

    #[test]
    fn empty_content_is_one_page() {
        assert_eq!(paginate_lines(&[], 10).len(), 1);
    }

    #[test]
    fn write_page_rejects_out_of_range() {
        let settings = PaginateDisplay { page_lines: 2 };
        let buf = b"a\nb\nc\nd\n";
        let mut sink = Vec::new();
        let err = write_page_to(buf, &settings, 9, &mut sink).unwrap_err();
        assert!(err.to_string().contains("out of range"));
        let err0 = write_page_to(buf, &settings, 0, &mut sink).unwrap_err();
        assert!(err0.to_string().contains("out of range"));
    }

    #[test]
    fn write_page_emits_content_and_footer() {
        let settings = PaginateDisplay { page_lines: 2 };
        let mut sink = Vec::new();
        write_page_to(b"a\nb\nc\nd\n", &settings, 2, &mut sink).unwrap();
        let text = String::from_utf8(sink).unwrap();
        assert!(text.contains("c\n"));
        assert!(text.contains("d\n"));
        assert!(!text.contains("a\n"));
        assert!(text.contains("--- page 2/2 (lines 3–4 of 4) ---"));
    }

    #[test]
    fn pagination_requested_from_flags() {
        assert!(!pagination_requested(false, None));
        assert!(pagination_requested(true, None));
        assert!(pagination_requested(false, Some(2)));
    }
}
