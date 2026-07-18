use std::fs::File;
use std::io::{BufReader, Write};
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::cursor::{Hide, Show};
use crossterm::execute;
use rodio::source::Source;
use rodio::{Decoder, DeviceSinkBuilder, Player};

pub fn play_audio_with_progress(
    path: &Path,
    title: &str,
    codec: Option<&str>,
    duration_secs: Option<f64>,
    out: &mut dyn Write,
) -> Result<()> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let source = Decoder::try_from(BufReader::new(file)).context("decode audio")?;
    let total = source
        .total_duration()
        .or_else(|| duration_secs.map(Duration::from_secs_f64));

    let mut handle =
        DeviceSinkBuilder::open_default_sink().context("no audio output device available")?;
    handle.log_on_drop(false);
    let player = Player::connect_new(handle.mixer());
    player.append(source);

    let mut stdout = std::io::stdout();
    let _ = execute!(stdout, Hide);

    let mut last_line = String::new();
    while !player.empty() {
        let elapsed = player.get_pos();
        let line = format_progress_line(title, codec, elapsed, total);
        if line != last_line {
            write!(out, "\r\x1b[K{line}")?;
            out.flush()?;
            last_line = line;
        }
        std::thread::sleep(Duration::from_millis(80));
    }

    let _ = execute!(stdout, Show);
    let done = format_progress_line(title, codec, total.unwrap_or(player.get_pos()), total);
    writeln!(out, "\r\x1b[K{done}")?;
    Ok(())
}

pub fn format_progress_line(
    title: &str,
    codec: Option<&str>,
    elapsed: Duration,
    total: Option<Duration>,
) -> String {
    const BAR_WIDTH: usize = 28;
    let (filled, time_label) = match total.filter(|t| !t.is_zero()) {
        Some(total) => {
            let ratio = (elapsed.as_secs_f64() / total.as_secs_f64()).clamp(0.0, 1.0);
            let filled = (ratio * BAR_WIDTH as f64).round() as usize;
            (
                filled,
                format!("{} / {}", fmt_time(elapsed), fmt_time(total)),
            )
        }
        None => (0, fmt_time(elapsed)),
    };
    let bar: String = (0..BAR_WIDTH)
        .map(|i| if i < filled { '█' } else { '░' })
        .collect();
    let codec_s = codec.map(|c| format!("  {c}")).unwrap_or_default();
    format!("♪ {title}  [{bar}] {time_label}{codec_s}")
}

fn fmt_time(d: Duration) -> String {
    let secs = d.as_secs();
    format!("{}:{:02}", secs / 60, secs % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_bar_renders_ratio() {
        let line = format_progress_line(
            "demo.wav",
            Some("pcm"),
            Duration::from_secs(3),
            Some(Duration::from_secs(6)),
        );
        assert!(line.contains("demo.wav"));
        assert!(line.contains('█'));
        assert!(line.contains('░'));
        assert!(line.contains("0:03 / 0:06"));
        assert!(line.contains("pcm"));
    }
}
