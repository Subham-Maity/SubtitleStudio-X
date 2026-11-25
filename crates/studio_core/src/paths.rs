//! Application directory layout under `%APPDATA%\SubtitleStudio`.

use std::path::{Path, PathBuf};

/// Resolved directories used by SubtitleStudio Native.
#[derive(Debug, Clone)]
pub struct StudioPaths {
    pub app_data_root: PathBuf,
    pub db_path: PathBuf,
    pub models_dir: PathBuf,
    pub user_fonts_dir: PathBuf,
    pub bundled_fonts_dir: PathBuf,
    pub presets_dir: PathBuf,
    pub storage_dir: PathBuf,
    pub uploads_dir: PathBuf,
    pub exports_dir: PathBuf,
    pub temp_dir: PathBuf,
}

impl StudioPaths {
    /// Resolve application paths using a base data root and bundled resource fonts directory.
    pub fn from_root(app_data_root: PathBuf, bundled_fonts_dir: PathBuf) -> std::io::Result<Self> {
        let storage_dir = app_data_root.join("storage");
        let paths = Self {
            db_path: app_data_root.join("studio.db"),
            models_dir: app_data_root.join("models"),
            user_fonts_dir: app_data_root.join("fonts"),
            bundled_fonts_dir,
            presets_dir: app_data_root.join("presets"),
            uploads_dir: storage_dir.join("uploads"),
            exports_dir: storage_dir.join("exports"),
            temp_dir: storage_dir.join("temp"),
            storage_dir,
            app_data_root,
        };
        paths.ensure_dirs()?;
        Ok(paths)
    }

    /// Resolve default paths under `%APPDATA%\SubtitleStudio`.
    pub fn default_windows(bundled_fonts_dir: PathBuf) -> std::io::Result<Self> {
        let base = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("SubtitleStudio");
        Self::from_root(base, bundled_fonts_dir)
    }

    /// Create all required runtime directories if they do not exist.
    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        for dir in [
            &self.app_data_root,
            &self.models_dir,
            &self.user_fonts_dir,
            &self.presets_dir,
            &self.storage_dir,
            &self.uploads_dir,
            &self.exports_dir,
            &self.temp_dir,
        ] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }

    /// Return the path for a project's upload working folder.
    #[must_use]
    pub fn project_upload_dir(&self, project_id: &str) -> PathBuf {
        self.uploads_dir.join(project_id)
    }

    /// Return the path for a project's export output folder.
    #[must_use]
    pub fn project_export_dir(&self, project_id: &str) -> PathBuf {
        self.exports_dir.join(project_id)
    }

    /// Check whether a path resides inside a given root directory.
    #[must_use]
    pub fn is_inside(root: &Path, candidate: &Path) -> bool {
        candidate.starts_with(root)
    }
}

