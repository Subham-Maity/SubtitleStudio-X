//! 60-second 1080p30 overlay export benchmark for `studio_core`.

use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, RwLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use studio_core::cues::generate_cues;
use studio_core::db::{CueInsertRow, SqliteStudioRepository, StudioRepository};
use studio_core::export::{ExportRequest, run_export_pipeline};
use studio_core::media::{SidecarBinaries, probe_video};
use studio_core::models::{ExportFormat, ExportMode, ExportQuality, UpdateStylePatch};
use studio_core::paths::StudioPaths;
use subtitle_render::{AnimationIn, FontRegistry, KaraokeMode};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir.join("..").join("..").canonicalize()?;
    let bin_dir = root.join("src-tauri").join("binaries");
    let sidecars = SidecarBinaries {
        ffmpeg: bin_dir.join("ffmpeg-x86_64-pc-windows-msvc.exe"),
        ffprobe: bin_dir.join("ffprobe-x86_64-pc-windows-msvc.exe"),
    };

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = std::env::temp_dir().join(format!("subtitle_studio_bench_{nanos}"));
    std::fs::create_dir_all(&tmp)?;

    let input_mp4 = tmp.join("input_60s_1080p30.mp4");
    let gen_status = Command::new(&sidecars.ffmpeg)
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=s=1920x1080:r=30:d=60",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:d=60",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
        ])
        .arg(&input_mp4)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await?;
    if !gen_status.success() {
        return Err("failed to synthesize 60s 1080p30 benchmark input".into());
    }

    let paths = StudioPaths::from_root(tmp.join("appdata"), root.join("src-tauri").join("fonts"))?;
    let meta = probe_video(&sidecars.ffprobe, &input_mp4).await?;
    let repo = SqliteStudioRepository::open(&paths.db_path)?;
    let proj = repo.create_project(
        "input_60s_1080p30.mp4",
        &input_mp4.to_string_lossy(),
        meta,
        Some("en"),
    )?;

    let mut words = Vec::new();
    for i in 0..120 {
        let start = i * 500;
        words.push((format!("Word{i}"), start, start + 420));
    }
    repo.replace_words(&proj.id, &words)?;
    let cues = generate_cues(&words, 6);
    let rows: Vec<CueInsertRow> = cues
        .into_iter()
        .map(|c| {
            (
                c.text,
                c.start_ms,
                c.end_ms,
                c.order,
                Some(c.word_start_idx),
                Some(c.word_end_idx),
            )
        })
        .collect();
    repo.replace_cues(&proj.id, &rows)?;
    repo.update_style(
        &proj.id,
        &UpdateStylePatch {
            anim_in: Some(AnimationIn::Pop),
            karaoke_mode: Some(KaraokeMode::HighlightCurrentWord),
            ..Default::default()
        },
    )?;

    let full_proj = repo
        .get_project(&proj.id)?
        .ok_or("project missing in benchmark")?;
    let mut reg = FontRegistry::new();
    reg.register_directories(&[paths.bundled_fonts_dir.as_path()]);
    let shared_fonts = Arc::new(RwLock::new(reg));

    let out_mp4 = tmp.join("bench_out_60s_1080p30.mp4");
    let t0 = Instant::now();
    let enc = run_export_pipeline(
        &sidecars.ffmpeg,
        &sidecars.ffprobe,
        &full_proj,
        shared_fonts,
        &ExportRequest {
            job_id: "bench-60s".to_string(),
            mode: ExportMode::Overlay,
            format: ExportFormat::Mp4H265,
            quality: ExportQuality::Balanced,
            background_hex: "#000000".to_string(),
            include_audio: true,
            output_path: out_mp4.clone(),
        },
        CancellationToken::new(),
        |_| {},
    )
    .await?;
    let elapsed = t0.elapsed();
    let out_meta = probe_video(&sidecars.ffprobe, &out_mp4).await?;
    let out_bytes = std::fs::metadata(&out_mp4)?.len();

    println!(
        "BENCHMARK_OK encoder={} is_hw={} duration_ms={} dims={}x{} size_bytes={} wall_sec={:.2} realtime_x={:.2}",
        enc.codec,
        enc.is_hardware,
        out_meta.duration_ms,
        out_meta.width,
        out_meta.height,
        out_bytes,
        elapsed.as_secs_f64(),
        60.0 / elapsed.as_secs_f64().max(0.01)
    );

    let _ = std::fs::remove_dir_all(&tmp);
    Ok(())
}

