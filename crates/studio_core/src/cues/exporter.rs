//! SRT and plain-text subtitle formatting and multi-project merge (`subtitle.service.ts` parity).

use chrono::Utc;

use crate::models::{ExportedSubtitles, SubtitleTextFormat, VideoProject};

/// Format milliseconds as `HH:MM:SS,mmm` for SubRip `.srt`.
#[must_use]
pub fn format_ms_to_srt_time(ms: i64) -> String {
    let clamped = ms.max(0);
    let total_sec = clamped / 1000;
    let hours = total_sec / 3600;
    let minutes = (total_sec % 3600) / 60;
    let seconds = total_sec % 60;
    let millis = clamped % 1000;
    format!("{hours:02}:{minutes:02}:{seconds:02},{millis:03}")
}

/// Export a single project's cues as `.srt` or `.txt`.
#[must_use]
pub fn export_project_subtitles(
    project: &VideoProject,
    format: SubtitleTextFormat,
) -> ExportedSubtitles {
    let stem = strip_extension(&project.original_filename);
    match format {
        SubtitleTextFormat::Text => {
            let content = project
                .cues
                .iter()
                .map(|c| c.text.trim())
                .collect::<Vec<_>>()
                .join("\n");
            ExportedSubtitles {
                filename: format!("{stem}_subtitles.txt"),
                content,
                format,
            }
        }
        SubtitleTextFormat::Srt => {
            let blocks: Vec<String> = project
                .cues
                .iter()
                .enumerate()
                .map(|(idx, cue)| {
                    let start = format_ms_to_srt_time(cue.start_ms);
                    let end = format_ms_to_srt_time(cue.end_ms);
                    format!("{}\n{start} --> {end}\n{}", idx + 1, cue.text)
                })
                .collect();
            ExportedSubtitles {
                filename: format!("{stem}_subtitles.srt"),
                content: blocks.join("\n\n"),
                format,
            }
        }
    }
}

/// Merge subtitles across multiple projects in order with cumulative `durationMs` time offsets.
#[must_use]
pub fn merge_project_subtitles(
    projects: &[VideoProject],
    format: SubtitleTextFormat,
) -> ExportedSubtitles {
    let stamp = Utc::now().timestamp_millis();
    match format {
        SubtitleTextFormat::Text => {
            let mut sections = Vec::new();
            for proj in projects {
                if !proj.cues.is_empty() {
                    let text = proj
                        .cues
                        .iter()
                        .map(|c| c.text.trim())
                        .collect::<Vec<_>>()
                        .join(" ");
                    sections.push(format!("--- [{}] ---\n{text}", proj.original_filename));
                }
            }
            ExportedSubtitles {
                filename: format!("merged_subtitles_{stamp}.txt"),
                content: sections.join("\n\n"),
                format,
            }
        }
        SubtitleTextFormat::Srt => {
            let mut global_idx = 1usize;
            let mut time_offset_ms = 0i64;
            let mut blocks = Vec::new();

            for proj in projects {
                for cue in &proj.cues {
                    let start = format_ms_to_srt_time(cue.start_ms + time_offset_ms);
                    let end = format_ms_to_srt_time(cue.end_ms + time_offset_ms);
                    blocks.push(format!("{global_idx}\n{start} --> {end}\n{}", cue.text));
                    global_idx += 1;
                }
                let duration = if proj.duration_ms > 0 {
                    proj.duration_ms
                } else {
                    proj.cues.last().map(|c| c.end_ms + 1000).unwrap_or(0)
                };
                time_offset_ms += duration;
            }

            ExportedSubtitles {
                filename: format!("merged_subtitles_{stamp}.srt"),
                content: blocks.join("\n\n"),
                format,
            }
        }
    }
}

fn strip_extension(filename: &str) -> &str {
    match filename.rfind('.') {
        Some(idx) if idx > 0 => &filename[..idx],
        _ => filename,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{PipelineStatus, SubtitleCue};

    #[test]
    fn srt_format_and_merge_offsets_match_reference() {
        assert_eq!(format_ms_to_srt_time(3_661_042), "01:01:01,042");

        let make_proj = |id: &str, name: &str, dur: i64, text: &str| VideoProject {
            id: id.into(),
            original_filename: name.into(),
            storage_path: String::new(),
            duration_ms: dur,
            width: 1920,
            height: 1080,
            fps: 30.0,
            status: PipelineStatus::Transcribed,
            language: None,
            error_message: None,
            created_at: String::new(),
            updated_at: String::new(),
            words: Vec::new(),
            cues: vec![SubtitleCue {
                id: format!("c_{id}"),
                video_project_id: id.into(),
                text: text.into(),
                start_ms: 500,
                end_ms: 1500,
                order: 0,
                color_hex: None,
                edited: false,
                word_start_idx: None,
                word_end_idx: None,
            }],
            style: None,
            export_jobs: Vec::new(),
            cue_count: 1,
        };

        let p1 = make_proj("1", "a.mp4", 5000, "first");
        let p2 = make_proj("2", "b.mp4", 4000, "second");
        let merged = merge_project_subtitles(&[p1, p2], SubtitleTextFormat::Srt);
        assert!(merged.content.contains("00:00:00,500 --> 00:00:01,500\nfirst"));
        assert!(merged.content.contains("00:00:05,500 --> 00:00:06,500\nsecond"));
    }
}

