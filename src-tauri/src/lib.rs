//! Tauri 2 desktop composition root for SubtitleStudio X.

#![warn(clippy::all)]

pub mod commands;
pub mod error;
pub mod state;

use state::AppState;
use tracing_subscriber::EnvFilter;

/// Bootstraps logging, application state, plugins, and IPC commands.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_state = AppState::initialize(app.handle())?;
            tauri::Manager::manage(app, app_state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::health_check,
            commands::list_projects,
            commands::get_project,
            commands::import_video,
            commands::delete_project,
            commands::retry_pipeline,
            commands::retranscribe_pipeline,
            commands::reset_pipeline,
            commands::stop_and_clear_queue,
            commands::clean_storage_junk,
            commands::create_cue,
            commands::update_cue,
            commands::delete_cue,
            commands::regenerate_cues,
            commands::update_style,
            commands::export_subtitles_text,
            commands::save_text_file,
            commands::merge_batch_subtitles,
            commands::list_fonts,
            commands::list_style_presets,
            commands::save_style_preset,
            commands::render_preview_frame,
            commands::list_whisper_models,
            commands::download_whisper_model,
            commands::set_preferred_model,
            commands::unload_whisper_model,
            commands::detect_export_encoder,
            commands::start_export,
            commands::cancel_export,
            commands::reveal_in_explorer,
            commands::scan_batch_folder,
            commands::start_batch_folder,
            commands::cancel_batch_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running SubtitleStudio X");
}
