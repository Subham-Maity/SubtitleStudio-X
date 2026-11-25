//! 1-frame hardware encoder detection (`nvenc` -> `amf` -> `qsv` -> `libx264` / `libx265`).

use std::path::Path;
use std::process::Stdio;

use serde::Serialize;
use tokio::process::Command;

use crate::models::{ExportFormat, ExportQuality};

/// Selected video encoder and whether hardware acceleration is active.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedEncoder {
    pub codec: String,
    pub is_hardware: bool,
    pub display_name: String,
    pub fallback_notice: Option<String>,
}

/// Probe encoders by test-encoding 1 black frame with FFmpeg.
pub async fn detect_best_encoder(ffmpeg_path: &Path, format: ExportFormat) -> DetectedEncoder {
    #[cfg(target_os = "macos")]
    let candidates: &[(&str, bool, &str)] = match format {
        ExportFormat::Mp4H265 => &[
            ("h264_videotoolbox", true, "Apple VideoToolbox (H.264)"),
            ("libx264", false, "CPU x264 (Fallback)"),
        ],
        ExportFormat::Mov => &[
            ("hevc_videotoolbox", true, "Apple VideoToolbox (HEVC)"),
            ("libx265", false, "CPU x265 (Fallback)"),
        ],
    };

    #[cfg(not(target_os = "macos"))]
    let candidates: &[(&str, bool, &str)] = match format {
        ExportFormat::Mp4H265 => &[
            ("h264_nvenc", true, "NVIDIA NVENC (H.264)"),
            ("h264_amf", true, "AMD AMF (H.264)"),
            ("h264_qsv", true, "Intel QuickSync (H.264)"),
            ("libx264", false, "CPU x264 (Fallback)"),
        ],
        ExportFormat::Mov => &[
            ("hevc_nvenc", true, "NVIDIA NVENC (HEVC)"),
            ("hevc_amf", true, "AMD AMF (HEVC)"),
            ("hevc_qsv", true, "Intel QuickSync (HEVC)"),
            ("libx265", false, "CPU x265 (Fallback)"),
        ],
    };

    for &(codec, is_hw, label) in candidates {
        if test_encode_one_frame(ffmpeg_path, codec).await {
            let fallback_notice = if !is_hw {
                Some("No compatible GPU hardware encoder responded; using CPU software encoding.".to_string())
            } else {
                None
            };
            return DetectedEncoder {
                codec: codec.to_string(),
                is_hardware: is_hw,
                display_name: label.to_string(),
                fallback_notice,
            };
        }
    }

    let cpu_codec = match format {
        ExportFormat::Mp4H265 => "libx264",
        ExportFormat::Mov => "libx265",
    };
    DetectedEncoder {
        codec: cpu_codec.to_string(),
        is_hardware: false,
        display_name: format!("CPU {cpu_codec}"),
        fallback_notice: Some("Using CPU software encoding fallback.".to_string()),
    }
}

async fn test_encode_one_frame(ffmpeg_path: &Path, codec: &str) -> bool {
    let mut cmd = Command::new(ffmpeg_path);
    cmd.args([
        "-v",
        "error",
        "-f",
        "lavfi",
        "-i",
        "color=c=black:s=256x256:r=30:d=0.1",
        "-frames:v",
        "1",
        "-pix_fmt",
        "yuv420p",
        "-c:v",
        codec,
        "-f",
        "null",
        "-",
    ])
    .stdout(Stdio::null())
    .stderr(Stdio::null());

    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);

    match cmd.status().await {
        Ok(st) => st.success(),
        Err(_) => false,
    }
}

/// Build quality-specific FFmpeg encoder flags.
#[must_use]
pub fn encoder_quality_args(codec: &str, quality: ExportQuality, is_mov: bool) -> Vec<String> {
    let mut args = vec!["-c:v".to_string(), codec.to_string()];

    if codec.ends_with("_nvenc") {
        let (preset, cq) = match quality {
            ExportQuality::Fast => ("p3", "22"),
            ExportQuality::Balanced => ("p5", "19"),
            ExportQuality::Quality => ("p7", "16"),
        };
        args.extend([
            "-preset".into(),
            preset.into(),
            "-cq".into(),
            cq.into(),
        ]);
    } else if codec.ends_with("_amf") {
        let (usage, qp) = match quality {
            ExportQuality::Fast => ("ultralowlatency", "23"),
            ExportQuality::Balanced => ("transcoding", "19"),
            ExportQuality::Quality => ("transcoding", "16"),
        };
        args.extend([
            "-quality".into(),
            if quality == ExportQuality::Fast { "speed".into() } else { "quality".into() },
            "-usage".into(),
            usage.into(),
            "-qp_i".into(),
            qp.into(),
            "-qp_p".into(),
            qp.into(),
        ]);
    } else if codec.ends_with("_videotoolbox") {
        let qv = match quality {
            ExportQuality::Fast => "55",
            ExportQuality::Balanced => "68",
            ExportQuality::Quality => "80",
        };
        args.extend(["-q:v".into(), qv.into(), "-allow_sw".into(), "1".into()]);
    } else if codec.ends_with("_qsv") {
        let (preset, q) = match quality {
            ExportQuality::Fast => ("veryfast", "23"),
            ExportQuality::Balanced => ("medium", "19"),
            ExportQuality::Quality => ("slow", "16"),
        };
        args.extend([
            "-preset".into(),
            preset.into(),
            "-global_quality".into(),
            q.into(),
        ]);
    } else {
        let (preset, crf) = match quality {
            ExportQuality::Fast => ("ultrafast", "22"),
            ExportQuality::Balanced => ("medium", "19"),
            ExportQuality::Quality => ("slow", "16"),
        };
        args.extend([
            "-preset".into(),
            preset.into(),
            "-crf".into(),
            crf.into(),
        ]);
    }

    if is_mov && codec.contains("hevc") || (is_mov && codec == "libx265") {
        args.extend(["-tag:v".into(), "hvc1".into()]);
    }
    args.extend(["-pix_fmt".into(), "yuv420p".into()]);
    args
}

