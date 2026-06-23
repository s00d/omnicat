#[derive(Debug, Clone, Default)]
pub struct FileOptions {
    pub preview: bool,
    pub preview_only: bool,
    pub paginate: bool,
}

#[derive(Debug, Clone)]
pub enum Command {
    Version,
    Help,
    Init { shell: String },
    Status,
    Native { args: Vec<String> },
    File { path: String, options: FileOptions },
}

pub struct Cli {
    pub command: Command,
}

impl Cli {
    pub fn help_text() -> &'static str {
        r"omnicat - universal file preview for terminal and GUI

Usage:
  omnicat <file>              Render a file when stdout is a TTY
  omnicat --preview <file>    Open a native preview window (if GUI available)
  omnicat --preview-only <file>  Preview window only, no terminal output
  omnicat --paginate <file>      Interactive pager for long terminal output
  omnicat -native ...           Force the vanilla cat

Pager keys (--paginate, long output on a TTY):
  Space, Enter, j, ↓, PgDn  next page
  b, k, ↑, PgUp             previous page
  g / G                     first / last page
  q, Esc                    exit
  omnicat -status               Show handlers and settings
  omnicat init zsh              Shell integration for zsh
  omnicat init bash             Shell integration for bash
  omnicat init powershell       Shell integration for PowerShell
  omnicat --help                Show this help
  omnicat --version             Show version

Behavior:
  omnicat renders files with built-in viewers in the terminal, or in a GUI
  window with --preview. Pipes, redirects, multiple files, and flags behave
  like plain cat — byte for byte.

Configuration:
  $OMNICAT_CONFIG (alias: $SMARTCAT_CONFIG)
  ${XDG_CONFIG_HOME:-$HOME/.config}/omnicat/config.yaml
  bundled config.default.yaml
"
    }

    pub fn parse() -> Self {
        let args: Vec<String> = std::env::args().collect();
        if args.len() <= 1 {
            return Self {
                command: Command::Help,
            };
        }

        match args[1].as_str() {
            "--version" | "-V" => Self {
                command: Command::Version,
            },
            "--help" | "-h" => Self {
                command: Command::Help,
            },
            "init" => {
                let shell = args.get(2).cloned().unwrap_or_else(|| "zsh".to_string());
                Self {
                    command: Command::Init { shell },
                }
            }
            "-status" | "--status" => Self {
                command: Command::Status,
            },
            "-native" | "--native" => Self {
                command: Command::Native {
                    args: args[2..].to_vec(),
                },
            },
            _ => Self::parse_file_command(&args[1..]),
        }
    }

    fn parse_file_command(args: &[String]) -> Self {
        let mut options = FileOptions::default();
        let mut path: Option<String> = None;

        for arg in args {
            match arg.as_str() {
                "--preview" | "-p" => options.preview = true,
                "--preview-only" => {
                    options.preview = true;
                    options.preview_only = true;
                }
                "--paginate" => options.paginate = true,
                a if a.starts_with('-') => {
                    return Self {
                        command: Command::Native {
                            args: args.to_vec(),
                        },
                    };
                }
                a => {
                    if path.is_some() {
                        return Self {
                            command: Command::Native {
                                args: args.to_vec(),
                            },
                        };
                    }
                    path = Some(a.to_string());
                }
            }
        }

        if let Some(path) = path {
            Self {
                command: Command::File { path, options },
            }
        } else {
            Self {
                command: Command::Help,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_mentions_preview() {
        assert!(Cli::help_text().contains("--preview"));
    }

    #[test]
    fn parse_preview_flag() {
        let cli = parse_args(&["omnicat", "--preview", "file.md"]);
        match cli.command {
            Command::File { options, .. } => assert!(options.preview),
            _ => panic!("expected file command"),
        }
    }

    #[test]
    fn parse_paginate_flag() {
        let cli = parse_args(&["omnicat", "--paginate", "file.py"]);
        match cli.command {
            Command::File { options, .. } => assert!(options.paginate),
            _ => panic!("expected file command"),
        }
    }

    fn parse_args(args: &[&str]) -> Cli {
        let old: Vec<String> = std::env::args().collect();
        let _ = old;
        // simulate by calling parse_file_command directly
        Cli {
            command: Cli::parse_file_command(
                &args[1..].iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            )
            .command,
        }
    }
}
