use std::io::{self, Write};

use anyhow::Result;
use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{self, DisableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::style::{Attribute, Print, ResetColor, SetAttribute};
use crossterm::terminal::{
    self, Clear, ClearType, DisableLineWrap, EnableLineWrap, EnterAlternateScreen,
    LeaveAlternateScreen,
};
use crossterm::{execute, queue};
use unicode_width::UnicodeWidthChar;

use crate::config::PaginateDisplay;
use crate::detect::HandlerKind;
use crate::orchestrator::ResolvedHandler;

type Page = Vec<Vec<u8>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Nav {
    Next,
    Prev,
    First,
    Last,
    Quit,
}

pub fn pagination_requested(config: &crate::config::OmnicatConfig, cli_paginate: bool) -> bool {
    let env_on = std::env::var_os("OMNICAT_PAGINATE").is_some_and(|v| {
        v != "0" && v != "false" && v != "no"
    });
    if !cli_paginate && !config.terminal.paginate.enabled && !env_on {
        return false;
    }
    is_terminal::IsTerminal::is_terminal(&io::stdout())
        && is_terminal::IsTerminal::is_terminal(&io::stdin())
}

pub fn skips_pagination(resolved: &ResolvedHandler) -> bool {
    matches!(
        resolved,
        ResolvedHandler::Builtin(HandlerKind::Image | HandlerKind::Media)
    )
}

/// Write `bytes` to stdout, interactively paging when content exceeds one screen.
pub fn write_paged(bytes: &[u8], settings: &PaginateDisplay) -> Result<()> {
    if bytes.is_empty() {
        return Ok(());
    }

    let (page_size, status_row) = page_layout(settings);
    let content_lines = visual_lines(bytes);
    if content_lines.len() <= page_size {
        io::stdout().write_all(bytes)?;
        return Ok(());
    }

    let pages = paginate_lines(&content_lines, page_size);
    eprintln!(
        "omnicat: paging (1/{}) — Space/Enter/Down: next, b/Up: prev, g/G: ends, q: quit",
        pages.len()
    );
    interactive_pager(&pages, status_row)
}

fn page_layout(settings: &PaginateDisplay) -> (usize, u16) {
    let (_, term_rows) = terminal::size().unwrap_or((80, 24));
    let status_row = term_rows.saturating_sub(1).max(1);
    let content_rows = if settings.page_lines > 0 {
        settings.page_lines.min(status_row) as usize
    } else {
        status_row as usize
    };
    (content_rows.max(1), status_row)
}

fn split_lines(bytes: &[u8]) -> Vec<Vec<u8>> {
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
    } else if bytes.last() == Some(&b'\n') {
        lines.push(Vec::new());
    }
    lines
}

fn terminal_cols() -> usize {
    terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(80)
        .max(1)
}

fn line_payload(line: &[u8]) -> Vec<u8> {
    let end = line
        .last()
        .is_some_and(|&b| b == b'\n')
        .then(|| line.len().saturating_sub(1))
        .unwrap_or(line.len());
    line[..end].to_vec()
}

fn sanitize_display_text(text: &str) -> String {
    text.replace('\r', "")
        .chars()
        .filter(|c| *c == '\t' || !c.is_control())
        .collect()
}

fn wrap_text(text: &str, width: usize) -> Vec<Vec<u8>> {
    if text.is_empty() {
        return vec![Vec::new()];
    }

    let mut rows = Vec::new();
    let mut current = String::new();
    let mut col = 0usize;

    for ch in text.chars() {
        if ch == '\t' {
            let spaces = 8 - (col % 8);
            for _ in 0..spaces {
                if col + 1 > width && !current.is_empty() {
                    rows.push(current.as_bytes().to_vec());
                    current.clear();
                    col = 0;
                }
                current.push(' ');
                col += 1;
            }
            continue;
        }

        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if w == 0 {
            continue;
        }
        if col + w > width && !current.is_empty() {
            rows.push(current.as_bytes().to_vec());
            current.clear();
            col = 0;
        }
        current.push(ch);
        col += w;
    }

    if !current.is_empty() {
        rows.push(current.as_bytes().to_vec());
    }
    rows
}

fn visual_lines(bytes: &[u8]) -> Vec<Vec<u8>> {
    let width = terminal_cols();
    let mut visual = Vec::new();
    for line in split_lines(bytes) {
        let payload = line_payload(&line);
        if payload.is_empty() {
            visual.push(Vec::new());
            continue;
        }
        let text = sanitize_display_text(&String::from_utf8_lossy(&payload));
        visual.extend(wrap_text(&text, width));
    }
    visual
}

fn paginate_lines(lines: &[Vec<u8>], page_size: usize) -> Vec<Page> {
    lines
        .chunks(page_size)
        .map(|chunk| chunk.to_vec())
        .collect()
}

fn draw_page(out: &mut io::Stdout, page: &Page, status_row: u16) -> Result<()> {
    queue!(out, Clear(ClearType::All))?;
    for (i, line) in page.iter().enumerate() {
        let row = i as u16;
        if row >= status_row {
            break;
        }
        queue!(out, MoveTo(0, row), Clear(ClearType::UntilNewLine))?;
        out.write_all(line)?;
    }
    out.flush()?;
    Ok(())
}

fn interactive_pager(pages: &[Page], status_row: u16) -> Result<()> {
    let mut stdout = io::stdout();

    terminal::enable_raw_mode()?;
    execute!(
        stdout,
        EnterAlternateScreen,
        Hide,
        DisableLineWrap,
        DisableMouseCapture,
    )?;

    let result = (|| -> Result<()> {
        let mut page = 0usize;
        loop {
            draw_page(&mut stdout, &pages[page], status_row)?;
            draw_status(&mut stdout, status_row, page + 1, pages.len())?;

            match read_nav_key()? {
                Nav::Quit => break,
                Nav::Next => {
                    if page + 1 < pages.len() {
                        page += 1;
                    } else {
                        break;
                    }
                }
                Nav::Prev => page = page.saturating_sub(1),
                Nav::First => page = 0,
                Nav::Last => page = pages.len().saturating_sub(1),
            }
        }
        Ok(())
    })();

    execute!(
        stdout,
        Show,
        LeaveAlternateScreen,
        EnableLineWrap,
    )?;
    terminal::disable_raw_mode()?;
    result
}

fn draw_status(out: &mut io::Stdout, row: u16, current: usize, total: usize) -> Result<()> {
    let line = format!(
        "Page {current}/{total} — Space/Enter/Down/PgDn: next  b/Up/PgUp: prev  g/G: first/last  q/Esc: quit"
    );
    queue!(
        out,
        MoveTo(0, row),
        Clear(ClearType::UntilNewLine),
        SetAttribute(Attribute::Reverse),
        Print(&line),
        ResetColor,
    )?;
    out.flush()?;
    Ok(())
}

fn read_nav_key() -> Result<Nav> {
    loop {
        match event::read()? {
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => {
                if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
                    return Ok(Nav::Quit);
                }
                return Ok(match code {
                    KeyCode::Char('q') | KeyCode::Esc => Nav::Quit,
                    KeyCode::Char(' ') | KeyCode::Enter | KeyCode::Char('j') | KeyCode::Down => {
                        Nav::Next
                    }
                    KeyCode::Char('b') | KeyCode::Char('k') | KeyCode::Up => Nav::Prev,
                    KeyCode::Char('g') => Nav::First,
                    KeyCode::Char('G') => Nav::Last,
                    KeyCode::PageDown => Nav::Next,
                    KeyCode::PageUp => Nav::Prev,
                    KeyCode::Home => Nav::First,
                    KeyCode::End => Nav::Last,
                    _ => continue,
                });
            }
            Event::Mouse(_) | Event::Resize(..) | Event::FocusGained | Event::FocusLost => continue,
            _ => continue,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_lines_preserves_trailing_newline_line() {
        let lines = split_lines(b"a\nb\n");
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], b"a\n");
        assert_eq!(lines[1], b"b\n");
        assert_eq!(lines[2], b"");
    }

    #[test]
    fn paginate_groups_lines() {
        let lines: Vec<Vec<u8>> = (1..=5).map(|n| vec![b'0' + n]).collect();
        let pages = paginate_lines(&lines, 2);
        assert_eq!(pages.len(), 3);
        assert_eq!(pages[0], vec![vec![b'1'], vec![b'2']]);
    }

    #[test]
    fn many_lines_trigger_multiple_pages() {
        let mut buf = String::new();
        for i in 0..100 {
            buf.push_str(&format!("line {i}\n"));
        }
        let lines = visual_lines(buf.as_bytes());
        let (page_size, _) = page_layout(&PaginateDisplay::default());
        assert!(lines.len() > page_size);
        assert!(paginate_lines(&lines, page_size).len() > 1);
    }

    #[test]
    fn long_logical_line_wraps_into_many_visual_lines() {
        let width = terminal_cols();
        let long = "x".repeat(width * 40);
        let mut buf = long.into_bytes();
        buf.push(b'\n');
        let visual = visual_lines(&buf);
        assert!(visual.len() >= 40, "got {}", visual.len());
    }
}
