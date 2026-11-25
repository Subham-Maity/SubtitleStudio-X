//! Core application engine (`studio_core`): SQLite persistence, FFmpeg/FFprobe
//! orchestration, Whisper transcription, queue management, font hot-reload, and batching.

#![warn(clippy::all)]

pub mod batch;
pub mod constants;
pub mod cues;
pub mod db;
pub mod error;
pub mod export;
pub mod fonts;
pub mod media;
pub mod models;
pub mod paths;
pub mod queue;
pub mod whisper;

pub use error::CoreError;
pub use paths::StudioPaths;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_match_reference_pipeline() {
        assert_eq!(constants::PAUSE_BREAK_MS, 700);
        assert_eq!(constants::TAIL_PADDING_MS, 150);
        assert_eq!(constants::DEFAULT_WORDS_PER_CUE, 6);
    }
}
