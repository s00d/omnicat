fn cargo_bin() -> std::path::PathBuf {
    assert_cmd::cargo::cargo_bin("omnicat")
}

#[cfg(unix)]
mod pty {
    use std::io::Read;
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant};

    use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};

    use super::cargo_bin;

    pub fn run_pty(args: &[&str]) -> String {
        let pty_system = NativePtySystem::default();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();

        let mut cmd = CommandBuilder::new(cargo_bin());
        // Headless CI / non-interactive runners must not open GUI or block on audio.
        cmd.env("OMNICAT_NO_GUI", "1");
        cmd.env("OMNICAT_NO_PLAYBACK", "1");
        for arg in args {
            cmd.arg(arg);
        }

        let mut child = pair.slave.spawn_command(cmd).unwrap();
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader().unwrap();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut buf = String::new();
            let _ = reader.read_to_string(&mut buf);
            let _ = tx.send(buf);
        });

        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break;
                }
            }
        }
        rx.recv_timeout(Duration::from_secs(5)).unwrap_or_default()
    }

    #[test]
    fn tty_markdown_renders_with_ansi() {
        let tmp = tempfile::NamedTempFile::with_suffix(".md").unwrap();
        std::fs::write(tmp.path(), "# Title\n\nSENTINEL-MD-CONTENT\n").unwrap();
        let path = tmp.path().to_string_lossy().to_string();

        let out = run_pty(&[&path]);
        assert!(out.contains("SENTINEL-MD-CONTENT"), "output: {out:?}");
        assert!(out.contains("\x1b["), "expected ANSI styling in TTY output");
    }

    #[test]
    fn tty_markdown_table_not_mashed() {
        let tmp = tempfile::NamedTempFile::with_suffix(".md").unwrap();
        std::fs::write(
            tmp.path(),
            "## CLI\n\n| Command | Description |\n|---------|-------------|\n| `omnicat` | Render TTY |\n",
        )
        .unwrap();
        let path = tmp.path().to_string_lossy().to_string();

        let out = run_pty(&[&path]);
        assert!(
            !out.contains("CommandDescription"),
            "table cells should not mash together: {out:?}"
        );
        assert!(out.contains("Command"), "output: {out:?}");
        assert!(out.contains("Description"), "output: {out:?}");
    }

    #[test]
    fn native_shows_raw_content() {
        let tmp = tempfile::NamedTempFile::with_suffix(".md").unwrap();
        std::fs::write(tmp.path(), "# Title\n\nSENTINEL-MD-CONTENT\n").unwrap();
        let path = tmp.path().to_string_lossy().to_string();

        let out = run_pty(&["-native", &path]);
        assert!(out.contains("SENTINEL-MD-CONTENT"), "output: {out:?}");
        assert!(
            !out.contains("\x1b[1;36m"),
            "native should not apply markdown styling"
        );
    }

    #[test]
    fn py_uses_code_renderer() {
        let tmp = tempfile::NamedTempFile::with_suffix(".py").unwrap();
        std::fs::write(tmp.path(), "print(\"SENTINEL-PY\")\n").unwrap();
        let path = tmp.path().to_string_lossy().to_string();

        let out = run_pty(&[&path]);
        assert!(out.contains("SENTINEL-PY"), "output: {out:?}");
        assert!(out.contains("\x1b["));
    }

    #[test]
    fn preview_headless_fallback_in_tty() {
        let tmp = tempfile::NamedTempFile::with_suffix(".txt").unwrap();
        std::fs::write(tmp.path(), "SENTINEL-PREVIEW-FALLBACK\n").unwrap();
        let path = tmp.path().to_string_lossy().to_string();

        std::env::set_var("OMNICAT_NO_GUI", "1");
        let out = run_pty(&["--preview", &path]);
        std::env::remove_var("OMNICAT_NO_GUI");

        assert!(
            out.contains("GUI preview unavailable"),
            "expected fallback message in output: {out:?}"
        );
        assert!(
            out.contains("SENTINEL-PREVIEW-FALLBACK"),
            "expected terminal fallback content: {out:?}"
        );
    }
}

#[test]
fn demo_fixtures_build() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let demo = root.join("demo");
    let cfg = omnicat::config::OmnicatConfig::default();

    let cases: &[(&str, &str)] = &[
        ("files/sample.md", "markdown"),
        ("files/sample.rs", "code"),
        ("files/sample.json", "data"),
        ("files/sample.csv", "data"),
        ("files/sample.png", "image"),
        ("files/sample.pdf", "pdf"),
        ("files/sample.zip", "archive"),
        ("files/sample.xlsx", "spreadsheet"),
        ("files/sample.docx", "document"),
        ("files/sample.pptx", "presentation"),
        ("files/sample.epub", "ebook"),
        ("files/sample.cbz", "ebook"),
        ("files/sample.mobi", "ebook"),
        ("files/sample.wav", "media"),
        ("files/sample.sqlite", "database"),
        ("files/sample.eml", "email"),
        ("files/sample.ipynb", "notebook"),
        ("files/sample.plist", "plist"),
        ("files/sample.txt", "fallback"),
        ("files/sample.bin", "fallback"),
        ("dir-tree", "directory"),
    ];

    for (rel, kind) in cases {
        let path = demo.join(rel);
        assert!(path.exists(), "missing demo fixture: {}", path.display());

        let resolved = omnicat::orchestrator::PreviewOrchestrator::resolve(&path, &cfg)
            .or_else(|| {
                if cfg.behavior.on_unknown_format == "fallback" {
                    Some(omnicat::orchestrator::resolve::ResolvedHandler::Builtin(
                        omnicat::detect::HandlerKind::Fallback,
                    ))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| panic!("no handler for {rel}"));
        let kind_name = match resolved {
            omnicat::orchestrator::resolve::ResolvedHandler::Builtin(k) => k.name(),
            omnicat::orchestrator::resolve::ResolvedHandler::Custom(n) => {
                panic!("unexpected custom handler {n} for {rel}")
            }
        };
        assert_eq!(kind_name, *kind, "handler mismatch for {}", path.display());

        let content =
            omnicat::orchestrator::PreviewOrchestrator::build_resolved(&resolved, &path, &cfg)
                .unwrap_or_else(|e| panic!("build failed for {}: {e:#}", path.display()));
        assert!(
            !content.plain_text().trim().is_empty()
                || matches!(
                    content,
                    omnicat::content::PreviewContent::Image(_)
                        | omnicat::content::PreviewContent::Unsupported { .. }
                ),
            "empty preview for {}",
            path.display()
        );

        #[cfg(unix)]
        {
            // Images need a real terminal graphics protocol; audio playback can block
            // indefinitely on headless runners without an output device.
            // ConPTY on Windows CI often yields empty output, so PTY checks are Unix-only.
            if !matches!(*kind, "image" | "media") {
                let path_str = path.to_string_lossy().to_string();
                let out = pty::run_pty(&[&path_str]);
                assert!(
                    !out.contains("cat:"),
                    "should not passthrough to cat for {}: {out:?}",
                    path.display()
                );
            }
        }
    }
}
