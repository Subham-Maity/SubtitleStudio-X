//! Tauri IPC command handlers (thin transport layer delegating to `studio_core` and `subtitle_render`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use studio_core::batch::{
    BatchIndexItem, BatchTimestampMode, load_or_create_batch_index, run_batch_folder,
};
use studio_core::cues::{export_project_subtitles, generate_cues, merge_project_subtitles};
use studio_core::db::{CueInsertRow, StudioRepository};
use studio_core::export::{
    DetectedEncoder, ExportRequest, detect_best_encoder, run_export_pipeline,
};
use studio_core::media::{clean_storage_junk as core_clean_storage_junk, probe_video};
use studio_core::models::{
    CleanJunkResult, ExportFormat, ExportJob, ExportMode, ExportQuality, ExportedSubtitles,
    FontInfo, PipelineStatus, SubtitleCue, SubtitleStyle, SubtitleTextFormat, UpdateStylePatch,
    VideoProject,
};
use studio_core::whisper::{
    WhisperModelInfo, download_model, is_vulkan_runtime_available, list_models,
};
use subtitle_render::{
    StylePreset, builtin_presets, load_preset_json, render_frame, save_preset_json,
};
use tauri::window::{ProgressBarState, ProgressBarStatus};
use tauri::{AppHandle, Emitter, Manager, State, ipc::Response};
use tokio_util::sync::CancellationToken;

use crate::error::CommandError;
use crate::state::AppState;

/// Health and environment diagnostics returned to the frontend.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthStatus {
    pub app_data_root: String,
    pub models_dir: String,
    pub user_fonts_dir: String,
    pub bundled_fonts_dir: String,
    pub presets_dir: String,
    pub ffmpeg_path: String,
    pub ffprobe_path: String,
    pub vulkan_available: bool,
    pub version: String,
}

#[tauri::command]
pub async fn health_check(state: State<'_, AppState>) -> Result<HealthStatus, CommandError> {
    Ok(HealthStatus {
        app_data_root: state.paths.app_data_root.to_string_lossy().into_owned(),
        models_dir: state.paths.models_dir.to_string_lossy().into_owned(),
        user_fonts_dir: state.paths.user_fonts_dir.to_string_lossy().into_owned(),
        bundled_fonts_dir: state.paths.bundled_fonts_dir.to_string_lossy().into_owned(),
        presets_dir: state.paths.presets_dir.to_string_lossy().into_owned(),
        ffmpeg_path: state.sidecars.ffmpeg.to_string_lossy().into_owned(),
        ffprobe_path: state.sidecars.ffprobe.to_string_lossy().into_owned(),
        vulkan_available: is_vulkan_runtime_available(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

#[tauri::command]
pub async fn list_projects(state: State<'_, AppState>) -> Result<Vec<VideoProject>, CommandError> {
    state.repo.list_projects().map_err(CommandError::from)
}

#[tauri::command]
pub async fn get_project(
    state: State<'_, AppState>,
    id: String,
) -> Result<VideoProject, CommandError> {
    state
        .repo
        .get_project(&id)?
        .ok_or_else(|| CommandError::NotFound(format!("project {id}")))
}

#[tauri::command]
pub async fn import_video(
    app: AppHandle,
    state: State<'_, AppState>,
    file_path: String,
    language: Option<String>,
    auto_transcribe: Option<bool>,
) -> Result<VideoProject, CommandError> {
    let src = PathBuf::from(&file_path);
    if !src.is_file() {
        return Err(CommandError::InvalidInput(format!(
            "video file does not exist: {file_path}"
        )));
    }
    let filename = src
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("video.mp4")
        .to_string();

    let metadata = probe_video(&state.sidecars.ffprobe, &src).await?;
    let project = state.repo.create_project(
        &filename,
        &src.to_string_lossy(),
        metadata,
        language.as_deref(),
    )?;

    if auto_transcribe.unwrap_or(true) {
        let app_clone = app.clone();
        state.queue.enqueue(project.id.clone(), language, move |ev| {
            let _ = app_clone.emit("pipeline://progress", &ev);
        });
    }

    Ok(project)
}

#[tauri::command]
pub async fn delete_project(state: State<'_, AppState>, id: String) -> Result<(), CommandError> {
    state.queue.cancel_project(&id);
    let upload_dir = state.paths.project_upload_dir(&id);
    if upload_dir.is_dir() {
        let _ = std::fs::remove_dir_all(&upload_dir);
    }
    let export_dir = state.paths.project_export_dir(&id);
    if export_dir.is_dir() {
        let _ = std::fs::remove_dir_all(&export_dir);
    }
    state.repo.delete_project(&id).map_err(CommandError::from)
}

#[tauri::command]
pub async fn retry_pipeline(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), CommandError> {
    let proj = state
        .repo
        .get_project(&id)?
        .ok_or_else(|| CommandError::NotFound(format!("project {id}")))?;
    let app_clone = app.clone();
    state
        .queue
        .enqueue(proj.id, proj.language, move |ev| {
            let _ = app_clone.emit("pipeline://progress", &ev);
        });
    Ok(())
}

#[tauri::command]
pub async fn retranscribe_pipeline(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    language: Option<String>,
) -> Result<(), CommandError> {
    if state.repo.get_project(&id)?.is_none() {
        return Err(CommandError::NotFound(format!("project {id}")));
    }
    let app_clone = app.clone();
    state.queue.enqueue(id, language, move |ev| {
        let _ = app_clone.emit("pipeline://progress", &ev);
    });
    Ok(())
}

#[tauri::command]
pub async fn reset_pipeline(
    state: State<'_, AppState>,
    id: String,
) -> Result<PipelineStatus, CommandError> {
    state.queue.cancel_project(&id);
    let proj = state
        .repo
        .get_project(&id)?
        .ok_or_else(|| CommandError::NotFound(format!("project {id}")))?;
    let next_status = if proj.cues.is_empty() {
        PipelineStatus::Uploaded
    } else {
        PipelineStatus::Transcribed
    };
    state.repo.update_project_status(&id, next_status, None)?;
    Ok(next_status)
}

#[tauri::command]
pub async fn stop_and_clear_queue(state: State<'_, AppState>) -> Result<usize, CommandError> {
    state
        .queue
        .stop_and_clear_all()
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn clean_storage_junk(
    state: State<'_, AppState>,
) -> Result<CleanJunkResult, CommandError> {
    let projects = state.repo.list_projects()?;
    core_clean_storage_junk(&state.paths, &projects).map_err(CommandError::from)
}

#[tauri::command]
pub async fn create_cue(
    state: State<'_, AppState>,
    project_id: String,
    text: String,
    start_ms: i64,
    end_ms: i64,
    color_hex: Option<String>,
) -> Result<SubtitleCue, CommandError> {
    state
        .repo
        .create_cue(&project_id, &text, start_ms, end_ms, color_hex.as_deref())
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn update_cue(
    state: State<'_, AppState>,
    cue_id: String,
    text: Option<String>,
    start_ms: Option<i64>,
    end_ms: Option<i64>,
    color_hex: Option<Option<String>>,
) -> Result<SubtitleCue, CommandError> {
    let color_ref = color_hex.as_ref().map(|opt| opt.as_deref());
    state
        .repo
        .update_cue(&cue_id, text.as_deref(), start_ms, end_ms, color_ref)
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn delete_cue(state: State<'_, AppState>, cue_id: String) -> Result<(), CommandError> {
    state.repo.delete_cue(&cue_id).map_err(CommandError::from)
}

#[tauri::command]
pub async fn regenerate_cues(
    state: State<'_, AppState>,
    project_id: String,
    words_per_cue: usize,
) -> Result<Vec<SubtitleCue>, CommandError> {
    let proj = state
        .repo
        .get_project(&project_id)?
        .ok_or_else(|| CommandError::NotFound(format!("project {project_id}")))?;
    let words: Vec<(String, i64, i64)> = proj
        .words
        .iter()
        .map(|w| (w.word.clone(), w.start_ms, w.end_ms))
        .collect();
    let generated = generate_cues(&words, words_per_cue.max(1));
    let rows: Vec<CueInsertRow> = generated
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
    state.repo.update_style(
        &project_id,
        &UpdateStylePatch {
            words_per_cue: Some(words_per_cue as i32),
            ..Default::default()
        },
    )?;
    state
        .repo
        .replace_cues(&project_id, &rows)
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn update_style(
    state: State<'_, AppState>,
    project_id: String,
    patch: UpdateStylePatch,
) -> Result<SubtitleStyle, CommandError> {
    let updated = state.repo.update_style(&project_id, &patch)?;
    if let Some(wpc) = patch.words_per_cue {
        let proj = state.repo.get_project(&project_id)?;
        if let Some(p) = proj
            && !p.words.is_empty()
        {
            let words: Vec<(String, i64, i64)> = p
                .words
                .iter()
                .map(|w| (w.word.clone(), w.start_ms, w.end_ms))
                .collect();
            let generated = generate_cues(&words, wpc.max(1) as usize);
            let rows: Vec<CueInsertRow> = generated
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
            let _ = state.repo.replace_cues(&project_id, &rows);
        }
    }
    Ok(updated)
}

#[tauri::command]
pub async fn export_subtitles_text(
    state: State<'_, AppState>,
    project_id: String,
    format: SubtitleTextFormat,
) -> Result<ExportedSubtitles, CommandError> {
    let proj = state
        .repo
        .get_project(&project_id)?
        .ok_or_else(|| CommandError::NotFound(format!("project {project_id}")))?;
    Ok(export_project_subtitles(&proj, format))
}

#[tauri::command]
pub async fn save_text_file(dest_path: String, content: String) -> Result<(), CommandError> {
    std::fs::write(&dest_path, content).map_err(|e| CommandError::Core(e.to_string()))
}

#[tauri::command]
pub async fn merge_batch_subtitles(
    state: State<'_, AppState>,
    project_ids: Vec<String>,
    format: SubtitleTextFormat,
) -> Result<ExportedSubtitles, CommandError> {
    let mut projects = Vec::new();
    for id in project_ids {
        if let Some(proj) = state.repo.get_project(&id)? {
            projects.push(proj);
        }
    }
    if projects.is_empty() {
        return Err(CommandError::NotFound(
            "no valid projects found to merge".to_string(),
        ));
    }
    Ok(merge_project_subtitles(&projects, format))
}

#[tauri::command]
pub async fn list_fonts(state: State<'_, AppState>) -> Result<Vec<FontInfo>, CommandError> {
    state.fonts.refresh();
    Ok(state.fonts.list_fonts())
}

#[tauri::command]
pub async fn list_style_presets(
    state: State<'_, AppState>,
) -> Result<Vec<StylePreset>, CommandError> {
    let mut presets = builtin_presets();
    if let Ok(entries) = std::fs::read_dir(&state.paths.presets_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().is_some_and(|e| e == "json")
                && let Ok(preset) = load_preset_json(&p)
            {
                presets.push(preset);
            }
        }
    }
    Ok(presets)
}

#[tauri::command]
pub async fn save_style_preset(
    state: State<'_, AppState>,
    preset: StylePreset,
) -> Result<StylePreset, CommandError> {
    let safe_id: String = preset
        .id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    let filename = if safe_id.is_empty() {
        format!("preset_{}.json", uuid::Uuid::new_v4().simple())
    } else {
        format!("{safe_id}.json")
    };
    let path = state.paths.presets_dir.join(filename);
    let mut saved = preset;
    saved.builtin = false;
    save_preset_json(&path, &saved)?;
    Ok(saved)
}

/// Render a single RGBA8888 unpremultiplied preview frame using the exact export renderer.
#[tauri::command]
pub async fn render_preview_frame(
    state: State<'_, AppState>,
    project_id: String,
    t_ms: i64,
    out_w: u32,
    out_h: u32,
) -> Result<Response, CommandError> {
    let proj = state
        .repo
        .get_project(&project_id)?
        .ok_or_else(|| CommandError::NotFound(format!("project {project_id}")))?;
    let timeline = proj.to_render_timeline();
    let reg_lock = state.fonts.registry();
    let reg = reg_lock
        .read()
        .map_err(|e| CommandError::Core(format!("font lock error: {e}")))?;
    let mut cache = state
        .preview_cache
        .lock()
        .map_err(|e| CommandError::Core(format!("preview cache lock error: {e}")))?;

    let bytes = render_frame(&timeline, t_ms, out_w, out_h, &reg, &mut cache)?.to_vec();
    Ok(Response::new(bytes))
}

#[tauri::command]
pub async fn list_whisper_models(
    state: State<'_, AppState>,
) -> Result<Vec<WhisperModelInfo>, CommandError> {
    Ok(list_models(&state.paths.models_dir))
}

#[tauri::command]
pub async fn download_whisper_model(
    app: AppHandle,
    state: State<'_, AppState>,
    model_id: String,
) -> Result<String, CommandError> {
    let app_clone = app.clone();
    let path = download_model(&state.paths.models_dir, &model_id, move |prog| {
        let _ = app_clone.emit("model://progress", &prog);
    })
    .await?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn set_preferred_model(
    state: State<'_, AppState>,
    model_id: Option<String>,
) -> Result<(), CommandError> {
    state.queue.set_preferred_model(model_id);
    Ok(())
}

#[tauri::command]
pub async fn unload_whisper_model(state: State<'_, AppState>) -> Result<(), CommandError> {
    state.whisper.unload();
    Ok(())
}

#[tauri::command]
pub async fn detect_export_encoder(
    state: State<'_, AppState>,
    format: ExportFormat,
) -> Result<DetectedEncoder, CommandError> {
    Ok(detect_best_encoder(&state.sidecars.ffmpeg, format).await)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartExportPayload {
    pub project_id: String,
    pub mode: ExportMode,
    pub format: ExportFormat,
    pub quality: ExportQuality,
    pub background_hex: Option<String>,
    pub include_audio: bool,
    pub dest_path: Option<String>,
}

#[tauri::command]
pub async fn start_export(
    app: AppHandle,
    state: State<'_, AppState>,
    payload: StartExportPayload,
) -> Result<ExportJob, CommandError> {
    let proj = state
        .repo
        .get_project(&payload.project_id)?
        .ok_or_else(|| CommandError::NotFound(format!("project {}", payload.project_id)))?;

    // Unload Whisper from GPU VRAM so 3 GB cards (e.g. GTX 1060) have headroom for NVENC.
    state.whisper.unload();

    let bg = payload.background_hex.as_deref().unwrap_or("#000000");
    let job = state.repo.create_export_job(
        &proj.id,
        payload.mode,
        payload.format,
        payload.quality,
        Some(bg),
        payload.include_audio,
    )?;

    let ext = match payload.format {
        ExportFormat::Mov => "mov",
        ExportFormat::Mp4H265 => "mp4",
    };
    let output_path = match payload.dest_path {
        Some(p) if !p.trim().is_empty() => PathBuf::from(p),
        _ => {
            let stem = Path::new(&proj.original_filename)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("video");
            state
                .paths
                .project_export_dir(&proj.id)
                .join(format!("{stem}-{}.{ext}", &job.id[..8.min(job.id.len())]))
        }
    };

    let cancel = CancellationToken::new();
    if let Ok(mut guard) = state.export_cancels.lock() {
        guard.insert(job.id.clone(), cancel.clone());
    }

    let req = ExportRequest {
        job_id: job.id.clone(),
        mode: payload.mode,
        format: payload.format,
        quality: payload.quality,
        background_hex: bg.to_string(),
        include_audio: payload.include_audio,
        output_path: output_path.clone(),
    };

    let repo = std::sync::Arc::clone(&state.repo);
    let ffmpeg = state.sidecars.ffmpeg.clone();
    let ffprobe = state.sidecars.ffprobe.clone();
    let font_reg = state.fonts.registry();
    let job_ret = job.clone();
    let app_clone = app.clone();

    tokio::spawn(async move {
        let _ = repo.update_export_progress(
            &req.job_id,
            2,
            Some(studio_core::models::ExportStatus::Rendering),
            None,
        );
        let app_ev = app_clone.clone();
        let repo_ev = std::sync::Arc::clone(&repo);
        let res = run_export_pipeline(
            &ffmpeg,
            &ffprobe,
            &proj,
            font_reg,
            &req,
            cancel,
            move |ev| {
                let _ = repo_ev.update_export_progress(
                    &ev.job_id,
                    i32::from(ev.progress_pct),
                    None,
                    Some(&ev.encoder),
                );
                if let Some(win) = app_ev.get_webview_window("main") {
                    let _ = win.set_progress_bar(ProgressBarState {
                        status: Some(ProgressBarStatus::Normal),
                        progress: Some(u64::from(ev.progress_pct)),
                    });
                }
                let _ = app_ev.emit("export://progress", &ev);
            },
        )
        .await;

        if let Some(win) = app_clone.get_webview_window("main") {
            let _ = win.set_progress_bar(ProgressBarState {
                status: Some(ProgressBarStatus::None),
                progress: None,
            });
        }
        if let Some(st) = app_clone.try_state::<AppState>()
            && let Ok(mut guard) = st.export_cancels.lock()
        {
            guard.remove(&req.job_id);
        }

        match res {
            Ok(enc) => {
                let out_str = output_path.to_string_lossy().into_owned();
                let _ = repo.update_export_progress(
                    &req.job_id,
                    100,
                    Some(studio_core::models::ExportStatus::Done),
                    Some(&enc.codec),
                );
                let _ = repo.complete_export_job(&req.job_id, &out_str);
            }
            Err(err) => {
                let msg = err.to_string();
                let _ = repo.fail_export_job(&req.job_id, &msg);
                let _ = app_clone.emit(
                    "export://progress",
                    studio_core::export::ExportProgressEvent {
                        job_id: req.job_id,
                        project_id: proj.id,
                        event_type: "error".to_string(),
                        progress_pct: 0,
                        encoder: String::new(),
                        is_hardware: false,
                        fallback_notice: None,
                        output_path: None,
                        message: Some(msg),
                    },
                );
            }
        }
    });

    Ok(job_ret)
}

#[tauri::command]
pub async fn cancel_export(state: State<'_, AppState>, job_id: String) -> Result<(), CommandError> {
    if let Ok(mut guard) = state.export_cancels.lock()
        && let Some(tok) = guard.remove(&job_id)
    {
        tok.cancel();
    }
    Ok(())
}

#[tauri::command]
pub async fn reveal_in_explorer(path: String) -> Result<(), CommandError> {
    tauri_plugin_opener::reveal_item_in_dir(&path)
        .map_err(|e| CommandError::Core(e.to_string()))
}

#[tauri::command]
pub async fn scan_batch_folder(folder_path: String) -> Result<Vec<BatchIndexItem>, CommandError> {
    let path = PathBuf::from(&folder_path);
    if !path.is_dir() {
        return Err(CommandError::InvalidInput(format!(
            "folder does not exist: {folder_path}"
        )));
    }
    load_or_create_batch_index(&path).map_err(CommandError::from)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchFolderResult {
    pub items: Vec<BatchIndexItem>,
    pub merged_txt_path: String,
}

#[tauri::command]
pub async fn start_batch_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    folder_path: String,
    timestamp_mode: BatchTimestampMode,
    force_restart: bool,
) -> Result<BatchFolderResult, CommandError> {
    let folder = PathBuf::from(&folder_path);
    if !folder.is_dir() {
        return Err(CommandError::InvalidInput(format!(
            "folder does not exist: {folder_path}"
        )));
    }
    let cancel = CancellationToken::new();
    if let Ok(mut guard) = state.batch_cancel.lock() {
        if let Some(prev) = guard.take() {
            prev.cancel();
        }
        *guard = Some(cancel.clone());
    }

    let app_clone = app.clone();
    let (items, merged_path) = run_batch_folder(
        &folder,
        timestamp_mode,
        force_restart,
        &state.paths,
        &state.sidecars,
        std::sync::Arc::clone(&state.whisper),
        Some("large-v3-turbo-q5_0"),
        cancel,
        move |ev| {
            let _ = app_clone.emit("batch://progress", &ev);
        },
    )
    .await?;

    Ok(BatchFolderResult {
        items,
        merged_txt_path: merged_path.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub async fn cancel_batch_folder(state: State<'_, AppState>) -> Result<(), CommandError> {
    if let Ok(mut guard) = state.batch_cancel.lock()
        && let Some(tok) = guard.take()
    {
        tok.cancel();
    }
    Ok(())
}
