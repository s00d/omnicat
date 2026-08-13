//! Zero-copy line reference into a buffer or mmap.

/// A single line slice (without trailing `\n` / `\r\n`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineRef<'a> {
    pub bytes: &'a [u8],
    pub line_no: u64,
}

impl<'a> LineRef<'a> {
    pub fn as_str(&self) -> Option<&str> {
        std::str::from_utf8(self.bytes).ok()
    }

    pub fn text_lossy(&self) -> String {
        String::from_utf8_lossy(self.bytes).into_owned()
    }
}
