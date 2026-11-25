//! Whisper AI transcription and model management.

pub mod engine;
pub mod models;

pub use engine::{
    TranscribedWord, TranscriptionResult, WhisperEngine, WhisperTaskConfig, extract_audio_wav_16k,
    is_vulkan_runtime_available, read_wav_pcm16_mono_f32, resolve_language_target,
};
pub use models::{ModelDownloadProgress, WhisperModelInfo, download_model, list_models, resolve_available_model};

