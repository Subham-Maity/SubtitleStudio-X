//! Tokio Semaphore(1) transcription queue with per-project and global `CancellationToken`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::Serialize;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::constants::DEFAULT_WORDS_PER_CUE;
use crate::cues::generate_cues;
use crate::db::{CueInsertRow, StudioRepository};
use crate::error::CoreError;
use crate::media::SidecarBinaries;
use crate::models::PipelineStatus;
use crate::paths::StudioPaths;
use crate::whisper::{
    WhisperEngine, extract_audio_wav_16k, read_wav_pcm16_mono_f32, resolve_available_model,
};

/// Real-time pipeline progress event sent to the UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineProgressEvent {
    pub project_id: String,
    pub event_type: String,
    pub stage: String,
    pub pct: u8,
    pub elapsed_ms: u64,
    pub language: Option<String>,
    pub device: Option<String>,
    pub word_count: Option<usize>,
    pub log_message: Option<String>,
    pub message: Option<String>,
}

/// Manages background transcription tasks with concurrency=1 and cancellation.
pub struct PipelineQueue<R: StudioRepository + 'static> {
    repo: Arc<R>,
    paths: StudioPaths,
    sidecars: SidecarBinaries,
    whisper: Arc<WhisperEngine>,
    semaphore: Arc<Semaphore>,
    active_tokens: Arc<Mutex<HashMap<String, CancellationToken>>>,
    preferred_model: Arc<Mutex<Option<String>>>,
}

impl<R: StudioRepository + 'static> PipelineQueue<R> {
    #[must_use]
    pub fn new(
        repo: Arc<R>,
        paths: StudioPaths,
        sidecars: SidecarBinaries,
        whisper: Arc<WhisperEngine>,
    ) -> Self {
        Self {
            repo,
            paths,
            sidecars,
            whisper,
            semaphore: Arc::new(Semaphore::new(1)),
            active_tokens: Arc::new(Mutex::new(HashMap::new())),
            preferred_model: Arc::new(Mutex::new(Some("large-v3-turbo-q5_0".to_string()))),
        }
    }

    pub fn set_preferred_model(&self, model_id: Option<String>) {
        if let Ok(mut guard) = self.preferred_model.lock() {
            *guard = model_id;
        }
    }

    /// Enqueue a transcription job for `project_id` and stream [`PipelineProgressEvent`] updates.
    pub fn enqueue<E>(
        &self,
        project_id: String,
        language: Option<String>,
        emit: E,
    ) -> tokio::task::JoinHandle<()>
    where
        E: Fn(PipelineProgressEvent) + Send + Sync + 'static,
    {
        // Cancel any previous in-flight run for the same project.
        self.cancel_project(&project_id);

        let token = CancellationToken::new();
        if let Ok(mut guard) = self.active_tokens.lock() {
            guard.insert(project_id.clone(), token.clone());
        }

        let repo = Arc::clone(&self.repo);
        let paths = self.paths.clone();
        let sidecars = self.sidecars.clone();
        let whisper = Arc::clone(&self.whisper);
        let semaphore = Arc::clone(&self.semaphore);
        let tokens = Arc::clone(&self.active_tokens);
        let pref_model = self
            .preferred_model
            .lock()
            .ok()
            .and_then(|g| g.clone());
        let emit_arc = Arc::new(emit);

        tokio::spawn(async move {
            let started = Instant::now();
            let _permit = match semaphore.acquire_owned().await {
                Ok(p) => p,
                Err(_) => return,
            };

            let res = run_pipeline_inner(
                repo.as_ref(),
                &paths,
                &sidecars,
                Arc::clone(&whisper),
                &project_id,
                language,
                pref_model.as_deref(),
                token.clone(),
                started,
                Arc::clone(&emit_arc),
            )
            .await;

            if let Ok(mut guard) = tokens.lock() {
                guard.remove(&project_id);
            }

            if let Err(err) = res {
                match err {
                    CoreError::Cancelled => {
                        tracing::info!(project_id, "transcription pipeline cancelled");
                    }
                    other => {
                        let msg = other.to_string();
                        tracing::error!(project_id, error = %msg, "transcription pipeline failed");
                        let _ = repo.update_project_status(
                            &project_id,
                            PipelineStatus::Failed,
                            Some(&msg),
                        );
                        emit_arc(PipelineProgressEvent {
                            project_id,
                            event_type: "error".to_string(),
                            stage: "FAILED".to_string(),
                            pct: 0,
                            elapsed_ms: started.elapsed().as_millis() as u64,
                            language: None,
                            device: None,
                            word_count: None,
                            log_message: Some(msg.clone()),
                            message: Some(msg),
                        });
                    }
                }
            }
        })
    }

    /// Cancel an active or queued project pipeline.
    pub fn cancel_project(&self, project_id: &str) {
        if let Ok(mut guard) = self.active_tokens.lock()
            && let Some(tok) = guard.remove(project_id)
        {
            tok.cancel();
        }
    }

    /// Stop all active and queued transcription tasks and reset DB statuses.
    pub fn stop_and_clear_all(&self) -> Result<usize, CoreError> {
        if let Ok(mut guard) = self.active_tokens.lock() {
            for (_, tok) in guard.drain() {
                tok.cancel();
            }
        }
        let projects = self.repo.list_projects()?;
        let mut reset_count = 0usize;
        for proj in projects {
            if matches!(
                proj.status,
                PipelineStatus::ExtractingAudio | PipelineStatus::Transcribing
            ) {
                let target_status = if proj.cue_count > 0 {
                    PipelineStatus::Transcribed
                } else {
                    PipelineStatus::Uploaded
                };
                self.repo
                    .update_project_status(&proj.id, target_status, None)?;
                reset_count += 1;
            }
        }
        Ok(reset_count)
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_pipeline_inner<R: StudioRepository, E>(
    repo: &R,
    paths: &StudioPaths,
    sidecars: &SidecarBinaries,
    whisper: Arc<WhisperEngine>,
    project_id: &str,
    language: Option<String>,
    preferred_model: Option<&str>,
    cancel: CancellationToken,
    started: Instant,
    emit: Arc<E>,
) -> Result<(), CoreError>
where
    E: Fn(PipelineProgressEvent) + Send + Sync + 'static,
{
    if cancel.is_cancelled() {
        return Err(CoreError::Cancelled);
    }

    let project = repo
        .get_project(project_id)?
        .ok_or_else(|| CoreError::NotFound(format!("video project {project_id}")))?;

    let effective_lang = language.or(project.language.clone());
    if effective_lang.is_some() {
        repo.update_project_language(project_id, effective_lang.as_deref())?;
    }

    emit(PipelineProgressEvent {
        project_id: project_id.to_string(),
        event_type: "log".to_string(),
        stage: "UPLOADED".to_string(),
        pct: 5,
        elapsed_ms: started.elapsed().as_millis() as u64,
        language: effective_lang.clone(),
        device: None,
        word_count: None,
        log_message: Some(format!("[Start] Initiating pipeline for {}", project.original_filename)),
        message: None,
    });

    // Stage 1: Audio extraction
    repo.update_project_status(project_id, PipelineStatus::ExtractingAudio, None)?;
    if cancel.is_cancelled() {
        return Err(CoreError::Cancelled);
    }

    emit(PipelineProgressEvent {
        project_id: project_id.to_string(),
        event_type: "stage".to_string(),
        stage: "EXTRACTING_AUDIO".to_string(),
        pct: 15,
        elapsed_ms: started.elapsed().as_millis() as u64,
        language: effective_lang.clone(),
        device: None,
        word_count: None,
        log_message: Some("Extracting 16kHz mono WAV audio track using FFmpeg...".to_string()),
        message: None,
    });

    let wav_path = paths.project_upload_dir(project_id).join("audio.wav");
    extract_audio_wav_16k(&sidecars.ffmpeg, PathBuf::from(&project.storage_path).as_path(), &wav_path)
        .await?;

    if cancel.is_cancelled() {
        return Err(CoreError::Cancelled);
    }

    // Stage 2: Transcription via whisper-rs
    let model_path = resolve_available_model(&paths.models_dir, preferred_model).ok_or_else(|| {
        CoreError::NotFound(format!(
            "No Whisper GGML model found in {}. Download a model first.",
            paths.models_dir.display()
        ))
    })?;

    repo.update_project_status(project_id, PipelineStatus::Transcribing, None)?;
    emit(PipelineProgressEvent {
        project_id: project_id.to_string(),
        event_type: "stage".to_string(),
        stage: "TRANSCRIBING".to_string(),
        pct: 35,
        elapsed_ms: started.elapsed().as_millis() as u64,
        language: effective_lang.clone(),
        device: None,
        word_count: None,
        log_message: Some(format!(
            "Running Whisper ({}) [Language: {}]...",
            model_path.file_name().and_then(|s| s.to_str()).unwrap_or("model"),
            effective_lang.as_deref().unwrap_or("auto")
        )),
        message: None,
    });

    let pcm = tokio::task::spawn_blocking({
        let wp = wav_path.clone();
        move || read_wav_pcm16_mono_f32(&wp)
    })
    .await
    .map_err(|e| CoreError::Process(format!("wav decode join error: {e}")))??;

    let pid_for_cb = project_id.to_string();
    let lang_for_cb = effective_lang.clone();
    let emit_for_cb = Arc::clone(&emit);

    let trans_result = tokio::task::spawn_blocking({
        let w = Arc::clone(&whisper);
        let mp = model_path.clone();
        let lang = effective_lang.clone();
        let cancel_cb = cancel.clone();
        let cancel_for_seg = cancel.clone();
        move || {
            w.transcribe(&mp, &pcm, lang.as_deref(), cancel_cb, move |seg_pct, msg| {
                if cancel_for_seg.is_cancelled() {
                    return;
                }
                let scaled = 35 + ((u16::from(seg_pct) * 50) / 100) as u8;
                emit_for_cb(PipelineProgressEvent {
                    project_id: pid_for_cb.clone(),
                    event_type: "progress".to_string(),
                    stage: "TRANSCRIBING".to_string(),
                    pct: scaled.min(88),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    language: lang_for_cb.clone(),
                    device: None,
                    word_count: None,
                    log_message: Some(msg),
                    message: None,
                });
            })
        }
    })
    .await
    .map_err(|e| CoreError::Process(format!("whisper join error: {e}")))??;

    if cancel.is_cancelled() {
        return Err(CoreError::Cancelled);
    }

    // Persist words & cues.
    let raw_words: Vec<(String, i64, i64)> = trans_result
        .words
        .iter()
        .map(|w| (w.word.clone(), w.start_ms, w.end_ms))
        .collect();
    let saved_words = repo.replace_words(project_id, &raw_words)?;

    let words_per_cue = project
        .style
        .as_ref()
        .map(|s| s.words_per_cue as usize)
        .unwrap_or(DEFAULT_WORDS_PER_CUE);
    let generated = generate_cues(&raw_words, words_per_cue);
    let cue_rows: Vec<CueInsertRow> = generated
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
    repo.replace_cues(project_id, &cue_rows)?;
    repo.update_project_status(project_id, PipelineStatus::Transcribed, None)?;

    // Remove temporary WAV after transcription.
    let _ = std::fs::remove_file(&wav_path);

    emit(PipelineProgressEvent {
        project_id: project_id.to_string(),
        event_type: "done".to_string(),
        stage: "TRANSCRIBED".to_string(),
        pct: 100,
        elapsed_ms: started.elapsed().as_millis() as u64,
        language: Some(trans_result.detected_language),
        device: Some(trans_result.device_label),
        word_count: Some(saved_words.len()),
        log_message: Some(format!(
            "Pipeline completed! Saved {} words.",
            saved_words.len()
        )),
        message: None,
    });

    Ok(())
}
