//! CLI parsing via [`clap`](https://docs.rs/clap) derive API.
//!
//! Shared option groups use `#[command(flatten)]` (composition / “inheritance”).
//! Unknown flags and multi-file invocations fall through to plain `cat` (Native).

use std::ffi::OsString;
use std::path::PathBuf;

use clap::{Args, CommandFactory, Parser, Subcommand};

use crate::db::options::{DbOptions, DbOutputFormat};
use crate::inspect::InspectOptions;
use crate::log::options::LogOptions;

#[derive(Debug, Clone, Default)]
pub struct FileOptions {
    pub preview: bool,
    pub preview_only: bool,
    pub paginate: bool,
    pub allow_unsafe: bool,
}

#[derive(Debug, Clone)]
pub enum Command {
    Init {
        shell: String,
    },
    Status,
    Native {
        args: Vec<String>,
    },
    File {
        path: String,
        options: FileOptions,
    },
    Inspect {
        path: Option<String>,
        options: InspectOptions,
    },
    Open {
        file: String,
        /// Override OS default with this program (`subl`, `code`, `vim`, …)
        editor: Option<String>,
    },
    Log {
        paths: Vec<PathBuf>,
        options: Box<LogOptions>,
    },
    Db {
        source: String,
        options: Box<DbOptions>,
    },
}

/// Top-level parser. Prefer [`Cli::parse`] / [`Cli::try_parse_from`].
#[derive(Debug, Parser)]
#[command(
    name = "omnicat",
    version,
    about = "Universal file preview and artifact inspector — a context-aware cat",
    long_about = LONG_ABOUT,
    after_help = AFTER_HELP,
    disable_help_subcommand = true,
    args_override_self = true,
    arg_required_else_help = true
)]
struct OmniCli {
    #[command(subcommand)]
    subcommand: Option<SubCmd>,

    #[command(flatten)]
    preview: PreviewOpts,

    #[command(flatten)]
    inspect: InspectOpts,

    #[command(flatten)]
    output: OutputOpts,

    /// File, directory, or virtual archive path (`archive.zip/inner.txt`)
    #[arg(value_name = "PATH")]
    paths: Vec<PathBuf>,
}

const LONG_ABOUT: &str = "\
Preview almost any file in the terminal (or GUI with --preview). \
Without inspect flags, pipes, redirects, multiple files, and unknown flags \
behave like plain cat — byte for byte.

Inspector flags (--info, --find, --query, …) always write to stdout, \
including when piped. Combine them with --json for scripts.";

const AFTER_HELP: &str = "\
Configuration:
  $OMNICAT_CONFIG (alias: $SMARTCAT_CONFIG)
  ${XDG_CONFIG_HOME:-$HOME/.config}/omnicat/config.yaml
  bundled config.default.yaml

Examples:
  omnicat README.md
  omnicat -i --json image.png
  omnicat app.jsonl --find 'level:error'
  omnicat -f TODO archive.zip
  omnicat db.sqlite -q 'SELECT * FROM users LIMIT 5'
  omnicat log app.log --around 12:42:17 --context 5
  omnicat log app.log --query 'top message limit 10'
  omnicat db backup.sql --query 'SELECT * FROM users LIMIT 5'
  omnicat db dump.rdb --stats
  omnicat db /var/lib/mysql/
  omnicat -d a.json b.json
  omnicat open notes.pdf
  omnicat open notes.md --editor=subl
  omnicat --hash file.bin
  omnicat --duplicates ~/Downloads
  omnicat --capabilities
  omnicat -native README.md
  omnicat -status
";

#[derive(Debug, Subcommand)]
enum SubCmd {
    /// Emit shell integration wrapper (zsh / bash / powershell)
    Init {
        /// Shell name (`zsh`, `bash`, or `powershell`)
        #[arg(default_value = "zsh")]
        shell: String,
    },
    /// Show handlers, settings, and inspect limits
    Status,
    /// Open with the OS / system default application (or a chosen program via --editor)
    Open {
        /// Path to open
        file: PathBuf,
        /// Program to open with instead of the OS default (`subl`, `code`, `"code -n"`, …)
        #[arg(short = 'e', long = "editor", value_name = "CMD")]
        editor: Option<String>,
    },
    /// Force vanilla system cat (`-native` / `--native` also accepted)
    Native {
        /// Arguments forwarded to system cat
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// CLI-first log analysis (streaming, no index)
    Log {
        #[command(flatten)]
        log: Box<LogCmdOpts>,
        /// Log files (supports .gz/.zst, multiple files merge by timestamp)
        #[arg(value_name = "FILE")]
        files: Vec<PathBuf>,
    },
    /// Read-only database backup inspector (MySQL dump, Redis RDB/AOF, datadir)
    Db {
        #[command(flatten)]
        db: Box<DbCmdOpts>,
        /// Backup file or MySQL datadir path (no live mysql:// / redis://)
        #[arg(value_name = "SOURCE")]
        source: PathBuf,
    },
}

/// Preview / pager options (shared via flatten).
#[derive(Debug, Clone, Default, Args)]
#[command(next_help_heading = "Preview")]
struct PreviewOpts {
    /// Open a native preview window (if GUI available)
    #[arg(short = 'p', long = "preview")]
    preview: bool,

    /// Preview window only, no terminal output
    #[arg(long = "preview-only")]
    preview_only: bool,

    /// Interactive pager for long terminal output
    #[arg(short = 'g', long = "paginate")]
    paginate: bool,
}

/// Machine-output and safety knobs (shared via flatten).
#[derive(Debug, Clone, Default, Args)]
#[command(next_help_heading = "Output")]
struct OutputOpts {
    /// Machine-readable JSON for inspect modes
    #[arg(short = 'j', long = "json")]
    json: bool,

    /// Disable byte and row limits for inspect modes
    #[arg(short = 'a', long = "all")]
    all: bool,

    /// Allow raw control / escape characters in text output
    #[arg(short = 'u', long = "unsafe")]
    allow_unsafe: bool,
}

/// Artifact inspector options (shared via flatten).
#[derive(Debug, Clone, Default, Args)]
#[command(next_help_heading = "Inspect")]
struct InspectOpts {
    /// Type-specific file metadata
    #[arg(short = 'i', long = "info")]
    info: bool,

    /// Schema for structured data (CSV, JSON, Parquet, SQLite, …)
    #[arg(short = 's', long = "schema")]
    schema: bool,

    /// Statistics for a file or directory
    #[arg(short = 'z', long = "stats")]
    stats: bool,

    /// Human-readable type + MIME
    #[arg(short = 't', long = "type")]
    type_only: bool,

    /// MIME type only
    #[arg(short = 'm', long = "mime")]
    mime_only: bool,

    /// Text encoding / BOM / line endings
    #[arg(short = 'e', long = "encoding")]
    encoding: bool,

    /// Handler capability matrix (optional path)
    #[arg(short = 'C', long = "capabilities")]
    capabilities: bool,

    /// Search inside supported artifacts
    #[arg(short = 'f', long = "find", value_name = "QUERY")]
    find: Option<String>,

    /// Query (SQL / jq-lite / CSV predicate)
    #[arg(short = 'q', long = "query", value_name = "EXPR")]
    query: Option<String>,

    /// Filter JSONL / structured rows
    #[arg(short = 'y', long = "where", value_name = "PRED")]
    where_clause: Option<String>,

    /// List table columns
    #[arg(short = 'c', long = "columns")]
    columns: bool,

    /// First N table rows
    #[arg(short = 'H', long = "head", value_name = "N")]
    head: Option<usize>,

    /// Last N table rows
    #[arg(short = 'L', long = "tail", value_name = "N")]
    tail: Option<usize>,

    /// Structural / text diff between two paths
    #[arg(short = 'd', long = "diff", num_args = 2, value_names = ["LEFT", "RIGHT"])]
    diff: Vec<String>,

    /// Re-render when the file changes
    #[arg(short = 'W', long = "watch")]
    watch: bool,

    /// Tail -f with log highlighting
    #[arg(short = 'F', long = "follow")]
    follow: bool,

    /// Log level filter for --follow (e.g. error)
    #[arg(short = 'l', long = "level", value_name = "LEVEL")]
    level: Option<String>,

    /// Extract text from containers (PDF, DOCX, …)
    #[arg(short = 'x', long = "text")]
    text: bool,

    /// Raw bytes to stdout
    #[arg(short = 'r', long = "raw")]
    raw: bool,

    /// Checksums (MD5/SHA1/SHA256/SHA512/BLAKE3); recursive for directories
    #[arg(long = "hash")]
    hash: bool,

    /// Find duplicate files under a directory
    #[arg(long = "duplicates")]
    duplicates: bool,
}

/// `omnicat log` options.
#[derive(Debug, Clone, Default, Args)]
#[command(next_help_heading = "Log")]
struct LogCmdOpts {
    #[command(flatten)]
    output: OutputOpts,

    /// Live tail with parsing and level coloring
    #[arg(long = "follow")]
    follow: bool,

    /// Show only error-level messages
    #[arg(long = "errors")]
    errors: bool,

    /// Show warnings and errors
    #[arg(long = "warnings")]
    warnings: bool,

    /// Level filter (`error`, `warn+`, …)
    #[arg(long = "level", value_name = "LEVEL")]
    level: Option<String>,

    /// Streaming log statistics
    #[arg(long = "stats")]
    stats: bool,

    /// ASCII timeline histogram
    #[arg(long = "timeline")]
    timeline: bool,

    /// Message rate per time bucket
    #[arg(long = "rate")]
    rate: bool,

    /// Top values (`message`, `endpoints`, `ips`, `errors`)
    #[arg(long = "top", value_name = "FIELD", num_args = 0..=1)]
    top: Option<Option<String>>,

    /// Slowest operations by duration
    #[arg(long = "slow")]
    slow: bool,

    /// Limit for --top / --slow
    #[arg(long = "top-limit", default_value_t = 20)]
    top_limit: usize,

    /// HTTP access log summary
    #[arg(long = "http")]
    http: bool,

    /// Filter by HTTP status code
    #[arg(long = "status", value_name = "CODE")]
    status: Option<u16>,

    /// Filter by HTTP method
    #[arg(long = "method", value_name = "METHOD")]
    method: Option<String>,

    /// Show lifecycle for request/correlation id
    #[arg(long = "request", value_name = "ID")]
    request: Option<String>,

    /// Show trace span lines
    #[arg(long = "trace", value_name = "ID")]
    trace: Option<String>,

    /// Context around timestamp
    #[arg(long = "around", value_name = "TIME")]
    around: Option<String>,

    /// Lines of context around match
    #[arg(long = "context", value_name = "N")]
    context: Option<usize>,

    /// Only logs after this time (`10m`, `1h`, RFC3339)
    #[arg(long = "since", value_name = "TIME")]
    since: Option<String>,

    /// Only logs before this time
    #[arg(long = "until", value_name = "TIME")]
    until: Option<String>,

    /// Filter DSL (`level:error service:api duration:>1s`)
    #[arg(long = "where", value_name = "EXPR")]
    where_clause: Option<String>,

    /// Aggregate query (`count by level`, `top message limit 10`)
    #[arg(long = "query", value_name = "EXPR")]
    query: Option<String>,

    /// First N matching lines
    #[arg(long = "head", value_name = "N")]
    head: Option<usize>,

    /// Last N matching lines
    #[arg(long = "tail", value_name = "N")]
    tail: Option<usize>,

    /// Show scan progress on stderr
    #[arg(long = "progress")]
    progress: bool,
}

/// `omnicat db` options.
#[derive(Debug, Clone, Default, Args)]
#[command(next_help_heading = "Database")]
struct DbCmdOpts {
    #[command(flatten)]
    output: OutputOpts,

    /// SQL query over MySQL dump tables (DataFusion)
    #[arg(long = "query", value_name = "SQL")]
    query: Option<String>,

    /// Table schemas from CREATE TABLE
    #[arg(long = "schema")]
    schema: bool,

    /// List tables with row estimates
    #[arg(long = "tables")]
    tables: bool,

    /// Per-table or key-type statistics
    #[arg(long = "stats")]
    stats: bool,

    /// Sample top-N keys (Redis RDB)
    #[arg(long = "sample", value_name = "N")]
    sample: Option<usize>,

    /// Search keys or raw dump text
    #[arg(long = "find", value_name = "PAT")]
    find: Option<String>,

    /// Top field (`size`, `commands`, …)
    #[arg(long = "top", value_name = "FIELD")]
    top: Option<String>,

    #[arg(long = "top-limit", default_value_t = 20)]
    top_limit: usize,

    /// Limit inspect to one table (MySQL dump)
    #[arg(long = "table", value_name = "NAME")]
    table: Option<String>,

    /// Output format for --query (`table`, `csv`, `json`, `jsonl`)
    #[arg(long = "output", value_name = "FMT", default_value = "table")]
    query_output: DbOutputFormatArg,

    /// Write query results to file
    #[arg(long = "extract", value_name = "PATH")]
    extract: Option<String>,

    /// Show scan progress on stderr
    #[arg(long = "progress")]
    progress: bool,

    /// Emit INSERT / insertMany from --query rows (same values as the table; stdout only)
    #[arg(long = "print-query", short = 'Q', requires = "query")]
    print_query: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
enum DbOutputFormatArg {
    #[default]
    Table,
    Csv,
    Json,
    Jsonl,
}

impl From<DbOutputFormatArg> for DbOutputFormat {
    fn from(v: DbOutputFormatArg) -> Self {
        match v {
            DbOutputFormatArg::Table => DbOutputFormat::Table,
            DbOutputFormatArg::Csv => DbOutputFormat::Csv,
            DbOutputFormatArg::Json => DbOutputFormat::Json,
            DbOutputFormatArg::Jsonl => DbOutputFormat::Jsonl,
        }
    }
}

impl DbCmdOpts {
    fn into_db_options(self) -> DbOptions {
        DbOptions {
            json: self.output.json,
            schema: self.schema,
            tables: self.tables,
            stats: self.stats,
            query: self.query,
            table: self.table,
            sample: self.sample,
            find: self.find,
            top: self.top,
            top_limit: self.top_limit,
            output: self.query_output.into(),
            extract: self.extract,
            progress: self.progress,
            print_query: self.print_query,
            all: self.output.all,
        }
    }
}

impl LogCmdOpts {
    fn into_log_options(self) -> LogOptions {
        LogOptions {
            json: self.output.json,
            follow: self.follow,
            errors: self.errors,
            warnings: self.warnings,
            level: self.level,
            stats: self.stats,
            timeline: self.timeline,
            rate: self.rate,
            rate_errors: self.rate && self.errors,
            top: self.top.flatten(),
            top_limit: self.top_limit,
            slow: self.slow,
            slow_limit: self.top_limit,
            http: self.http,
            status: self.status,
            method: self.method,
            request: self.request,
            trace: self.trace,
            around: self.around,
            context: self.context,
            since: self.since,
            until: self.until,
            where_clause: self.where_clause,
            query: self.query,
            tail: self.tail,
            head: self.head,
            progress: self.progress,
            allow_unsafe: self.output.allow_unsafe,
            all: self.output.all,
        }
    }
}

pub struct Cli {
    pub command: Command,
}

impl Cli {
    /// Parse process arguments. Prints help/version and exits when requested.
    pub fn parse() -> Self {
        match Self::try_parse_from(std::env::args_os()) {
            Ok(cli) => cli,
            Err(err) => err.exit(),
        }
    }

    /// Parse an arbitrary argv (for tests). Does not exit.
    pub fn try_parse_from<I, T>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        let raw: Vec<OsString> = args.into_iter().map(Into::into).collect();
        let normalized = normalize_legacy_flags(raw);

        match OmniCli::try_parse_from(&normalized) {
            Ok(cli) => Ok(Self {
                command: cli.into_command(&normalized),
            }),
            Err(err) => match err.kind() {
                clap::error::ErrorKind::DisplayHelp
                | clap::error::ErrorKind::DisplayVersion
                | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => Err(err),
                clap::error::ErrorKind::UnknownArgument
                | clap::error::ErrorKind::InvalidSubcommand => Ok(Self {
                    command: Command::Native {
                        args: os_to_strings(&normalized[1..]),
                    },
                }),
                _ => Err(err),
            },
        }
    }

    /// Help text from clap (for docs / tests).
    pub fn help_text() -> String {
        OmniCli::command().render_long_help().to_string()
    }
}

impl OmniCli {
    fn into_command(self, normalized: &[OsString]) -> Command {
        if let Some(sub) = self.subcommand {
            return match sub {
                SubCmd::Init { shell } => Command::Init { shell },
                SubCmd::Status => Command::Status,
                SubCmd::Native { args } => Command::Native { args },
                SubCmd::Open { file, editor } => Command::Open {
                    file: file.to_string_lossy().into_owned(),
                    editor,
                },
                SubCmd::Log { log, files } => Command::Log {
                    paths: files,
                    options: Box::new(LogCmdOpts::into_log_options(*log)),
                },
                SubCmd::Db { db, source } => Command::Db {
                    source: source.to_string_lossy().into_owned(),
                    options: Box::new(DbCmdOpts::into_db_options(*db)),
                },
            };
        }

        let mut options = InspectOptions::from_clap(&self.inspect, &self.output);
        let paths: Vec<String> = self
            .paths
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();

        if !self.inspect.diff.is_empty() {
            if self.inspect.diff.len() == 2 {
                options.diff = Some((self.inspect.diff[0].clone(), self.inspect.diff[1].clone()));
            }
            return Command::Inspect {
                path: None,
                options,
            };
        }

        if options.capabilities && paths.is_empty() {
            return Command::Inspect {
                path: None,
                options,
            };
        }

        if options.has_action() {
            if paths.len() > 1 {
                return Command::Native {
                    args: os_to_strings(&normalized[1..]),
                };
            }
            return Command::Inspect {
                path: paths.first().cloned(),
                options,
            };
        }

        if paths.len() != 1 {
            return Command::Native {
                args: os_to_strings(&normalized[1..]),
            };
        }

        let mut file_opts = FileOptions {
            preview: self.preview.preview || self.preview.preview_only,
            preview_only: self.preview.preview_only,
            paginate: self.preview.paginate,
            allow_unsafe: self.output.allow_unsafe,
        };
        if file_opts.preview_only {
            file_opts.preview = true;
        }

        Command::File {
            path: paths[0].clone(),
            options: file_opts,
        }
    }
}

impl InspectOptions {
    fn from_clap(inspect: &InspectOpts, output: &OutputOpts) -> Self {
        Self {
            json: output.json,
            info: inspect.info,
            schema: inspect.schema,
            stats: inspect.stats,
            type_only: inspect.type_only,
            mime_only: inspect.mime_only,
            encoding: inspect.encoding,
            capabilities: inspect.capabilities,
            find: inspect.find.clone(),
            query: inspect.query.clone(),
            where_clause: inspect.where_clause.clone(),
            columns: inspect.columns,
            head: inspect.head,
            tail: inspect.tail,
            watch: inspect.watch,
            follow: inspect.follow,
            level: inspect.level.clone(),
            text: inspect.text,
            raw: inspect.raw,
            hash: inspect.hash,
            duplicates: inspect.duplicates,
            diff: None,
            allow_unsafe: output.allow_unsafe,
            all: output.all,
        }
    }
}

/// Legacy flags from docs/tests: `-status`, `--status`, `-native`, `--native`.
fn normalize_legacy_flags(args: Vec<OsString>) -> Vec<OsString> {
    if args.len() < 2 {
        return args;
    }
    let mut out = Vec::with_capacity(args.len());
    out.push(args[0].clone());
    let first = args[1].to_string_lossy();
    match first.as_ref() {
        "-status" | "--status" => {
            out.push(OsString::from("status"));
            out.extend(args.into_iter().skip(2));
        }
        "-native" | "--native" => {
            out.push(OsString::from("native"));
            out.extend(args.into_iter().skip(2));
        }
        _ => out.extend(args.into_iter().skip(1)),
    }
    out
}

fn os_to_strings(args: &[OsString]) -> Vec<String> {
    args.iter()
        .map(|s| s.to_string_lossy().into_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Command {
        let argv: Vec<OsString> = std::iter::once(OsString::from("omnicat"))
            .chain(args.iter().map(|s| OsString::from(*s)))
            .collect();
        Cli::try_parse_from(argv)
            .unwrap_or_else(|e| panic!("parse failed: {e}"))
            .command
    }

    #[test]
    fn help_mentions_preview_and_info() {
        let help = Cli::help_text();
        assert!(help.contains("--preview"));
        assert!(help.contains("--info"));
        assert!(help.contains("-i"));
    }

    #[test]
    fn parse_preview_short() {
        match parse(&["-p", "file.md"]) {
            Command::File { options, .. } => assert!(options.preview),
            other => panic!("expected file, got {other:?}"),
        }
    }

    #[test]
    fn parse_paginate() {
        match parse(&["--paginate", "file.py"]) {
            Command::File { options, .. } => assert!(options.paginate),
            other => panic!("expected file, got {other:?}"),
        }
    }

    #[test]
    fn parse_info_json_short() {
        match parse(&["-i", "-j", "file.png"]) {
            Command::Inspect { path, options } => {
                assert_eq!(path.as_deref(), Some("file.png"));
                assert!(options.info);
                assert!(options.json);
            }
            other => panic!("expected inspect, got {other:?}"),
        }
    }

    #[test]
    fn parse_find_short() {
        match parse(&["-f", "TODO", "README.md"]) {
            Command::Inspect { options, .. } => {
                assert_eq!(options.find.as_deref(), Some("TODO"));
            }
            other => panic!("expected inspect, got {other:?}"),
        }
    }

    #[test]
    fn parse_diff_short() {
        match parse(&["-d", "a.json", "b.json"]) {
            Command::Inspect { options, .. } => {
                assert_eq!(options.diff, Some(("a.json".into(), "b.json".into())));
            }
            other => panic!("expected inspect, got {other:?}"),
        }
    }

    #[test]
    fn parse_hash_and_duplicates() {
        match parse(&["--hash", "file.bin"]) {
            Command::Inspect { options, .. } => assert!(options.hash),
            other => panic!("expected inspect, got {other:?}"),
        }
        match parse(&["--duplicates", "dir/"]) {
            Command::Inspect { options, .. } => assert!(options.duplicates),
            other => panic!("expected inspect, got {other:?}"),
        }
    }

    #[test]
    fn parse_capabilities_no_path() {
        match parse(&["--capabilities"]) {
            Command::Inspect {
                path: None,
                options,
            } => assert!(options.capabilities),
            other => panic!("expected inspect, got {other:?}"),
        }
    }

    #[test]
    fn unknown_flag_falls_through_to_native() {
        match parse(&["-n", "file.md"]) {
            Command::Native { args } => {
                assert!(args.iter().any(|a| a == "-n"));
            }
            other => panic!("expected native, got {other:?}"),
        }
    }

    #[test]
    fn multi_file_falls_through_to_native() {
        match parse(&["a.txt", "b.txt"]) {
            Command::Native { args } => assert_eq!(args, vec!["a.txt", "b.txt"]),
            other => panic!("expected native, got {other:?}"),
        }
    }

    #[test]
    fn open_subcommand() {
        match parse(&["open", "./text.txt"]) {
            Command::Open { file, editor } => {
                assert_eq!(file, "./text.txt");
                assert!(editor.is_none());
            }
            other => panic!("expected open, got {other:?}"),
        }
    }

    #[test]
    fn open_with_editor() {
        match parse(&["open", "./text.txt", "--editor=subl"]) {
            Command::Open { file, editor } => {
                assert_eq!(file, "./text.txt");
                assert_eq!(editor.as_deref(), Some("subl"));
            }
            other => panic!("expected open, got {other:?}"),
        }
        match parse(&["open", "-e", "code -n", "notes.md"]) {
            Command::Open { file, editor } => {
                assert_eq!(file, "notes.md");
                assert_eq!(editor.as_deref(), Some("code -n"));
            }
            other => panic!("expected open, got {other:?}"),
        }
    }

    #[test]
    fn legacy_status_flag() {
        match parse(&["-status"]) {
            Command::Status => {}
            other => panic!("expected status, got {other:?}"),
        }
    }

    #[test]
    fn legacy_native_flag() {
        match parse(&["-native", "file.md"]) {
            Command::Native { args } => assert_eq!(args, vec!["file.md"]),
            other => panic!("expected native, got {other:?}"),
        }
    }

    #[test]
    fn status_long() {
        assert!(matches!(parse(&["--status"]), Command::Status));
        assert!(matches!(parse(&["status"]), Command::Status));
    }
}
