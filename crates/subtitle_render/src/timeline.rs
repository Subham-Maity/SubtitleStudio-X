//! Timeline cue lookup and index-based word timing resolution.

use crate::types::{RenderCue, RenderTimeline, RenderWord};

/// A resolved display token inside an active cue, paired with its timing window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedToken {
    pub text: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub word_index: usize,
}

/// Find the active cue at timestamp `t_ms` (inclusive `[start_ms, end_ms]`).
#[must_use]
pub fn find_active_cue(timeline: &RenderTimeline, t_ms: i64) -> Option<&RenderCue> {
    timeline
        .cues
        .iter()
        .find(|cue| t_ms >= cue.start_ms && t_ms <= cue.end_ms)
}

/// Resolve per-word timing for `cue`.
///
/// Uses stored word index range (`word_start_idx..word_end_idx`) whenever valid and
/// token-aligned with `cue.text`. When the user edited or manually created a cue,
/// distributes `[cue.start_ms, cue.end_ms]` proportionally across the new tokens.
#[must_use]
pub fn resolve_cue_tokens(
    cue: &RenderCue,
    all_words: &[RenderWord],
    uppercase: bool,
) -> Vec<ResolvedToken> {
    let raw_text = if uppercase {
        cue.text.to_uppercase()
    } else {
        cue.text.clone()
    };
    let text_tokens: Vec<&str> = raw_text.split_whitespace().collect();
    if text_tokens.is_empty() {
        return Vec::new();
    }

    if let (Some(s), Some(e)) = (cue.word_start_idx, cue.word_end_idx)
        && s < e
        && e <= all_words.len()
        && e - s == text_tokens.len()
    {
        return text_tokens
            .into_iter()
            .enumerate()
            .map(|(offset, tok)| {
                let w = &all_words[s + offset];
                ResolvedToken {
                    text: tok.to_string(),
                    start_ms: w.start_ms,
                    end_ms: w.end_ms,
                    word_index: s + offset,
                }
            })
            .collect();
    }

    // Fallback for edited or manually created cues: proportional timing by token weight.
    let cue_dur = (cue.end_ms - cue.start_ms).max(1);
    let weights: Vec<usize> = text_tokens
        .iter()
        .map(|t| t.chars().count().max(1))
        .collect();
    let total_weight: usize = weights.iter().sum::<usize>().max(1);

    let mut acc_weight = 0usize;
    text_tokens
        .into_iter()
        .enumerate()
        .map(|(idx, tok)| {
            let start_frac = acc_weight as f64 / total_weight as f64;
            acc_weight += weights[idx];
            let end_frac = acc_weight as f64 / total_weight as f64;
            let start_ms = cue.start_ms + (start_frac * cue_dur as f64).round() as i64;
            let end_ms = cue.start_ms + (end_frac * cue_dur as f64).round() as i64;
            ResolvedToken {
                text: tok.to_string(),
                start_ms,
                end_ms: end_ms.max(start_ms + 1),
                word_index: idx,
            }
        })
        .collect()
}

/// Determine the active token index inside `tokens` at `t_ms`, using index/time (never text matching).
#[must_use]
pub fn active_token_index(tokens: &[ResolvedToken], t_ms: i64) -> Option<usize> {
    tokens
        .iter()
        .position(|tok| t_ms >= tok.start_ms && t_ms <= tok.end_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_words_highlight_only_by_index_and_time() {
        let words = vec![
            RenderWord {
                word: "go".into(),
                start_ms: 0,
                end_ms: 400,
            },
            RenderWord {
                word: "go".into(),
                start_ms: 500,
                end_ms: 900,
            },
        ];
        let cue = RenderCue {
            id: "c1".into(),
            text: "go go".into(),
            start_ms: 0,
            end_ms: 1000,
            color_hex: None,
            word_start_idx: Some(0),
            word_end_idx: Some(2),
        };
        let tokens = resolve_cue_tokens(&cue, &words, false);
        assert_eq!(active_token_index(&tokens, 200), Some(0));
        assert_eq!(active_token_index(&tokens, 700), Some(1));
    }

    #[test]
    fn edited_cue_distributes_timing_proportionally() {
        let cue = RenderCue {
            id: "c2".into(),
            text: "hello world".into(),
            start_ms: 1000,
            end_ms: 2000,
            color_hex: None,
            word_start_idx: None,
            word_end_idx: None,
        };
        let tokens = resolve_cue_tokens(&cue, &[], false);
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].start_ms, 1000);
        assert_eq!(tokens[1].end_ms, 2000);
        assert!(tokens[0].end_ms < tokens[1].end_ms);
    }
}

