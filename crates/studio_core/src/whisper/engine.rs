//! `whisper-rs` inference engine with Vulkan GPU + automatic CPU fallback and cancellation.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters,
};

use crate::error::CoreError;

/// Word-level transcription result in milliseconds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscribedWord {
    pub word: String,
    pub start_ms: i64,
    pub end_ms: i64,
}

/// Complete transcription output.
#[derive(Debug, Clone)]
pub struct TranscriptionResult {
    pub words: Vec<TranscribedWord>,
    pub text: String,
    pub detected_language: String,
    pub is_hindi: bool,
    pub task: String,
    pub device_label: String,
}

/// Resolved Whisper parameters matching `transcription-service/main.py::resolve_target_and_kwargs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhisperTaskConfig {
    pub language: Option<String>,
    pub initial_prompt: Option<String>,
    pub translate: bool,
}

/// Map UI language option (`en`, `hi-strict`, `hi`, `hi-roman`, `auto`, `ur`, `es`, `fr`) to Whisper params.
#[must_use]
pub fn resolve_language_target(language: Option<&str>) -> WhisperTaskConfig {
    let mut target_language: Option<String> = None;
    let mut target_prompt: Option<String> = None;
    let mut translate = false;

    if let Some(raw) = language {
        let clean = raw.trim().to_ascii_lowercase();
        if !clean.is_empty() && clean != "auto" {
            if matches!(
                clean.as_str(),
                "en" | "en-translate" | "translate-en" | "translate"
            ) {
                translate = true;
            } else if clean.contains("hi-roman") || clean.contains("hinglish") {
                target_language = Some("hi".to_string());
                target_prompt = Some(
                    "Namaste dosto, yeh audio Hinglish mein hai aur Roman script mein likha gaya hai."
                        .to_string(),
                );
            } else if clean.contains("hi-strict")
                || clean.contains("hi-devanagari")
                || clean == "hi"
            {
                target_language = Some("hi".to_string());
                target_prompt = Some("यह हिंदी भाषा में एक बातचीत है।".to_string());
            } else if let Some((prefix, _)) = clean.split_once('-') {
                target_language = Some(prefix.to_string());
            } else {
                target_language = Some(clean);
            }
        }
    }

    if translate {
        target_prompt = None;
    }

    WhisperTaskConfig {
        language: target_language,
        initial_prompt: target_prompt,
        translate,
    }
}

/// Extract 16 kHz mono 16-bit PCM WAV using FFmpeg sidecar.
pub async fn extract_audio_wav_16k(
    ffmpeg_path: &Path,
    video_path: &Path,
    out_wav_path: &Path,
) -> Result<(), CoreError> {
    if let Some(parent) = out_wav_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut cmd = Command::new(ffmpeg_path);
    cmd.arg("-y")
        .arg("-i")
        .arg(video_path)
        .args(["-vn", "-acodec", "pcm_s16le", "-ar", "16000", "-ac", "1"])
        .arg(out_wav_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);

    let out = cmd.output().await?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(CoreError::Process(format!(
            "FFmpeg 16kHz audio extraction failed: {stderr}"
        )));
    }
    Ok(())
}

/// Decode a 16-bit mono PCM WAV file into `f32` samples normalized to `[-1.0, 1.0]`.
pub fn read_wav_pcm16_mono_f32(wav_path: &Path) -> Result<Vec<f32>, CoreError> {
    let bytes = std::fs::read(wav_path)?;
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(CoreError::InvalidInput(format!(
            "invalid WAV header in {}",
            wav_path.display()
        )));
    }

    let mut pos = 12usize;
    while pos + 8 <= bytes.len() {
        let chunk_id = &bytes[pos..pos + 4];
        let chunk_size = u32::from_le_bytes([
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]) as usize;
        pos += 8;
        if chunk_id == b"data" {
            let end = pos.saturating_add(chunk_size).min(bytes.len());
            let data = &bytes[pos..end];
            let mut out = Vec::with_capacity(data.len() / 2);
            for &pair in data.as_chunks::<2>().0 {
                let sample = i16::from_le_bytes(pair);
                out.push(f32::from(sample) / 32768.0);
            }
            return Ok(out);
        }
        pos = pos.saturating_add(chunk_size);
        if chunk_size % 2 == 1 {
            pos = pos.saturating_add(1);
        }
    }

    Err(CoreError::InvalidInput(format!(
        "missing WAV data chunk in {}",
        wav_path.display()
    )))
}

/// Check whether a GPU compute backend (Vulkan on Windows/Linux, Metal on macOS) is usable.
///
/// On Windows, `/DELAYLOAD:vulkan-1.dll` in `src-tauri/build.rs` ensures GPU-less
/// machines fall back cleanly to CPU without startup loader errors.
#[must_use]
pub fn is_vulkan_runtime_available() -> bool {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn LoadLibraryW(lp_lib_file_name: *const u16) -> *mut core::ffi::c_void;
            fn FreeLibrary(h_lib_module: *mut core::ffi::c_void) -> i32;
        }
        let dll_wide: Vec<u16> = "vulkan-1.dll\0".encode_utf16().collect();
        // SAFETY: `dll_wide` is a valid null-terminated UTF-16 string; handle is immediately freed if non-null.
        let handle = unsafe { LoadLibraryW(dll_wide.as_ptr()) };
        if handle.is_null() {
            return false;
        }
        // SAFETY: `handle` was returned non-null by `LoadLibraryW`.
        unsafe {
            let _ = FreeLibrary(handle);
        }
        !whisper_rs::vulkan::list_devices().is_empty()
    }
    #[cfg(target_os = "linux")]
    {
        !whisper_rs::vulkan::list_devices().is_empty()
    }
    #[cfg(target_os = "macos")]
    {
        true
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        false
    }
}

struct LoadedModel {
    path: PathBuf,
    used_gpu: bool,
    device_label: String,
    ctx: Arc<WhisperContext>,
}

static WHISPER_GLOBAL_LOCK: Mutex<()> = Mutex::new(());

fn lock_global_whisper() -> std::sync::MutexGuard<'static, ()> {
    WHISPER_GLOBAL_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Thread-safe lazy loader & cache for [`WhisperContext`].
///
/// Supports explicit `unload()` to reclaim 3 GB GPU VRAM prior to NVENC export.
#[derive(Default)]
pub struct WhisperEngine {
    loaded: Mutex<Option<LoadedModel>>,
}

impl WhisperEngine {
    /// Unload the currently cached Whisper model to free GPU VRAM / RAM.
    pub fn unload(&self) {
        let _ggml = lock_global_whisper();
        if let Ok(mut guard) = self.loaded.lock() {
            *guard = None;
        }
    }

    fn get_or_load(&self, model_path: &Path, prefer_gpu: bool) -> Result<(Arc<WhisperContext>, String), CoreError> {
        let mut guard = self
            .loaded
            .lock()
            .map_err(|e| CoreError::Process(format!("whisper lock poisoned: {e}")))?;

        if let Some(ref existing) = *guard
            && existing.path == model_path
            && (!prefer_gpu || existing.used_gpu || !is_vulkan_runtime_available())
        {
            return Ok((Arc::clone(&existing.ctx), existing.device_label.clone()));
        }

        // Drop previous model first so VRAM is freed before allocating a new one.
        *guard = None;

        let path_str = model_path.to_str().ok_or_else(|| {
            CoreError::InvalidInput(format!("non-UTF8 model path: {}", model_path.display()))
        })?;

        if prefer_gpu && is_vulkan_runtime_available() {
            let mut params = WhisperContextParameters::default();
            params.use_gpu(true);
            match WhisperContext::new_with_params(path_str, params) {
                Ok(ctx) => {
                    #[cfg(not(target_os = "macos"))]
                    let dev_name = whisper_rs::vulkan::list_devices()
                        .into_iter()
                        .next()
                        .map(|d| format!("Vulkan GPU ({})", d.name))
                        .unwrap_or_else(|| "Vulkan GPU".to_string());
                    #[cfg(target_os = "macos")]
                    let dev_name = "Apple Metal GPU".to_string();
                    let arc = Arc::new(ctx);
                    *guard = Some(LoadedModel {
                        path: model_path.to_path_buf(),
                        used_gpu: true,
                        device_label: dev_name.clone(),
                        ctx: Arc::clone(&arc),
                    });
                    return Ok((arc, dev_name));
                }
                Err(err) => {
                    tracing::warn!(error = %err, "Vulkan Whisper context failed, falling back to CPU");
                }
            }
        }

        let mut cpu_params = WhisperContextParameters::default();
        cpu_params.use_gpu(false);
        let ctx = WhisperContext::new_with_params(path_str, cpu_params)
            .map_err(|e| CoreError::Process(format!("failed to load Whisper model on CPU: {e}")))?;
        let label = "CPU (Fallback)".to_string();
        let arc = Arc::new(ctx);
        *guard = Some(LoadedModel {
            path: model_path.to_path_buf(),
            used_gpu: false,
            device_label: label.clone(),
            ctx: Arc::clone(&arc),
        });
        Ok((arc, label))
    }

    /// Detect the dominant spoken language in the first audio window.
    pub fn detect_language(
        &self,
        model_path: &Path,
        pcm_f32: &[f32],
    ) -> Result<(String, f32), CoreError> {
        let _ggml = lock_global_whisper();
        let (ctx, _) = self.get_or_load(model_path, true)?;
        let mut state = ctx
            .create_state()
            .map_err(|e| CoreError::Process(format!("whisper state init failed: {e}")))?;

        let threads = std::thread::available_parallelism()
            .map(|n| n.get().clamp(1, 8))
            .unwrap_or(4);
        let slice_len = pcm_f32.len().min(16_000 * 30);
        state
            .pcm_to_mel(&pcm_f32[..slice_len], threads)
            .map_err(|e| CoreError::Process(format!("pcm_to_mel failed: {e}")))?;
        let (lang_id, probs) = state
            .lang_detect(0, threads)
            .map_err(|e| CoreError::Process(format!("lang_detect failed: {e}")))?;
        let lang = whisper_rs::get_lang_str(lang_id).unwrap_or("en").to_string();
        let prob = probs.get(lang_id as usize).copied().unwrap_or(0.0);
        Ok((lang, prob))
    }

    /// Run full transcription/translation with word-level timestamps, segment callbacks, and abort support.
    pub fn transcribe<F>(
        &self,
        model_path: &Path,
        pcm_f32: &[f32],
        language_mode: Option<&str>,
        cancel: CancellationToken,
        on_segment: F,
    ) -> Result<TranscriptionResult, CoreError>
    where
        F: Fn(u8, String) + Send + Sync + 'static,
    {
        if cancel.is_cancelled() {
            return Err(CoreError::Cancelled);
        }

        let _ggml = lock_global_whisper();
        if cancel.is_cancelled() {
            return Err(CoreError::Cancelled);
        }

        let (ctx, device_label) = self.get_or_load(model_path, true)?;
        let mut state = ctx
            .create_state()
            .map_err(|e| CoreError::Process(format!("whisper state init failed: {e}")))?;

        let cfg = resolve_language_target(language_mode);
        let mut params = FullParams::new(SamplingStrategy::BeamSearch {
            beam_size: 5,
            patience: -1.0,
        });
        let threads = std::thread::available_parallelism()
            .map(|n| n.get().clamp(1, 8))
            .unwrap_or(4);
        params.set_n_threads(threads as i32);
        params.set_no_context(true);
        params.set_token_timestamps(true);
        params.set_translate(cfg.translate);
        params.set_language(cfg.language.as_deref());
        if let Some(ref prompt) = cfg.initial_prompt {
            params.set_initial_prompt(prompt);
        }
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_print_special(false);

        let total_ms = ((pcm_f32.len() as f64 / 16_000.0) * 1000.0).max(1.0);
        let cb_arc = Arc::new(on_segment);
        let cb_clone = Arc::clone(&cb_arc);
        let seg_cb: Box<dyn FnMut(whisper_rs::SegmentCallbackData)> = Box::new(move |seg| {
            let end_ms = seg.end_timestamp * 10;
            let pct = ((end_ms as f64 / total_ms) * 100.0).clamp(1.0, 99.0) as u8;
            let start_s = seg.start_timestamp as f64 / 100.0;
            let end_s = seg.end_timestamp as f64 / 100.0;
            let msg = format!("[{start_s:.1}s -> {end_s:.1}s]: \"{}\"", seg.text.trim());
            cb_clone(pct, msg);
        });
        params.set_segment_callback_safe::<_, Box<dyn FnMut(whisper_rs::SegmentCallbackData)>>(
            Some(seg_cb),
        );

        let cancel_flag = Arc::new(AtomicBool::new(false));
        let flag_for_watcher = Arc::clone(&cancel_flag);
        let cancel_token_watcher = cancel.clone();
        let watcher_handle = std::thread::spawn(move || {
            while !flag_for_watcher.load(Ordering::Relaxed) {
                if cancel_token_watcher.is_cancelled() {
                    flag_for_watcher.store(true, Ordering::Relaxed);
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        });

        let abort_flag = Arc::clone(&cancel_flag);
        let abort_cb: Box<dyn FnMut() -> bool> =
            Box::new(move || abort_flag.load(Ordering::Relaxed));
        params.set_abort_callback_safe::<_, Box<dyn FnMut() -> bool>>(abort_cb);

        let run_res = state.full(params, pcm_f32);
        cancel_flag.store(true, Ordering::Relaxed);
        let _ = watcher_handle.join();

        if cancel.is_cancelled() {
            return Err(CoreError::Cancelled);
        }
        run_res.map_err(|e| CoreError::Process(format!("whisper transcription failed: {e}")))?;

        let detected_lang_id = state.full_lang_id_from_state();
        let detected_language = whisper_rs::get_lang_str(detected_lang_id)
            .unwrap_or("en")
            .to_string();
        let is_hindi = matches!(detected_language.as_str(), "hi" | "ur");

        let mut words = Vec::new();
        let mut text_parts = Vec::new();

        for seg in state.as_iter() {
            if let Ok(seg_txt) = seg.to_str_lossy() {
                let trimmed = seg_txt.trim();
                if !trimmed.is_empty() {
                    text_parts.push(trimmed.to_string());
                }
            }
            let seg_words = extract_words_from_segment(&seg);
            words.extend(seg_words);
        }

        Ok(TranscriptionResult {
            words,
            text: text_parts.join(" "),
            detected_language,
            is_hindi,
            task: if cfg.translate {
                "translate".to_string()
            } else {
                "transcribe".to_string()
            },
            device_label,
        })
    }
}

fn extract_words_from_segment(seg: &whisper_rs::WhisperSegment<'_>) -> Vec<TranscribedWord> {
    let n = seg.n_tokens();
    let mut words: Vec<TranscribedWord> = Vec::new();

    for i in 0..n {
        let Some(tok) = seg.get_token(i) else {
            continue;
        };
        let Ok(raw_cow) = tok.to_str_lossy() else {
            continue;
        };
        let raw = raw_cow.as_ref();
        // Skip Whisper control/special tokens like [_BEG_], <|endoftext|>, etc.
        if raw.is_empty() || (raw.starts_with('[') && raw.ends_with(']')) || (raw.starts_with("<|") && raw.ends_with("|>")) {
            continue;
        }
        let td = tok.token_data();
        let start_ms = (td.t0 * 10).max(seg.start_timestamp() * 10);
        let end_ms = (td.t1 * 10).max(start_ms + 20);

        let starts_new_word = raw.starts_with(' ') || words.is_empty();
        let clean = raw.trim();
        if clean.is_empty() {
            continue;
        }

        if starts_new_word {
            words.push(TranscribedWord {
                word: clean.to_string(),
                start_ms,
                end_ms,
            });
        } else if let Some(prev) = words.last_mut() {
            prev.word.push_str(clean);
            prev.end_ms = prev.end_ms.max(end_ms);
        }
    }

    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_all_language_modes_accurately() {
        let en = resolve_language_target(Some("en"));
        assert!(en.translate);
        assert_eq!(en.language, None);

        let hi_roman = resolve_language_target(Some("hi-roman"));
        assert!(!hi_roman.translate);
        assert_eq!(hi_roman.language.as_deref(), Some("hi"));
        assert!(hi_roman.initial_prompt.as_deref().unwrap_or_default().contains("Hinglish"));

        let hi_strict = resolve_language_target(Some("hi-strict"));
        assert_eq!(hi_strict.language.as_deref(), Some("hi"));
        assert_eq!(
            hi_strict.initial_prompt.as_deref(),
            Some("यह हिंदी भाषा में एक बातचीत है।")
        );

        let auto = resolve_language_target(Some("auto"));
        assert!(!auto.translate);
        assert_eq!(auto.language, None);
    }
}
