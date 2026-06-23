# Releasing omnicat

## Version 0.5.0

- Config `handlers` section (legacy smartcat-compatible): external command chains + custom handlers
- Render policy: **external_then_builtin** with configurable timeout
- `-status` shows builtin `(+)`, external `(+)`/`(-)`, and custom handlers
- New builtin drivers: email (eml), notebook (ipynb), plist, parquet/feather/msgpack in data, fb2 in ebook, mp4parse in media
- README format matrix (Rust crates + recommended external tools)

### Migration from 0.4

- Visual config unchanged (`terminal` / `gui` / `behavior`)
- Empty or omitted `handlers: {}` — builtin-only behavior (same as 0.4)
- Optional: uncomment handlers in `assets/config.default.yaml` or add glow/bat/imgcat chains

## Version 0.4.0

- Orchestrator + driver architecture
- GUI always compiled (runtime `gui_available()` only)
- Directory and archive tree previews
- Extended formats: legacy Office, ebook, media, font, sqlite, 7z

## Cut a release

1. Bump `version` in [`Cargo.toml`](Cargo.toml), commit, push.
2. Tag: `git tag v0.5.0 && git push --tags`
3. Checksum release tarball.
4. Update Homebrew formula (`class Omnicat`).

## Verify locally

```bash
cargo test --all
cargo clippy -- -D warnings
cargo build --release
OMNICAT_BIN=target/release/omnicat ./test/run.sh
cargo publish --dry-run
```

## Notes

- Binary is larger than v0.3 (eframe, rusqlite bundled, symphonia, calamine, arrow/parquet always linked).
- Headless CI: set `OMNICAT_NO_GUI=1` for preview fallback tests.
- External renderers (glow, bat, jupytext) are optional; builtins work without them.
