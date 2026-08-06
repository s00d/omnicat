//! Throttled stderr progress bar for long scans.

use std::io::{self, Write};
use std::time::{Duration, Instant};

const THROTTLE: Duration = Duration::from_millis(100);

pub struct ProgressBar {
    label: String,
    last: Instant,
    last_count: u64,
    width: usize,
}

impl ProgressBar {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            last: Instant::now(),
            last_count: 0,
            width: 40,
        }
    }

    pub fn tick(&mut self, count: u64) {
        let now = Instant::now();
        if now.duration_since(self.last) < THROTTLE && count.saturating_sub(self.last_count) < 1000
        {
            return;
        }
        self.last = now;
        self.last_count = count;
        let filled = (count % 1000) as usize * self.width / 1000;
        let bar = "█".repeat(filled.min(self.width));
        let empty = "░".repeat(self.width.saturating_sub(filled));
        let mut stderr = io::stderr().lock();
        let _ = write!(
            stderr,
            "\r{} [{}{}] {} lines",
            self.label, bar, empty, count
        );
        let _ = stderr.flush();
    }

    pub fn finish(&self) {
        let mut stderr = io::stderr().lock();
        let _ = writeln!(stderr);
        let _ = stderr.flush();
    }
}
