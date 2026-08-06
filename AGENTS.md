# AGENTS.md

## Cursor Cloud specific instructions

`omnicat` is a single Rust CLI crate (binary + library, no server, no database, no network). You build a binary and run it against files; there are no long-running services to start.

### Toolchain / system deps
- Requires Rust `1.92+` (see `Cargo.toml` `rust-version`). The VM's default rustup toolchain is set to `stable` (currently 1.97.x); do not use the older pinned toolchain.
- Linux build needs system libraries `libasound2-dev`, `liblzma-dev`, `pkg-config` (for the `rodio`/`symphonia` audio deps and `xz2`). These are already installed in the environment snapshot; the update script does not reinstall them.
- No `Cargo.lock` is committed (it is git-ignored), so dependency versions resolve fresh. The first `cargo build` after a clean checkout downloads/compiles many crates and is slow; the release profile uses LTO + `codegen-units=1` and takes several minutes.

### Build / lint / test / run (mirrors `.github/workflows/ci.yml`)
- Lint: `cargo fmt --check` and `cargo clippy -- -D warnings`
- Test: `cargo test --all`
- Build: `cargo build` (debug) or `cargo build --release` (binary at `target/release/omnicat`)
- Smoke tests (Unix): `OMNICAT_BIN=target/release/omnicat ./test/run.sh`

### Running / demonstrating the CLI (non-obvious)
- omnicat only renders a formatted preview when stdout is a TTY and exactly one file/dir argument is given. When output is piped or redirected it behaves like plain `cat` (raw bytes). To see rendered output non-interactively, allocate a pty, e.g. `script -qec "target/release/omnicat demo/files/sample.md" /dev/null`.
- This VM is headless: GUI `--preview` mode is unavailable and falls back to the terminal. Set `OMNICAT_NO_GUI=1` to skip GUI attempts, and `CI=1`/`OMNICAT_NO_PLAYBACK` to avoid audio playback when previewing media.
- Sample files for manual testing live in `demo/files/` (md, rs, csv, xlsx, json, pdf, epub, images, archives, sqlite, …) and `demo/dir-tree/`.
- Syntax-highlighting theme is background-aware (`terminal.code.theme: auto`). Auto-detection queries the terminal via OSC 11 (then `COLORFGBG`) and only runs when both stdin and stdout are TTYs, so it is skipped under pipes/redirects and in most test harnesses (falls back to a dark theme). For deterministic manual testing force it with `OMNICAT_THEME=light` or `OMNICAT_THEME=dark`; `omnicat -status` prints the resolved theme.
