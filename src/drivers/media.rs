use std::fs::File;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};

use crate::config::OmnicatConfig;
use crate::content::{preview_context, MediaInfoContent, PreviewContent, PreviewContext};
use crate::detect::HandlerKind;
use crate::drivers::PreviewDriver;
use crate::sinks::audio_playback;

pub struct MediaDriver;

const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "wav", "flac", "ogg", "oga", "opus", "m4a", "aac", "aiff", "aif", "wma", "wv",
];

const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mkv", "avi", "mov", "webm", "m4v", "wmv", "flv"];

impl MediaDriver {
    pub fn render_terminal(
        &self,
        path: &Path,
        config: &OmnicatConfig,
        out: &mut dyn Write,
    ) -> Result<()> {
        let ctx = preview_context(path);
        let ext = extension_lower(path);
        let info = if is_video_ext(&ext) {
            extract_video_meta(path, &ctx)?
        } else {
            extract_audio_meta(path, &ctx)?
        };

        write_media_info(&info, out)?;

        if config.terminal.media.playback && is_audio_ext(&ext) {
            writeln!(out)?;
            match audio_playback::play_audio_with_progress(
                path,
                &info.title,
                info.codec.as_deref(),
                info.duration_secs,
                out,
            ) {
                Ok(()) => {}
                Err(err) => writeln!(out, "playback: {err:#}")?,
            }
        }

        Ok(())
    }
}

impl PreviewDriver for MediaDriver {
    fn kind(&self) -> HandlerKind {
        HandlerKind::Media
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[
            "mp3", "wav", "flac", "ogg", "oga", "opus", "m4a", "aac", "aiff", "aif", "wma", "wv",
            "mp4", "mkv", "avi", "mov", "webm", "m4v",
        ]
    }

    fn mime_patterns(&self) -> &'static [&'static str] {
        &["audio/*", "video/*"]
    }

    fn build(
        &self,
        path: &Path,
        config: &OmnicatConfig,
        ctx: &PreviewContext,
    ) -> Result<PreviewContent> {
        let ext = extension_lower(path);
        let info = if is_video_ext(&ext) {
            extract_video_meta(path, ctx)?
        } else {
            extract_audio_meta(path, ctx)?
        };
        let _ = config;
        Ok(PreviewContent::MediaInfo(info))
    }
}

pub fn is_audio_ext(ext: &str) -> bool {
    AUDIO_EXTENSIONS.contains(&ext)
}

pub fn is_video_ext(ext: &str) -> bool {
    VIDEO_EXTENSIONS.contains(&ext)
}

fn extension_lower(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn write_media_info(info: &MediaInfoContent, out: &mut dyn Write) -> Result<()> {
    writeln!(out, "{}", info.title)?;
    writeln!(out, "format: {}", info.format)?;
    if let Some(d) = info.duration_secs {
        writeln!(out, "duration: {d:.1}s")?;
    }
    if let Some(c) = &info.codec {
        writeln!(out, "codec: {c}")?;
    }
    for (k, v) in &info.extra {
        writeln!(out, "{k}: {v}")?;
    }
    Ok(())
}

fn extract_audio_meta(path: &Path, ctx: &PreviewContext) -> Result<MediaInfoContent> {
    use symphonia::core::codecs::audio::CODEC_ID_NULL_AUDIO;
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::formats::{FormatOptions, TrackType};
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::units::Timestamp;

    let src = File::open(path).context("open audio")?;
    let mss = MediaSourceStream::new(Box::new(src), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let format = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .context("probe audio")?;
    let track = format.default_track(TrackType::Audio);
    let codec = track.as_ref().and_then(|t| {
        t.codec_params.as_ref().and_then(|params| {
            params.audio().and_then(|audio| {
                if audio.codec == CODEC_ID_NULL_AUDIO {
                    return None;
                }
                symphonia::default::get_codecs()
                    .get_audio_decoder(audio.codec)
                    .map(|dec| dec.codec.info.short_name.to_string())
            })
        })
    });
    let duration = track.as_ref().and_then(|t| {
        if let (Some(num_frames), Some(params)) = (t.num_frames, t.codec_params.as_ref()) {
            if let Some(audio) = params.audio() {
                if let Some(sr) = audio.sample_rate {
                    return Some(num_frames as f64 / sr as f64);
                }
            }
        }
        match (t.duration, t.time_base) {
            (Some(dur), Some(tb)) => tb
                .calc_time(Timestamp::new(dur.get() as i64))
                .map(|time| time.as_secs_f64()),
            _ => None,
        }
    });

    Ok(MediaInfoContent {
        title: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        format: ctx.mime.clone().unwrap_or_else(|| "audio".into()),
        duration_secs: duration,
        codec,
        bitrate: None,
        extra: vec![("path".into(), path.display().to_string())],
    })
}

fn extract_video_meta(path: &Path, ctx: &PreviewContext) -> Result<MediaInfoContent> {
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let ext = extension_lower(path);

    let mut duration_secs = None;
    let mut codec = None;

    if matches!(ext.as_str(), "mp4" | "mov") {
        if let Ok(file) = File::open(path) {
            use std::io::BufReader;
            let mut reader = BufReader::new(file);
            if let Ok(ctx_mp4) = mp4parse::read_mp4(&mut reader) {
                for track in ctx_mp4.tracks.iter() {
                    if track.track_type == mp4parse::TrackType::Video {
                        if let Some(stsd) = &track.stsd {
                            for desc in &stsd.descriptions {
                                if let mp4parse::SampleEntry::Video(v) = desc {
                                    codec = Some(format!("{:?}", v.codec_type));
                                    break;
                                }
                            }
                        }
                        if let (Some(duration), Some(timescale)) =
                            (&track.duration, &track.timescale)
                        {
                            duration_secs = Some(duration.0 as f64 / timescale.0.max(1) as f64);
                        }
                        break;
                    }
                }
            }
        }
    }

    Ok(MediaInfoContent {
        title: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        format: ctx.mime.clone().unwrap_or_else(|| "video".into()),
        duration_secs,
        codec,
        bitrate: None,
        extra: vec![
            ("path".into(), path.display().to_string()),
            ("size".into(), format!("{size} bytes")),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_audio_and_video_extensions() {
        assert!(is_audio_ext("wav"));
        assert!(is_audio_ext("flac"));
        assert!(!is_audio_ext("mp4"));
        assert!(is_video_ext("mkv"));
    }
}
