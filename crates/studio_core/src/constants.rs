//! Pipeline constants ported 1:1 from `pipeline.constants.ts`.

/// Gap in milliseconds between consecutive words that forces a new cue.
pub const PAUSE_BREAK_MS: i64 = 700;

/// Tail padding in milliseconds added to the end of each cue before overlap clamping.
pub const TAIL_PADDING_MS: i64 = 150;

/// Default number of words per generated subtitle cue.
pub const DEFAULT_WORDS_PER_CUE: usize = 6;

