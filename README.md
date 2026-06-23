[![Version](https://img.shields.io/badge/version-0.5.0-blue?style=for-the-badge)](https://github.com/s00d/omnicat)
[![CI](https://img.shields.io/github/actions/workflow/status/s00d/omnicat/ci.yml?branch=main&style=for-the-badge&label=CI)](https://github.com/s00d/omnicat/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/License-MIT-blue?style=for-the-badge)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.92%2B-orange?style=for-the-badge&logo=rust)](https://www.rust-lang.org/)
[![Homebrew](https://img.shields.io/badge/Homebrew-s00d%2Fomnicat-FBB040?style=for-the-badge&logo=homebrew)](https://github.com/s00d/omnicat#install)
[![Donate](https://img.shields.io/badge/Donate-Donationalerts-ff4081?style=for-the-badge)](https://www.donationalerts.com/r/s00d88)

<p align="center">
<img src="https://github.com/s00d/omnicat/blob/main/images/omnicat-logo.png?raw=true" alt="omnicat logo" width="180">
</p>

# omnicat

**Preview almost any file in your terminal — or in a GUI window.**

omnicat is a smarter replacement for `cat` when you are working interactively. Point it at a file or folder and get a readable preview: Markdown with formatting, syntax-highlighted code, spreadsheet tables, PDF text, ebook chapters, archive trees, images, and more.

When you pipe output to another command or redirect to a file, omnicat behaves exactly like plain `cat` — raw bytes, no surprises.

**Make `cat` smart — transparently.** Keep typing `cat` as you always have. With the optional shell shim (below), a single file in an interactive terminal renders as a preview; pipes, redirects, multiple files, and flags stay plain `cat`, byte for byte. You can also call `omnicat` directly — both names do the same thing once the shim is enabled.

## Demo

Terminal previews (Markdown, syntax-highlighted code, images, spreadsheets, archives, ebooks with paging) and `--preview` GUI window — one command, no extra tools required.

<p align="center">
<img src="https://github.com/s00d/omnicat/blob/main/demo/omnicat-demo.gif?raw=true" alt="omnicat terminal demo: Markdown, Python, PNG, XLSX, directory tree, hex dump, EPUB, MOBI, CBZ, and GUI preview" width="900">
</p>

<em>Recorded with <a href="https://github.com/charmbracelet/vhs">VHS</a> — re-record: <code>vhs demo/omnicat-demo.tape</code> (see <a href="demo/README.md">demo/README.md</a>).</em>

## Who is this for?

- **Developers** who live in the terminal and want one command to inspect any artifact in a repo
- **Data and ops folks** who need quick looks at JSON, CSV, Parquet, SQLite, logs, and configs
- **Anyone on macOS or Linux** who is tired of remembering which tool opens which format

No server, no account, no external renderers required for built-in formats. Optional tools (glow, bat, imgcat, …) can be wired in through config if you want them.

## Quick start

```bash
cat README.md                 # same as omnicat README.md (with shim enabled)
omnicat README.md             # rendered Markdown
cat src/main.rs               # syntax-highlighted source
omnicat report.xlsx           # spreadsheet preview
cat archive.zip               # archive tree
omnicat notes.epub            # ebook text (paged in the terminal)
cat project/                  # directory tree

omnicat --preview diagram.png # GUI window (when a display is available)
cat file.md | grep keyword    # pipe → plain cat (raw bytes)
cat a.txt b.txt               # multiple files → plain cat
cat -n file.md                # flags → plain cat
```

## Install

### Homebrew (recommended)

macOS and Linux:

```bash
brew install s00d/omnicat/omnicat
```

If the tap is not on your machine yet:

```bash
brew tap s00d/omnicat
brew install omnicat
```

Install the latest development build from Git:

```bash
brew install s00d/omnicat/omnicat --HEAD
```

Install from a local checkout (before the formula is in the tap):

```bash
brew install rust
git clone https://github.com/s00d/omnicat.git
cd omnicat
brew install --HEAD Formula/omnicat.rb
```

Upgrade later:

```bash
brew upgrade omnicat
```

### Cargo (global install)

Install the `omnicat` binary into `~/.cargo/bin` (available everywhere if that directory is on your `PATH`):

```bash
# Rust toolchain (once) — https://rustup.rs
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# from crates.io (when published)
cargo install omnicat
```

From a git checkout:

```bash
git clone https://github.com/s00d/omnicat.git
cd omnicat
cargo install --path .
```

From this directory without publishing:

```bash
cargo install --path .
```

Upgrade after a new release:

```bash
cargo install omnicat --force
```

Requires Rust **1.92+**. If `omnicat` is not found, add Cargo’s bin dir to your shell profile:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

### Build from source

```bash
git clone https://github.com/s00d/omnicat.git
cd omnicat
cargo build --release
# binary: target/release/omnicat
```

## Make `cat` smart (opt-in)

The installed command is **`omnicat`**. It does not replace `/bin/cat` unless you ask it to.

To route interactive `cat` through omnicat, add **one line** to your shell config. After that, `cat` and `omnicat` are interchangeable in everyday use; the examples above work with either name.

### Setup

**zsh** — add to `~/.zshrc`:

```zsh
eval "$(omnicat init zsh)"
```

**bash** — add to `~/.bashrc` (or `~/.bash_profile` on macOS):

```bash
eval "$(omnicat init bash)"
```

**PowerShell**:

```powershell
omnicat init powershell | Invoke-Expression
```

Reload the shell (`source ~/.zshrc`, etc.). The shim defines a shell function named `cat` that forwards to `omnicat` when it is on your `PATH`, and falls back to the real `cat` otherwise.

What `omnicat init zsh` emits (simplified):

```bash
cat() { if command -v omnicat >/dev/null 2>&1; then command omnicat "$@"; else command cat "$@"; fi; }
```

Only your **interactive** shell runs this wrapper. Scripts, `cron`, and non-interactive `sh script.sh` keep using the system `cat`.

### Why it is safe

Preview mode runs only when **all** of these are true:

- stdout is a terminal (not a pipe or a file redirect);
- exactly **one** argument was given;
- that argument is a readable **file or directory**, not a flag (`-n`, `--help`, etc.).

Anything else delegates to the real `cat` with your arguments unchanged:

| Situation | What happens |
|-----------|----------------|
| `cat file.md` in a TTY | Rendered preview |
| `cat file.md \| grep x` | Raw bytes (stdout is not a TTY) |
| `cat a.txt b.txt` | Plain `cat` (multiple files) |
| `cat -n file.md` | Plain `cat` (flag present) |
| `cat < file.md` | Plain `cat` (no filename argument) |

### Force plain `cat`

Pass **`-native`** (or `--native`) as the **first** argument to skip rendering even in an interactive terminal:

```bash
cat -native README.md       # raw file, no Markdown rendering
omnicat -native -n file.md  # remaining args go to system cat
```

`-native` is an omnicat directive and must come first.

### Undo

Remove the `eval "$(omnicat init …)"` line from your shell config and reload. Optionally delete `~/.config/omnicat/`.

## Everyday commands

| What you want | Command |
|---------------|---------|
| Terminal preview | `cat <file>` or `omnicat <file>` (with shim) |
| Folder tree | `cat <directory>/` |
| GUI preview | `omnicat --preview <path>` |
| GUI only (no terminal output) | `omnicat --preview-only <path>` |
| Force plain `cat` | `cat -native <file> …` or `omnicat -native …` |
| Dump everything (no pager) | `omnicat --no-paginate <file>` |
| Check what works on your system | `cat -status` or `omnicat -status` |
| Help | `omnicat --help` |

## Terminal paging

Long output (big ebooks, large logs, wide tables) opens an interactive pager in the terminal.

| Key | Action |
|-----|--------|
| `Space`, `Enter`, `j`, `↓`, `PgDn` | Next page |
| `b`, `k`, `↑`, `PgUp` | Previous page |
| `g` / `G` | First / last page |
| `q`, `Esc` | Quit |

Disable paging: `--no-paginate`, or set `terminal.paginate.enabled: false` in config.

Environment: `OMNICAT_NO_PAGINATE=1`.

## GUI preview

Add `--preview` to open a native window (spreadsheets, images, slides, source with highlighting, and more).

- Works when a display is available (local desktop).
- On SSH or CI without a display: message on stderr, then terminal fallback.
- Disable GUI attempts: `OMNICAT_NO_GUI=1`.

## Supported file types

Built-in previews (no extra installs):

| Category | Examples |
|----------|----------|
| Text & markup | `.md`, `.txt`, `.rtf`, `.org` |
| Code | `.rs`, `.py`, `.js`, `.go`, `.sh`, `.sql`, … |
| Data | `.json`, `.yaml`, `.toml`, `.csv`, `.tsv`, `.parquet`, `.feather`, `.msgpack` |
| Documents | `.pdf`, `.docx`, `.odt`, `.epub`, `.mobi`, `.azw3`, `.fb2`, `.cbz` |
| Spreadsheets | `.xlsx`, `.xls`, `.ods` |
| Presentations | `.pptx`, `.odp` |
| Archives | `.zip`, `.tar`, `.gz`, `.7z`, `.bz2`, `.xz`, … |
| Media | `.mp3`, `.wav`, `.mp4`, `.mkv`, … (metadata; audio may play in terminal) |
| Images | `.png`, `.jpg`, `.gif`, `.webp`, `.svg`, … |
| Other | `.eml`, `.ipynb`, `.plist`, `.sqlite`, `.ttf`, `.ini`, directories |

Unknown binary files fall back to a hex dump.

You can plug in external tools (glow, bat, jupytext, imgcat, …) via the `handlers` section in config; they run first when installed, then built-in renderers take over.

## Configuration

Optional YAML — pick the first file that exists:

1. `$OMNICAT_CONFIG`
2. `~/.config/omnicat/config.yaml`
3. Bundled defaults shipped with the binary

Tune terminal themes, pager size, archive depth, GUI window size, document limits, and external command chains. See `assets/config.default.yaml` for all options.

```bash
omnicat -status    # show active settings and which handlers are available
```

## Platforms

- **macOS** — fully supported (terminal + GUI)
- **Linux** — fully supported (terminal + GUI when a display is present)
- **Windows** — terminal preview; GUI depends on display support

## Development

```bash
cargo test
cargo clippy -- -D warnings
cargo build --release
OMNICAT_BIN=target/release/omnicat ./test/run.sh
```

Release checklist: [`RELEASING.md`](RELEASING.md).

## License

MIT — see [LICENSE](LICENSE).
