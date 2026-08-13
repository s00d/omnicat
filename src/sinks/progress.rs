//! Throttled progress for long scans (`--progress`).
//!
//! Prefers `/dev/tty` so the line stays visible even when stderr is redirected
//! (IDE runners, `2>file`). Falls back to stderr when it is a TTY.

use std::fs::OpenOptions;
use std::io::{self, IsTerminal, Write};
use std::time::{Duration, Instant};

const THROTTLE: Duration = Duration::from_millis(100);
const CHUNK_BYTES: u64 = 1024 * 1024;
const CHUNK_UNITS: u64 = 100;

enum ProgressOut {
    Stderr,
    #[cfg(unix)]
    Tty(std::fs::File),
    #[cfg(test)]
    Capture(std::sync::Arc<std::sync::Mutex<Vec<u8>>>),
}

/// Opt-in progress line. No-op when disabled or no interactive tty is available.
pub struct ProgressReporter {
    enabled: bool,
    out: ProgressOut,
    label: String,
    started: Instant,
    last: Instant,
    last_value: u64,
    painted: bool,
    finished: bool,
    /// How many times a line was actually written (tests / diagnostics).
    paint_count: u64,
}

impl ProgressReporter {
    /// Create a reporter. When `enabled` is false, ticks are no-ops.
    pub fn new(label: impl Into<String>, enabled: bool) -> Self {
        if !enabled {
            return Self::raw(label.into(), false, ProgressOut::Stderr);
        }
        #[cfg(unix)]
        {
            if let Ok(tty) = OpenOptions::new().write(true).open("/dev/tty") {
                return Self::raw(label.into(), true, ProgressOut::Tty(tty));
            }
        }
        let enabled = io::stderr().is_terminal();
        Self::raw(label.into(), enabled, ProgressOut::Stderr)
    }

    /// Always-enabled reporter (for unit tests; ignores TTY).
    #[cfg(test)]
    pub fn for_test(label: impl Into<String>) -> Self {
        Self::raw(label.into(), true, ProgressOut::Stderr)
    }

    /// Capturing reporter for tests that assert progress text.
    #[cfg(test)]
    pub fn capturing(
        label: impl Into<String>,
    ) -> (Self, std::sync::Arc<std::sync::Mutex<Vec<u8>>>) {
        let buf = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        (
            Self::raw(label.into(), true, ProgressOut::Capture(buf.clone())),
            buf,
        )
    }

    fn raw(label: String, enabled: bool, out: ProgressOut) -> Self {
        let now = Instant::now();
        Self {
            enabled,
            out,
            label,
            started: now,
            last: now,
            last_value: 0,
            painted: false,
            finished: false,
            paint_count: 0,
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn paint_count(&self) -> u64 {
        self.paint_count
    }

    /// Byte progress. `total` when known enables a percentage.
    pub fn tick_bytes(&mut self, done: u64, total: Option<u64>) {
        if !self.enabled || self.finished {
            return;
        }
        if !self.should_emit(done, CHUNK_BYTES) {
            return;
        }
        let line = match total.filter(|t| *t > 0) {
            Some(total) => {
                let pct = ((done as f64 / total as f64) * 100.0).min(100.0);
                format!(
                    "{}  {:>5.1}%  {}/{}",
                    self.label,
                    pct,
                    human_bytes(done),
                    human_bytes(total)
                )
            }
            None => {
                let rate = self.rate_bytes(done);
                format!("{}  {}  ({})", self.label, human_bytes(done), rate)
            }
        };
        self.paint(done, &line);
    }

    /// Unit progress (rows, files, …) without a known total.
    pub fn tick_count(&mut self, done: u64, unit: &str) {
        if !self.enabled || self.finished {
            return;
        }
        if !self.should_emit(done, CHUNK_UNITS) {
            return;
        }
        let elapsed = self.started.elapsed().as_secs_f64().max(0.001);
        let rate = done as f64 / elapsed;
        let line = format!(
            "{}  {} {}  ({:.0} {}/s)",
            self.label, done, unit, rate, unit
        );
        self.paint(done, &line);
    }

    /// Unit progress with known total.
    pub fn tick_count_total(&mut self, done: u64, total: u64, unit: &str) {
        if !self.enabled || self.finished {
            return;
        }
        if !self.should_emit(done, CHUNK_UNITS) {
            return;
        }
        let pct = if total > 0 {
            ((done as f64 / total as f64) * 100.0).min(100.0)
        } else {
            0.0
        };
        let line = format!("{}  {:>5.1}%  {}/{} {}", self.label, pct, done, total, unit);
        self.paint(done, &line);
    }

    /// Immediate status. Writes a new line (not `\r`) so it stays visible
    /// after the table starts printing — `\r` ticks are otherwise gone in <50ms.
    pub fn status(&mut self, message: &str) {
        if !self.enabled || self.finished {
            return;
        }
        let line = format!("{}  {}\n", self.label, message);
        self.last = Instant::now();
        let _ = self.write_raw(&line);
        self.painted = true;
        self.paint_count += 1;
    }

    /// Clear the progress line (once). Safe to call multiple times.
    pub fn finish(&mut self) {
        if !self.enabled || self.finished {
            return;
        }
        self.finished = true;
        if !self.painted {
            return;
        }
        let _ = self.write_raw(
            "\r                                                                                \r",
        );
    }

    fn should_emit(&self, value: u64, chunk: u64) -> bool {
        if self.paint_count == 0 {
            return true;
        }
        let elapsed = self.last.elapsed() >= THROTTLE;
        let chunked = value.saturating_sub(self.last_value) >= chunk;
        elapsed || chunked
    }

    fn paint(&mut self, value: u64, line: &str) {
        self.last = Instant::now();
        self.last_value = value;
        let padded = format!("\r{line:80}");
        let _ = self.write_raw(&padded);
        self.painted = true;
        self.paint_count += 1;
    }

    fn write_raw(&mut self, s: &str) -> io::Result<()> {
        match &mut self.out {
            ProgressOut::Stderr => {
                let mut stderr = io::stderr().lock();
                stderr.write_all(s.as_bytes())?;
                stderr.flush()
            }
            #[cfg(unix)]
            ProgressOut::Tty(file) => {
                file.write_all(s.as_bytes())?;
                file.flush()
            }
            #[cfg(test)]
            ProgressOut::Capture(buf) => {
                buf.lock().unwrap().extend_from_slice(s.as_bytes());
                Ok(())
            }
        }
    }

    fn rate_bytes(&self, done: u64) -> String {
        let secs = self.started.elapsed().as_secs_f64().max(0.001);
        let bps = done as f64 / secs;
        format!("{}/s", human_bytes(bps as u64))
    }
}

impl Drop for ProgressReporter {
    fn drop(&mut self) {
        self.finish();
    }
}

fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = n as f64;
    let mut i = 0usize;
    while v >= 1024.0 && i + 1 < UNITS.len() {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} {}", UNITS[i])
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_never_paints() {
        let mut p = ProgressReporter::raw("x".into(), false, ProgressOut::Stderr);
        p.tick_bytes(10_000_000, Some(10_000_000));
        p.tick_count(100_000, "rows");
        assert_eq!(p.paint_count(), 0);
        p.finish();
        assert_eq!(p.paint_count(), 0);
    }

    #[test]
    fn throttle_skips_rapid_ticks() {
        let mut p = ProgressReporter::for_test("scan");
        // First paint is immediate
        p.tick_bytes(100, Some(10_000_000));
        assert_eq!(p.paint_count(), 1);
        // tiny delta + no time → skip
        p.tick_bytes(200, Some(10_000_000));
        assert_eq!(p.paint_count(), 1);
        // Chunk gate without waiting
        p.tick_bytes(200 + CHUNK_BYTES, Some(10_000_000));
        assert_eq!(p.paint_count(), 2);
    }

    #[test]
    fn finish_is_idempotent() {
        let mut p = ProgressReporter::for_test("scan");
        p.tick_count(1, "files");
        p.finish();
        p.finish();
        assert!(p.finished);
    }

    #[test]
    fn status_paints_immediately() {
        let mut p = ProgressReporter::for_test("preview");
        p.status("2.3 MiB");
        assert_eq!(p.paint_count(), 1);
    }

    #[test]
    fn status_is_sticky_newline_not_cr_only() {
        let (mut p, buf) = ProgressReporter::capturing("preview");
        p.status("sample.csv  2.2 MiB");
        p.tick_count_total(100, 500, "rows");
        let s = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        assert!(
            s.contains("preview  sample.csv  2.2 MiB\n"),
            "status must stay on its own line, got {s:?}"
        );
        assert!(s.contains("100/500 rows"), "row ticks missing: {s:?}");
    }
}
