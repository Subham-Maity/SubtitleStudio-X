//! Pipelined Rust-rendered RGBA overlay & captions-only FFmpeg export pipeline.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, RwLock};

use serde::Serialize;
use subtitle_render::{FontRegistry, RenderCache, RenderTimeline, render_frame};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::error::CoreError;
use crate::export::encoder::{DetectedEncoder, detect_best_encoder, encoder_quality_args};
use crate::media::probe_audio_codec;
use crate::models::{ExportFormat, ExportMode, ExportQuality, VideoProject};

/// Real-time export progress payload emitted to UI and taskbar.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgressEvent {
    pub job_id: String,
    pub project_id: String,
    pub event_type: String,
    pub progress_pct: u8,
    pub encoder: String,
    pub is_hardware: bool,
    pub fallback_notice: Option<String>,
    pub output_path: Option<String>,
    pub message: Option<String>,
}

/// Request options for exporting a video.
#[derive(Debug, Clone)]
pub struct ExportRequest {
    pub job_id: String,
    pub mode: ExportMode,
    pub format: ExportFormat,
    pub quality: ExportQuality,
    pub background_hex: String,
    pub include_audio: bool,
    pub output_path: PathBuf,
}

/// Export a video project by piping deterministic `subtitle_render::render_frame` RGBA frames into FFmpeg.
pub async fn run_export_pipeline<F>(
    ffmpeg_path: &Path,
    ffprobe_path: &Path,
    project: &VideoProject,
    font_registry: Arc<RwLock<FontRegistry>>,
    req: &ExportRequest,
    cancel: CancellationToken,
    on_progress: F,
) -> Result<DetectedEncoder, CoreError>
where
    F: Fn(ExportProgressEvent) + Send + Sync + 'static,
{
    if let Some(parent) = req.output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let encoder = detect_best_encoder(ffmpeg_path, req.format).await;
    let out_w = project.width.max(2);
    let out_h = project.height.max(2);
    let fps = if project.fps > 1.0 { project.fps } else { 30.0 };
    let duration_sec = (project.duration_ms as f64 / 1000.0).max(0.1);
    let total_frames = ((duration_sec * fps).ceil() as u64).saturating_add(2);

    on_progress(ExportProgressEvent {
        job_id: req.job_id.clone(),
        project_id: project.id.clone(),
        event_type: "progress".to_string(),
        progress_pct: 2,
        encoder: encoder.display_name.clone(),
        is_hardware: encoder.is_hardware,
        fallback_notice: encoder.fallback_notice.clone(),
        output_path: None,
        message: Some(format!("Using {}", encoder.display_name)),
    });

    let is_mov = req.format == ExportFormat::Mov;
    let audio_codec = if req.include_audio {
        probe_audio_codec(ffprobe_path, Path::new(&project.storage_path))
            .await
            .unwrap_or(None)
    } else {
        None
    };

    let mut args: Vec<String> = vec!["-y".into()];
    match req.mode {
        ExportMode::Overlay => {
            if cfg!(not(target_os = "linux")) && encoder.is_hardware {
                args.extend(["-hwaccel".into(), "auto".into()]);
            }
            args.extend([
                "-i".into(),
                project.storage_path.clone(),
                "-f".into(),
                "rawvideo".into(),
                "-pix_fmt".into(),
                "rgba".into(),
                "-s".into(),
                format!("{out_w}x{out_h}"),
                "-framerate".into(),
                format!("{fps:.3}"),
                "-i".into(),
                "pipe:0".into(),
                "-filter_complex".into(),
                format!(
                    "[0:v]fps={fps:.3},format=yuv420p[bg];[bg][1:v]overlay=0:0:format=auto:eof_action=pass:shortest=1[v]"
                ),
                "-map".into(),
                "[v]".into(),
                "-map".into(),
                "0:a?".into(),
            ]);
        }
        ExportMode::CaptionsOnly => {
            let color_hex = normalize_lavfi_hex(&req.background_hex);
            args.extend([
                "-f".into(),
                "lavfi".into(),
                "-i".into(),
                format!("color=c={color_hex}:s={out_w}x{out_h}:r={fps:.3}:d={duration_sec:.3}"),
                "-f".into(),
                "rawvideo".into(),
                "-pix_fmt".into(),
                "rgba".into(),
                "-s".into(),
                format!("{out_w}x{out_h}"),
                "-framerate".into(),
                format!("{fps:.3}"),
                "-i".into(),
                "pipe:0".into(),
            ]);
            if req.include_audio {
                args.extend(["-i".into(), project.storage_path.clone()]);
            }
            args.extend([
                "-filter_complex".into(),
                "[0:v]format=yuv420p[bg];[bg][1:v]overlay=0:0:format=auto:eof_action=pass:shortest=1[v]"
                    .into(),
                "-map".into(),
                "[v]".into(),
            ]);
            if req.include_audio {
                args.extend(["-map".into(), "2:a?".into(), "-shortest".into()]);
            }
        }
    }

    args.extend(encoder_quality_args(&encoder.codec, req.quality, is_mov));

    if req.include_audio && let Some(ref ac) = audio_codec {
        let can_copy = matches!(ac.as_str(), "aac" | "alac" | "mp3");
        if can_copy {
            args.extend(["-c:a".into(), "copy".into()]);
        } else {
            args.extend(["-c:a".into(), "aac".into(), "-b:a".into(), "192k".into()]);
        }
    }

    args.extend([
        "-movflags".into(),
        "+faststart".into(),
        "-progress".into(),
        "pipe:2".into(),
        req.output_path.to_string_lossy().into_owned(),
    ]);

    let mut cmd = Command::new(ffmpeg_path);
    cmd.args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);

    let mut child = cmd.spawn()?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| CoreError::Process("failed to open FFmpeg stdin".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| CoreError::Process("failed to open FFmpeg stderr".into()))?;

    // Bounded channel (capacity 8 frames) so full video never buffers in memory.
    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(8);
    let timeline: RenderTimeline = project.to_render_timeline();
    let cancel_render = cancel.clone();

    let render_handle = tokio::task::spawn_blocking(move || -> Result<(), CoreError> {
        let mut cache = RenderCache::default();
        let reg_guard = font_registry
            .read()
            .map_err(|e| CoreError::Process(format!("font lock poisoned: {e}")))?;
        for frame_idx in 0..total_frames {
            if cancel_render.is_cancelled() {
                return Err(CoreError::Cancelled);
            }
            let t_ms = ((frame_idx as f64 / fps) * 1000.0).round() as i64;
            let bytes = render_frame(&timeline, t_ms, out_w, out_h, &reg_guard, &mut cache)
                .map_err(|e| CoreError::Process(e.to_string()))?;
            if tx.blocking_send(bytes.to_vec()).is_err() {
                // FFmpeg closed stdin early (shortest=1 / EOS) — normal exit.
                break;
            }
        }
        Ok(())
    });

    let writer_handle = tokio::spawn(async move {
        while let Some(frame_bytes) = rx.recv().await {
            if let Err(err) = stdin.write_all(&frame_bytes).await {
                if err.kind() == std::io::ErrorKind::BrokenPipe {
                    break;
                }
                return Err(CoreError::Io(err));
            }
        }
        let _ = stdin.shutdown().await;
        Ok::<(), CoreError>(())
    });

    // Parse `-progress pipe:2` key=value output (`out_time_us`).
    let total_us = (duration_sec * 1_000_000.0).max(1.0);
    let mut stderr_lines = BufReader::new(stderr).lines();
    let mut last_pct: u8 = 2;
    let mut stderr_tail: Vec<String> = Vec::new();

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                let _ = std::fs::remove_file(&req.output_path);
                return Err(CoreError::Cancelled);
            }
            line_res = stderr_lines.next_line() => {
                match line_res {
                    Ok(Some(line)) => {
                        if let Some(us_str) = line.strip_prefix("out_time_us=")
                            && let Ok(us) = us_str.trim().parse::<f64>()
                        {
                            let pct = ((us / total_us) * 98.0).clamp(2.0, 99.0) as u8;
                            if pct > last_pct {
                                last_pct = pct;
                                on_progress(ExportProgressEvent {
                                    job_id: req.job_id.clone(),
                                    project_id: project.id.clone(),
                                    event_type: "progress".to_string(),
                                    progress_pct: pct,
                                    encoder: encoder.display_name.clone(),
                                    is_hardware: encoder.is_hardware,
                                    fallback_notice: encoder.fallback_notice.clone(),
                                    output_path: None,
                                    message: None,
                                });
                            }
                        } else if !line.starts_with("frame=")
                            && !line.starts_with("fps=")
                            && !line.starts_with("stream_")
                            && !line.starts_with("bitrate=")
                            && !line.starts_with("total_size=")
                            && !line.starts_with("out_time")
                            && !line.starts_with("dup_frames=")
                            && !line.starts_with("drop_frames=")
                            && !line.starts_with("speed=")
                            && !line.starts_with("progress=")
                        {
                            stderr_tail.push(line);
                            if stderr_tail.len() > 20 {
                                stderr_tail.remove(0);
                            }
                        }
                    }
                    Ok(None) => break,
                    Err(e) => return Err(CoreError::Io(e)),
                }
            }
        }
    }

    let status = child.wait().await?;
    let _ = writer_handle.await;
    let _ = render_handle.await;

    if cancel.is_cancelled() {
        let _ = std::fs::remove_file(&req.output_path);
        return Err(CoreError::Cancelled);
    }

    if !status.success() {
        let _ = std::fs::remove_file(&req.output_path);
        return Err(CoreError::Process(format!(
            "FFmpeg export exited with code {:?}: {}",
            status.code(),
            stderr_tail.join(" | ")
        )));
    }

    on_progress(ExportProgressEvent {
        job_id: req.job_id.clone(),
        project_id: project.id.clone(),
        event_type: "done".to_string(),
        progress_pct: 100,
        encoder: encoder.display_name.clone(),
        is_hardware: encoder.is_hardware,
        fallback_notice: encoder.fallback_notice.clone(),
        output_path: Some(req.output_path.to_string_lossy().into_owned()),
        message: Some("Export complete".to_string()),
    });

    Ok(encoder)
}

fn normalize_lavfi_hex(hex: &str) -> String {
    let clean = hex.trim().trim_start_matches('#');
    if clean.len() == 6 {
        format!("0x{clean}")
    } else {
        "0x000000".to_string()
    }
}

