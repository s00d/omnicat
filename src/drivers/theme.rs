use std::sync::OnceLock;

use syntect::highlighting::{Theme, ThemeSet};

/// Syntect theme tuned for dark terminal backgrounds.
const DARK_THEME: &str = "base16-ocean.dark";
/// Syntect theme tuned for light terminal backgrounds (GitHub-like, high contrast on white).
const LIGHT_THEME: &str = "InspiredGitHub";

const FALLBACKS: &[&str] = &[DARK_THEME, "Solarized (dark)", LIGHT_THEME];

/// Detected (or configured) lightness of the terminal background.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Background {
    Light,
    Dark,
}

impl Background {
    /// Concrete syntect theme name that reads well on this background.
    pub fn theme_name(self) -> &'static str {
        match self {
            Background::Light => LIGHT_THEME,
            Background::Dark => DARK_THEME,
        }
    }
}

/// Look up a syntect theme by name, interpreting the special names
/// `auto`/`default`/`light`/`dark` (see [`resolve_theme_name`]).
pub fn resolve_theme<'a>(ts: &'a ThemeSet, name: &str) -> &'a Theme {
    let resolved = resolve_theme_name(name);
    if let Some(theme) = ts.themes.get(resolved.as_str()) {
        return theme;
    }
    for fallback in FALLBACKS {
        if let Some(theme) = ts.themes.get(*fallback) {
            return theme;
        }
    }
    ts.themes.values().next().expect("theme set is non-empty")
}

/// Map a configured theme value to a concrete syntect theme name.
///
/// - `dark` / `light` pick a fixed high-contrast theme for that background.
/// - `auto` / `default` / empty detect the terminal background (see
///   [`detect_background`]) and pick accordingly.
/// - anything else is treated as a literal syntect theme name.
pub fn resolve_theme_name(name: &str) -> String {
    match name.trim().to_ascii_lowercase().as_str() {
        "dark" => DARK_THEME.to_string(),
        "light" => LIGHT_THEME.to_string(),
        "" | "auto" | "default" => detect_background().theme_name().to_string(),
        _ => name.trim().to_string(),
    }
}

/// Detect the terminal background lightness, cached for the lifetime of the process.
pub fn detect_background() -> Background {
    static CACHE: OnceLock<Background> = OnceLock::new();
    *CACHE.get_or_init(detect_background_uncached)
}

fn detect_background_uncached() -> Background {
    if let Some(bg) = background_from_env() {
        return bg;
    }
    if let Some(bg) = background_from_osc() {
        return bg;
    }
    if let Some(bg) = background_from_colorfgbg() {
        return bg;
    }
    // No signal: assume a dark terminal (the historical default).
    Background::Dark
}

/// Explicit override via `OMNICAT_THEME` / `OMNICAT_BACKGROUND` (`light` or `dark`).
fn background_from_env() -> Option<Background> {
    for key in ["OMNICAT_THEME", "OMNICAT_BACKGROUND"] {
        if let Ok(value) = std::env::var(key) {
            match value.trim().to_ascii_lowercase().as_str() {
                "light" => return Some(Background::Light),
                "dark" => return Some(Background::Dark),
                _ => {}
            }
        }
    }
    None
}

/// Heuristic based on the `COLORFGBG` variable exported by many terminals.
/// The last field is the background color index: 0-6 and 8 are dark, 7 and 9-15 are light.
fn background_from_colorfgbg() -> Option<Background> {
    let value = std::env::var("COLORFGBG").ok()?;
    let bg = value.split(';').next_back()?.trim();
    let index: u8 = bg.parse().ok()?;
    if index == 7 || (9..=15).contains(&index) {
        Some(Background::Light)
    } else {
        Some(Background::Dark)
    }
}

/// Classify a background color (each channel scaled to 0.0..=1.0) as light or dark
/// using perceived luminance.
fn classify_luminance(r: f32, g: f32, b: f32) -> Background {
    let luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    if luminance > 0.5 {
        Background::Light
    } else {
        Background::Dark
    }
}

/// Parse an OSC 11 response payload, e.g. `\x1b]11;rgb:ffff/ffff/ffff\x1b\\`.
fn parse_osc_background(bytes: &[u8]) -> Option<Background> {
    let text = String::from_utf8_lossy(bytes);
    let idx = text.find("rgb:")?;
    let spec = &text[idx + 4..];
    let mut channels = spec.split('/');
    let r = parse_hex_channel(channels.next()?)?;
    let g = parse_hex_channel(channels.next()?)?;
    let b = parse_hex_channel(channels.next()?)?;
    Some(classify_luminance(r, g, b))
}

/// Parse a hex color channel of arbitrary width (`ff`, `ffff`, …) into 0.0..=1.0.
fn parse_hex_channel(raw: &str) -> Option<f32> {
    let hex: String = raw.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
    if hex.is_empty() {
        return None;
    }
    let value = u32::from_str_radix(&hex, 16).ok()?;
    let max = 16u32.pow(hex.len() as u32) - 1;
    Some(value as f32 / max as f32)
}

/// Query the terminal for its background color via OSC 11 (Unix only).
///
/// Best-effort: returns `None` when stdout/stdin are not a TTY, the terminal is
/// `dumb`, or no response arrives within a short timeout.
#[cfg(unix)]
fn background_from_osc() -> Option<Background> {
    use std::io::{IsTerminal, Read, Write};
    use std::time::Duration;

    if !std::io::stdout().is_terminal() || !std::io::stdin().is_terminal() {
        return None;
    }
    match std::env::var("TERM").as_deref() {
        Ok("dumb") | Ok("") => return None,
        _ => {}
    }

    let mut writer = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .ok()?;
    let reader = std::fs::OpenOptions::new()
        .read(true)
        .open("/dev/tty")
        .ok()?;

    if crossterm::terminal::enable_raw_mode().is_err() {
        return None;
    }
    let _guard = RawModeGuard;

    // Query background color; terminate the query with ST (ESC \\).
    writer.write_all(b"\x1b]11;?\x1b\\").ok()?;
    writer.flush().ok()?;

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = reader;
        let mut buf = Vec::new();
        let mut byte = [0u8; 1];
        while buf.len() < 128 {
            match reader.read(&mut byte) {
                Ok(1) => {
                    buf.push(byte[0]);
                    // Response ends with BEL or ST (ESC \\).
                    if byte[0] == 0x07 {
                        break;
                    }
                    if byte[0] == b'\\' && buf.len() >= 2 && buf[buf.len() - 2] == 0x1b {
                        break;
                    }
                }
                _ => break,
            }
        }
        let _ = tx.send(buf);
    });

    let buf = rx.recv_timeout(Duration::from_millis(150)).ok()?;
    parse_osc_background(&buf)
}

#[cfg(not(unix))]
fn background_from_osc() -> Option<Background> {
    None
}

#[cfg(unix)]
struct RawModeGuard;

#[cfg(unix)]
impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_names_map_to_fixed_themes() {
        assert_eq!(resolve_theme_name("dark"), DARK_THEME);
        assert_eq!(resolve_theme_name("light"), LIGHT_THEME);
        assert_eq!(resolve_theme_name(" Light "), LIGHT_THEME);
    }

    #[test]
    fn literal_theme_names_pass_through() {
        assert_eq!(resolve_theme_name("Solarized (dark)"), "Solarized (dark)");
        assert_eq!(resolve_theme_name("InspiredGitHub"), "InspiredGitHub");
    }

    #[test]
    fn parse_osc_white_is_light() {
        let resp = b"\x1b]11;rgb:ffff/ffff/ffff\x1b\\";
        assert_eq!(parse_osc_background(resp), Some(Background::Light));
    }

    #[test]
    fn parse_osc_black_is_dark() {
        let resp = b"\x1b]11;rgb:0000/0000/0000\x07";
        assert_eq!(parse_osc_background(resp), Some(Background::Dark));
    }

    #[test]
    fn parse_osc_eight_bit_channels() {
        let resp = b"\x1b]11;rgb:ff/ff/ff\x07";
        assert_eq!(parse_osc_background(resp), Some(Background::Light));
    }

    #[test]
    fn parse_osc_dark_gray_is_dark() {
        let resp = b"\x1b]11;rgb:2020/2020/2020\x1b\\";
        assert_eq!(parse_osc_background(resp), Some(Background::Dark));
    }

    #[test]
    fn parse_osc_garbage_is_none() {
        assert_eq!(parse_osc_background(b"not a response"), None);
    }

    #[test]
    fn classify_by_luminance() {
        assert_eq!(classify_luminance(1.0, 1.0, 1.0), Background::Light);
        assert_eq!(classify_luminance(0.0, 0.0, 0.0), Background::Dark);
    }

    // Small helper mirroring `background_from_colorfgbg` but for an explicit value,
    // so the parsing logic is testable without mutating process environment.
    fn background_from_colorfgbg_value(value: &str) -> Option<Background> {
        let bg = value.split(';').next_back()?.trim();
        let index: u8 = bg.parse().ok()?;
        if index == 7 || (9..=15).contains(&index) {
            Some(Background::Light)
        } else {
            Some(Background::Dark)
        }
    }

    #[test]
    fn colorfgbg_value_classification() {
        assert_eq!(
            background_from_colorfgbg_value("15;0"),
            Some(Background::Dark)
        );
        assert_eq!(
            background_from_colorfgbg_value("0;15"),
            Some(Background::Light)
        );
        assert_eq!(
            background_from_colorfgbg_value("7;0"),
            Some(Background::Dark)
        );
        assert_eq!(
            background_from_colorfgbg_value("0;7"),
            Some(Background::Light)
        );
        assert_eq!(background_from_colorfgbg_value("bogus"), None);
    }
}
