//! Tier 2 audio/video bridge: FFmpeg with live progress parsed from stderr.

use std::path::Path;
use std::time::Duration;

use tokio::io::AsyncReadExt;

use super::{command, tail, ConvertError, Result};
use crate::types::ConversionOptions;

const MEDIA_TIMEOUT: Duration = Duration::from_secs(6 * 60 * 60);

fn is_audio(ext: &str) -> bool {
    matches!(ext, "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" | "opus")
}

/// Audio codec arguments for an audio container.
fn audio_args(to: &str) -> Vec<String> {
    let a: &[&str] = match to {
        "mp3" => &["-c:a", "libmp3lame", "-q:a", "2"],
        "wav" => &["-c:a", "pcm_s16le"],
        "flac" => &["-c:a", "flac"],
        "ogg" => &["-c:a", "libvorbis", "-q:a", "5"],
        "m4a" => &["-c:a", "aac", "-b:a", "192k"],
        "aac" => &["-c:a", "aac", "-b:a", "192k", "-f", "adts"],
        "opus" => &["-c:a", "libopus", "-b:a", "128k"],
        _ => &[],
    };
    a.iter().map(|s| s.to_string()).collect()
}

/// Builds the codec/filter arguments between `-i input` and the output path.
pub fn build_args(from: &str, to: &str, opts: &ConversionOptions) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    let scale = opts.width.map(|w| format!("scale={w}:-2"));

    if is_audio(to) {
        args.push("-vn".into());
        args.extend(audio_args(to));
        return args;
    }

    fn push(args: &mut Vec<String>, items: &[&str]) {
        args.extend(items.iter().map(|s| s.to_string()));
    }
    match to {
        "gif" => {
            let w = opts.width.unwrap_or(480);
            push(&mut args, &["-an", "-vf"]);
            args.push(format!(
                "fps=12,scale={w}:-1:flags=lanczos,split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4"
            ));
            push(&mut args, &["-loop", "0"]);
            return args;
        }
        "webm" => push(&mut args, &["-c:v", "libvpx-vp9", "-crf", "32", "-b:v", "0", "-row-mt", "1", "-deadline", "good", "-cpu-used", "4", "-c:a", "libopus", "-b:a", "128k"]),
        "avi" => push(&mut args, &["-c:v", "mpeg4", "-q:v", "4", "-c:a", "libmp3lame", "-q:a", "3"]),
        // mp4 / mov / mkv
        _ => {
            push(&mut args, &["-c:v", "libx264", "-preset", "medium", "-crf", "21", "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "192k"]);
            if matches!(to, "mp4" | "mov") {
                push(&mut args, &["-movflags", "+faststart"]);
            }
        }
    }
    // A GIF source has no audio stream; mapping is implicit either way.
    if from == "gif" {
        args.push("-an".into());
    }
    match scale {
        Some(s) => args.extend(["-vf".to_string(), format!("{s},pad=ceil(iw/2)*2:ceil(ih/2)*2")]),
        // H.264 with yuv420p needs even dimensions.
        None => args.extend(["-vf".to_string(), "pad=ceil(iw/2)*2:ceil(ih/2)*2".to_string()]),
    }
    args
}

/// Parses `HH:MM:SS.ss` into seconds.
fn parse_timestamp(s: &str) -> Option<f64> {
    let mut parts = s.trim().split(':');
    let h: f64 = parts.next()?.parse().ok()?;
    let m: f64 = parts.next()?.parse().ok()?;
    let sec: f64 = parts.next()?.parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + sec)
}

fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let start = line.find(key)? + key.len();
    let rest = line[start..].trim_start();
    Some(rest.split([',', ' ']).next().unwrap_or(rest))
}

pub async fn convert(
    ffmpeg: &Path,
    input: &Path,
    output: &Path,
    from: &str,
    to: &str,
    opts: &ConversionOptions,
    on_progress: impl Fn(f32) + Send,
) -> Result<()> {
    let mut cmd = command(ffmpeg);
    cmd.args(["-hide_banner", "-nostdin", "-y", "-i"])
        .arg(input)
        .args(build_args(from, to, opts))
        .arg(output);

    let mut child = cmd
        .spawn()
        .map_err(|e| ConvertError::failed(format!("Could not start FFmpeg: {e}")))?;
    let mut stderr = child.stderr.take().expect("stderr is piped");

    let work = async {
        let mut duration: Option<f64> = None;
        let mut recent: Vec<String> = Vec::new();
        let mut line: Vec<u8> = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let n = stderr.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            // FFmpeg rewrites its stats line with '\r', so split on both.
            for &b in &buf[..n] {
                if b != b'\r' && b != b'\n' {
                    line.push(b);
                    continue;
                }
                if line.is_empty() {
                    continue;
                }
                let text = String::from_utf8_lossy(&line).into_owned();
                line.clear();
                if duration.is_none() {
                    duration = field(&text, "Duration:").and_then(parse_timestamp).filter(|d| *d > 0.0);
                }
                if let (Some(total), Some(t)) = (duration, field(&text, "time=").and_then(parse_timestamp)) {
                    on_progress((t / total).clamp(0.0, 1.0) as f32);
                }
                recent.push(text);
                if recent.len() > 40 {
                    recent.remove(0);
                }
            }
        }
        let status = child.wait().await?;
        Ok::<_, std::io::Error>((status, recent.join("\n")))
    };

    let (status, log) = tokio::time::timeout(MEDIA_TIMEOUT, work)
        .await
        .map_err(|_| ConvertError::failed("FFmpeg timed out"))??;
    if !status.success() {
        return Err(ConvertError::failed(format!("FFmpeg failed: {}", tail(&log, 3))));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_progress_fields() {
        let line = "  Duration: 00:01:30.50, start: 0.000000, bitrate: 1205 kb/s";
        assert_eq!(field(line, "Duration:").and_then(parse_timestamp), Some(90.5));
        let stats = "frame=  240 fps=60 q=28.0 size=    512kB time=00:00:10.00 bitrate= 419.4kbits/s";
        assert_eq!(field(stats, "time=").and_then(parse_timestamp), Some(10.0));
    }

    #[test]
    fn audio_extraction_drops_video() {
        let args = build_args("mp4", "mp3", &Default::default());
        assert_eq!(&args[..3], ["-vn", "-c:a", "libmp3lame"]);
    }
}
