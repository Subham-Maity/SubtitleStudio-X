//! Storage junk cleaner ported 1:1 from `videos.controller.ts` (`cleanStorageJunk`).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::error::CoreError;
use crate::models::{CleanJunkResult, VideoProject};
use crate::paths::StudioPaths;

/// Remove temporary WAV extracts, orphan upload/export folders, `.ass` files, and older renders.
pub fn clean_storage_junk(
    paths: &StudioPaths,
    active_projects: &[VideoProject],
) -> Result<CleanJunkResult, CoreError> {
    let active_ids: HashSet<&str> = active_projects.iter().map(|p| p.id.as_str()).collect();
    let active_storage_paths: HashSet<PathBuf> = active_projects
        .iter()
        .filter_map(|p| std::fs::canonicalize(&p.storage_path).ok().or_else(|| Some(PathBuf::from(&p.storage_path))))
        .collect();

    let mut freed_bytes: u64 = 0;
    let mut deleted_count: usize = 0;

    // 1. Purge temp directory completely.
    if paths.temp_dir.is_dir()
        && let Ok(entries) = std::fs::read_dir(&paths.temp_dir)
    {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.file_name().is_some_and(|n| n == ".gitkeep") {
                continue;
            }
            if p.is_file() {
                delete_file_tracked(&p, &mut freed_bytes, &mut deleted_count);
            } else if p.is_dir() {
                delete_dir_tracked(&p, &mut freed_bytes, &mut deleted_count);
            }
        }
    }

    // 2. Clean export directories (remove orphan project folders & old renders/ass files).
    if paths.exports_dir.is_dir()
        && let Ok(entries) = std::fs::read_dir(&paths.exports_dir)
    {
        for entry in entries.flatten() {
            let folder = entry.path();
            if !folder.is_dir() {
                continue;
            }
            let Some(dir_name) = folder.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !active_ids.contains(dir_name) {
                delete_dir_tracked(&folder, &mut freed_bytes, &mut deleted_count);
            } else {
                clean_active_export_folder(&folder, &mut freed_bytes, &mut deleted_count);
            }
        }
    }

    // 3. Clean temporary audio extracts and orphan folders inside uploads/.
    if paths.uploads_dir.is_dir()
        && let Ok(entries) = std::fs::read_dir(&paths.uploads_dir)
    {
        for entry in entries.flatten() {
            let folder = entry.path();
            if !folder.is_dir() {
                continue;
            }
            let mut has_original = false;
            if let Ok(files) = std::fs::read_dir(&folder) {
                for f_entry in files.flatten() {
                    let f = f_entry.path();
                    let name = f.file_name().and_then(|n| n.to_str()).unwrap_or_default();
                    if name == ".gitkeep" {
                        continue;
                    }
                    if name.starts_with("audio") || name.ends_with(".wav") {
                        delete_file_tracked(&f, &mut freed_bytes, &mut deleted_count);
                    } else {
                        let resolved = std::fs::canonicalize(&f).unwrap_or_else(|_| f.clone());
                        if active_storage_paths.contains(&resolved) {
                            has_original = true;
                        }
                    }
                }
            }
            if !has_original {
                delete_dir_tracked(&folder, &mut freed_bytes, &mut deleted_count);
            }
        }
    }

    let freed_mb = ((freed_bytes as f64 / (1024.0 * 1024.0)) * 100.0).round() / 100.0;
    Ok(CleanJunkResult {
        freed_mb,
        deleted_files_count: deleted_count,
        message: format!(
            "Cleaned {freed_mb:.2} MB of storage junk ({deleted_count} temporary files removed)"
        ),
    })
}

fn clean_active_export_folder(folder: &Path, freed_bytes: &mut u64, deleted_count: &mut usize) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    let mut newest: Option<(PathBuf, SystemTime)> = None;
    let mut render_files = Vec::new();

    for entry in entries.flatten() {
        let p = entry.path();
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        if name == ".gitkeep" {
            continue;
        }
        if name.ends_with(".ass") || name.ends_with(".part") {
            delete_file_tracked(&p, freed_bytes, deleted_count);
        } else if p.is_file() && (name.ends_with(".mp4") || name.ends_with(".mov")) {
            let mtime = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            if newest.as_ref().is_none_or(|(_, t)| mtime > *t) {
                newest = Some((p.clone(), mtime));
            }
            render_files.push(p);
        }
    }

    let newest_path = newest.map(|(p, _)| p);
    for p in render_files {
        if newest_path.as_ref() != Some(&p) {
            delete_file_tracked(&p, freed_bytes, deleted_count);
        }
    }
}

fn delete_file_tracked(path: &Path, freed_bytes: &mut u64, deleted_count: &mut usize) {
    if let Ok(meta) = std::fs::metadata(path)
        && meta.is_file()
        && std::fs::remove_file(path).is_ok()
    {
        *freed_bytes += meta.len();
        *deleted_count += 1;
    }
}

fn delete_dir_tracked(dir: &Path, freed_bytes: &mut u64, deleted_count: &mut usize) {
    let size = dir_size(dir);
    if std::fs::remove_dir_all(dir).is_ok() {
        *freed_bytes += size;
        *deleted_count += 1;
    }
}

fn dir_size(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut total = 0;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            total += dir_size(&p);
        } else if let Ok(m) = entry.metadata() {
            total += m.len();
        }
    }
    total
}

