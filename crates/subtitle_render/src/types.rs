//! Core data-driven subtitle timeline and style types.

use serde::{Deserialize, Serialize};

/// Subtitle entrance animation kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum AnimationIn {
    None,
    Fade,
    #[default]
    Pop,
    SlideUp,
    Zoom,
    Typewriter,
    WordReveal,
    Bounce,
    BlurIn,
}

/// Subtitle exit animation kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum AnimationOut {
    #[default]
    None,
    FadeOut,
    SlideDown,
}

/// Closed-form easing curve identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum EasingKind {
    Linear,
    #[default]
    Cubic,
    Back,
    Elastic,
    Spring,
}

/// Karaoke word highlight rendering mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum KaraokeMode {
    None,
    #[default]
    HighlightCurrentWord,
    ProgressiveFill,
    PopCurrent,
}

/// Stroke alignment relative to glyph boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum StrokePosition {
    #[default]
    Outside,
    Inside,
    Center,
}

/// Drop-shadow effect configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShadowConfig {
    pub enabled: bool,
    pub color_hex: String,
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
}

/// Neon glow effect configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlowConfig {
    pub enabled: bool,
    pub color_hex: String,
    pub radius: f32,
    pub intensity: f32,
}

/// Linear gradient fill configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GradientConfig {
    pub enabled: bool,
    pub start_hex: String,
    pub end_hex: String,
    pub angle_deg: f32,
}

/// Word-level timing token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderWord {
    pub word: String,
    pub start_ms: i64,
    pub end_ms: i64,
}

/// Single subtitle cue on the render timeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderCue {
    pub id: String,
    pub text: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub color_hex: Option<String>,
    pub word_start_idx: Option<usize>,
    pub word_end_idx: Option<usize>,
}

/// Complete style specification (data-driven, JSON-serializable).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderStyle {
    pub font_file_name: String,
    pub font_size_px: f32,
    pub font_color_hex: String,
    pub outline_color_hex: String,
    pub outline_width_px: f32,
    pub stroke_position: StrokePosition,
    pub background_box_on: bool,
    pub background_color_hex: String,
    pub background_opacity: f32,
    pub position_x: f32,
    pub position_y: f32,
    pub rotation_deg: f32,
    pub uppercase: bool,
    pub bold: bool,
    pub italic: bool,
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

impl Default for RenderStyle {
    fn default() -> Self {
        Self {
            font_file_name: String::new(),
            font_size_px: 42.0,
            font_color_hex: "#FFFFFF".to_string(),
            outline_color_hex: "#000000".to_string(),
            outline_width_px: 2.0,
            stroke_position: StrokePosition::Outside,
            background_box_on: false,
            background_color_hex: "#000000".to_string(),
            background_opacity: 0.5,
            position_x: 50.0,
            position_y: 80.0,
            rotation_deg: 0.0,
            uppercase: false,
            bold: false,
            italic: false,
            anim_in: AnimationIn::Pop,
            anim_out: AnimationOut::None,
            anim_duration_ms: 300,
            easing: EasingKind::Cubic,
            karaoke_mode: KaraokeMode::HighlightCurrentWord,
            karaoke_color_hex: "#FFE600".to_string(),
            karaoke_scale: 1.1,
            shadow: None,
            glow: None,
            gradient: None,
            line_height: 1.25,
            letter_spacing: 0.0,
            box_radius: 8.0,
            box_padding_x: 12.0,
            box_padding_y: 6.0,
            max_width_pct: 88.0,
        }
    }
}

/// Complete timeline passed to `render_frame`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderTimeline {
    pub cues: Vec<RenderCue>,
    pub words: Vec<RenderWord>,
    pub style: RenderStyle,
}

