//! Built-in and user-saved JSON style presets.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::RenderError;
use crate::types::{
    AnimationIn, GlowConfig, KaraokeMode, RenderStyle, StrokePosition,
};

/// Named preset persisted as JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StylePreset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub builtin: bool,
    pub style: RenderStyle,
}

/// Port of the 5 toolbar presets from `canvas-floating-toolbar.tsx`.
#[must_use]
pub fn builtin_presets() -> Vec<StylePreset> {
    let base = RenderStyle::default();

    let minimal = RenderStyle {
        font_color_hex: "#FFFFFF".to_string(),
        outline_width_px: 0.0,
        background_box_on: false,
        bold: false,
        italic: false,
        karaoke_mode: KaraokeMode::None,
        glow: None,
        ..base.clone()
    };

    let karaoke = RenderStyle {
        font_color_hex: "#FFE600".to_string(),
        outline_color_hex: "#000000".to_string(),
        outline_width_px: 3.0,
        stroke_position: StrokePosition::Outside,
        background_box_on: false,
        bold: true,
        karaoke_mode: KaraokeMode::HighlightCurrentWord,
        karaoke_color_hex: "#FFE600".to_string(),
        karaoke_scale: 1.1,
        anim_in: AnimationIn::Pop,
        ..base.clone()
    };

    let outline = RenderStyle {
        font_color_hex: "#FFFFFF".to_string(),
        outline_color_hex: "#000000".to_string(),
        outline_width_px: 4.0,
        stroke_position: StrokePosition::Outside,
        background_box_on: false,
        bold: true,
        uppercase: true,
        ..base.clone()
    };

    let boxed = RenderStyle {
        font_color_hex: "#FFFFFF".to_string(),
        background_box_on: true,
        background_color_hex: "#000000".to_string(),
        background_opacity: 0.75,
        outline_width_px: 0.0,
        ..base.clone()
    };

    let neon = RenderStyle {
        font_color_hex: "#00E5FF".to_string(),
        outline_color_hex: "#0055FF".to_string(),
        outline_width_px: 3.0,
        background_box_on: false,
        bold: true,
        italic: true,
        glow: Some(GlowConfig {
            enabled: true,
            color_hex: "#00E5FF".to_string(),
            radius: 10.0,
            intensity: 1.0,
        }),
        ..base
    };

    vec![
        StylePreset {
            id: "minimal".to_string(),
            name: "Clean Minimalist".to_string(),
            description: "No border".to_string(),
            builtin: true,
            style: minimal,
        },
        StylePreset {
            id: "karaoke".to_string(),
            name: "Yellow Karaoke Pop".to_string(),
            description: "Word highlight + pop".to_string(),
            builtin: true,
            style: karaoke,
        },
        StylePreset {
            id: "outline".to_string(),
            name: "Bold Outline".to_string(),
            description: "Heavy stroke + CAPS".to_string(),
            builtin: true,
            style: outline,
        },
        StylePreset {
            id: "box".to_string(),
            name: "Dark Boxed Subtitle".to_string(),
            description: "Rounded dark box".to_string(),
            builtin: true,
            style: boxed,
        },
        StylePreset {
            id: "neon".to_string(),
            name: "Cyan Neon Glow".to_string(),
            description: "Vibrant glow".to_string(),
            builtin: true,
            style: neon,
        },
    ]
}

/// Save a user style preset to JSON on disk.
pub fn save_preset_json(path: &Path, preset: &StylePreset) -> Result<(), RenderError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| RenderError::FontLoad {
            path: path.display().to_string(),
            reason: e.to_string(),
        })?;
    }
    let json = serde_json::to_string_pretty(preset).map_err(|e| RenderError::FontLoad {
        path: path.display().to_string(),
        reason: e.to_string(),
    })?;
    std::fs::write(path, json).map_err(|e| RenderError::FontLoad {
        path: path.display().to_string(),
        reason: e.to_string(),
    })
}

/// Load a user style preset from a JSON file.
pub fn load_preset_json(path: &Path) -> Result<StylePreset, RenderError> {
    let raw = std::fs::read_to_string(path).map_err(|e| RenderError::FontLoad {
        path: path.display().to_string(),
        reason: e.to_string(),
    })?;
    serde_json::from_str(&raw).map_err(|e| RenderError::FontLoad {
        path: path.display().to_string(),
        reason: e.to_string(),
    })
}

