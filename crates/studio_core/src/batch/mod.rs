//! Batch folder video transcription & English translation engine (`batch_transcribe_translate.js` parity).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::cues::generate_cues;
use crate::error::CoreError;
use crate::media::SidecarBinaries;
use crate::paths::StudioPaths;
use crate::whisper::{
    TranscriptionResult, WhisperEngine, extract_audio_wav_16k, read_wav_pcm16_mono_f32,
    resolve_available_model,
};

/// Output timestamp mode for batch `.txt` exports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BatchTimestampMode {
    With,
    Without,
    Both,
}

/// Persistent resume entry stored in `transcript/index.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchIndexItem {
    pub id: usize,
    pub filename: String,
    pub file_path: String,
    pub size_bytes: u64,
    pub status: String,
    pub detected_language: Option<String>,
    pub is_hindi: bool,
    pub translated_to_english: bool,
    pub transcript_txt_path: Option<String>,
    pub cue_count: usize,
    pub word_count: usize,
    pub error: Option<String>,
    pub processed_at: Option<String>,
}

/// Progress update emitted for a batch folder run.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchProgressEvent {
    pub current_index: usize,
    pub total: usize,
    pub filename: String,
    pub status: String,
    pub pct: u8,
    pub log_message: String,
    pub merged_txt_path: Option<String>,
}

const VIDEO_EXTS: &[&str] = &["mp4", "mov", "mkv", "avi", "webm", "flv", "wmv", "m4v"];

/// Format milliseconds as `HH:MM:SS.mmm` matching `batch_transcribe_translate.js`.
#[must_use]
pub fn format_batch_ms(ms: i64) -> String {
    let clamped = ms.max(0);
    let total_sec = clamped / 1000;
    let h = total_sec / 3600;
    let m = (total_sec % 3600) / 60;
    let s = total_sec % 60;
    let millis = clamped % 1000;
    format!("{h:02}:{m:02}:{s:02}.{millis:03}")
}

/// Scan `folder` for videos and merge with existing `transcript/index.json` resume state.
pub fn load_or_create_batch_index(folder: &Path) -> Result<Vec<BatchIndexItem>, CoreError> {
    let transcript_dir = folder.join("transcript");
    let main_dir = transcript_dir.join("main");
    std::fs::create_dir_all(&transcript_dir)?;
    std::fs::create_dir_all(&main_dir)?;

    let index_path = transcript_dir.join("index.json");
    let existing: Vec<BatchIndexItem> = if index_path.is_file() {
        std::fs::read_to_string(&index_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let existing_map: HashMap<String, BatchIndexItem> = existing
        .into_iter()
        .map(|item| (item.filename.clone(), item))
        .collect();

    let mut video_files: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(folder)? {
        let entry = entry?;
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let is_video = p
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| VIDEO_EXTS.contains(&ext.to_ascii_lowercase().as_str()));
        if is_video {
            video_files.push(p);
        }
    }
    video_files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

    let updated: Vec<BatchIndexItem> = video_files
        .into_iter()
        .enumerate()
        .map(|(idx, p)| {
            let filename = p
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            let size_bytes = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            if let Some(mut prev) = existing_map.get(&filename).cloned() {
                // Auto-reset items that completed with 0 words.
                if prev.status == "completed" && prev.word_count == 0 {
                    prev.status = "pending".to_string();
                }
                prev.id = idx + 1;
                prev.file_path = p.to_string_lossy().into_owned();
                prev.size_bytes = size_bytes;
                prev
            } else {
                BatchIndexItem {
                    id: idx + 1,
                    filename,
                    file_path: p.to_string_lossy().into_owned(),
                    size_bytes,
                    status: "pending".to_string(),
                    detected_language: None,
                    is_hindi: false,
                    translated_to_english: false,
                    transcript_txt_path: None,
                    cue_count: 0,
                    word_count: 0,
                    error: None,
                    processed_at: None,
                }
            }
        })
        .collect();

    save_batch_index(folder, &updated)?;
    Ok(updated)
}

fn save_batch_index(folder: &Path, list: &[BatchIndexItem]) -> Result<(), CoreError> {
    let json = serde_json::to_string_pretty(list)?;
    std::fs::write(folder.join("transcript").join("index.json"), &json)?;
    std::fs::write(folder.join("index.json"), json)?;
    Ok(())
}

/// Execute batch folder transcription/translation to English with resume support.
#[allow(clippy::too_many_arguments)]
pub async fn run_batch_folder<F>(
    folder: &Path,
    timestamp_mode: BatchTimestampMode,
    force_restart: bool,
    paths: &StudioPaths,
    sidecars: &SidecarBinaries,
    whisper: Arc<WhisperEngine>,
    preferred_model: Option<&str>,
    cancel: CancellationToken,
    on_progress: F,
) -> Result<(Vec<BatchIndexItem>, PathBuf), CoreError>
where
    F: Fn(BatchProgressEvent) + Send + Sync + 'static,
{
    let model_path = resolve_available_model(&paths.models_dir, preferred_model).ok_or_else(|| {
        CoreError::NotFound("No Whisper model downloaded for batch translation".to_string())
    })?;

    let mut index_list = load_or_create_batch_index(folder)?;
    let transcript_dir = folder.join("transcript");
    let merged_path = transcript_dir
        .join("main")
        .join("merged_all_translations_en.txt");

    let pending_indices: Vec<usize> = index_list
        .iter()
        .enumerate()
        .filter(|(_, item)| force_restart || item.status != "completed")
        .map(|(i, _)| i)
        .collect();

    let total = pending_indices.len();
    for (pos, list_idx) in pending_indices.into_iter().enumerate() {
        if cancel.is_cancelled() {
            return Err(CoreError::Cancelled);
        }

        let filename = index_list[list_idx].filename.clone();
        let video_path = PathBuf::from(&index_list[list_idx].file_path);
        index_list[list_idx].status = "in-progress".to_string();
        save_batch_index(folder, &index_list)?;

        on_progress(BatchProgressEvent {
            current_index: pos + 1,
            total,
            filename: filename.clone(),
            status: "in-progress".to_string(),
            pct: 10,
            log_message: format!("Extracting audio for {filename}..."),
            merged_txt_path: None,
        });

        let temp_wav = paths
            .temp_dir
            .join(format!("batch_{}_{}.wav", list_idx, Utc::now().timestamp_millis()));

        let res: Result<(), CoreError> = async {
            extract_audio_wav_16k(&sidecars.ffmpeg, &video_path, &temp_wav).await?;
            let pcm = Arc::new(
                tokio::task::spawn_blocking({
                    let wp = temp_wav.clone();
                    move || read_wav_pcm16_mono_f32(&wp)
                })
                .await
                .map_err(|e| CoreError::Process(e.to_string()))??,
            );

            let (detected_lang, conf) = tokio::task::spawn_blocking({
                let w = Arc::clone(&whisper);
                let mp = model_path.clone();
                let p = Arc::clone(&pcm);
                move || w.detect_language(&mp, &p)
            })
            .await
            .map_err(|e| CoreError::Process(e.to_string()))??;
            let is_hindi = matches!(detected_lang.as_str(), "hi" | "ur");
            let effective_mode = if detected_lang == "en" {
                "en-same"
            } else {
                "en"
            };

            let trans = tokio::task::spawn_blocking({
                let w = Arc::clone(&whisper);
                let mp = model_path.clone();
                let p = Arc::clone(&pcm);
                let cancel_cb = cancel.clone();
                move || w.transcribe(&mp, &p, Some(effective_mode), cancel_cb, |_, _| {})
            })
            .await
            .map_err(|e| CoreError::Process(e.to_string()))??;

            let raw_words: Vec<(String, i64, i64)> = trans
                .words
                .iter()
                .map(|w| (w.word.clone(), w.start_ms, w.end_ms))
                .collect();
            let cues = generate_cues(&raw_words, 7);

            let stem = Path::new(&filename)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&filename);
            let txt_path = transcript_dir.join(format!("{stem}.txt"));
            let txt = build_txt_content(
                &filename,
                &trans,
                &cues,
                &detected_lang,
                is_hindi,
                (conf * 100.0).round() as u32,
                timestamp_mode,
            );
            std::fs::write(&txt_path, txt)?;

            index_list[list_idx].status = "completed".to_string();
            index_list[list_idx].detected_language = Some(detected_lang);
            index_list[list_idx].is_hindi = is_hindi;
            index_list[list_idx].translated_to_english = true;
            index_list[list_idx].transcript_txt_path = Some(txt_path.to_string_lossy().into_owned());
            index_list[list_idx].cue_count = cues.len();
            index_list[list_idx].word_count = trans.words.len();
            index_list[list_idx].error = None;
            index_list[list_idx].processed_at = Some(Utc::now().to_rfc3339());
            save_batch_index(folder, &index_list)?;
            write_merged_master(folder, &index_list, timestamp_mode, &merged_path)?;
            Ok(())
        }
        .await;

        let _ = std::fs::remove_file(&temp_wav);

        match res {
            Ok(()) => {
                on_progress(BatchProgressEvent {
                    current_index: pos + 1,
                    total,
                    filename,
                    status: "completed".to_string(),
                    pct: 100,
                    log_message: "Completed and merged.".to_string(),
                    merged_txt_path: Some(merged_path.to_string_lossy().into_owned()),
                });
            }
            Err(CoreError::Cancelled) => return Err(CoreError::Cancelled),
            Err(err) => {
                let msg = err.to_string();
                index_list[list_idx].status = "failed".to_string();
                index_list[list_idx].error = Some(msg.clone());
                save_batch_index(folder, &index_list)?;
                on_progress(BatchProgressEvent {
                    current_index: pos + 1,
                    total,
                    filename,
                    status: "failed".to_string(),
                    pct: 0,
                    log_message: msg,
                    merged_txt_path: None,
                });
            }
        }
    }

    write_merged_master(folder, &index_list, timestamp_mode, &merged_path)?;
    Ok((index_list, merged_path))
}

fn build_txt_content(
    filename: &str,
    trans: &TranscriptionResult,
    cues: &[crate::cues::GeneratedCue],
    detected_lang: &str,
    is_hindi: bool,
    confidence_pct: u32,
    mode: BatchTimestampMode,
) -> String {
    let mut out = String::new();
    let sep = "================================================================================\n";
    out.push_str(sep);
    out.push_str(&format!("FILE: {filename}\n"));
    out.push_str(&format!(
        "ORIGINAL LANGUAGE: {} ({}) [Confidence: {confidence_pct}%]\n",
        detected_lang.to_uppercase(),
        if is_hindi { "Hindi" } else { detected_lang }
    ));
    out.push_str(&format!(
        "TASK: {}\n",
        if trans.task == "translate" {
            format!("Translated from {detected_lang} to English")
        } else {
            "Direct English Transcription".to_string()
        }
    ));
    out.push_str(&format!("TOTAL WORDS: {}\n", trans.words.len()));
    if mode != BatchTimestampMode::Without {
        out.push_str(&format!("TOTAL CUES: {}\n", cues.len()));
    }
    let fmt_label = match mode {
        BatchTimestampMode::Without => "Without Timestamps (Clean Text)",
        BatchTimestampMode::With => "With Timestamps (Timed Cues)",
        BatchTimestampMode::Both => "Both (Timed Cues + Full Text)",
    };
    out.push_str(&format!("FORMAT: {fmt_label}\n"));
    out.push_str(&format!("PROCESSED AT: {}\n", Utc::now().to_rfc3339()));
    out.push_str(sep);
    out.push('\n');

    let full_text = if !trans.text.trim().is_empty() {
        trans.text.trim().to_string()
    } else {
        cues.iter()
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    };
    let display = if full_text.is_empty() {
        "[No spoken dialogue detected — background music only or silence]".to_string()
    } else {
        full_text
    };

    match mode {
        BatchTimestampMode::Without => {
            out.push_str("--- ENGLISH TRANSLATION (CLEAN TEXT) ---\n");
            out.push_str(&display);
            out.push('\n');
        }
        BatchTimestampMode::With => {
            out.push_str("--- SUBTITLE CUES (TIMED) ---\n");
            if cues.is_empty() {
                out.push_str(&display);
                out.push('\n');
            } else {
                for c in cues {
                    out.push_str(&format!(
                        "[{} --> {}] {}\n",
                        format_batch_ms(c.start_ms),
                        format_batch_ms(c.end_ms),
                        c.text
                    ));
                }
            }
            out.push('\n');
        }
        BatchTimestampMode::Both => {
            out.push_str("--- SUBTITLE CUES (TIMED) ---\n");
            if cues.is_empty() {
                out.push_str(&display);
                out.push('\n');
            } else {
                for c in cues {
                    out.push_str(&format!(
                        "[{} --> {}] {}\n",
                        format_batch_ms(c.start_ms),
                        format_batch_ms(c.end_ms),
                        c.text
                    ));
                }
            }
            out.push_str("\n--- FULL ENGLISH TRANSCRIPT ---\n");
            out.push_str(&display);
            out.push('\n');
        }
    }
    out
}

fn write_merged_master(
    folder: &Path,
    index_list: &[BatchIndexItem],
    mode: BatchTimestampMode,
    merged_path: &Path,
) -> Result<(), CoreError> {
    let completed: Vec<&BatchIndexItem> = index_list
        .iter()
        .filter(|i| i.status == "completed" && i.transcript_txt_path.is_some())
        .collect();

    let mut out = String::new();
    let sep = "================================================================================\n";
    out.push_str(sep);
    out.push_str("SUBTITLESTUDIO X — MASTER ENGLISH TRANSLATIONS REPOSITORY\n");
    out.push_str(&format!("Source Directory: {}\n", folder.display()));
    out.push_str(&format!("Total Videos Discovered: {}\n", index_list.len()));
    out.push_str(&format!("Total Completed: {}\n", completed.len()));
    out.push_str(&format!("Format Mode: {mode:?}\n"));
    out.push_str(&format!("Last Updated: {}\n", Utc::now().to_rfc3339()));
    out.push_str(sep);
    out.push_str("\nTABLE OF CONTENTS / INDEX:\n");
    for (idx, item) in completed.iter().enumerate() {
        out.push_str(&format!(
            "  {}. [{}] - Original: {} (Hindi: {}) -> English | Words: {}\n",
            idx + 1,
            item.filename,
            item.detected_language.as_deref().unwrap_or("unknown"),
            if item.is_hindi { "YES" } else { "NO" },
            item.word_count
        ));
    }
    out.push_str(&format!("\n{sep}\n"));

    for (idx, item) in completed.iter().enumerate() {
        if let Some(ref p) = item.transcript_txt_path
            && let Ok(text) = std::fs::read_to_string(p)
        {
            out.push_str("\n################################################################################\n");
            out.push_str(&format!(
                "### VIDEO [{}/{}]: {}\n",
                idx + 1,
                completed.len(),
                item.filename
            ));
            out.push_str("################################################################################\n\n");
            out.push_str(&text);
            out.push_str("\n\n");
        }
    }

    std::fs::write(merged_path, out)?;
    Ok(())
}
