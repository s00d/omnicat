[![Version](https://img.shields.io/badge/version-0.9.0-blue?style=for-the-badge)](https://github.com/s00d/omnicat)
[![CI](https://img.shields.io/github/actions/workflow/status/s00d/omnicat/ci.yml?branch=main&style=for-the-badge&label=CI)](https://github.com/s00d/omnicat/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/License-MIT-blue?style=for-the-badge)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.92%2B-orange?style=for-the-badge&logo=rust)](https://www.rust-lang.org/)
[![Donate](https://img.shields.io/badge/Donate-Donationalerts-ff4081?style=for-the-badge)](https://www.donationalerts.com/r/s00d88)

<p align="center">
<img src="https://github.com/s00d/omnicat/blob/main/images/omnicat-logo.png?raw=true" alt="omnicat logo" width="180">
</p>

# omnicat

**Preview almost any file in your terminal — or in a GUI window.**  
**Also: inspect artifacts, analyze logs, and query database backups — file-only, no live servers.**

omnicat is a smarter replacement for `cat` when you are working interactively. Point it at a file or folder and get a readable preview: Markdown with formatting, syntax-highlighted code, spreadsheet tables, PDF text, ebook chapters, archive trees, images, and more.

When you pipe output to another command or redirect to a file, omnicat behaves exactly like plain `cat` — raw bytes, no surprises.

**Make `cat` smart — transparently.** Keep typing `cat` as you always have. With the optional shell shim (below), a single file in an interactive terminal renders as a preview; pipes, redirects, multiple files, and flags stay plain `cat`, byte for byte. You can also call `omnicat` directly — both names do the same thing once the shim is enabled.

## What's new in 0.9

- **`omnicat db`** — universal **file-only** database inspector: MySQL dumps (streaming SQL), Redis RDB/AOF, PostgreSQL dumps, **SQLite**, **MongoDB mongodump** (BSON / archive / JSON filter `--query`), plus tier-2 exports (DynamoDB, mongoexport JSONL, Elasticsearch snapshots).
- **`--print-query` / `-Q`** — after `--query`, emit **INSERT** (SQL) or **`insertOne`/`insertMany`** (Mongo) with the **same row values** as the result table — ready to paste into a live DB. No table output when `-Q` is set.
- **Streaming MySQL dump scan** — `LIMIT` / simple `WHERE` push-down; schema/load stop at first `INSERT`; multi‑GB dumps stay low-RSS.
- **`omnicat log`** — streaming log toolkit (JSON / logfmt / nginx / tracing / text), aggregates, multi-file merge, compressed logs.
- **Inspect surface** — `--info`, `--schema`, `--find`, `--hash`, `--duplicates`, `--diff`, virtual `archive.zip/path`, JSONL log tables, and more.
- Demo GIF / fixtures updated (`demo/db`, `demo/log`); smoke: `./demo/smoke-log-db.sh`.

## Demo

Terminal previews (Markdown, syntax-highlighted code, CSV, **`db --query`**, **`log --stats`**, directory tree, `--status`) — recorded with [VHS](https://github.com/charmbracelet/vhs).

<p align="center">
<img src="https://github.com/s00d/omnicat/blob/main/demo/omnicat-demo.gif?raw=true" alt="omnicat terminal demo: Markdown, syntax-highlighted code, CSV table, db SQL query, log stats, directory tree, and --status" width="900">
</p>

<em>Re-record: <code>cargo build --release && vhs demo/omnicat-demo.tape</code> (see <a href="demo/README.md">demo/README.md</a>). Commands are typed hidden so the GIF shows results, not keystrokes.</em>

## Who is this for?

- **Developers** who live in the terminal and want one command to inspect any artifact in a repo
- **Data and ops folks** who need quick looks at JSON, CSV, Parquet, SQLite, logs, SQL dumps, and mongodump trees
- **Anyone on macOS or Linux** who is tired of remembering which tool opens which format

No server, no account, no external renderers required for built-in formats. Optional tools (glow, bat, imgcat, …) can be wired in through config if you want them.

## Quick start

```bash
cat README.md                 # same as omnicat README.md (with shim enabled)
omnicat README.md             # rendered Markdown
cat src/main.rs               # syntax-highlighted source
omnicat report.xlsx           # spreadsheet preview
cat archive.zip               # archive tree
omnicat notes.epub --paginate  # ebook text with interactive pager
cat project/                  # directory tree

omnicat --preview diagram.png # GUI window (when a display is available)
cat file.md | grep keyword    # pipe → plain cat (raw bytes)
cat a.txt b.txt               # multiple files → plain cat
cat -n file.md                # flags → plain cat

# Artifact inspector (works when piped; use --json for scripts)
omnicat --info image.png
omnicat --schema users.csv
omnicat --find TODO archive.zip
omnicat db.sqlite --query 'SELECT * FROM users LIMIT 5'
omnicat --stats .
omnicat --hash file.iso
omnicat --duplicates ~/Downloads
omnicat --diff a.json b.json
omnicat backup.zip/config.yaml
omnicat report.pdf --text
omnicat --capabilities

# Logs & DB backups
omnicat log app.log --stats
omnicat log app.log.gz --errors
omnicat db backup.sql --query "SELECT email FROM users WHERE status = 'failed' LIMIT 10"
omnicat db backup.sql --query "SELECT email FROM users LIMIT 1" --print-query
omnicat db mongodump/ --table users --query '{"status":"failed"}'
```

## Inspect

omnicat can inspect artifacts without opening a file manager:

| Flag | Purpose |
|------|---------|
| `--info` | Type-specific metadata |
| `--type` / `--mime` | Type label / MIME |
| `--schema` | Columns / tables for structured data |
| `--stats` | Counts for files, dirs, archives, text, images, media |
| `--find <q>` | Search text / JSONL fields (`level:error` or substring) |
| `--capabilities` | Supported inspect features per handler |
| `--query <expr>` | SQL (SQLite), jq-lite (JSON), predicates (CSV/JSONL/Parquet) |
| `--hash` | MD5/SHA1/SHA256/SHA512/BLAKE3 (recursive for directories) |
| `--duplicates` | Duplicate files under a directory |
| `--diff a b` | Structural / rendered diff (JSON/CSV/Markdown; SQLite tables + schema) |
| `--watch` / `--follow` | Live re-render / tail -f (JSON logs + stack traces) |
| `--text` / `--raw` | Extract text or dump bytes |
| `--encoding` | Detect encoding / BOM / line endings |
| `--json` | Machine-readable output for any inspect mode |
| `--all` | Disable byte and row limits |
| `archive.zip/path` | Virtual path into zip/tar/tar.gz/tar.zst without unzip |

JSONL logs get a first-class table layout when fields look like logs (`TIME` / `LEVEL` / `SERVICE` / `MESSAGE`). `--follow` pretty-prints JSON log lines the same way and keeps stack-trace continuations with the matching level block.

Image `--info` includes dimensions, color, and bit depth. Markdown `--diff` compares terminal-rendered text (not raw source), so equivalent markup with the same visible content is identical.

Inspector commands always write to stdout (including pipes). Without inspect flags, pipe/multi-file behavior stays plain `cat`.

## Log analysis (`omnicat log`)

CLI-first log toolkit — streaming scan, no database or index. Reads huge files via chunked I/O; supports `.gz` / `.bz2` / `.xz` / `.zst` on the fly.

```bash
omnicat log app.log
omnicat log app.log --follow
omnicat log app.log --errors
omnicat log app.log --stats
omnicat log app.log --timeline
omnicat log app.log --rate
omnicat log app.log --top message
omnicat log app.log --slow
omnicat log access.log --http
omnicat log app.log --request abc123
omnicat log app.log --where 'level:error service:api'
omnicat log app.log --since 1h --until 30m
omnicat log api.log worker.log nginx.log --errors   # merge by timestamp
omnicat log app.log --around 12:42:17 --context 5
omnicat log app.log --query 'top message limit 10'
omnicat log app.log --rate --errors
omnicat log app.log --top errors
omnicat app.jsonl --find 'level:error'
```

Auto-detects JSON logs, logfmt, nginx combined, plain text (`2026-08-06 ERROR …`), and tracing-style fields.

Multi-file mode merges lines by timestamp (best-effort). Aggregates (`--stats`, `--timeline`, `--top`, `--http`) update counters in one pass — memory stays bounded.

Fixtures: `demo/log/` — verify with `./demo/smoke-log-db.sh`.

## Database inspector (`omnicat db`)

Read-only streaming inspector for **backup files and local DB files** — no live `mysql://`, `mongodb://`, `postgres://`, or `redis://` connections.

### Examples

```bash
# MySQL dump — streaming SQL (DataFusion)
omnicat db backup.sql --tables
omnicat db backup.sql --schema
omnicat db backup.sql --stats
omnicat db backup.sql --query "SELECT id, status FROM orders WHERE status = 'failed' LIMIT 100"
omnicat db backup.sql.gz --query 'SELECT status, COUNT(*) FROM orders GROUP BY status' --output jsonl
omnicat db backup.sql --query 'SELECT * FROM orders' --table orders --extract out.jsonl

# Turn matched rows into INSERT for a live DB (same values as the table; no table output)
omnicat db backup.sql --query "SELECT email FROM users LIMIT 1" --print-query
# → INSERT INTO `users` (`email`) VALUES
#   ('a@example.com');

# SQLite
omnicat db app.db --tables
omnicat db app.db --query "SELECT * FROM users LIMIT 5"
omnicat db app.db --query "SELECT email FROM users WHERE status = 'failed'" -Q

# MongoDB mongodump (JSON filter, not SQL)
omnicat db mongodump/ --stats
omnicat db mongodump/ --table users --query '{"status":"failed"}' --output jsonl
omnicat db mongodump/ --query '{"collection":"mydb.users","filter":{"status":"failed"},"limit":10}'
omnicat db mongodump/ --table users --query '{"status":"failed"}' -Q
# → db.users.insertOne({...}) / insertMany([...])

# Redis / PostgreSQL / datadir (metadata)
omnicat db dump.rdb --stats
omnicat db dump.rdb --schema
omnicat db appendonly.aof --find 'user:123'
omnicat db backup.dump --schema
omnicat db /var/lib/mysql/            # WARNING: metadata only
```

### Supported sources

| Source | Detected by | `--query` | Inspect |
|--------|-------------|-----------|---------|
| MySQL dump (`.sql`, `.sql.gz`, `.sql.zst`, `.sql.bz2`, `.sql.xz`) | extension / sniff | SQL via DataFusion (**streaming**) | overview, `--stats`, `--schema`, `--tables`, `--find` |
| SQLite (`.db`, `.sqlite`, `.sqlite3`) | extension + magic | SQL via rusqlite (read-only) | tables / schema / row counts |
| MongoDB mongodump (dir, `.bson`, `.archive`) | `.bson` + `.metadata.json`, archive magic | **JSON filter** or `{collection,filter,limit,projection}` | collections, docs, indexes from metadata |
| mongoexport JSON/JSONL | Extended JSON lines | JSON filter (line streaming) | counts, `--find`, `--sample` |
| MongoDB WiredTiger datadir | `WiredTiger`, `journal/` | — | metadata + WARNING |
| Redis RDB (`.rdb`) | extension | — | key types, `--sample`, `--schema` patterns |
| Redis AOF (`.aof`, manifest dir) | extension | — | command counts, `--find`, `--top` |
| PostgreSQL dump (`.dump`, `.backup`, `-Fd` dir) | `PGDMP` / `toc.dat` | — (V1) | TOC via `pg_restore --list` |
| MySQL datadir (`ibdata*`, `*.ibd`) | directory layout | — | listing + WARNING |
| DynamoDB export | AWS JSON/Parquet layout | Parquet → DataFusion (V1) | tables / sizes |
| Elasticsearch snapshot | `index-*`, `meta-*` | — | indices + WARNING |

### Flags (shared)

| Flag | Meaning |
|------|---------|
| `--query` | SQL (MySQL dump / SQLite) or Mongo JSON filter |
| `--print-query` / `-Q` | After query: print **INSERT** / Mongo **insert\*** with matched values (stdout only; suppresses the result table) |
| `--table NAME` | Restrict to one table / collection (`users` or `mydb.users`) |
| `--schema` / `--tables` / `--stats` | Schema, listing, statistics |
| `--find` / `--sample` / `--top` | Search / sample / top‑N (where supported) |
| `--output table\|csv\|json\|jsonl` | Query result format (default `table`) |
| `--extract PATH` | Write query rows to a file |
| `--progress` | Scan progress on stderr |
| `--json` | Machine-readable `DbReport` for non-query inspect modes |

### MySQL dump `--query`

- Streaming: only tables in `FROM` are registered; simple predicates and `LIMIT` are pushed into the dump scan (early exit — multi‑GB dumps stay low memory).
- Use **single-quoted** SQL string literals (`'failed'`, not `"failed"`).
- `--output jsonl` writes row JSONL to stdout (not a `DbReport` envelope).

### Mongo `--query`

Not SQL. Either:

```bash
# filter only — collection from --table
omnicat db dump/ --table users --query '{"status":"failed"}'

# envelope
omnicat db dump/ --query '{"collection":"mydb.users","filter":{"status":"failed"},"limit":100,"projection":{"email":1}}'
```

V1 filter ops: equality, `$eq`, `$gt`/`$gte`/`$lt`/`$lte`, `$in`, `$exists`, `$regex`, nested paths `a.b`. No `$aggregate` / `$lookup` / cross-collection JOIN.

### `--print-query` / `-Q`

Runs the same `--query`, then prints paste-ready statements built from **result rows** (same values you would see in the table):

```text
# SQL
INSERT INTO `users` (`email`) VALUES
('a@example.com'),
('b@example.com');

# Mongo
db.users.insertMany([
  {"_id": 2, "email": "b@example.com", "status": "failed"},
]);
```

Requires `--query`. Does **not** echo your input SQL.

### Out of scope

- Live DB URLs / restore / write / index builds
- Full MQL aggregation
- Hot datadir page reads (MySQL `.ibd`, WiredTiger) — metadata + WARNING only
- PostgreSQL `--query` (file-only TOC in V1)
- Redis 8+ RDB may fail (parser `rdb 0.3`)

Fixtures: `demo/db/` — verify with `./demo/smoke-log-db.sh`. Large dump stress helper: `demo/write_large_sql_dump.py`.

## Install

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
| Open with system default app | `omnicat open <file>` |
| Open with a chosen program | `omnicat open <file> --editor=subl` |
| Force plain `cat` | `cat -native <file> …` or `omnicat -native …` |
| Page long output | `omnicat --paginate <file>` |
| Log scan | `omnicat log <file> [--stats\|--errors\|…]` |
| DB backup inspect | `omnicat db <dump> [--query\|--schema\|…]` |
| Paste-ready INSERT from query | `omnicat db <dump> --query '…' -Q` |
| Check what works on your system | `cat -status` or `omnicat -status` |
| Help | `omnicat --help` |

## Terminal paging

Long output can be browsed with an interactive pager. Paging is **off by default**; use `--paginate` (or `terminal.paginate.enabled: true` in config).

| Key | Action |
|-----|--------|
| `Space`, `Enter`, `j`, `↓`, `PgDn` | Next page |
| `b`, `k`, `↑`, `PgUp` | Previous page |
| `g` / `G` | First / last page |
| `q`, `Esc` | Quit |

Enable paging: `--paginate`, or set `terminal.paginate.enabled: true` in config.

Environment: `OMNICAT_PAGINATE=1`.

## GUI preview

Add `--preview` to open a native window (spreadsheets, images, slides, source with highlighting, and more).

- Works when a display is available (local desktop).
- On SSH or CI without a display: message on stderr, then terminal fallback.
- Disable GUI attempts: `OMNICAT_NO_GUI=1`.

## Open with the system app

```bash
omnicat open notes.md              # OS default (Preview / Browser / …)
omnicat open report.pdf
omnicat open notes.md --editor=subl
omnicat open notes.md -e "code -n"
```

Without `--editor`, `omnicat open` launches the **OS default application** (macOS `open`, Linux `xdg-open`, Windows association). With `--editor=CMD`, it runs that program instead (`subl`, `code`, `vim`, …).

## Supported file types

Built-in previews (no extra installs):

| Category | Extensions |
|----------|------------|
| Markdown | `.md`, `.markdown`, `.mdown`, `.mkd`, `.mkdn` |
| Code | `.rs`, `.py`, `.js`, `.mjs`, `.cjs`, `.ts`, `.tsx`, `.jsx`, `.sh`, `.zsh`, `.bash`, `.fish`, `.rb`, `.go`, `.c`, `.h`, `.cpp`, `.cc`, `.cxx`, `.hpp`, `.hh`, `.cs`, `.java`, `.kt`, `.kts`, `.scala`, `.groovy`, `.gradle`, `.swift`, `.dart`, `.lua`, `.sql`, `.html`, `.htm`, `.xhtml`, `.css`, `.scss`, `.sass`, `.less`, `.styl`, `.vue`, `.svelte`, `.php`, `.pl`, `.pm`, `.r`, `.xml`, `.svg`, `.hs`, `.clj`, `.cljs`, `.cljc`, `.edn`, `.ex`, `.exs`, `.erl`, `.jl`, `.nim`, `.zig`, `.ml`, `.mli`, `.fs`, `.fsx`, `.ps1`, `.bat`, `.cmd`, `.diff`, `.patch`, `.tex`, `.latex`, `.proto`, `.graphql`, `.gql`, `.sol`, `.m`, `.mm`, `.cmake`, `.coffee`, `.vim`, `.asm`, `.s` |
| Data & config | `.json`, `.jsonl`, `.ndjson`, `.yaml`, `.yml`, `.toml`, `.ini`, `.conf`, `.cfg`, `.properties`, `.env`, `.csv`, `.tsv`, `.parquet`, `.feather`, `.msgpack` |
| Documents | `.pdf`, `.docx`, `.docm`, `.odt`, `.rtf`, `.doc` (via anydoc → Markdown) |
| Ebooks | `.epub` (anydoc → Markdown), `.mobi`/`.azw*` (→ Markdown), `.fb2`/`.fbz` (→ Markdown); `.lit`/`.djvu`/`.cbr`/`.opf` unsupported stubs |
| Spreadsheets | `.xlsx`, `.xls`, `.xlsm`, `.xlsb`, `.ods` |
| Presentations | `.pptx`, `.pptm`, `.ppsx`, `.ppsm`, `.ppt`, `.pps`, `.pot`, `.odp` (via anydoc → Markdown) |
| Archives | `.zip`, `.jar`, `.war`, `.ear`, `.apk`, `.ipa`, `.xpi`, `.whl`, `.nupkg`, `.cbz`, `.tar`, `.tgz`, `.gz`, `.bz2`, `.xz`, `.7z` |
| Media | `.mp3`, `.wav`, `.flac`, `.ogg`, `.oga`, `.opus`, `.m4a`, `.aac`, `.aiff`, `.aif`, `.wma`, `.wv`, `.mp4`, `.mkv`, `.avi`, `.mov`, `.webm`, `.m4v` (metadata; audio may play in terminal) |
| Images | `.png`, `.apng`, `.jpg`, `.jpeg`, `.jfif`, `.jpe`, `.gif`, `.webp`, `.bmp`, `.tiff`, `.tif`, `.heic`, `.ico`, `.tga`, `.qoi`, `.pnm`, `.pbm`, `.pgm`, `.ppm` |
| Fonts | `.ttf`, `.otf`, `.woff`, `.woff2` |
| Databases | `.sqlite`, `.sqlite3`, `.db` (+ dumps via `omnicat db`) |
| Email | `.eml` (→ Markdown) |
| Notebooks | `.ipynb` (→ Markdown) |
| Property lists | `.plist` |
| Directories | any folder (rendered as a file tree) |

Any other file is handled by a smart fallback: UTF‑8 text (for example `.txt`, `.log`, `.env`, `.org`) is shown as text, and its content is sniffed so JSON/XML/HTML/YAML/INI‑style files are syntax‑highlighted even without a known extension. Binary files that can't be identified lead with a metadata header (path, size, detected MIME) followed by a hex dump.

You can plug in external tools (glow, bat, jupytext, imgcat, …) via the `handlers` section in config; they run first when installed, then built-in renderers take over.

## Configuration

Optional YAML — pick the first file that exists:

1. `$OMNICAT_CONFIG`
2. `~/.config/omnicat/config.yaml`
3. Bundled defaults shipped with the binary

Tune terminal themes, pager size, archive depth, GUI window size, `terminal.document.max_chars`, and external command chains. See `assets/config.default.yaml` for all options.

### Terminal color theme

Syntax highlighting adapts to your terminal background. The `terminal.code.theme` option accepts:

- `auto` (default) — detect the background (OSC 11 query, then the `COLORFGBG` variable) and pick a readable theme: a dark theme on dark terminals, a light one on light terminals.
- `light` / `dark` — force a high-contrast theme for that background.
- any [syntect](https://github.com/trishume/syntect) theme name, e.g. `base16-ocean.dark`, `InspiredGitHub`, `Solarized (light)`.

Override detection without editing config via `OMNICAT_THEME=light` or `OMNICAT_THEME=dark`.

```bash
omnicat -status    # show active settings (including the resolved code theme) and which handlers are available
```

## Platforms

- **macOS** — fully supported (terminal + GUI)
- **Linux** — fully supported (terminal + GUI when a display is present)
- **Windows** — terminal preview; GUI depends on display support

## Development

```bash
cargo test --all
cargo clippy -- -D warnings
cargo build --release
OMNICAT_BIN=target/release/omnicat ./test/run.sh
./demo/smoke.sh
./demo/smoke-log-db.sh
```

Release checklist: [`RELEASING.md`](RELEASING.md).

## License

MIT — see [LICENSE](LICENSE).
