# Demo fixtures for omnicat

Sample files for manual testing of terminal preview and `--preview` GUI.

## Demo GIF

![omnicat terminal demo](omnicat-demo.gif)

Shows markdown, code, CSV table, directory tree, hex fallback, and `--status`.

Tape uses `echo` for section labels (bare `#` lines are shell comments in zsh). Terminal code theme defaults to `base16-ocean.dark` for readable colors on dark backgrounds.

Re-record (requires [VHS](https://github.com/charmbracelet/vhs)):

```bash
cargo build --release
brew install vhs   # once
vhs demo/omnicat-demo.tape
```

## Generate binaries

Text fixtures are committed as-is. Office archives, images, SQLite, etc. are produced by:

```bash
chmod +x demo/generate.sh demo/smoke.sh
./demo/generate.sh
```

Requires: `python3`, `sqlite3`, `zip` (standard on macOS). Optional: `ffmpeg` (mp3/flac/ogg), `ebook-convert` from [Calibre](https://calibre-ebook.com/) (mobi/azw3).

Verify fixtures:

```bash
./demo/smoke.sh
SMOKE_RENDER=1 ./demo/smoke.sh   # also runs cargo test demo_fixtures
```

## Quick smoke

```bash
cargo build --release
BIN=./target/release/omnicat

# Terminal
$BIN demo/files/sample.md
$BIN demo/files/sample.rs
$BIN --preview demo/files/sample.md

# All handlers (prints detected kind)
for f in demo/files/* demo/dir-tree; do
  echo "=== $f ==="
  $BIN "$f" 2>/dev/null | head -5
done
```

## Fixture map

| Handler | File | Notes |
|---------|------|-------|
| markdown | `files/sample.md` | GFM source |
| code | `files/sample.rs`, `sample.py` | syntax highlight |
| data | `files/sample.json`, `.yaml`, `.toml`, `.ini`, `.csv`, `.tsv` | |
| image | `files/sample.png`, `sample-icon.png`, `sample-wide.png`, `sample.gif` | generated |
| pdf | `files/sample.pdf` | generated |
| archive | `files/sample.zip`, `sample.tar`, `sample.tar.gz` | generated |
| spreadsheet | `files/sample.xlsx`, `sample.ods` | generated |
| document | `files/sample.docx`, `sample.odt`, `sample.rtf` | |
| presentation | `files/sample.pptx`, `sample.odp` | generated |
| legacy_office | `files/sample.doc` | OLE stub → often `Unsupported` |
| directory | `dir-tree/` | tree widget |
| ebook | `files/sample.epub`, `sample.fb2`, `sample.mobi`, `sample.azw3`, `sample.cbz`, `sample-large.mobi` | large mobi: 100 pages (`LARGE_PAGES=100`); needs calibre |
| media | `files/sample.wav` (+ `.mp3`, `.flac`, `.ogg` via ffmpeg) | playback + progress in TTY |
| font | `files/sample.ttf` | copied system font (after generate) |
| database | `files/sample.sqlite` | generated |
| email | `files/sample.eml` | |
| notebook | `files/sample.ipynb` | slides in GUI |
| plist | `files/sample.plist` | |
| fallback (text) | `files/sample.txt` | UTF-8 → source editor |
| fallback (hex) | `files/sample.bin` | binary dump |

Legacy `.doc` / `.xls` / `.ppt` need real Microsoft Office binaries for useful output; the included `.doc` is a minimal OLE container for detection only.
