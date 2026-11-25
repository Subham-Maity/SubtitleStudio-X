//! Whisper GGML model catalog and streaming downloader (`%APPDATA%\SubtitleStudio\models`).

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::CoreError;

/// Metadata for a supported Whisper GGML model.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhisperModelInfo {
    pub id: String,
    pub label: String,
    pub file_name: String,
    pub download_url: String,
    pub approx_size_mb: u64,
    pub downloaded: bool,
    pub local_path: String,
    pub size_bytes: u64,
    pub vram_note: String,
}

/// Progress update emitted during model download.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDownloadProgress {
    pub model_id: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub pct: u8,
    pub done: bool,
}

struct CatalogEntry {
    id: &'static str,
    label: &'static str,
    file_name: &'static str,
    approx_size_mb: u64,
    vram_note: &'static str,
}

const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        id: "large-v3-turbo-q5_0",
        label: "Large v3 Turbo (Q5_0 Quantized — Recommended for 3–4 GB GPU)",
        file_name: "ggml-large-v3-turbo-q5_0.bin",
        approx_size_mb: 574,
        vram_note: "~900 MB VRAM",
    },
    CatalogEntry {
        id: "large-v3-turbo",
        label: "Large v3 Turbo (Full Precision)",
        file_name: "ggml-large-v3-turbo.bin",
        approx_size_mb: 1620,
        vram_note: "~2.1 GB VRAM",
    },
    CatalogEntry {
        id: "small",
        label: "Small (Multilingual)",
        file_name: "ggml-small.bin",
        approx_size_mb: 488,
        vram_note: "~750 MB VRAM",
    },
    CatalogEntry {
        id: "base",
        label: "Base (Fastest / Low RAM)",
        file_name: "ggml-base.bin",
        approx_size_mb: 148,
        vram_note: "~350 MB VRAM",
    },
];

/// List all catalog models and their local download status inside `models_dir`.
#[must_use]
pub fn list_models(models_dir: &Path) -> Vec<WhisperModelInfo> {
    CATALOG
        .iter()
        .map(|e| {
            let local = models_dir.join(e.file_name);
            let meta = std::fs::metadata(&local).ok();
            let downloaded = meta.as_ref().is_some_and(|m| m.is_file() && m.len() > 1024 * 1024);
            let size_bytes = meta.map(|m| m.len()).unwrap_or(0);
            WhisperModelInfo {
                id: e.id.to_string(),
                label: e.label.to_string(),
                file_name: e.file_name.to_string(),
                download_url: format!(
                    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
                    e.file_name
                ),
                approx_size_mb: e.approx_size_mb,
                downloaded,
                local_path: local.to_string_lossy().into_owned(),
                size_bytes,
                vram_note: e.vram_note.to_string(),
            }
        })
        .collect()
}

/// Find the best available local model path, preferring `preferred_id` or falling back to downloaded models.
#[must_use]
pub fn resolve_available_model(models_dir: &Path, preferred_id: Option<&str>) -> Option<PathBuf> {
    let models = list_models(models_dir);
    if let Some(pref) = preferred_id
        && let Some(found) = models.iter().find(|m| m.id == pref && m.downloaded)
    {
        return Some(PathBuf::from(&found.local_path));
    }
    models
        .into_iter()
        .find(|m| m.downloaded)
        .map(|m| PathBuf::from(m.local_path))
}

/// Download a catalog model via streaming HTTP to `<models_dir>/<file>.part` then atomically rename.
pub async fn download_model<F>(
    models_dir: &Path,
    model_id: &str,
    mut on_progress: F,
) -> Result<PathBuf, CoreError>
where
    F: FnMut(ModelDownloadProgress),
{
    let entry = CATALOG
        .iter()
        .find(|e| e.id == model_id)
        .ok_or_else(|| CoreError::InvalidInput(format!("unknown whisper model '{model_id}'")))?;

    std::fs::create_dir_all(models_dir)?;
    let target = models_dir.join(entry.file_name);
    if target.is_file() && std::fs::metadata(&target)?.len() > 1024 * 1024 {
        on_progress(ModelDownloadProgress {
            model_id: model_id.to_string(),
            downloaded_bytes: std::fs::metadata(&target)?.len(),
            total_bytes: None,
            pct: 100,
            done: true,
        });
        return Ok(target);
    }

    let part_path = models_dir.join(format!("{}.part", entry.file_name));
    let url = format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
        entry.file_name
    );

    let client = reqwest::Client::builder()
        .user_agent("SubtitleStudioX/0.1")
        .build()
        .map_err(|e| CoreError::Process(format!("http client init failed: {e}")))?;

    let mut resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| CoreError::Process(format!("model download failed: {e}")))?
        .error_for_status()
        .map_err(|e| CoreError::Process(format!("model download HTTP error: {e}")))?;

    let total_bytes = resp.content_length();
    let mut file = tokio::fs::File::create(&part_path).await?;
    let mut downloaded: u64 = 0;
    let mut last_pct: u8 = 255;

    use tokio::io::AsyncWriteExt;
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| CoreError::Process(format!("download stream error: {e}")))?
    {
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        let pct = total_bytes
            .filter(|t| *t > 0)
            .map(|t| ((downloaded.saturating_mul(100)) / t).min(99) as u8)
            .unwrap_or(0);
        if pct != last_pct {
            last_pct = pct;
            on_progress(ModelDownloadProgress {
                model_id: model_id.to_string(),
                downloaded_bytes: downloaded,
                total_bytes,
                pct,
                done: false,
            });
        }
    }
    file.flush().await?;
    drop(file);

    if target.exists() {
        let _ = std::fs::remove_file(&target);
    }
    std::fs::rename(&part_path, &target)?;

    on_progress(ModelDownloadProgress {
        model_id: model_id.to_string(),
        downloaded_bytes: downloaded,
        total_bytes,
        pct: 100,
        done: true,
    });
    Ok(target)
}

