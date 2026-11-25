//! Subtitle cue generation and SRT/TXT export utilities.

pub mod exporter;
pub mod generator;

pub use exporter::{export_project_subtitles, format_ms_to_srt_time, merge_project_subtitles};
pub use generator::{GeneratedCue, generate_cues};

