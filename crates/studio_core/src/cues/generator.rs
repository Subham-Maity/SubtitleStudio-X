//! Cue generation algorithm ported 1:1 from `cue-generation.layer.ts`.

use crate::constants::{PAUSE_BREAK_MS, TAIL_PADDING_MS};

/// Generated cue with word index range `[word_start_idx, word_end_idx)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedCue {
    pub text: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub order: i32,
    pub word_start_idx: usize,
    pub word_end_idx: usize,
}

/// Group word-level timestamps into subtitle cues.
///
/// Mirrors `CueGenerationLayer.generateCues` from the reference repo:
/// - Breaks on `words_per_cue` cap OR when gap between consecutive words exceeds `PAUSE_BREAK_MS` (700 ms).
/// - Adds `TAIL_PADDING_MS` (150 ms) to each cue's end.
/// - Clamps `end_ms` to avoid overlapping the next cue (`max(start_ms + 100, next_start_ms - 20)`).
/// - Preserves `[word_start_idx, word_end_idx)` for index-based karaoke rendering.
#[must_use]
pub fn generate_cues(words: &[(String, i64, i64)], words_per_cue: usize) -> Vec<GeneratedCue> {
    if words.is_empty() {
        return Vec::new();
    }
    let cap = words_per_cue.max(1);

    struct RawBatch<'a> {
        start_idx: usize,
        words: Vec<&'a (String, i64, i64)>,
        start_ms: i64,
        end_ms: i64,
    }

    let mut cues_raw: Vec<RawBatch<'_>> = Vec::new();
    let mut current: Vec<&(String, i64, i64)> = Vec::new();
    let mut batch_start_idx = 0usize;

    for (idx, word) in words.iter().enumerate() {
        let gap_ms = current
            .last()
            .map(|last| word.1 - last.2)
            .unwrap_or(0);

        let reached_cap = current.len() >= cap;
        let reached_pause = !current.is_empty() && gap_ms > PAUSE_BREAK_MS;

        if reached_cap || reached_pause {
            if let (Some(first), Some(last)) = (current.first(), current.last()) {
                cues_raw.push(RawBatch {
                    start_idx: batch_start_idx,
                    start_ms: first.1,
                    end_ms: last.2 + TAIL_PADDING_MS,
                    words: current,
                });
            }
            current = Vec::new();
            batch_start_idx = idx;
        }
        current.push(word);
    }

    if let (Some(first), Some(last)) = (current.first(), current.last()) {
        cues_raw.push(RawBatch {
            start_idx: batch_start_idx,
            start_ms: first.1,
            end_ms: last.2 + TAIL_PADDING_MS,
            words: current,
        });
    }

    let mut out = Vec::with_capacity(cues_raw.len());
    for idx in 0..cues_raw.len() {
        let mut end_ms = cues_raw[idx].end_ms;
        if let Some(next_cue) = cues_raw.get(idx + 1)
            && end_ms > next_cue.start_ms
        {
            end_ms = (cues_raw[idx].start_ms + 100).max(next_cue.start_ms - 20);
        }

        let text = cues_raw[idx]
            .words
            .iter()
            .map(|w| w.0.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        let word_start_idx = cues_raw[idx].start_idx;
        let word_end_idx = word_start_idx + cues_raw[idx].words.len();

        out.push(GeneratedCue {
            text,
            start_ms: cues_raw[idx].start_ms,
            end_ms,
            order: idx as i32,
            word_start_idx,
            word_end_idx,
        });
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn respects_pause_break_tail_padding_and_overlap_clamp() {
        let words = vec![
            ("one".into(), 0, 200),
            ("two".into(), 220, 400),
            ("three".into(), 450, 600), // close: next word starts at 700 -> overlap clamp
            ("four".into(), 700, 900),
            ("five".into(), 1700, 1900), // gap = 800 > 700 -> pause break
        ];
        let cues = generate_cues(&words, 3);
        assert_eq!(cues.len(), 3);
        assert_eq!(cues[0].text, "one two three");
        assert_eq!(cues[0].end_ms, 680); // clamped from 600+150=750 to 700-20=680
        assert_eq!(cues[0].word_start_idx, 0);
        assert_eq!(cues[0].word_end_idx, 3);
        assert_eq!(cues[1].text, "four");
        assert_eq!(cues[1].end_ms, 1050);
        assert_eq!(cues[2].text, "five");
        assert_eq!(cues[2].end_ms, 2050);
    }
}

