//! Skia font loading and thread-safe font registry (`FontRegistry`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use skia_safe::textlayout::{FontCollection, TypefaceFontProvider};
use skia_safe::{Data, FontMgr};

use crate::error::RenderError;

static NEXT_REGISTRY_ID: AtomicU64 = AtomicU64::new(1);

/// Registered font entry mapping a file name to its resolved font family name.
#[derive(Debug, Clone)]
pub struct RegisteredFont {
    pub file_name: String,
    pub family_name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
struct FontBlob {
    meta: RegisteredFont,
    native_family: String,
    bytes: Arc<[u8]>,
}

/// Thread-safe font registry holding custom `.ttf`/`.otf`/`.ttc` bytes for deterministic Skia layout.
#[derive(Debug, Clone)]
pub struct FontRegistry {
    registry_id: u64,
    revision: u64,
    blobs: Vec<FontBlob>,
    by_file: HashMap<String, RegisteredFont>,
}

impl Default for FontRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl FontRegistry {
    /// Create an empty font registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            registry_id: NEXT_REGISTRY_ID.fetch_add(1, Ordering::Relaxed),
            revision: 0,
            blobs: Vec::new(),
            by_file: HashMap::new(),
        }
    }

    /// Register a font file (`.ttf`, `.otf`, `.ttc`) under its file name stem alias.
    pub fn register_file(&mut self, path: &Path) -> Result<RegisteredFont, RenderError> {
        let bytes = std::fs::read(path).map_err(|e| RenderError::FontLoad {
            path: path.display().to_string(),
            reason: e.to_string(),
        })?;
        let font_mgr = FontMgr::new();
        let data = Data::new_copy(&bytes);
        let typeface = font_mgr
            .new_from_data(data, None)
            .ok_or_else(|| RenderError::FontLoad {
                path: path.display().to_string(),
                reason: "Skia failed to decode font data".to_string(),
            })?;

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let alias = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&file_name)
            .to_string();
        let native_family = typeface.family_name();

        let entry = RegisteredFont {
            file_name: file_name.clone(),
            family_name: alias,
            path: path.to_path_buf(),
        };

        self.blobs.retain(|b| b.meta.file_name != file_name);
        self.blobs.push(FontBlob {
            meta: entry.clone(),
            native_family,
            bytes: Arc::from(bytes.into_boxed_slice()),
        });
        self.by_file.insert(file_name, entry.clone());
        self.revision = self.revision.saturating_add(1);
        Ok(entry)
    }

    /// Register all `.ttf`, `.otf`, and `.ttc` files found in the provided directories.
    pub fn register_directories(&mut self, dirs: &[&Path]) -> Vec<RegisteredFont> {
        let mut loaded = Vec::new();
        for dir in dirs {
            let Ok(entries) = std::fs::read_dir(dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let is_font = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "ttf" | "otf" | "ttc"))
                    .unwrap_or(false);
                if is_font && let Ok(reg) = self.register_file(&path) {
                    loaded.push(reg);
                }
            }
        }
        loaded
    }

    /// Build ordered font family fallback list for a `font_file_name`.
    #[must_use]
    pub fn resolve_families(&self, font_file_name: &str) -> Vec<String> {
        let mut families = Vec::new();
        if let Some(reg) = self.by_file.get(font_file_name) {
            families.push(reg.family_name.clone());
        } else if !font_file_name.trim().is_empty() {
            let stem = Path::new(font_file_name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(font_file_name);
            families.push(stem.to_string());
        }
        // Cross-platform multilingual fallbacks (Latin, Devanagari, Urdu/Arabic, Emoji).
        for fallback in [
            "Segoe UI",
            "Nirmala UI",
            "SF Pro Text",
            "Helvetica Neue",
            "Kohinoor Devanagari",
            "Noto Sans",
            "Noto Sans Devanagari",
            "Noto Sans Arabic",
            "Liberation Sans",
            "Arial",
            "Segoe UI Emoji",
            "Apple Color Emoji",
            "Noto Color Emoji",
        ] {
            if !families.iter().any(|f| f == fallback) {
                families.push(fallback.to_string());
            }
        }
        families
    }

    /// Build a thread-local Skia [`FontCollection`] combining custom registered fonts and system fallbacks.
    #[must_use]
    pub fn build_collection(&self) -> FontCollection {
        let font_mgr = FontMgr::new();
        let mut provider = TypefaceFontProvider::new();
        for blob in &self.blobs {
            let data = Data::new_copy(&blob.bytes);
            if let Some(tf) = font_mgr.new_from_data(data, None) {
                provider.register_typeface(tf.clone(), Some(blob.meta.family_name.as_str()));
                if blob.native_family != blob.meta.family_name {
                    provider.register_typeface(tf, Some(blob.native_family.as_str()));
                }
            }
        }
        let mut collection = FontCollection::new();
        collection.set_asset_font_manager(Some(provider.into()));
        collection.set_default_font_manager(Some(font_mgr), None);
        collection
    }

    /// Unique generation token identifying this registry instance and its loaded fonts.
    #[must_use]
    pub fn fingerprint(&self) -> u64 {
        (self.registry_id << 32) ^ self.revision
    }
}
