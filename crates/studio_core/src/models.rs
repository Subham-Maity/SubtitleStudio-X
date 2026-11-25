//! Domain models and wire DTOs for `studio_core`.

use serde::{Deserialize, Serialize};
use subtitle_render::{
    AnimationIn, AnimationOut, EasingKind, GlowConfig, GradientConfig, KaraokeMode, RenderCue,
    RenderStyle, RenderTimeline, RenderWord, ShadowConfig, StrokePosition,
};

/// Processing status of a video project pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PipelineStatus {
    Uploaded,
    ExtractingAudio,
    Transcribing,
    Transcribed,
    Failed,
}

impl PipelineStatus {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Uploaded => "UPLOADED",
            Self::ExtractingAudio => "EXTRACTING_AUDIO",
            Self::Transcribing => "TRANSCRIBING",
            Self::Transcribed => "TRANSCRIBED",
            Self::Failed => "FAILED",
        }
    }

    #[must_use]
    pub fn from_db(s: &str) -> Self {
        match s {
            "EXTRACTING_AUDIO" => Self::ExtractingAudio,
            "TRANSCRIBING" => Self::Transcribing,
            "TRANSCRIBED" => Self::Transcribed,
            "FAILED" => Self::Failed,
            _ => Self::Uploaded,
        }
    }
}

/// Export mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExportMode {
    Overlay,
    CaptionsOnly,
}

impl ExportMode {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Overlay => "OVERLAY",
            Self::CaptionsOnly => "CAPTIONS_ONLY",
        }
    }

    #[must_use]
    pub fn from_db(s: &str) -> Self {
        match s {
            "CAPTIONS_ONLY" => Self::CaptionsOnly,
            _ => Self::Overlay,
        }
    }
}

/// Video container/codec preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportFormat {
    #[serde(rename = "MP4_H265")]
    Mp4H265,
    #[serde(rename = "MOV")]
    Mov,
}

impl ExportFormat {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mp4H265 => "MP4_H265",
            Self::Mov => "MOV",
        }
    }

    #[must_use]
    pub fn from_db(s: &str) -> Self {
        match s {
            "MOV" => Self::Mov,
            _ => Self::Mp4H265,
        }
    }
}

/// Quality preset for video encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ExportQuality {
    Fast,
    #[default]
    Balanced,
    Quality,
}

impl ExportQuality {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Balanced => "balanced",
            Self::Quality => "quality",
        }
    }

    #[must_use]
    pub fn from_db(s: &str) -> Self {
        match s {
            "fast" => Self::Fast,
            "quality" => Self::Quality,
            _ => Self::Balanced,
        }
    }
}

/// Export job status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExportStatus {
    Queued,
    Rendering,
    Done,
    Failed,
}

impl ExportStatus {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "QUEUED",
            Self::Rendering => "RENDERING",
            Self::Done => "DONE",
            Self::Failed => "FAILED",
        }
    }

    #[must_use]
    pub fn from_db(s: &str) -> Self {
        match s {
            "RENDERING" => Self::Rendering,
            "DONE" => Self::Done,
            "FAILED" => Self::Failed,
            _ => Self::Queued,
        }
    }
}

/// Probed video metadata.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoMetadata {
    pub duration_ms: i64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
}

/// Word-level transcript entry persisted in SQLite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptWord {
    pub id: String,
    pub video_project_id: String,
    pub word: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub order: i32,
}

/// Subtitle cue persisted in SQLite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleCue {
    pub id: String,
    pub video_project_id: String,
    pub text: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub order: i32,
    pub color_hex: Option<String>,
    pub edited: bool,
    pub word_start_idx: Option<usize>,
    pub word_end_idx: Option<usize>,
}

/// Complete persisted subtitle style including animation and effect config.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleStyle {
    pub id: String,
    pub video_project_id: String,
    pub font_file_name: String,
    pub font_size_px: f32,
    pub font_color_hex: String,
    pub outline_color_hex: String,
    pub outline_width_px: f32,
    pub background_box_on: bool,
    pub background_color_hex: String,
    pub background_opacity: f32,
    pub position: String,
    pub vertical_offset_pct: i32,
    pub position_x: f32,
    pub position_y: f32,
    pub rotation_deg: f32,
    pub animation: String,
    pub words_per_cue: i32,
    pub uppercase: bool,
    pub bold: bool,
    pub italic: bool,
    pub stroke_position: StrokePosition,
    pub anim_in: AnimationIn,
    pub anim_out: AnimationOut,
    pub anim_duration_ms: i64,
    pub easing: EasingKind,
    pub karaoke_mode: KaraokeMode,
    pub karaoke_color_hex: String,
    pub karaoke_scale: f32,
    pub shadow: Option<ShadowConfig>,
    pub glow: Option<GlowConfig>,
    pub gradient: Option<GradientConfig>,
    pub line_height: f32,
    pub letter_spacing: f32,
    pub box_radius: f32,
    pub box_padding_x: f32,
    pub box_padding_y: f32,
    pub max_width_pct: f32,
}

impl SubtitleStyle {
    #[must_use]
    pub fn default_for_project(id: String, video_project_id: String) -> Self {
        let r = RenderStyle::default();
        Self {
            id,
            video_project_id,
            font_file_name: "BalooBhai2-VariableFont_wght.ttf".to_string(),
            font_size_px: r.font_size_px,
            font_color_hex: r.font_color_hex,
            outline_color_hex: r.outline_color_hex,
            outline_width_px: r.outline_width_px,
            background_box_on: r.background_box_on,
            background_color_hex: r.background_color_hex,
            background_opacity: r.background_opacity,
            position: "bottom".to_string(),
            vertical_offset_pct: 10,
            position_x: r.position_x,
            position_y: r.position_y,
            rotation_deg: r.rotation_deg,
            animation: "pop".to_string(),
            words_per_cue: 6,
            uppercase: r.uppercase,
            bold: r.bold,
            italic: r.italic,
            stroke_position: r.stroke_position,
            anim_in: r.anim_in,
            anim_out: r.anim_out,
            anim_duration_ms: r.anim_duration_ms,
            easing: r.easing,
            karaoke_mode: r.karaoke_mode,
            karaoke_color_hex: r.karaoke_color_hex,
            karaoke_scale: r.karaoke_scale,
            shadow: r.shadow,
            glow: r.glow,
            gradient: r.gradient,
            line_height: r.line_height,
            letter_spacing: r.letter_spacing,
            box_radius: r.box_radius,
            box_padding_x: r.box_padding_x,
            box_padding_y: r.box_padding_y,
            max_width_pct: r.max_width_pct,
        }
    }

    #[must_use]
    pub fn to_render_style(&self) -> RenderStyle {
        RenderStyle {
            font_file_name: self.font_file_name.clone(),
            font_size_px: self.font_size_px,
            font_color_hex: self.font_color_hex.clone(),
            outline_color_hex: self.outline_color_hex.clone(),
            outline_width_px: self.outline_width_px,
            stroke_position: self.stroke_position,
            background_box_on: self.background_box_on,
            background_color_hex: self.background_color_hex.clone(),
            background_opacity: self.background_opacity,
            position_x: self.position_x,
            position_y: self.position_y,
            rotation_deg: self.rotation_deg,
            uppercase: self.uppercase,
            bold: self.bold,
            italic: self.italic,
            anim_in: self.anim_in,
            anim_out: self.anim_out,
            anim_duration_ms: self.anim_duration_ms,
            easing: self.easing,
            karaoke_mode: self.karaoke_mode,
            karaoke_color_hex: self.karaoke_color_hex.clone(),
            karaoke_scale: self.karaoke_scale,
            shadow: self.shadow.clone(),
            glow: self.glow.clone(),
            gradient: self.gradient.clone(),
            line_height: self.line_height,
            letter_spacing: self.letter_spacing,
            box_radius: self.box_radius,
            box_padding_x: self.box_padding_x,
            box_padding_y: self.box_padding_y,
            max_width_pct: self.max_width_pct,
        }
    }
}

/// Export job persisted in SQLite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportJob {
    pub id: String,
    pub video_project_id: String,
    pub mode: ExportMode,
    pub background_hex: Option<String>,
    pub include_audio: bool,
    pub format: ExportFormat,
    pub quality: ExportQuality,
    pub encoder: Option<String>,
    pub status: ExportStatus,
    pub progress_pct: i32,
    pub output_path: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
}

/// Video project persisted in SQLite.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoProject {
    pub id: String,
    pub original_filename: String,
    pub storage_path: String,
    pub duration_ms: i64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub status: PipelineStatus,
    pub language: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub words: Vec<TranscriptWord>,
    pub cues: Vec<SubtitleCue>,
    pub style: Option<SubtitleStyle>,
    pub export_jobs: Vec<ExportJob>,
    pub cue_count: usize,
}

impl VideoProject {
    /// Convert project cues, words, and style into a pure [`RenderTimeline`].
    #[must_use]
    pub fn to_render_timeline(&self) -> RenderTimeline {
        RenderTimeline {
            cues: self
                .cues
                .iter()
                .map(|c| RenderCue {
                    id: c.id.clone(),
                    text: c.text.clone(),
                    start_ms: c.start_ms,
                    end_ms: c.end_ms,
                    color_hex: c.color_hex.clone(),
                    word_start_idx: c.word_start_idx,
                    word_end_idx: c.word_end_idx,
                })
                .collect(),
            words: self
                .words
                .iter()
                .map(|w| RenderWord {
                    word: w.word.clone(),
                    start_ms: w.start_ms,
                    end_ms: w.end_ms,
                })
                .collect(),
            style: self
                .style
                .as_ref()
                .map(SubtitleStyle::to_render_style)
                .unwrap_or_default(),
        }
    }
}

/// Partial update patch for [`SubtitleStyle`].
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStylePatch {
    pub font_file_name: Option<String>,
    pub font_size_px: Option<f32>,
    pub font_color_hex: Option<String>,
    pub outline_color_hex: Option<String>,
    pub outline_width_px: Option<f32>,
    pub background_box_on: Option<bool>,
    pub background_color_hex: Option<String>,
    pub background_opacity: Option<f32>,
    pub position: Option<String>,
    pub vertical_offset_pct: Option<i32>,
    pub position_x: Option<f32>,
    pub position_y: Option<f32>,
    pub rotation_deg: Option<f32>,
    pub animation: Option<String>,
    pub words_per_cue: Option<i32>,
    pub uppercase: Option<bool>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub stroke_position: Option<StrokePosition>,
    pub anim_in: Option<AnimationIn>,
    pub anim_out: Option<AnimationOut>,
    pub anim_duration_ms: Option<i64>,
    pub easing: Option<EasingKind>,
    pub karaoke_mode: Option<KaraokeMode>,
    pub karaoke_color_hex: Option<String>,
    pub karaoke_scale: Option<f32>,
    pub shadow: Option<Option<ShadowConfig>>,
    pub glow: Option<Option<GlowConfig>>,
    pub gradient: Option<Option<GradientConfig>>,
    pub line_height: Option<f32>,
    pub letter_spacing: Option<f32>,
    pub box_radius: Option<f32>,
    pub box_padding_x: Option<f32>,
    pub box_padding_y: Option<f32>,
    pub max_width_pct: Option<f32>,
}

/// Storage cleaner telemetry response.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanJunkResult {
    pub freed_mb: f64,
    pub deleted_files_count: usize,
    pub message: String,
}

/// Single font entry exposed to UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FontInfo {
    pub file_name: String,
    pub display_name: String,
    pub path: String,
}

/// Subtitle text export format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SubtitleTextFormat {
    Srt,
    Text,
}

/// Exported subtitle payload.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedSubtitles {
    pub filename: String,
    pub content: String,
    pub format: SubtitleTextFormat,
}

