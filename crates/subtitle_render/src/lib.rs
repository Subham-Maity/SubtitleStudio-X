//! Pure, deterministic subtitle frame renderer (`subtitle_render`).
//!
//! Renders RGBA8888 unpremultiplied subtitle overlays for both live canvas
//! preview and FFmpeg video export without any Tauri or global state dependency.

#![warn(clippy::all)]

pub mod color;
pub mod easing;
pub mod error;
pub mod fonts;
pub mod presets;
pub mod renderer;
pub mod timeline;
pub mod types;

pub use error::RenderError;
pub use fonts::{FontRegistry, RegisteredFont};
pub use presets::{StylePreset, builtin_presets, load_preset_json, save_preset_json};
pub use renderer::{RenderCache, render_frame, render_frame_owned};
pub use timeline::{ResolvedToken, active_token_index, find_active_cue, resolve_cue_tokens};
pub use types::{
    AnimationIn, AnimationOut, EasingKind, GlowConfig, GradientConfig, KaraokeMode, RenderCue,
    RenderStyle, RenderTimeline, RenderWord, ShadowConfig, StrokePosition,
};

/// Reference canvas height used for 1:1 WYSIWYG scaling between preview and export.
pub const REFERENCE_HEIGHT: f32 = 540.0;

/// Compute the WYSIWYG scale factor for a target output height.
#[must_use]
pub fn wysiwyg_scale(out_h: u32) -> f32 {
    out_h as f32 / REFERENCE_HEIGHT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wysiwyg_scale_matches_reference_height() {
        assert!((wysiwyg_scale(540) - 1.0).abs() < f32::EPSILON);
        assert!((wysiwyg_scale(1080) - 2.0).abs() < f32::EPSILON);
        assert!((wysiwyg_scale(2160) - 4.0).abs() < f32::EPSILON);
    }
}
