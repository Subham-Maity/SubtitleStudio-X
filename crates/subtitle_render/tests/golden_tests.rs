use std::path::Path;

use subtitle_render::{
    AnimationIn, AnimationOut, EasingKind, FontRegistry, GlowConfig, GradientConfig, KaraokeMode,
    RenderCache, RenderCue, RenderStyle, RenderTimeline, RenderWord, ShadowConfig, StrokePosition,
    builtin_presets, load_preset_json, render_frame, render_frame_owned, save_preset_json,
};

fn count_non_transparent(rgba: &[u8]) -> usize {
    rgba.as_chunks::<4>().0.iter().filter(|px| px[3] > 0).count()
}

fn sample_timeline(text: &str, style: RenderStyle) -> RenderTimeline {
    let words: Vec<RenderWord> = text
        .split_whitespace()
        .enumerate()
        .map(|(i, w)| RenderWord {
            word: w.to_string(),
            start_ms: (i as i64) * 350,
            end_ms: (i as i64 + 1) * 350 - 20,
        })
        .collect();
    let end_ms = words.last().map(|w| w.end_ms + 300).unwrap_or(2000);
    RenderTimeline {
        cues: vec![RenderCue {
            id: "cue-1".to_string(),
            text: text.to_string(),
            start_ms: 0,
            end_ms,
            color_hex: None,
            word_start_idx: Some(0),
            word_end_idx: Some(words.len()),
        }],
        words,
        style,
    }
}

#[test]
fn multilingual_and_emoji_render_non_empty_pixels() {
    let mut fonts = FontRegistry::new();
    let bundled = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/fonts");
    fonts.register_directories(&[bundled.as_path()]);
    let mut cache = RenderCache::default();

    let cases = [
        ("english", "Hello world this is SubtitleStudio X"),
        ("hindi", "यह हिंदी भाषा में एक बातचीत है।"),
        ("urdu", "یہ اردو زبان میں ایک گفتگو ہے۔"),
        ("emoji", "Cinema quality captions 🔥🎬✨"),
        (
            "wrapping",
            "This is a deliberately very long subtitle line that must wrap cleanly inside 88 percent width without clipping off screen",
        ),
    ];

    for (name, text) in cases {
        let style = RenderStyle {
            anim_in: AnimationIn::None,
            ..Default::default()
        };
        let tl = sample_timeline(text, style);
        let rgba = render_frame(&tl, 500, 960, 540, &fonts, &mut cache)
            .expect("render must succeed");
        let non_empty = count_non_transparent(rgba);
        assert!(
            non_empty > 500,
            "case '{name}' produced too few visible pixels: {non_empty}"
        );
    }
}

#[test]
fn deterministic_preview_export_parity_zero_diff() {
    let fonts = FontRegistry::new();
    let style = RenderStyle {
        anim_in: AnimationIn::Pop,
        glow: Some(GlowConfig {
            enabled: true,
            color_hex: "#00E5FF".to_string(),
            radius: 8.0,
            intensity: 0.9,
        }),
        ..Default::default()
    };
    let tl = sample_timeline("Pixel identical preview and export", style);

    let mut cache_preview = RenderCache::default();
    let mut cache_export = RenderCache::default();

    for t_ms in [0, 80, 180, 500, 900] {
        let preview = render_frame_owned(&tl, t_ms, 1280, 720, &fonts, &mut cache_preview)
            .expect("preview render");
        let export = render_frame_owned(&tl, t_ms, 1280, 720, &fonts, &mut cache_export)
            .expect("export render");
        assert_eq!(preview, export, "pixel mismatch at t_ms={t_ms}");
    }
}

#[test]
fn all_animations_strokes_and_effects_render_cleanly() {
    let fonts = FontRegistry::new();
    let mut cache = RenderCache::default();

    // Verify empty buffer outside cue range.
    let tl_empty = sample_timeline("Test", RenderStyle::default());
    let blank = render_frame(&tl_empty, 99_999, 640, 360, &fonts, &mut cache).expect("blank");
    assert_eq!(count_non_transparent(blank), 0);

    for anim in [
        AnimationIn::None,
        AnimationIn::Fade,
        AnimationIn::Pop,
        AnimationIn::SlideUp,
        AnimationIn::Zoom,
        AnimationIn::Typewriter,
        AnimationIn::WordReveal,
        AnimationIn::Bounce,
        AnimationIn::BlurIn,
    ] {
        let style = RenderStyle {
            anim_in: anim,
            anim_out: AnimationOut::SlideDown,
            easing: EasingKind::Spring,
            karaoke_mode: KaraokeMode::ProgressiveFill,
            background_box_on: true,
            shadow: Some(ShadowConfig {
                enabled: true,
                color_hex: "#000000".into(),
                offset_x: 3.0,
                offset_y: 3.0,
                blur: 4.0,
            }),
            gradient: Some(GradientConfig {
                enabled: true,
                start_hex: "#FF7700".into(),
                end_hex: "#FFE600".into(),
                angle_deg: 45.0,
            }),
            ..Default::default()
        };
        let tl = sample_timeline("One two three four", style);
        let frame = render_frame(&tl, 150, 640, 360, &fonts, &mut cache).expect("anim frame");
        assert!(count_non_transparent(frame) > 100, "anim {anim:?} empty");
    }

    for stroke in [StrokePosition::Outside, StrokePosition::Inside, StrokePosition::Center] {
        let style = RenderStyle {
            anim_in: AnimationIn::None,
            outline_width_px: 4.0,
            stroke_position: stroke,
            ..Default::default()
        };
        let tl = sample_timeline("Stroke test", style);
        let frame = render_frame(&tl, 400, 640, 360, &fonts, &mut cache).expect("stroke frame");
        assert!(count_non_transparent(frame) > 200, "stroke {stroke:?} empty");
    }
}

#[test]
fn presets_roundtrip_json() {
    let presets = builtin_presets();
    assert_eq!(presets.len(), 5);
    let tmp = std::env::temp_dir().join("subtitle_studio_preset_test.json");
    save_preset_json(&tmp, &presets[4]).expect("save preset");
    let loaded = load_preset_json(&tmp).expect("load preset");
    assert_eq!(loaded, presets[4]);
    let _ = std::fs::remove_file(tmp);
}
