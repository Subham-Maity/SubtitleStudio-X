//! Font scanning, Skia `FontRegistry` synchronization, and hot-reload via `notify`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use subtitle_render::FontRegistry;

use crate::error::CoreError;
use crate::models::FontInfo;
use crate::paths::StudioPaths;

/// Thread-safe font manager synchronizing bundled and user fonts with Skia.
pub struct FontManager {
    bundled_dir: PathBuf,
    user_dir: PathBuf,
    registry: Arc<RwLock<FontRegistry>>,
    fonts: Arc<RwLock<Vec<FontInfo>>>,
    _watcher: Option<RecommendedWatcher>,
}

impl FontManager {
    /// Scan font directories and start a filesystem watcher on `user_fonts_dir`.
    pub fn new_with_watcher<F>(paths: &StudioPaths, on_reload: F) -> Result<Self, CoreError>
    where
        F: Fn() + Send + Sync + 'static,
    {
        let registry = Arc::new(RwLock::new(FontRegistry::new()));
        let fonts = Arc::new(RwLock::new(Vec::new()));

        let bundled_dir = paths.bundled_fonts_dir.clone();
        let user_dir = paths.user_fonts_dir.clone();

        reload_fonts(&bundled_dir, &user_dir, &registry, &fonts);

        let b_clone = bundled_dir.clone();
        let u_clone = user_dir.clone();
        let reg_clone = Arc::clone(&registry);
        let fonts_clone = Arc::clone(&fonts);

        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            if res.is_ok() {
                reload_fonts(&b_clone, &u_clone, &reg_clone, &fonts_clone);
                on_reload();
            }
        })
        .map_err(|e| CoreError::InvalidInput(format!("font watcher init failed: {e}")))?;

        if user_dir.is_dir() {
            let _ = watcher.watch(&user_dir, RecursiveMode::NonRecursive);
        }
        if bundled_dir.is_dir() {
            let _ = watcher.watch(&bundled_dir, RecursiveMode::NonRecursive);
        }

        Ok(Self {
            bundled_dir,
            user_dir,
            registry,
            fonts,
            _watcher: Some(watcher),
        })
    }

    /// Return current list of discovered fonts.
    #[must_use]
    pub fn list_fonts(&self) -> Vec<FontInfo> {
        self.fonts
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    /// Return shared [`FontRegistry`] handle for `subtitle_render::render_frame`.
    #[must_use]
    pub fn registry(&self) -> Arc<RwLock<FontRegistry>> {
        Arc::clone(&self.registry)
    }

    /// Force manual rescan.
    pub fn refresh(&self) {
        reload_fonts(&self.bundled_dir, &self.user_dir, &self.registry, &self.fonts);
    }
}

fn reload_fonts(
    bundled_dir: &Path,
    user_dir: &Path,
    registry: &Arc<RwLock<FontRegistry>>,
    fonts: &Arc<RwLock<Vec<FontInfo>>>,
) {
    let mut new_reg = FontRegistry::new();
    let registered = new_reg.register_directories(&[bundled_dir, user_dir]);
    let mut infos: Vec<FontInfo> = registered
        .into_iter()
        .map(|r| FontInfo {
            display_name: beautify_font_name(&r.file_name),
            file_name: r.file_name,
            path: r.path.to_string_lossy().into_owned(),
        })
        .collect();
    infos.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    infos.dedup_by(|a, b| a.file_name == b.file_name);

    if let Ok(mut reg_guard) = registry.write() {
        *reg_guard = new_reg;
    }
    if let Ok(mut f_guard) = fonts.write() {
        *f_guard = infos;
    }
}

fn beautify_font_name(file_name: &str) -> String {
    let stem = std::path::Path::new(file_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(file_name);
    stem.replace(['-', '_'], " ")
}

