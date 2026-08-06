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

        if config.terminal.media.playback
            && is_audio_ext(&ext)
            && std::env::var_os("OMNICAT_NO_PLAYBACK").is_none()
            && std::env::var_os("CI").is_none()
        {
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

fn container_label(ext: &str, is_video: bool) -> String {
    match ext {
        "mp4" | "m4v" | "m4a" => "MPEG-4".into(),
        "mov" => "QuickTime".into(),
        "mkv" => "Matroska".into(),
        "webm" => "WebM".into(),
        "avi" => "AVI".into(),
        "wmv" | "asf" => "ASF".into(),
        "flv" => "Flash Video".into(),
        "mp3" => "MP3".into(),
        "wav" => "WAV".into(),
        "flac" => "FLAC".into(),
        "ogg" | "oga" => "Ogg".into(),
        "opus" => "Opus".into(),
        "aac" => "AAC".into(),
        "aiff" | "aif" => "AIFF".into(),
        "wma" => "WMA".into(),
        "wv" => "WavPack".into(),
        other if !other.is_empty() => other.to_ascii_uppercase(),
        _ if is_video => "Video".into(),
        _ => "Audio".into(),
    }
}

fn write_media_info(info: &MediaInfoContent, out: &mut dyn Write) -> Result<()> {
    writeln!(out, "{}", info.title)?;
    if let Some(c) = &info.container {
        writeln!(out, "container: {c}")?;
    } else {
        writeln!(out, "format: {}", info.format)?;
    }
    if let Some(d) = info.duration_secs {
        writeln!(out, "duration: {d:.1}s")?;
    }
    if let Some(v) = &info.video {
        writeln!(out, "video: {v}")?;
    } else if let Some(c) = &info.codec {
        writeln!(out, "codec: {c}")?;
    }
    if let Some(a) = &info.audio {
        writeln!(out, "audio: {a}")?;
    }
    if let Some(b) = info.bitrate {
        writeln!(out, "bitrate: {}", format_bitrate(b))?;
    }
    for (k, v) in &info.extra {
        writeln!(out, "{k}: {v}")?;
    }
    Ok(())
}

fn format_bitrate(bps: u64) -> String {
    if bps >= 1_000_000 {
        format!("{:.1} Mbps", bps as f64 / 1_000_000.0)
    } else if bps >= 1000 {
        format!("{:.0} kbps", bps as f64 / 1000.0)
    } else {
        format!("{bps} bps")
    }
}

fn estimate_bitrate(size: u64, duration_secs: Option<f64>) -> Option<u64> {
    let d = duration_secs.filter(|d| *d > 0.0)?;
    Some(((size as f64 * 8.0) / d).round() as u64)
}

fn extract_audio_meta(path: &Path, ctx: &PreviewContext) -> Result<MediaInfoContent> {
    use symphonia::core::codecs::audio::CODEC_ID_NULL_AUDIO;
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::formats::{FormatOptions, TrackType};
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::units::Timestamp;

    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let ext = extension_lower(path);
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

    let mut codec = None;
    let mut sample_rate = None;
    let mut channels = None;
    let mut bits = None;

    if let Some(t) = track.as_ref() {
        if let Some(params) = t.codec_params.as_ref() {
            if let Some(audio) = params.audio() {
                if audio.codec != CODEC_ID_NULL_AUDIO {
                    codec = symphonia::default::get_codecs()
                        .get_audio_decoder(audio.codec)
                        .map(|dec| dec.codec.info.short_name.to_string());
                }
                sample_rate = audio.sample_rate;
                channels = audio.channels.clone().map(|c| c.count() as u16);
                bits = audio.bits_per_sample;
            }
        }
    }

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

    let audio_summary = {
        let mut parts = Vec::new();
        if let Some(c) = &codec {
            parts.push(c.clone());
        }
        if let Some(ch) = channels {
            parts.push(channel_label(ch));
        }
        if let Some(sr) = sample_rate {
            parts.push(format!("{sr} Hz"));
        }
        if let Some(b) = bits {
            parts.push(format!("{b}-bit"));
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" "))
        }
    };

    let bitrate = estimate_bitrate(size, duration);
    let container = container_label(&ext, false);

    Ok(MediaInfoContent {
        title: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        format: ctx.mime.clone().unwrap_or_else(|| "audio".into()),
        container: Some(container),
        duration_secs: duration,
        codec: codec.clone(),
        video: None,
        audio: audio_summary,
        width: None,
        height: None,
        fps: None,
        channels,
        sample_rate,
        bitrate,
        extra: vec![("path".into(), path.display().to_string())],
    })
}

fn channel_label(n: u16) -> String {
    match n {
        1 => "mono".into(),
        2 => "stereo".into(),
        6 => "5.1".into(),
        8 => "7.1".into(),
        other => format!("{other}ch"),
    }
}

fn extract_video_meta(path: &Path, ctx: &PreviewContext) -> Result<MediaInfoContent> {
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let ext = extension_lower(path);
    let container = container_label(&ext, true);

    let mut duration_secs = None;
    let mut codec = None;
    let mut width = None;
    let mut height = None;
    let mut fps = None;
    let mut audio = None;
    let mut extra = vec![("path".into(), path.display().to_string())];

    if matches!(ext.as_str(), "mp4" | "mov" | "m4v") {
        fill_mp4_meta(
            path,
            &mut duration_secs,
            &mut codec,
            &mut width,
            &mut height,
            &mut fps,
            &mut audio,
        );
    } else {
        // Best-effort probe for mkv/webm/avi via symphonia (duration/codec when available).
        if let Ok(partial) = probe_container_symphonia(path) {
            duration_secs = partial.duration_secs.or(duration_secs);
            if codec.is_none() {
                codec = partial.codec;
            }
            if audio.is_none() {
                audio = partial.audio;
            }
        } else {
            extra.push((
                "note".into(),
                "limited metadata for this container (resolution/fps unavailable)".into(),
            ));
        }
    }

    let video = match (&codec, width, height, fps) {
        (Some(c), Some(w), Some(h), Some(f)) => Some(format!("{c} {w}×{h} {f:.2}fps")),
        (Some(c), Some(w), Some(h), None) => Some(format!("{c} {w}×{h}")),
        (Some(c), _, _, _) => Some(c.clone()),
        (None, Some(w), Some(h), Some(f)) => Some(format!("{w}×{h} {f:.2}fps")),
        (None, Some(w), Some(h), None) => Some(format!("{w}×{h}")),
        _ => None,
    };

    let bitrate = estimate_bitrate(size, duration_secs);

    Ok(MediaInfoContent {
        title: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        format: ctx.mime.clone().unwrap_or_else(|| "video".into()),
        container: Some(container),
        duration_secs,
        codec,
        video,
        audio,
        width,
        height,
        fps,
        channels: None,
        sample_rate: None,
        bitrate,
        extra,
    })
}

fn fill_mp4_meta(
    path: &Path,
    duration_secs: &mut Option<f64>,
    codec: &mut Option<String>,
    width: &mut Option<u32>,
    height: &mut Option<u32>,
    fps: &mut Option<f64>,
    audio: &mut Option<String>,
) {
    let Ok(file) = File::open(path) else {
        return;
    };
    use std::io::BufReader;
    let mut reader = BufReader::new(file);
    let Ok(ctx_mp4) = mp4parse::read_mp4(&mut reader) else {
        return;
    };

    for track in ctx_mp4.tracks.iter() {
        match track.track_type {
            mp4parse::TrackType::Video => {
                if let Some(stsd) = &track.stsd {
                    for desc in &stsd.descriptions {
                        if let mp4parse::SampleEntry::Video(v) = desc {
                            *codec = Some(format!("{:?}", v.codec_type));
                            *width = Some(u32::from(v.width));
                            *height = Some(u32::from(v.height));
                            break;
                        }
                    }
                }
                if let (Some(duration), Some(timescale)) = (&track.duration, &track.timescale) {
                    let secs = duration.0 as f64 / timescale.0.max(1) as f64;
                    *duration_secs = Some(secs);
                    if let Some(stts) = &track.stts {
                        if let Some(sample) = stts.samples.first() {
                            if sample.sample_delta > 0 {
                                *fps = Some(timescale.0 as f64 / f64::from(sample.sample_delta));
                            }
                        }
                    }
                }
            }
            mp4parse::TrackType::Audio => {
                if let Some(stsd) = &track.stsd {
                    for desc in &stsd.descriptions {
                        if let mp4parse::SampleEntry::Audio(a) = desc {
                            let mut parts = vec![format!("{:?}", a.codec_type)];
                            if a.channelcount > 0 {
                                parts.push(channel_label(a.channelcount as u16));
                            }
                            if a.samplerate > 0.0 {
                                parts.push(format!("{} Hz", a.samplerate.round() as u32));
                            }
                            *audio = Some(parts.join(" "));
                            break;
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

struct PartialMedia {
    duration_secs: Option<f64>,
    codec: Option<String>,
    audio: Option<String>,
}

fn probe_container_symphonia(path: &Path) -> Result<PartialMedia> {
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::formats::{FormatOptions, TrackType};
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::units::Timestamp;

    let src = File::open(path).context("open media")?;
    let mss = MediaSourceStream::new(Box::new(src), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let format = symphonia::default::get_probe().probe(
        &hint,
        mss,
        FormatOptions::default(),
        MetadataOptions::default(),
    )?;

    let mut duration_secs = None;
    let mut codec = None;
    let mut audio = None;

    if let Some(t) = format.default_track(TrackType::Video) {
        duration_secs = match (t.duration, t.time_base) {
            (Some(dur), Some(tb)) => tb
                .calc_time(Timestamp::new(dur.get() as i64))
                .map(|time| time.as_secs_f64()),
            _ => None,
        };
        if let Some(params) = t.codec_params.as_ref() {
            if let Some(v) = params.video() {
                codec = Some(format!("{:?}", v.codec));
            }
        }
    }

    if let Some(t) = format.default_track(TrackType::Audio) {
        if duration_secs.is_none() {
            duration_secs = match (t.duration, t.time_base) {
                (Some(dur), Some(tb)) => tb
                    .calc_time(Timestamp::new(dur.get() as i64))
                    .map(|time| time.as_secs_f64()),
                _ => None,
            };
        }
        if let Some(params) = t.codec_params.as_ref() {
            if let Some(a) = params.audio() {
                let name = symphonia::default::get_codecs()
                    .get_audio_decoder(a.codec)
                    .map(|dec| dec.codec.info.short_name.to_string());
                let mut parts = Vec::new();
                if let Some(n) = name {
                    parts.push(n);
                }
                if let Some(ch) = a.channels.clone() {
                    parts.push(channel_label(ch.count() as u16));
                }
                if let Some(sr) = a.sample_rate {
                    parts.push(format!("{sr} Hz"));
                }
                if !parts.is_empty() {
                    audio = Some(parts.join(" "));
                }
            }
        }
    }

    Ok(PartialMedia {
        duration_secs,
        codec,
        audio,
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

    #[test]
    fn container_labels() {
        assert_eq!(container_label("mkv", true), "Matroska");
        assert_eq!(container_label("flac", false), "FLAC");
    }
}
