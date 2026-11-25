//! FFprobe metadata extraction & FFmpeg sidecar path resolution.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::Deserialize;
use tokio::process::Command;

use crate::error::CoreError;
use crate::models::VideoMetadata;

/// Resolved sidecar binary paths for FFmpeg and FFprobe.
#[derive(Debug, Clone)]
pub struct SidecarBinaries {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

impl SidecarBinaries {
    /// Locate `ffmpeg` and `ffprobe` next to the current executable, in `src-tauri/binaries`, or on `PATH`.
    #[must_use]
    pub fn discover() -> Self {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf));

        let ffmpeg = find_binary(
            exe_dir.as_deref(),
            &[
                "ffmpeg.exe",
                "ffmpeg",
                "ffmpeg-x86_64-pc-windows-msvc.exe",
                "ffmpeg-x86_64-unknown-linux-gnu",
                "ffmpeg-aarch64-apple-darwin",
                "ffmpeg-x86_64-apple-darwin",
                "binaries/ffmpeg-x86_64-pc-windows-msvc.exe",
                "binaries/ffmpeg",
            ],
            "ffmpeg",
        );
        let ffprobe = find_binary(
            exe_dir.as_deref(),
            &[
                "ffprobe.exe",
                "ffprobe",
                "ffprobe-x86_64-pc-windows-msvc.exe",
                "ffprobe-x86_64-unknown-linux-gnu",
                "ffprobe-aarch64-apple-darwin",
                "ffprobe-x86_64-apple-darwin",
                "binaries/ffprobe-x86_64-pc-windows-msvc.exe",
                "binaries/ffprobe",
            ],
            "ffprobe",
        );

        Self { ffmpeg, ffprobe }
    }
}

fn find_binary(exe_dir: Option<&Path>, candidates: &[&str], fallback: &str) -> PathBuf {
    if let Some(dir) = exe_dir {
        for rel in candidates {
            let p = dir.join(rel);
            if p.is_file() {
                return p;
            }
        }
    }
    let workspace_bin = PathBuf::from("src-tauri").join("binaries");
    for rel in candidates {
        let name = Path::new(rel).file_name().unwrap_or_default();
        let p = workspace_bin.join(name);
        if p.is_file() {
            return p;
        }
    }
    let parent_workspace_bin = PathBuf::from("../../src-tauri/binaries");
    for rel in candidates {
        let name = Path::new(rel).file_name().unwrap_or_default();
        let p = parent_workspace_bin.join(name);
        if p.is_file() {
            return p;
        }
    }
    PathBuf::from(fallback)
}

#[derive(Debug, Deserialize)]
struct FfprobeOutput {
    streams: Option<Vec<FfprobeStream>>,
    format: Option<FfprobeFormat>,
}

#[derive(Debug, Deserialize)]
struct FfprobeFormat {
    duration: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FfprobeStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    r_frame_rate: Option<String>,
    avg_frame_rate: Option<String>,
    duration: Option<String>,
    tags: Option<FfprobeTags>,
    side_data_list: Option<Vec<FfprobeSideData>>,
}

#[derive(Debug, Deserialize)]
struct FfprobeTags {
    rotate: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FfprobeSideData {
    rotation: Option<serde_json::Value>,
}

/// Extract post-rotation width/height, duration_ms, and fps using `ffprobe`.
pub async fn probe_video(ffprobe_path: &Path, video_path: &Path) -> Result<VideoMetadata, CoreError> {
    let mut cmd = Command::new(ffprobe_path);
    cmd.args([
        "-v",
        "quiet",
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
    ])
    .arg(video_path)
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW

    let out = cmd.output().await?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(CoreError::Process(format!("ffprobe failed: {err}")));
    }
    let json_str = String::from_utf8_lossy(&out.stdout);
    parse_ffprobe_json(&json_str)
}

/// Probe the primary audio codec name (if an audio stream exists).
pub async fn probe_audio_codec(
    ffprobe_path: &Path,
    video_path: &Path,
) -> Result<Option<String>, CoreError> {
    let mut cmd = Command::new(ffprobe_path);
    cmd.args([
        "-v",
        "quiet",
        "-print_format",
        "json",
        "-show_streams",
        "-select_streams",
        "a:0",
    ])
    .arg(video_path)
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);

    let out = cmd.output().await?;
    if !out.status.success() {
        return Ok(None);
    }
    let parsed: FfprobeOutput = serde_json::from_slice(&out.stdout)?;
    Ok(parsed
        .streams
        .and_then(|s| s.into_iter().next())
        .and_then(|s| s.codec_name))
}

/// Parse ffprobe JSON output, swapping dimensions for ±90/±270 rotation and parsing fractional FPS.
pub fn parse_ffprobe_json(json_str: &str) -> Result<VideoMetadata, CoreError> {
    let data: FfprobeOutput = serde_json::from_str(json_str)?;
    let streams = data.streams.as_deref().unwrap_or(&[]);
    let video_stream = streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("video"));

    let duration_secs = data
        .format
        .as_ref()
        .and_then(|f| f.duration.as_deref())
        .or_else(|| streams.first().and_then(|s| s.duration.as_deref()))
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    let duration_ms = (duration_secs * 1000.0).round().max(0.0) as i64;

    let raw_w = video_stream.and_then(|s| s.width).unwrap_or(1920);
    let raw_h = video_stream.and_then(|s| s.height).unwrap_or(1080);

    let rotation = extract_rotation(video_stream);
    let (width, height) = if rotation.abs() == 90 || rotation.abs() == 270 {
        (raw_h, raw_w)
    } else {
        (raw_w, raw_h)
    };

    let fps_raw = video_stream
        .and_then(|s| s.r_frame_rate.as_deref().or(s.avg_frame_rate.as_deref()))
        .map(parse_fps_fraction)
        .filter(|v| *v > 0.0)
        .unwrap_or(30.0);
    let fps = (fps_raw * 100.0).round() / 100.0;

    Ok(VideoMetadata {
        duration_ms,
        width,
        height,
        fps,
    })
}

fn extract_rotation(stream: Option<&FfprobeStream>) -> i32 {
    let Some(s) = stream else {
        return 0;
    };
    if let Some(ref tags) = s.tags
        && let Some(ref rot) = tags.rotate
        && let Ok(v) = rot.trim().parse::<i32>()
    {
        return normalize_rotation(v);
    }
    if let Some(ref side) = s.side_data_list {
        for item in side {
            if let Some(ref val) = item.rotation {
                if let Some(n) = val.as_i64() {
                    return normalize_rotation(n as i32);
                }
                if let Some(n) = val.as_f64() {
                    return normalize_rotation(n.round() as i32);
                }
                if let Some(str_v) = val.as_str()
                    && let Ok(n) = str_v.trim().parse::<i32>()
                {
                    return normalize_rotation(n);
                }
            }
        }
    }
    0
}

fn normalize_rotation(deg: i32) -> i32 {
    let m = deg % 360;
    if m == -270 {
        90
    } else if m == 270 {
        270
    } else if m == -90 {
        -90
    } else {
        m
    }
}

fn parse_fps_fraction(raw: &str) -> f64 {
    if let Some((num_s, den_s)) = raw.split_once('/') {
        let num = num_s.trim().parse::<f64>().unwrap_or(0.0);
        let den = den_s.trim().parse::<f64>().unwrap_or(0.0);
        if den > 0.0 {
            return num / den;
        }
        return num;
    }
    raw.trim().parse::<f64>().unwrap_or(30.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swaps_dimensions_for_90_and_270_rotation() {
        let json = r#"{
          "streams": [{
            "codec_type": "video",
            "width": 1920,
            "height": 1080,
            "r_frame_rate": "30000/1001",
            "side_data_list": [{ "rotation": -90 }]
          }],
          "format": { "duration": "12.5" }
        }"#;
        let meta = parse_ffprobe_json(json).expect("parse");
        assert_eq!(meta.width, 1080);
        assert_eq!(meta.height, 1920);
        assert_eq!(meta.duration_ms, 12_500);
        assert!((meta.fps - 29.97).abs() < 0.01);
    }
}

