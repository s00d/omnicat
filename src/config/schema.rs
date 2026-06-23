use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub type AppConfig = OmnicatConfig;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OmnicatConfig {
    #[serde(default, alias = "display")]
    pub terminal: TerminalSettings,
    #[serde(default)]
    pub gui: GuiSettings,
    #[serde(default)]
    pub behavior: BehaviorSettings,
    #[serde(default)]
    pub handlers: HashMap<String, HandlerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct HandlerConfig {
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub mime: Vec<String>,
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TerminalSettings {
    #[serde(default)]
    pub markdown: MarkdownDisplay,
    #[serde(default)]
    pub code: CodeDisplay,
    #[serde(default)]
    pub data: DataDisplay,
    #[serde(default)]
    pub image: ImageDisplay,
    #[serde(default)]
    pub media: MediaDisplay,
    #[serde(default)]
    pub pdf: PdfDisplay,
    #[serde(default)]
    pub archive: ArchiveDisplay,
    #[serde(default)]
    pub directory: DirectoryDisplay,
    #[serde(default)]
    pub fallback: FallbackDisplay,
    #[serde(default)]
    pub paginate: PaginateDisplay,
    #[serde(default)]
    pub document: TerminalDocumentDisplay,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TerminalDocumentDisplay {
    /// Max characters for ebook/document text in terminal (0 = no limit).
    #[serde(default = "default_terminal_document_max_chars")]
    pub max_chars: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PaginateDisplay {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Lines per page (0 = fit to terminal height).
    #[serde(default)]
    pub page_lines: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GuiSettings {
    #[serde(default)]
    pub window: GuiWindow,
    #[serde(default)]
    pub theme: GuiTheme,
    #[serde(default)]
    pub preview: GuiPreview,
    #[serde(default)]
    pub spreadsheet: GuiSpreadsheet,
    #[serde(default)]
    pub document: GuiDocument,
    #[serde(default)]
    pub image: GuiImage,
    #[serde(default)]
    pub hex: GuiHex,
    #[serde(default)]
    pub directory: GuiDirectory,
    #[serde(default)]
    pub markdown: GuiMarkdown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BehaviorSettings {
    #[serde(default = "default_preview_fallback")]
    pub preview_fallback: String,
    #[serde(default = "default_on_unknown")]
    pub on_unknown_format: String,
    #[serde(default = "default_external_timeout")]
    pub external_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MarkdownDisplay {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_wrap_width")]
    pub wrap_width: u16,
    #[serde(default = "default_heading_color")]
    pub heading_color: String,
    #[serde(default = "default_link_color")]
    pub link_color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodeDisplay {
    #[serde(default = "default_true")]
    pub line_numbers: bool,
    #[serde(default = "default_code_theme")]
    pub theme: String,
    #[serde(default = "default_code_style")]
    pub style: String,
    #[serde(default = "default_tab_width")]
    pub tab_width: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DataDisplay {
    #[serde(default = "default_true")]
    pub pretty: bool,
    #[serde(default = "default_true")]
    pub table_border: bool,
    #[serde(default = "default_max_rows")]
    pub max_rows: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImageDisplay {
    #[serde(default)]
    pub max_width: u16,
    #[serde(default = "default_protocol")]
    pub protocol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MediaDisplay {
    #[serde(default = "default_true")]
    pub playback: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PdfDisplay {
    #[serde(default = "default_true")]
    pub page_separator: bool,
    #[serde(default)]
    pub max_pages: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArchiveDisplay {
    #[serde(default = "default_true")]
    pub long_format: bool,
    #[serde(default = "default_max_entries")]
    pub max_entries: usize,
    #[serde(default = "default_tree_style")]
    pub tree_style: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DirectoryDisplay {
    #[serde(default = "default_dir_depth")]
    pub max_depth: usize,
    #[serde(default = "default_max_entries")]
    pub max_entries: usize,
    #[serde(default)]
    pub show_hidden: bool,
    #[serde(default = "default_true")]
    pub icons: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FallbackDisplay {
    #[serde(default = "default_max_bytes")]
    pub max_bytes: usize,
    #[serde(default = "default_hex_cols")]
    pub hex_cols: usize,
    #[serde(default = "default_true")]
    pub show_metadata: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GuiWindow {
    #[serde(default = "default_window_width")]
    pub width: u32,
    #[serde(default = "default_window_height")]
    pub height: u32,
    #[serde(default = "default_true")]
    pub resizable: bool,
    #[serde(default = "default_title_template")]
    pub title_template: String,
    #[serde(default)]
    pub always_on_top: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GuiTheme {
    #[serde(default = "default_theme_mode")]
    pub mode: String,
    #[serde(default = "default_accent")]
    pub accent: String,
    #[serde(default = "default_font_family")]
    pub font_family: String,
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    #[serde(default = "default_line_height")]
    pub line_height: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GuiPreview {
    #[serde(default)]
    pub open_maximized: bool,
    #[serde(default = "default_true")]
    pub remember_size: bool,
    #[serde(default = "default_true")]
    pub show_toolbar: bool,
    #[serde(default = "default_true")]
    pub show_status_bar: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GuiSpreadsheet {
    #[serde(default = "default_gui_max_rows")]
    pub max_rows: usize,
    #[serde(default = "default_gui_max_cols")]
    pub max_cols: usize,
    #[serde(default = "default_true")]
    pub header_row: bool,
    #[serde(default = "default_true")]
    pub grid_lines: bool,
    #[serde(default = "default_true")]
    pub freeze_header: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GuiDocument {
    #[serde(default = "default_max_paragraphs")]
    pub max_paragraphs: usize,
    #[serde(default = "default_true")]
    pub show_page_breaks: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GuiImage {
    #[serde(default = "default_image_fit")]
    pub fit: String,
    #[serde(default = "default_image_bg")]
    pub background: String,
    #[serde(default = "default_true")]
    pub checkerboard: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GuiHex {
    #[serde(default = "default_hex_cols")]
    pub bytes_per_row: usize,
    #[serde(default = "default_true")]
    pub uppercase: bool,
    #[serde(default = "default_true")]
    pub show_ascii: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GuiMarkdown {
    #[serde(default)]
    pub code_theme: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GuiDirectory {
    #[serde(default = "default_gui_dir_depth")]
    pub max_depth: usize,
    #[serde(default = "default_true")]
    pub lazy_expand: bool,
}

fn default_theme() -> String {
    "default".into()
}
fn default_wrap_width() -> u16 {
    80
}
fn default_heading_color() -> String {
    "cyan".into()
}
fn default_link_color() -> String {
    "blue".into()
}
fn default_true() -> bool {
    true
}
fn default_code_theme() -> String {
    "base16-ocean.dark".into()
}
fn default_code_style() -> String {
    "numbers".into()
}
fn default_tab_width() -> u8 {
    4
}
fn default_protocol() -> String {
    "auto".into()
}
fn default_max_rows() -> usize {
    1000
}
fn default_max_bytes() -> usize {
    65536
}
fn default_hex_cols() -> usize {
    16
}
fn default_preview_fallback() -> String {
    "terminal".into()
}
fn default_on_unknown() -> String {
    "fallback".into()
}
fn default_external_timeout() -> u64 {
    30
}
fn default_window_width() -> u32 {
    960
}
fn default_window_height() -> u32 {
    720
}
fn default_title_template() -> String {
    "omnicat — {file}".into()
}
fn default_theme_mode() -> String {
    "auto".into()
}
fn default_accent() -> String {
    "#6c9eff".into()
}
fn default_font_family() -> String {
    "default".into()
}
fn default_font_size() -> f32 {
    14.0
}
fn default_line_height() -> f32 {
    1.4
}
fn default_gui_max_rows() -> usize {
    500
}
fn default_gui_max_cols() -> usize {
    50
}
fn default_max_paragraphs() -> usize {
    5000
}

fn default_terminal_document_max_chars() -> usize {
    5000
}
fn default_image_fit() -> String {
    "contain".into()
}
fn default_image_bg() -> String {
    "#1e1e1e".into()
}
fn default_max_entries() -> usize {
    500
}
fn default_tree_style() -> String {
    "unicode".into()
}
fn default_dir_depth() -> usize {
    3
}
fn default_gui_dir_depth() -> usize {
    5
}

impl Default for MarkdownDisplay {
    fn default() -> Self {
        OmnicatConfig::default().terminal.markdown
    }
}

impl Default for CodeDisplay {
    fn default() -> Self {
        OmnicatConfig::default().terminal.code
    }
}

impl Default for DataDisplay {
    fn default() -> Self {
        OmnicatConfig::default().terminal.data
    }
}

impl Default for ImageDisplay {
    fn default() -> Self {
        OmnicatConfig::default().terminal.image
    }
}

impl Default for MediaDisplay {
    fn default() -> Self {
        OmnicatConfig::default().terminal.media
    }
}

impl Default for PdfDisplay {
    fn default() -> Self {
        OmnicatConfig::default().terminal.pdf
    }
}

impl Default for ArchiveDisplay {
    fn default() -> Self {
        OmnicatConfig::default().terminal.archive
    }
}

impl Default for DirectoryDisplay {
    fn default() -> Self {
        OmnicatConfig::default().terminal.directory
    }
}

impl Default for FallbackDisplay {
    fn default() -> Self {
        OmnicatConfig::default().terminal.fallback
    }
}

impl Default for PaginateDisplay {
    fn default() -> Self {
        OmnicatConfig::default().terminal.paginate
    }
}

impl Default for TerminalDocumentDisplay {
    fn default() -> Self {
        OmnicatConfig::default().terminal.document
    }
}

impl Default for GuiWindow {
    fn default() -> Self {
        OmnicatConfig::default().gui.window
    }
}

impl Default for GuiTheme {
    fn default() -> Self {
        OmnicatConfig::default().gui.theme
    }
}

impl Default for GuiPreview {
    fn default() -> Self {
        OmnicatConfig::default().gui.preview
    }
}

impl Default for GuiSpreadsheet {
    fn default() -> Self {
        OmnicatConfig::default().gui.spreadsheet
    }
}

impl Default for GuiDocument {
    fn default() -> Self {
        OmnicatConfig::default().gui.document
    }
}

impl Default for GuiImage {
    fn default() -> Self {
        OmnicatConfig::default().gui.image
    }
}

impl Default for GuiHex {
    fn default() -> Self {
        OmnicatConfig::default().gui.hex
    }
}

impl Default for GuiDirectory {
    fn default() -> Self {
        OmnicatConfig::default().gui.directory
    }
}

impl Default for GuiMarkdown {
    fn default() -> Self {
        OmnicatConfig::default().gui.markdown
    }
}

impl Default for OmnicatConfig {
    fn default() -> Self {
        serde_yaml::from_str(include_str!("../../assets/config.default.yaml"))
            .expect("bundled config must parse")
    }
}

impl Default for TerminalSettings {
    fn default() -> Self {
        OmnicatConfig::default().terminal
    }
}

impl Default for GuiSettings {
    fn default() -> Self {
        OmnicatConfig::default().gui
    }
}

impl Default for BehaviorSettings {
    fn default() -> Self {
        OmnicatConfig::default().behavior
    }
}

// Backward compat alias used by render modules
pub type DisplayConfig = OmnicatConfig;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_config_parses() {
        let cfg = OmnicatConfig::default();
        assert_eq!(cfg.terminal.code.line_numbers, true);
        assert_eq!(cfg.gui.window.width, 960);
    }

    #[test]
    fn legacy_display_alias() {
        let cfg: OmnicatConfig = serde_yaml::from_str(
            "display:\n  code:\n    line_numbers: false\n    theme: x\n    style: plain\n    tab_width: 4\n",
        )
        .unwrap();
        assert!(!cfg.terminal.code.line_numbers);
    }

    #[test]
    fn legacy_handlers_alias_parses() {
        let cfg: OmnicatConfig = serde_yaml::from_str(
            "handlers:\n  markdown:\n    commands:\n      - glow {file}\n    hint: install glow\n",
        )
        .unwrap();
        let md = cfg.handlers.get("markdown").unwrap();
        assert_eq!(md.commands, vec!["glow {file}"]);
        assert_eq!(md.hint.as_deref(), Some("install glow"));
    }
}
