//! Error types for subtitle rendering.

use thiserror::Error;

/// Errors raised by `subtitle_render`.
#[derive(Debug, Error)]
pub enum RenderError {
    #[error("invalid output dimensions: {width}x{height}")]
    InvalidDimensions { width: u32, height: u32 },

    #[error("failed to allocate raster surface ({width}x{height})")]
    SurfaceAllocation { width: u32, height: u32 },

    #[error("failed to read unpremultiplied pixels from surface")]
    PixelReadback,

    #[error("failed to load font '{path}': {reason}")]
    FontLoad { path: String, reason: String },
}

