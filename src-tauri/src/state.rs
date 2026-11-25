//! Shared application state managed by Tauri.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use studio_core::StudioPaths;
use studio_core::db::SqliteStudioRepository;
use studio_core::fonts::FontManager;
use studio_core::media::SidecarBinaries;
use studio_core::queue::PipelineQueue;
use studio_core::whisper::WhisperEngine;
use subtitle_render::RenderCache;
use tauri::{Emitter, Manager};
use tokio_util::sync::CancellationToken;

/// Shared state container accessible in Tauri commands.
pub struct AppState {
    pub paths: StudioPaths,
    pub sidecars: SidecarBinaries,
    pub repo: Arc<SqliteStudioRepository>,
    pub whisper: Arc<WhisperEngine>,
    pub queue: Arc<PipelineQueue<SqliteStudioRepository>>,
    pub fonts: Arc<FontManager>,
    pub export_cancels: Mutex<HashMap<String, CancellationToken>>,
    pub batch_cancel: Mutex<Option<CancellationToken>>,
    pub preview_cache: Mutex<RenderCache>,
}

impl AppState {
    /// Initialize application directories, SQLite DB, font watcher, and transcription queue.
    pub fn initialize(app: &tauri::AppHandle) -> Result<Self, Box<dyn std::error::Error>> {
        let resource_fonts = app
            .path()
            .resource_dir()
            .map(|d| d.join("fonts"))
            .ok()
            .filter(|d| d.is_dir())
            .unwrap_or_else(|| {
                let dev_local = PathBuf::from("fonts");
                if dev_local.is_dir() {
                    dev_local
                } else {
                    PathBuf::from("src-tauri").join("fonts")
                }
            });

        let paths = StudioPaths::default_windows(resource_fonts)?;
        let sidecars = SidecarBinaries::discover();
        let repo = Arc::new(SqliteStudioRepository::open(&paths.db_path)?);
        let whisper = Arc::new(WhisperEngine::default());
        let queue = Arc::new(PipelineQueue::new(
            Arc::clone(&repo),
            paths.clone(),
            sidecars.clone(),
            Arc::clone(&whisper),
        ));
        if let Err(err) = queue.stop_and_clear_all() {
            tracing::warn!(error = %err, "failed to reset interrupted pipeline jobs on startup");
        }

        let app_handle = app.clone();
        let fonts = Arc::new(FontManager::new_with_watcher(&paths, move || {
            let _ = app_handle.emit("fonts://updated", ());
        })?);

        Ok(Self {
            paths,
            sidecars,
            repo,
            whisper,
            queue,
            fonts,
            export_cancels: Mutex::new(HashMap::new()),
            batch_cancel: Mutex::new(None),
            preview_cache: Mutex::new(RenderCache::default()),
        })
    }
}
