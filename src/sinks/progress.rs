//! Single-line progress bar for `--progress` (stderr TTY, ANSI erase).

use std::io::{self, IsTerminal, Write};
use std::time::{Duration, Instant};

const MIN_INTERVAL: Duration = Duration::from_millis(250);
const CHUNK_BYTES: u64 = 512 * 1024;
const BAR_WIDTH: usize = 24;

/// Opt-in progress bar. No-op when disabled or stderr is not a TTY.
pub struct ProgressReporter {
    enabled: bool,
    label: String,
    started: Instant,
    last_paint: Instant,
    last_value: u64,
    last_pct: u8,
    painted: bool,
    finished: bool,
    paint_count: u64,
    #[cfg(test)]
    test_fast: bool,
}

impl ProgressReporter {
    pub fn new(label: impl Into<String>, enabled: bool) -> Self {
        let enabled = enabled && io::stderr().is_terminal();
        Self {
            enabled,
            label: label.into(),
            started: Instant::now(),
            last_paint: Instant::now(),
            last_value: 0,
            last_pct: 0,
            painted: false,
            finished: false,
            paint_count: 0,
            #[cfg(test)]
            test_fast: false,
        }
    }

    #[cfg(test)]
    pub fn for_test(label: impl Into<String>) -> Self {
        Self {
            enabled: true,
            label: label.into(),
            started: Instant::now(),
            last_paint: Instant::now(),
            last_value: 0,
            last_pct: 0,
            painted: false,
            finished: false,
            paint_count: 0,
            test_fast: true,
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn paint_count(&self) -> u64 {
        self.paint_count
    }

    pub fn tick_bytes(&mut self, done: u64, total: Option<u64>) {
        if !self.enabled || self.finished {
            return;
        }
        let Some(total) = total.filter(|t| *t > 0) else {
            if self.should_emit(done, None, false) {
                let line = format!(
                    "{} [{}] {} ({})",
                    self.label,
                    render_bar_indeterminate(self.started.elapsed()),
                    human_bytes(done),
                    rate_bytes(self.started.elapsed(), done)
                );
                self.paint(done, None, &line);
            }
            return;
        };
        let pct = pct(done, total);
        if !self.should_emit(done, Some(pct), done >= total) {
            return;
        }
        let line = format!(
            "{} [{}] {:>3}% {}/{} ({})",
            self.label,
            render_bar(pct),
            pct,
            human_bytes(done),
            human_bytes(total),
            rate_bytes(self.started.elapsed(), done)
        );
        self.paint(done, Some(pct), &line);
    }

    pub fn tick_count(&mut self, done: u64, unit: &str) {
        if !self.enabled || self.finished {
            return;
        }
        if !self.should_emit(done, None, false) {
            return;
        }
        let line = format!(
            "{} [{}] {} {} ({})",
            self.label,
            render_bar_indeterminate(self.started.elapsed()),
            done,
            unit,
            rate_count(self.started.elapsed(), done, unit)
        );
        self.paint(done, None, &line);
    }

    pub fn tick_count_total(&mut self, done: u64, total: u64, unit: &str) {
        if !self.enabled || self.finished {
            return;
        }
        let total = total.max(1);
        let pct = pct(done, total);
        if !self.should_emit(done, Some(pct), done >= total) {
            return;
        }
        let line = format!(
            "{} [{}] {:>3}% {}/{} {} ({})",
            self.label,
            render_bar(pct),
            pct,
            done,
            total,
            unit,
            rate_count(self.started.elapsed(), done, unit)
        );
        self.paint(done, Some(pct), &line);
    }

    /// One-line status (same `\r` bar slot). Prefer tick_* with totals when possible.
    pub fn status(&mut self, message: &str) {
        if !self.enabled || self.finished {
            return;
        }
        let line = format!("{} [{}] {}", self.label, render_bar(0), message);
        self.paint(self.last_value, None, &line);
    }

    pub fn finish(&mut self) {
        if !self.enabled || self.finished {
            return;
        }
        self.finished = true;
        if self.painted {
            let _ = write_stderr("\x1b[2K\r");
        }
    }

    fn should_emit(&self, value: u64, pct: Option<u8>, force: bool) -> bool {
        if force {
            return true;
        }
        if self.paint_count == 0 {
            return true;
        }
        #[cfg(test)]
        let fast = self.test_fast;
        #[cfg(not(test))]
        let fast = false;
        if !fast && self.last_paint.elapsed() < MIN_INTERVAL {
            return false;
        }
        match pct {
            Some(p) => p > self.last_pct,
            None => value.saturating_sub(self.last_value) >= CHUNK_BYTES,
        }
    }

    fn paint(&mut self, value: u64, pct: Option<u8>, line: &str) {
        self.last_paint = Instant::now();
        self.last_value = value;
        if let Some(p) = pct {
            self.last_pct = p;
        }
        let _ = write_stderr(&format!("\x1b[2K\r{line}"));
        self.painted = true;
        self.paint_count += 1;
    }
}

impl Drop for ProgressReporter {
    fn drop(&mut self) {
        self.finish();
    }
}

fn write_stderr(s: &str) -> io::Result<()> {
    let mut stderr = io::stderr().lock();
    stderr.write_all(s.as_bytes())?;
    stderr.flush()
}

fn pct(done: u64, total: u64) -> u8 {
    ((done as f64 / total as f64) * 100.0).min(100.0) as u8
}

fn render_bar(pct: u8) -> String {
    let ratio = pct as f64 / 100.0;
    let filled = (ratio * BAR_WIDTH as f64).round() as usize;
    let mut bar = String::with_capacity(BAR_WIDTH + 2);
    bar.push('[');
    for i in 0..BAR_WIDTH {
        bar.push(if i < filled { '█' } else { '░' });
    }
    bar.push(']');
    bar
}

fn render_bar_indeterminate(elapsed: Duration) -> String {
    let pos = ((elapsed.as_millis() / 200) as usize) % (BAR_WIDTH * 2);
    let head = if pos < BAR_WIDTH {
        pos
    } else {
        BAR_WIDTH * 2 - pos
    };
    let mut bar = String::with_capacity(BAR_WIDTH + 2);
    bar.push('[');
    for i in 0..BAR_WIDTH {
        bar.push(if i >= head && i.saturating_add(3) > head && i < head + 4 {
            '█'
        } else {
            '░'
        });
    }
    bar.push(']');
    bar
}

fn rate_bytes(elapsed: Duration, done: u64) -> String {
    let secs = elapsed.as_secs_f64().max(0.001);
    format!("{}/s", human_bytes((done as f64 / secs) as u64))
}

fn rate_count(elapsed: Duration, done: u64, unit: &str) -> String {
    let secs = elapsed.as_secs_f64().max(0.001);
    format!("{:.0} {}/s", done as f64 / secs, unit)
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
        format!("{n}{}", UNITS[i])
    } else {
        format!("{v:.1}{}", UNITS[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_never_paints() {
        let mut p = ProgressReporter::new("x", false);
        p.tick_bytes(10_000_000, Some(10_000_000));
        assert_eq!(p.paint_count(), 0);
    }

    #[test]
    fn count_total_draws_bar() {
        let mut p = ProgressReporter::for_test("preview");
        p.tick_count_total(250, 500, "rows");
        assert_eq!(p.paint_count(), 1);
        p.tick_count_total(251, 500, "rows");
        assert_eq!(p.paint_count(), 1, "same pct throttled");
        p.tick_count_total(300, 500, "rows");
        assert!(p.paint_count() >= 2);
    }

    #[test]
    fn bar_half_filled() {
        let bar = render_bar(50);
        assert!(bar.contains('█') && bar.contains('░'));
    }
}
