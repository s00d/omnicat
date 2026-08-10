//! CLI parsing via clap. No subcommands — file preview + inspect flags only.
//! Unknown flags and multi-file invocations fall through to plain `cat`.

use std::ffi::OsString;
use std::path::PathBuf;

use clap::{Args, CommandFactory, Parser};

use crate::inspect::InspectOptions;

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
}

/// Top-level parser. Prefer [`Cli::parse`] / [`Cli::try_parse_from`].
#[derive(Debug, Parser)]
#[command(
    name = "omnicat",
    version,
    about = "Universal file preview — a context-aware cat",
    long_about = LONG_ABOUT,
    after_help = AFTER_HELP,
    disable_help_subcommand = true,
    args_override_self = true,
    arg_required_else_help = true
)]
struct OmniCli {
    #[command(flatten)]
    preview: PreviewOpts,

    #[command(flatten)]
    inspect: InspectOpts,

    #[command(flatten)]
    output: OutputOpts,

    /// Force vanilla system cat
    #[arg(long = "native")]
    native: bool,

    /// Show handlers and settings
    #[arg(long = "status")]
    status: bool,

    /// Emit shell integration wrapper (`zsh` / `bash` / `powershell`)
    #[arg(long = "init", value_name = "SHELL", num_args = 0..=1, default_missing_value = "zsh")]
    init: Option<String>,

    /// File, directory, or virtual archive path (`archive.zip/inner.txt`)
    #[arg(value_name = "PATH")]
    paths: Vec<PathBuf>,
}

const LONG_ABOUT: &str = "\
Preview almost any file in the terminal (or GUI with --preview). \
Pipes, redirects, multiple files, and unknown flags behave like plain cat.

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
  omnicat backup.sql
  omnicat -d a.json b.json
  omnicat --hash file.bin
  omnicat --duplicates ~/Downloads
  omnicat --capabilities
  omnicat --status
";

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

    /// Schema / structure summary
    #[arg(long = "schema")]
    schema: bool,

    /// Aggregate stats
    #[arg(long = "stats")]
    stats: bool,

    /// Detected handler / type only
    #[arg(short = 't', long = "type")]
    type_only: bool,

    /// MIME type only
    #[arg(long = "mime")]
    mime_only: bool,

    /// Text encoding details
    #[arg(long = "encoding")]
    encoding: bool,

    /// List inspect capabilities
    #[arg(long = "capabilities")]
    capabilities: bool,

    /// Search / filter content
    #[arg(short = 'f', long = "find", value_name = "PATTERN")]
    find: Option<String>,

    /// SQL / structured query
    #[arg(short = 'q', long = "query", value_name = "QUERY")]
    query: Option<String>,

    /// Extra predicate for tabular data
    #[arg(long = "where", value_name = "EXPR")]
    where_clause: Option<String>,

    /// Show column names
    #[arg(long = "columns")]
    columns: bool,

    /// First N lines / rows
    #[arg(long = "head", value_name = "N")]
    head: Option<usize>,

    /// Last N lines / rows
    #[arg(long = "tail", value_name = "N")]
    tail: Option<usize>,

    /// Re-run on file changes
    #[arg(short = 'w', long = "watch")]
    watch: bool,

    /// Follow appended lines (tail -f)
    #[arg(short = 'F', long = "follow")]
    follow: bool,

    /// Level filter for --follow (e.g. error, warn+)
    #[arg(long = "level", value_name = "LEVEL")]
    level: Option<String>,

    /// Extract plain text
    #[arg(long = "text")]
    text: bool,

    /// Raw bytes to stdout
    #[arg(long = "raw")]
    raw: bool,

    /// Content hashes
    #[arg(long = "hash")]
    hash: bool,

    /// Find duplicate files under a directory
    #[arg(long = "duplicates")]
    duplicates: bool,

    /// Diff two files
    #[arg(short = 'd', long = "diff", value_name = "PATH", num_args = 2)]
    diff: Vec<String>,
}

#[derive(Debug)]
pub struct Cli {
    pub command: Command,
}

impl Cli {
    pub fn parse() -> Self {
        match Self::try_parse_from(std::env::args_os()) {
            Ok(cli) => cli,
            Err(err) => err.exit(),
        }
    }

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

    pub fn help_text() -> String {
        OmniCli::command().render_long_help().to_string()
    }
}

impl OmniCli {
    fn into_command(self, normalized: &[OsString]) -> Command {
        if self.status {
            return Command::Status;
        }
        if let Some(shell) = self.init {
            return Command::Init { shell };
        }
        if self.native {
            return Command::Native {
                args: self
                    .paths
                    .iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect(),
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

/// Legacy `-status` / `-native` from docs/tests.
fn normalize_legacy_flags(args: Vec<OsString>) -> Vec<OsString> {
    if args.len() < 2 {
        return args;
    }
    let mut out = Vec::with_capacity(args.len());
    out.push(args[0].clone());
    let first = args[1].to_string_lossy();
    match first.as_ref() {
        "-status" => {
            out.push(OsString::from("--status"));
            out.extend(args.into_iter().skip(2));
        }
        "-native" => {
            out.push(OsString::from("--native"));
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
        Cli::try_parse_from(argv).unwrap().command
    }

    #[test]
    fn file_path() {
        match parse(&["README.md"]) {
            Command::File { path, .. } => assert_eq!(path, "README.md"),
            other => panic!("expected file, got {other:?}"),
        }
    }

    #[test]
    fn status_flag() {
        assert!(matches!(parse(&["--status"]), Command::Status));
        assert!(matches!(parse(&["-status"]), Command::Status));
    }

    #[test]
    fn inspect_info() {
        match parse(&["-i", "x.png"]) {
            Command::Inspect { path, options } => {
                assert_eq!(path.as_deref(), Some("x.png"));
                assert!(options.info);
            }
            other => panic!("expected inspect, got {other:?}"),
        }
    }

    #[test]
    fn rejects_log_subcommand() {
        // Former `log`/`db` subcommands are now treated as paths / native fallthrough.
        let argv = ["omnicat", "log", "app.log"];
        let cmd = Cli::try_parse_from(argv).unwrap().command;
        match cmd {
            Command::File { path, .. } => assert_eq!(path, "log"),
            Command::Native { .. } => {}
            other => panic!("unexpected {other:?}"),
        }
    }
}
