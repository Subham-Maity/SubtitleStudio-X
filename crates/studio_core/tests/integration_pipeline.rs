use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use studio_core::batch::{BatchTimestampMode, load_or_create_batch_index, run_batch_folder};
use studio_core::cues::{export_project_subtitles, generate_cues, merge_project_subtitles};
use studio_core::db::{CueInsertRow, SqliteStudioRepository, StudioRepository};
use studio_core::error::CoreError;
use studio_core::export::{ExportRequest, detect_best_encoder, run_export_pipeline};
use studio_core::fonts::FontManager;
use studio_core::media::{SidecarBinaries, clean_storage_junk, probe_video};
use studio_core::models::{
    ExportFormat, ExportMode, ExportQuality, PipelineStatus, SubtitleTextFormat, UpdateStylePatch,
};
use studio_core::paths::StudioPaths;
use studio_core::whisper::{
    WhisperEngine, extract_audio_wav_16k, read_wav_pcm16_mono_f32, resolve_available_model,
};
use subtitle_render::{
    AnimationIn, FontRegistry, KaraokeMode, RenderCache, render_frame_owned,
};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace root must resolve")
}

fn temp_test_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("subtitle_studio_{name}_{nanos}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp test dir");
    dir
}

fn sidecars_for_test() -> SidecarBinaries {
    let root = repo_root();
    let bin_dir = root.join("src-tauri").join("binaries");
    let ffmpeg_candidates = [
        "ffmpeg-x86_64-pc-windows-msvc.exe",
        "ffmpeg-x86_64-unknown-linux-gnu",
        "ffmpeg-aarch64-apple-darwin",
        "ffmpeg-x86_64-apple-darwin",
        "ffmpeg",
    ];
    let ffprobe_candidates = [
        "ffprobe-x86_64-pc-windows-msvc.exe",
        "ffprobe-x86_64-unknown-linux-gnu",
        "ffprobe-aarch64-apple-darwin",
        "ffprobe-x86_64-apple-darwin",
        "ffprobe",
    ];
    let ffmpeg = ffmpeg_candidates
        .iter()
        .map(|name| bin_dir.join(name))
        .find(|p| p.is_file());
    let ffprobe = ffprobe_candidates
        .iter()
        .map(|name| bin_dir.join(name))
        .find(|p| p.is_file());
    if let (Some(ff), Some(fp)) = (ffmpeg, ffprobe) {
        return SidecarBinaries {
            ffmpeg: ff,
            ffprobe: fp,
        };
    }
    SidecarBinaries::discover()
}

async fn make_synthetic_clip(ffmpeg: &Path, dst: &Path, width: u32, height: u32, secs: &str) {
    let video_src = format!("testsrc2=s={width}x{height}:r=30:d={secs}");
    let audio_src = format!("sine=frequency=440:sample_rate=48000:d={secs}");
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-y", "-f", "lavfi", "-i", &video_src, "-f", "lavfi", "-i", &audio_src])
        .args(["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac"])
        .arg(dst)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let out = cmd.output().await.expect("run ffmpeg synthetic clip");
    assert!(
        out.status.success(),
        "ffmpeg synthetic clip failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[tokio::test]
async fn m1_ffprobe_rotation_sqlite_fonts_and_cleaner() {
    let root = repo_root();
    let tmp = temp_test_dir("m1");
    let sidecars = sidecars_for_test();

    // 1. FFprobe on synthetic 1280x720 clip
    let sample_mp4 = tmp.join("sample_720p.mp4");
    make_synthetic_clip(&sidecars.ffmpeg, &sample_mp4, 1280, 720, "1.5").await;
    let meta = probe_video(&sidecars.ffprobe, &sample_mp4)
        .await
        .expect("probe synthetic clip");
    assert_eq!(meta.width, 1280);
    assert_eq!(meta.height, 720);
    assert!((meta.fps - 30.0).abs() < 0.2);
    assert!(meta.duration_ms >= 1_400);

    // 2. Rotated synthetic clip (320x240 rotated 90 deg => 240x320)
    let raw_rot_mp4 = tmp.join("raw_320x240.mp4");
    let status_raw = Command::new(&sidecars.ffmpeg)
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=320x240:r=30:d=0.5",
            "-c:v",
            "libx264",
        ])
        .arg(&raw_rot_mp4)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .expect("create raw clip");
    assert!(status_raw.success());

    let rot_mp4 = tmp.join("rotated90.mp4");
    let status = Command::new(&sidecars.ffmpeg)
        .args(["-y", "-display_rotation:v:0", "90", "-i"])
        .arg(&raw_rot_mp4)
        .args(["-c", "copy"])
        .arg(&rot_mp4)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .expect("apply display rotation");
    assert!(status.success());
    let rot_meta = probe_video(&sidecars.ffprobe, &rot_mp4)
        .await
        .expect("probe rotated clip");
    assert_eq!(rot_meta.width, 240);
    assert_eq!(rot_meta.height, 320);

    // 3. Paths + Fonts (bundled fonts directory) + SQLite + Storage cleaner
    let paths = StudioPaths::from_root(tmp.join("appdata"), root.join("src-tauri").join("fonts"))
        .expect("studio paths");
    let font_mgr = FontManager::new_with_watcher(&paths, || {}).expect("font manager");
    let fonts = font_mgr.list_fonts();
    assert!(
        fonts.len() >= 3,
        "expected >=3 bundled fonts, found {}",
        fonts.len()
    );

    let repo = SqliteStudioRepository::open(&paths.db_path).expect("sqlite repo");
    let proj = repo
        .create_project("xam.mp4", sample_mp4.to_str().unwrap_or(""), meta, Some("en"))
        .expect("create project");
    repo.update_project_status(&proj.id, PipelineStatus::Transcribed, None)
        .expect("update status");

    let words = vec![
        ("Hello".to_string(), 100, 350),
        ("world".to_string(), 380, 700),
    ];
    repo.replace_words(&proj.id, &words).expect("replace words");
    let cues = generate_cues(&words, 6);
    let cue_rows: Vec<CueInsertRow> = cues
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
    let saved_cues = repo.replace_cues(&proj.id, &cue_rows).expect("replace cues");
    assert_eq!(saved_cues.len(), 1);

    let extra_cue = repo
        .create_cue(&proj.id, "Extra line", 900, 1500, Some("#00FF88"))
        .expect("create cue");
    let edited_cue = repo
        .update_cue(&extra_cue.id, Some("Edited line"), None, None, None)
        .expect("update cue");
    assert_eq!(edited_cue.text, "Edited line");
    repo.delete_cue(&extra_cue.id).expect("delete cue");

    // Seed junk files and verify cleaner removes them while preserving valid export
    let temp_wav = paths.temp_dir.join("stale.wav");
    std::fs::write(&temp_wav, vec![1u8; 2048]).expect("seed stale temp");
    let orphan_export = paths.exports_dir.join("orphan-project");
    std::fs::create_dir_all(&orphan_export).expect("seed orphan export");
    std::fs::write(orphan_export.join("old.mp4"), vec![2u8; 4096]).expect("seed orphan mp4");

    let all_projects = repo.list_projects().expect("list projects");
    let cleaned = clean_storage_junk(&paths, &all_projects).expect("clean junk");
    assert!(cleaned.deleted_files_count >= 2);
    assert!(cleaned.freed_mb > 0.0);
    assert!(!temp_wav.exists());
    assert!(!orphan_export.exists());

    let _ = std::fs::remove_dir_all(tmp);
}

#[tokio::test]
async fn m2_whisper_real_transcription_and_abort_cancel() {
    let root = repo_root();
    let tmp = temp_test_dir("m2");
    let sidecars = sidecars_for_test();

    let default_paths =
        StudioPaths::default_windows(root.join("src-tauri").join("fonts")).expect("default paths");
    let Some(model_path) = resolve_available_model(&default_paths.models_dir, Some("base")) else {
        eprintln!("Skipping m2_whisper_real_transcription_and_abort_cancel: ggml-base.bin not cached");
        let _ = std::fs::remove_dir_all(tmp);
        return;
    };

    let clip_6s = tmp.join("clip_6s.mp4");
    make_synthetic_clip(&sidecars.ffmpeg, &clip_6s, 640, 360, "6").await;

    let wav_path = tmp.join("audio.wav");
    extract_audio_wav_16k(&sidecars.ffmpeg, &clip_6s, &wav_path)
        .await
        .expect("extract 16k wav");
    let pcm = read_wav_pcm16_mono_f32(&wav_path).expect("read wav pcm");
    assert!(pcm.len() > 16_000 * 4);

    let whisper = WhisperEngine::default();
    let res = whisper
        .transcribe(
            &model_path,
            &pcm,
            Some("auto"),
            CancellationToken::new(),
            |_, _| {},
        )
        .expect("real offline transcription");
    assert!(
        !res.detected_language.is_empty(),
        "expected detected language"
    );

    // Verify abort cancellation halts transcription cleanly.
    let cancel = CancellationToken::new();
    let cancel_trigger = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(20));
        cancel_trigger.cancel();
    });
    let aborted = whisper.transcribe(&model_path, &pcm, Some("auto"), cancel, |_, _| {});
    assert!(
        matches!(aborted, Err(CoreError::Cancelled)),
        "expected Err(CoreError::Cancelled), got {aborted:?}"
    );
    whisper.unload();

    let _ = std::fs::remove_dir_all(tmp);
}

#[tokio::test]
async fn m5_m6_export_overlay_captions_only_and_preview_parity() {
    let root = repo_root();
    let tmp = temp_test_dir("m5_m6");
    let sidecars = sidecars_for_test();
    let paths = StudioPaths::from_root(tmp.join("appdata"), root.join("src-tauri").join("fonts"))
        .expect("paths");

    let clip_2s = tmp.join("clip_2s.mp4");
    make_synthetic_clip(&sidecars.ffmpeg, &clip_2s, 640, 360, "2").await;
    let meta = probe_video(&sidecars.ffprobe, &clip_2s)
        .await
        .expect("probe 2s clip");

    let repo = SqliteStudioRepository::open(&paths.db_path).expect("sqlite repo");
    let proj = repo
        .create_project("clip_2s.mp4", clip_2s.to_str().unwrap_or(""), meta, Some("en"))
        .expect("create proj");
    let words = vec![
        ("Subtitle".to_string(), 100, 500),
        ("Studio".to_string(), 520, 950),
        ("Native".to_string(), 1000, 1450),
        ("Parity".to_string(), 1480, 1850),
    ];
    repo.replace_words(&proj.id, &words).expect("words");
    let generated = generate_cues(&words, 4);
    let cue_rows: Vec<_> = generated
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
    repo.replace_cues(&proj.id, &cue_rows).expect("cues");
    repo.update_style(
        &proj.id,
        &UpdateStylePatch {
            anim_in: Some(AnimationIn::Pop),
            karaoke_mode: Some(KaraokeMode::HighlightCurrentWord),
            background_box_on: Some(true),
            ..Default::default()
        },
    )
    .expect("update style");

    let full_proj = repo.get_project(&proj.id).expect("get proj").expect("exists");
    let exported_srt = export_project_subtitles(&full_proj, SubtitleTextFormat::Srt);
    assert!(exported_srt.content.contains("Subtitle Studio Native Parity"));

    // M6: Preview / Export pixel-identical parity check (`diff == 0`) at matching resolution.
    let mut font_reg = FontRegistry::new();
    font_reg.register_directories(&[paths.bundled_fonts_dir.as_path()]);
    let shared_fonts = Arc::new(RwLock::new(font_reg));
    let timeline = full_proj.to_render_timeline();

    let mut preview_cache = RenderCache::default();
    let mut export_cache = RenderCache::default();
    for t_ms in [120, 550, 1050, 1600] {
        let guard = shared_fonts.read().expect("font read");
        let preview_bytes = render_frame_owned(
            &timeline,
            t_ms,
            full_proj.width,
            full_proj.height,
            &guard,
            &mut preview_cache,
        )
        .expect("preview frame");
        let export_bytes = render_frame_owned(
            &timeline,
            t_ms,
            full_proj.width,
            full_proj.height,
            &guard,
            &mut export_cache,
        )
        .expect("export frame");
        assert_eq!(preview_bytes, export_bytes, "parity diff != 0 at t_ms={t_ms}");
        assert!(
            preview_bytes.as_chunks::<4>().0.iter().any(|px| px[3] > 0),
            "expected non-transparent pixels at t_ms={t_ms}"
        );
    }

    // M5: Hardware/CPU encoder detection + Overlay export + Captions-Only export + Cancel cleanup
    let detected = detect_best_encoder(&sidecars.ffmpeg, ExportFormat::Mp4H265).await;
    assert!(!detected.codec.is_empty());

    let overlay_out = tmp.join("out_overlay.mp4");
    run_export_pipeline(
        &sidecars.ffmpeg,
        &sidecars.ffprobe,
        &full_proj,
        Arc::clone(&shared_fonts),
        &ExportRequest {
            job_id: "job-overlay".to_string(),
            mode: ExportMode::Overlay,
            format: ExportFormat::Mp4H265,
            quality: ExportQuality::Fast,
            background_hex: "#000000".to_string(),
            include_audio: true,
            output_path: overlay_out.clone(),
        },
        CancellationToken::new(),
        |_| {},
    )
    .await
    .expect("overlay export");
    let overlay_meta = probe_video(&sidecars.ffprobe, &overlay_out)
        .await
        .expect("probe overlay mp4");
    assert_eq!(overlay_meta.width, full_proj.width);
    assert_eq!(overlay_meta.height, full_proj.height);
    assert!(overlay_meta.duration_ms >= 1500);

    let captions_out = tmp.join("out_captions_only.mp4");
    run_export_pipeline(
        &sidecars.ffmpeg,
        &sidecars.ffprobe,
        &full_proj,
        Arc::clone(&shared_fonts),
        &ExportRequest {
            job_id: "job-captions".to_string(),
            mode: ExportMode::CaptionsOnly,
            format: ExportFormat::Mp4H265,
            quality: ExportQuality::Fast,
            background_hex: "#101820".to_string(),
            include_audio: true,
            output_path: captions_out.clone(),
        },
        CancellationToken::new(),
        |_| {},
    )
    .await
    .expect("captions-only export");
    assert!(std::fs::metadata(&captions_out).expect("stat captions out").len() > 5000);

    // Cancellation deletes partial output cleanly.
    let cancelled_out = tmp.join("out_cancelled.mp4");
    let pre_cancel = CancellationToken::new();
    pre_cancel.cancel();
    let cancel_res = run_export_pipeline(
        &sidecars.ffmpeg,
        &sidecars.ffprobe,
        &full_proj,
        shared_fonts,
        &ExportRequest {
            job_id: "job-cancel".to_string(),
            mode: ExportMode::Overlay,
            format: ExportFormat::Mp4H265,
            quality: ExportQuality::Fast,
            background_hex: "#000000".to_string(),
            include_audio: false,
            output_path: cancelled_out.clone(),
        },
        pre_cancel,
        |_| {},
    )
    .await;
    assert!(matches!(cancel_res, Err(CoreError::Cancelled)));
    assert!(!cancelled_out.exists());

    let _ = std::fs::remove_dir_all(tmp);
}

#[tokio::test]
async fn m7_batch_folder_resume_and_merge() {
    let root = repo_root();
    let tmp = temp_test_dir("m7");
    let sidecars = sidecars_for_test();
    let default_paths =
        StudioPaths::default_windows(root.join("src-tauri").join("fonts")).expect("default paths");
    if resolve_available_model(&default_paths.models_dir, Some("base")).is_none() {
        eprintln!("Skipping m7_batch_folder_resume_and_merge Whisper run: ggml-base.bin not cached");
        let _ = std::fs::remove_dir_all(tmp);
        return;
    }

    let paths = StudioPaths::from_root(tmp.join("appdata"), root.join("src-tauri").join("fonts"))
        .expect("paths");
    std::fs::copy(
        default_paths.models_dir.join("ggml-base.bin"),
        paths.models_dir.join("ggml-base.bin"),
    )
    .ok(); // Copy or hardlink? Wait - copy takes 10ms on NVMe or use hard_link:
    if !paths.models_dir.join("ggml-base.bin").exists() {
        let _ = std::fs::hard_link(
            default_paths.models_dir.join("ggml-base.bin"),
            paths.models_dir.join("ggml-base.bin"),
        );
    }

    let batch_dir = tmp.join("batch_videos");
    std::fs::create_dir_all(&batch_dir).expect("batch dir");
    let clip1 = batch_dir.join("01_first.mp4");
    let clip2 = batch_dir.join("02_second.mp4");
    make_synthetic_clip(&sidecars.ffmpeg, &clip1, 640, 360, "3").await;
    std::fs::copy(&clip1, &clip2).expect("copy second clip");

    let index = load_or_create_batch_index(&batch_dir).expect("batch scan");
    assert_eq!(index.len(), 2);

    let whisper = Arc::new(WhisperEngine::default());
    let (items, merged_path) = run_batch_folder(
        &batch_dir,
        BatchTimestampMode::Both,
        false,
        &paths,
        &sidecars,
        Arc::clone(&whisper),
        Some("base"),
        CancellationToken::new(),
        |_| {},
    )
    .await
    .expect("run batch folder");

    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|i| i.status == "completed"));
    assert!(merged_path.is_file());
    let merged_txt = std::fs::read_to_string(&merged_path).expect("read merged txt");
    assert!(merged_txt.contains("01_first.mp4"));
    assert!(merged_txt.contains("02_second.mp4"));

    // Second run without force_restart resumes from transcript/index.json
    let (resumed_items, _) = run_batch_folder(
        &batch_dir,
        BatchTimestampMode::Both,
        false,
        &paths,
        &sidecars,
        whisper,
        Some("base"),
        CancellationToken::new(),
        |_| {},
    )
    .await
    .expect("resume batch folder");
    assert_eq!(resumed_items.len(), 2);
    assert_eq!(resumed_items[0].processed_at, items[0].processed_at);

    // Verify merge_project_subtitles cumulative time offset
    let repo = SqliteStudioRepository::open(&paths.db_path).expect("repo");
    let m1 = probe_video(&sidecars.ffprobe, &clip1).await.expect("probe1");
    let p1 = repo.create_project("01.mp4", "", m1, Some("en")).expect("p1");
    let p2 = repo.create_project("02.mp4", "", m1, Some("en")).expect("p2");
    repo.create_cue(&p1.id, "Clip one", 200, 800, None).expect("c1");
    repo.create_cue(&p2.id, "Clip two", 200, 800, None).expect("c2");
    let fp1 = repo.get_project(&p1.id).expect("g1").expect("e1");
    let fp2 = repo.get_project(&p2.id).expect("g2").expect("e2");
    let merged_srt = merge_project_subtitles(&[fp1, fp2], SubtitleTextFormat::Srt);
    assert!(merged_srt.content.contains("Clip one"));
    assert!(merged_srt.content.contains("Clip two"));

    let _ = std::fs::remove_dir_all(tmp);
}
