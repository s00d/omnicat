#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl RgbColor {
    pub const TRANSPARENT: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };

    pub fn opaque(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColoredSpan {
    pub text: String,
    pub foreground: RgbColor,
    pub background: Option<RgbColor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightedLine {
    pub line_no: Option<usize>,
    pub spans: Vec<ColoredSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightedCode {
    pub lang: String,
    pub lines: Vec<HighlightedLine>,
}

impl HighlightedCode {
    pub fn plain_text(&self) -> String {
        self.lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.text.as_str())
                    .collect::<String>()
            })
            .collect()
    }
}
