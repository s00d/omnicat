use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub type AppConfig = OmnicatConfig;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OmnicatConfig {
    #[serde(default, alias = "display")]
    pub terminal: TerminalSettings,
    #[serde(default)]
    pub behavior: BehaviorSettings,
    #[serde(default)]
    pub inspect: InspectSettings,
    #[serde(default)]
    pub handlers: HashMap<String, HandlerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InspectSettings {
    #[serde(default = "default_inspect_max_bytes")]
    pub max_bytes: usize,
    #[serde(default = "default_inspect_max_matches")]
    pub max_matches: usize,
    #[serde(default = "default_inspect_max_rows")]
    pub max_rows: usize,
    #[serde(default)]
    pub allow_unsafe: bool,
    #[serde(default)]
    pub no_limit: bool,
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
    pub archive: ArchiveDisplay,
    #[serde(default)]
    pub directory: DirectoryDisplay,
    #[serde(default)]
    pub fallback: FallbackDisplay,
    #[serde(default)]
    pub paginate: PaginateDisplay,
    #[serde(default)]
    pub document: TerminalDocumentDisplay,
    /// Runtime-only: plain terminal output without ANSI (set when paging).
    #[serde(skip, default)]
    pub plain: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TerminalDocumentDisplay {
    /// Max characters for ebook/document text in terminal (0 = no limit).
    #[serde(default = "default_terminal_document_max_chars")]
    pub max_chars: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PaginateDisplay {
    /// Lines per page (0 = terminal height − 1, or 50 if unknown).
    #[serde(default)]
    pub page_lines: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BehaviorSettings {
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
    #[serde(default = "default_max_cols")]
    pub max_cols: usize,
    #[serde(default = "default_true")]
    pub header_row: bool,
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
    #[serde(default)]
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
    "auto".into()
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
fn default_max_cols() -> usize {
    50
}
fn default_max_bytes() -> usize {
    65536
}
fn default_hex_cols() -> usize {
    16
}
fn default_on_unknown() -> String {
    "fallback".into()
}
fn default_external_timeout() -> u64 {
    30
}

fn default_terminal_document_max_chars() -> usize {
    5000
}
fn default_max_entries() -> usize {
    500
}
fn default_tree_style() -> String {
    "unicode".into()
}
fn default_dir_depth() -> usize {
    // Shallow preview: immediate children only (+ collapsed hints).
    // Deeper nesting is unreadable in a TTY; use `--all` to expand.
    1
}
fn default_inspect_max_bytes() -> usize {
    10_485_760 // 10 MB
}
fn default_inspect_max_matches() -> usize {
    200
}
fn default_inspect_max_rows() -> usize {
    1000
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

impl Default for BehaviorSettings {
    fn default() -> Self {
        OmnicatConfig::default().behavior
    }
}

impl Default for InspectSettings {
    fn default() -> Self {
        Self {
            max_bytes: default_inspect_max_bytes(),
            max_matches: default_inspect_max_matches(),
            max_rows: default_inspect_max_rows(),
            allow_unsafe: false,
            no_limit: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_config_parses() {
        let cfg = OmnicatConfig::default();
        assert_eq!(cfg.terminal.code.line_numbers, true);
        assert_eq!(cfg.terminal.data.max_rows, 1000);
        assert_eq!(cfg.terminal.directory.max_depth, 1);
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
