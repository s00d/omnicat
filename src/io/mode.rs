//! Read modes and open options for the unified file engine.

/// How to read a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReadMode {
    /// Sequential forward scan (default for stats / full read).
    #[default]
    Stream,
    /// Last N lines from EOF (reverse chunk read).
    Tail { lines: usize },
    /// Incremental read from current offset (tail -f style).
    Follow { from_end: bool },
    /// Memory-map for random access / search (not for growing files).
    Mmap,
}

/// Options when opening a file through the I/O engine.
#[derive(Debug, Clone)]
pub struct OpenOptions {
    pub mode: ReadMode,
    /// Auto-decompress by extension (.gz, .bz2, .xz, .zst).
    pub decompress: bool,
    /// Stop after scanning this many bytes (None = unlimited).
    pub max_scan_bytes: Option<u64>,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self {
            mode: ReadMode::Stream,
            decompress: true,
            max_scan_bytes: None,
        }
    }
}

impl OpenOptions {
    pub fn stream() -> Self {
        Self::default()
    }

    pub fn tail(lines: usize) -> Self {
        Self {
            mode: ReadMode::Tail { lines },
            ..Self::default()
        }
    }

    pub fn follow() -> Self {
        Self {
            mode: ReadMode::Follow { from_end: true },
            ..Self::default()
        }
    }

    pub fn mmap() -> Self {
        Self {
            mode: ReadMode::Mmap,
            ..Self::default()
        }
    }
}
