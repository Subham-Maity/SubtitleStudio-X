//! Deterministic Skia CPU subtitle frame renderer (`render_frame`).

use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

thread_local! {
    static THREAD_FONT_CACHE: RefCell<Option<(u64, FontCollection)>> = const { RefCell::new(None) };
}

fn cached_font_collection(font_registry: &FontRegistry) -> FontCollection {
    let fp = font_registry.fingerprint();
    THREAD_FONT_CACHE.with(|cell| {
        let mut guard = cell.borrow_mut();
        if let Some((cached_fp, ref col)) = *guard
            && cached_fp == fp
        {
            return col.clone();
        }
        let col = font_registry.build_collection();
        *guard = Some((fp, col.clone()));
        col
    })
}

use skia_safe::textlayout::{
    FontCollection, Paragraph, ParagraphBuilder, ParagraphStyle, TextAlign, TextStyle,
};
use skia_safe::{
    AlphaType, BlendMode, Color, Color4f, ColorSpace, ColorType, FontStyle, Image, ImageInfo,
    Paint, PaintStyle, Point, RRect, Rect, Surface, TileMode, font_style, image_filters,
    surfaces,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::color::parse_hex_color;
use crate::easing::{bounce_out, evaluate_easing};
use crate::error::RenderError;
use crate::fonts::FontRegistry;
use crate::timeline::{ResolvedToken, active_token_index, find_active_cue, resolve_cue_tokens};
use crate::types::{
    AnimationIn, AnimationOut, KaraokeMode, RenderCue, RenderStyle, RenderTimeline,
    StrokePosition,
};
use crate::wysiwyg_scale;

/// Reusable scratch/cache buffer across frames during preview and export.
#[derive(Default)]
pub struct RenderCache {
    transparent_w: u32,
    transparent_h: u32,
    transparent_buf: Vec<u8>,
    last_key: Option<u64>,
    last_w: u32,
    last_h: u32,
    last_buf: Vec<u8>,
}

impl RenderCache {
    /// Allocate or return the shared zeroed RGBA buffer for `out_w x out_h`.
    fn transparent(&mut self, out_w: u32, out_h: u32) -> &[u8] {
        let len = (out_w as usize) * (out_h as usize) * 4;
        if self.transparent_w != out_w || self.transparent_h != out_h || self.transparent_buf.len() != len {
            self.transparent_buf = vec![0u8; len];
            self.transparent_w = out_w;
            self.transparent_h = out_h;
        }
        &self.transparent_buf
    }
}

/// Pure, deterministic subtitle overlay renderer for both live preview and FFmpeg export.
///
/// Output format is straight (unpremultiplied) RGBA8888 (`AlphaType::Unpremul`).
pub fn render_frame<'a>(
    timeline: &RenderTimeline,
    t_ms: i64,
    out_w: u32,
    out_h: u32,
    font_registry: &FontRegistry,
    scratch: &'a mut RenderCache,
) -> Result<&'a [u8], RenderError> {
    if out_w == 0 || out_h == 0 {
        return Err(RenderError::InvalidDimensions {
            width: out_w,
            height: out_h,
        });
    }

    let Some(cue) = find_active_cue(timeline, t_ms) else {
        return Ok(scratch.transparent(out_w, out_h));
    };

    let style = &timeline.style;
    let tokens = resolve_cue_tokens(cue, &timeline.words, style.uppercase);
    if tokens.is_empty() {
        return Ok(scratch.transparent(out_w, out_h));
    }

    let anim_state = compute_anim_state(cue, style, t_ms, &tokens);
    if anim_state.alpha <= 0.001 {
        return Ok(scratch.transparent(out_w, out_h));
    }

    let active_tok = active_token_index(&tokens, t_ms);
    let active_tok_progress = compute_active_token_progress(&tokens, active_tok, t_ms, style.karaoke_mode);

    let key = compute_frame_key(
        cue,
        style,
        &anim_state,
        active_tok,
        active_tok_progress,
        out_w,
        out_h,
        font_registry.fingerprint(),
    );

    if scratch.last_key == Some(key)
        && scratch.last_w == out_w
        && scratch.last_h == out_h
        && scratch.last_buf.len() == (out_w as usize) * (out_h as usize) * 4
    {
        return Ok(&scratch.last_buf);
    }

    let buf = render_cue_pixels(
        cue,
        style,
        &tokens,
        &anim_state,
        active_tok,
        active_tok_progress,
        out_w,
        out_h,
        font_registry,
    )?;

    scratch.last_key = Some(key);
    scratch.last_w = out_w;
    scratch.last_h = out_h;
    scratch.last_buf = buf;
    Ok(&scratch.last_buf)
}

/// Owned convenience wrapper around [`render_frame`].
pub fn render_frame_owned(
    timeline: &RenderTimeline,
    t_ms: i64,
    out_w: u32,
    out_h: u32,
    font_registry: &FontRegistry,
    scratch: &mut RenderCache,
) -> Result<Vec<u8>, RenderError> {
    Ok(render_frame(timeline, t_ms, out_w, out_h, font_registry, scratch)?.to_vec())
}

#[derive(Debug, Clone)]
struct AnimState {
    alpha: f32,
    scale_x: f32,
    scale_y: f32,
    offset_y_px: f32,
    blur_sigma: f32,
    visible_graphemes: Option<usize>,
    visible_words: Option<usize>,
    quantized_key: u64,
}

fn compute_anim_state(
    cue: &RenderCue,
    style: &RenderStyle,
    t_ms: i64,
    tokens: &[ResolvedToken],
) -> AnimState {
    let scale = wysiwyg_scale(540);
    let _ = scale;
    let dur = style.anim_duration_ms.max(1);
    let elapsed = (t_ms - cue.start_ms).max(0);
    let raw_in = (elapsed as f32 / dur as f32).clamp(0.0, 1.0);
    let eased_in = evaluate_easing(style.easing, raw_in);

    let mut alpha = 1.0_f32;
    let mut sx = 1.0_f32;
    let mut sy = 1.0_f32;
    let mut off_y_ref = 0.0_f32; // in 540p reference px
    let mut blur_ref = 0.0_f32;
    let mut visible_graphemes = None;
    let mut visible_words = None;

    match style.anim_in {
        AnimationIn::None => {}
        AnimationIn::Fade => {
            alpha = (raw_in * 2.0).min(1.0);
        }
        AnimationIn::Pop => {
            // Mirrors reference 1.2 overshoot profile over 300ms.
            if raw_in < 0.27 {
                let s = (raw_in / 0.27) * 1.2;
                sx = s;
                sy = s;
            } else if raw_in < 0.67 {
                let s = 1.2 - ((raw_in - 0.27) / 0.4) * 0.2;
                sx = s;
                sy = s;
            }
            alpha = (raw_in * 4.0).min(1.0);
        }
        AnimationIn::SlideUp => {
            let p = evaluate_easing(style.easing, (raw_in * 2.5).min(1.0));
            off_y_ref = (1.0 - p) * 60.0;
            alpha = (raw_in * 3.0).min(1.0);
        }
        AnimationIn::Zoom => {
            let p = eased_in;
            sx = p.max(0.01);
            sy = p.max(0.01);
            alpha = (raw_in * 2.0).min(1.0);
        }
        AnimationIn::Typewriter => {
            let total_graphemes: usize = tokens
                .iter()
                .map(|t| t.text.graphemes(true).count())
                .sum::<usize>()
                + tokens.len().saturating_sub(1);
            let cue_dur = (cue.end_ms - cue.start_ms).max(1);
            let reveal_window = (cue_dur as f32 * 0.85).max(dur as f32);
            let p = (elapsed as f32 / reveal_window).clamp(0.0, 1.0);
            visible_graphemes = Some(((p * total_graphemes as f32).ceil() as usize).max(1));
        }
        AnimationIn::WordReveal => {
            let revealed = tokens
                .iter()
                .filter(|tok| t_ms >= tok.start_ms)
                .count()
                .max(1);
            visible_words = Some(revealed);
        }
        AnimationIn::Bounce => {
            let p = bounce_out(raw_in);
            off_y_ref = (1.0 - p) * -48.0;
            sx = 0.85 + 0.15 * p;
            sy = 0.85 + 0.15 * p;
            alpha = (raw_in * 4.0).min(1.0);
        }
        AnimationIn::BlurIn => {
            blur_ref = (1.0 - eased_in) * 10.0;
            alpha = (eased_in * 1.2).min(1.0);
        }
    }

    // Exit animation over trailing window (clamped to half the cue duration).
    let cue_dur = (cue.end_ms - cue.start_ms).max(1);
    let exit_dur = dur.min(cue_dur / 2).max(1);
    let remaining = (cue.end_ms - t_ms).max(0);
    if remaining < exit_dur {
        let p_out = 1.0 - (remaining as f32 / exit_dur as f32).clamp(0.0, 1.0);
        match style.anim_out {
            AnimationOut::None => {}
            AnimationOut::FadeOut => {
                alpha *= (1.0 - p_out).clamp(0.0, 1.0);
            }
            AnimationOut::SlideDown => {
                alpha *= (1.0 - p_out).clamp(0.0, 1.0);
                off_y_ref += p_out * 45.0;
            }
        }
    }

    let mut hasher = DefaultHasher::new();
    (alpha * 1000.0).round().to_bits().hash(&mut hasher);
    (sx * 1000.0).round().to_bits().hash(&mut hasher);
    (sy * 1000.0).round().to_bits().hash(&mut hasher);
    (off_y_ref * 100.0).round().to_bits().hash(&mut hasher);
    (blur_ref * 100.0).round().to_bits().hash(&mut hasher);
    visible_graphemes.hash(&mut hasher);
    visible_words.hash(&mut hasher);

    AnimState {
        alpha: alpha.clamp(0.0, 1.0),
        scale_x: sx.max(0.01),
        scale_y: sy.max(0.01),
        offset_y_px: off_y_ref,
        blur_sigma: blur_ref,
        visible_graphemes,
        visible_words,
        quantized_key: hasher.finish(),
    }
}

fn compute_active_token_progress(
    tokens: &[ResolvedToken],
    active_idx: Option<usize>,
    t_ms: i64,
    mode: KaraokeMode,
) -> Option<u16> {
    let idx = active_idx?;
    match mode {
        KaraokeMode::None => None,
        KaraokeMode::HighlightCurrentWord => Some(1000),
        KaraokeMode::ProgressiveFill | KaraokeMode::PopCurrent => {
            let tok = tokens.get(idx)?;
            let dur = (tok.end_ms - tok.start_ms).max(1) as f32;
            let p = ((t_ms - tok.start_ms) as f32 / dur).clamp(0.0, 1.0);
            Some((p * 100.0).round() as u16)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn compute_frame_key(
    cue: &RenderCue,
    style: &RenderStyle,
    anim: &AnimState,
    active_tok: Option<usize>,
    active_tok_progress: Option<u16>,
    out_w: u32,
    out_h: u32,
    font_fingerprint: u64,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    cue.id.hash(&mut hasher);
    cue.text.hash(&mut hasher);
    cue.color_hex.hash(&mut hasher);
    anim.quantized_key.hash(&mut hasher);
    active_tok.hash(&mut hasher);
    active_tok_progress.hash(&mut hasher);
    out_w.hash(&mut hasher);
    out_h.hash(&mut hasher);
    font_fingerprint.hash(&mut hasher);
    if let Ok(style_json) = serde_json::to_string(style) {
        style_json.hash(&mut hasher);
    }
    hasher.finish()
}

#[derive(Debug, Clone, Copy)]
struct LineBox {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
}

#[allow(clippy::too_many_arguments)]
fn render_cue_pixels(
    cue: &RenderCue,
    style: &RenderStyle,
    tokens: &[ResolvedToken],
    anim: &AnimState,
    active_tok: Option<usize>,
    active_tok_progress: Option<u16>,
    out_w: u32,
    out_h: u32,
    font_registry: &FontRegistry,
) -> Result<Vec<u8>, RenderError> {
    let mut main_surface =
        surfaces::raster_n32_premul((out_w as i32, out_h as i32)).ok_or(
            RenderError::SurfaceAllocation {
                width: out_w,
                height: out_h,
            },
        )?;

    let scale = wysiwyg_scale(out_h);
    let font_size = (style.font_size_px * scale).max(4.0);
    let outline_w = (style.outline_width_px * scale).max(0.0);
    let max_w = (out_w as f32 * (style.max_width_pct.clamp(20.0, 100.0) / 100.0)).max(40.0);

    let families = font_registry.resolve_families(&style.font_file_name);
    let collection = cached_font_collection(font_registry);
    let base_color_hex = cue
        .color_hex
        .as_deref()
        .unwrap_or(&style.font_color_hex);

    // Filter tokens if typewriter / word-reveal is active.
    let display_tokens = slice_visible_tokens(tokens, anim);
    if display_tokens.is_empty() {
        return read_unpremul_rgba(&mut main_surface, out_w, out_h);
    }

    // Build fill paragraph to measure layout & line metrics cleanly.
    let mut fill_para = build_styled_paragraph(
        &display_tokens,
        style,
        base_color_hex,
        active_tok,
        active_tok_progress,
        &families,
        &collection,
        font_size,
        scale,
        None,
        None,
    );
    fill_para.layout(max_w);

    let para_h = fill_para.height().max(font_size * style.line_height);
    let pad_x = style.box_padding_x * scale;
    let pad_y = style.box_padding_y * scale;
    let (line_boxes, max_line_w) = {
        let metrics = fill_para.get_line_metrics();
        let max_lw = metrics
            .iter()
            .map(|lm| lm.width as f32)
            .fold(0.0_f32, f32::max)
            .max(1.0);
        let boxes: Vec<LineBox> = metrics
            .iter()
            .map(|lm| LineBox {
                left: lm.left as f32 - pad_x,
                right: (lm.left + lm.width) as f32 + pad_x,
                top: (lm.baseline - lm.ascent) as f32 - pad_y,
                bottom: (lm.baseline + lm.descent) as f32 + pad_y,
            })
            .collect();
        (boxes, max_lw)
    };

    // Rebuild fill paragraph with gradient shader across actual measured bounds if enabled.
    let grad_shader = style.gradient.as_ref().and_then(|g| {
        if !g.enabled {
            return None;
        }
        let rad = g.angle_deg.to_radians();
        let half_w = max_line_w * 0.5;
        let half_h = para_h * 0.5;
        let dx = rad.cos() * half_w;
        let dy = rad.sin() * half_h;
        let c1 = Color4f::from(parse_hex_color(&g.start_hex, 1.0));
        let c2 = Color4f::from(parse_hex_color(&g.end_hex, 1.0));
        let colors = [c1, c2];
        let grad_colors = skia_safe::gradient::Colors::new(
            &colors[..],
            None,
            TileMode::Clamp,
            None,
        );
        let grad = skia_safe::gradient::Gradient::new(
            grad_colors,
            skia_safe::gradient::Interpolation::default(),
        );
        skia_safe::gradient::shaders::linear_gradient(
            (
                Point::new(max_w * 0.5 - dx, half_h - dy),
                Point::new(max_w * 0.5 + dx, half_h + dy),
            ),
            &grad,
            None,
        )
    });

    if let Some(ref shader) = grad_shader {
        fill_para = build_styled_paragraph(
            &display_tokens,
            style,
            base_color_hex,
            active_tok,
            active_tok_progress,
            &families,
            &collection,
            font_size,
            scale,
            None,
            Some(shader),
        );
        fill_para.layout(max_w);
    }

    // Render fill + clip/mask stroke into dedicated layer buffers so effects compose accurately.
    let pad = (out_h as i32 / 12).max(24);
    let fill_img = raster_paragraph_image(&fill_para, max_w, para_h, out_w, out_h, pad)?;
    let text_composite = if outline_w > 0.05 {
        let stroke_w = match style.stroke_position {
            StrokePosition::Center => outline_w,
            StrokePosition::Outside | StrokePosition::Inside => outline_w * 2.0,
        };
        let mut stroke_paint = Paint::default();
        stroke_paint.set_anti_alias(true);
        stroke_paint.set_style(PaintStyle::Stroke);
        stroke_paint.set_stroke_width(stroke_w);
        stroke_paint.set_stroke_join(skia_safe::paint::Join::Round);
        stroke_paint.set_stroke_cap(skia_safe::paint::Cap::Round);
        stroke_paint.set_color(parse_hex_color(&style.outline_color_hex, 1.0));

        let mut stroke_para = build_styled_paragraph(
            &display_tokens,
            style,
            base_color_hex,
            active_tok,
            active_tok_progress,
            &families,
            &collection,
            font_size,
            scale,
            Some(&stroke_paint),
            None,
        );
        stroke_para.layout(max_w);
        let stroke_img = raster_paragraph_image(&stroke_para, max_w, para_h, out_w, out_h, pad)?;

        compose_stroke_and_fill(&fill_img, &stroke_img, style.stroke_position, out_w, out_h)?
    } else {
        fill_img.clone()
    };

    // Position and transform on the main output surface.
    let pos_x = (style.position_x / 100.0) * out_w as f32;
    let pos_y = (style.position_y / 100.0) * out_h as f32 + anim.offset_y_px * scale;
    let img_origin = (-pad as f32, -pad as f32);

    let canvas = main_surface.canvas();
    canvas.clear(Color::TRANSPARENT);
    canvas.save();

    if anim.alpha < 0.999 || anim.blur_sigma > 0.05 {
        let mut layer_paint = Paint::default();
        layer_paint.set_alpha_f(anim.alpha);
        if anim.blur_sigma > 0.05 {
            let sigma = anim.blur_sigma * scale;
            if let Some(blur_filter) = image_filters::blur((sigma, sigma), TileMode::Decal, None, None) {
                layer_paint.set_image_filter(blur_filter);
            }
        }
        canvas.save_layer(&skia_safe::canvas::SaveLayerRec::default().paint(&layer_paint));
    }

    canvas.translate((pos_x, pos_y));
    if style.rotation_deg.abs() > 0.01 {
        canvas.rotate(style.rotation_deg, None);
    }
    canvas.scale((anim.scale_x, anim.scale_y));
    canvas.translate((-max_w * 0.5, -para_h * 0.5));

    // Background box per line.
    if style.background_box_on {
        draw_background_boxes(canvas, &line_boxes, style, scale);
    }

    // Drop shadow behind text.
    if let Some(ref shadow) = style.shadow
        && shadow.enabled
        && let Some(filter) = image_filters::drop_shadow_only(
            (shadow.offset_x * scale, shadow.offset_y * scale),
            (shadow.blur.max(0.1) * scale, shadow.blur.max(0.1) * scale),
            Color4f::from(parse_hex_color(&shadow.color_hex, 1.0)),
            None,
            None,
            None,
        )
    {
        let mut shadow_paint = Paint::default();
        shadow_paint.set_image_filter(filter);
        canvas.draw_image(&text_composite, img_origin, Some(&shadow_paint));
    }

    // Neon glow halo behind text.
    if let Some(ref glow) = style.glow
        && glow.enabled
    {
        let sigma = (glow.radius.max(1.0) * scale) * 0.6;
        let glow_color = parse_hex_color(&glow.color_hex, glow.intensity.clamp(0.1, 1.0));
        if let Some(cf) = skia_safe::color_filters::blend(glow_color, BlendMode::SrcIn)
            && let Some(tinted) = image_filters::color_filter(cf, None, None)
            && let Some(blurred) = image_filters::blur((sigma, sigma), TileMode::Decal, tinted, None)
        {
            let mut glow_paint = Paint::default();
            glow_paint.set_image_filter(blurred);
            canvas.draw_image(&fill_img, img_origin, Some(&glow_paint));
            canvas.draw_image(&fill_img, img_origin, Some(&glow_paint));
        }
    }

    // Final crisp stroke+fill composite.
    canvas.draw_image(&text_composite, img_origin, None);

    if anim.alpha < 0.999 || anim.blur_sigma > 0.05 {
        canvas.restore();
    }
    canvas.restore();

    read_unpremul_rgba(&mut main_surface, out_w, out_h)
}

fn slice_visible_tokens(tokens: &[ResolvedToken], anim: &AnimState) -> Vec<ResolvedToken> {
    if let Some(limit_words) = anim.visible_words {
        return tokens.iter().take(limit_words).cloned().collect();
    }
    if let Some(limit_graphemes) = anim.visible_graphemes {
        let mut remaining = limit_graphemes;
        let mut out = Vec::new();
        for (idx, tok) in tokens.iter().enumerate() {
            if remaining == 0 {
                break;
            }
            if idx > 0 {
                remaining = remaining.saturating_sub(1);
                if remaining == 0 {
                    break;
                }
            }
            let graphemes: Vec<&str> = tok.text.graphemes(true).collect();
            if graphemes.len() <= remaining {
                remaining -= graphemes.len();
                out.push(tok.clone());
            } else {
                let partial: String = graphemes.into_iter().take(remaining).collect();
                out.push(ResolvedToken {
                    text: partial,
                    start_ms: tok.start_ms,
                    end_ms: tok.end_ms,
                    word_index: tok.word_index,
                });
                break;
            }
        }
        return out;
    }
    tokens.to_vec()
}

#[allow(clippy::too_many_arguments)]
fn build_styled_paragraph(
    tokens: &[ResolvedToken],
    style: &RenderStyle,
    base_color_hex: &str,
    active_tok: Option<usize>,
    active_tok_progress: Option<u16>,
    families: &[String],
    collection: &FontCollection,
    font_size: f32,
    scale: f32,
    stroke_paint: Option<&Paint>,
    gradient_shader: Option<&skia_safe::Shader>,
) -> Paragraph {
    let mut para_style = ParagraphStyle::new();
    para_style.set_text_align(TextAlign::Center);

    let slant = if style.italic {
        font_style::Slant::Italic
    } else {
        font_style::Slant::Upright
    };
    let weight = if style.bold {
        font_style::Weight::BOLD
    } else {
        font_style::Weight::NORMAL
    };
    let skia_font_style = FontStyle::new(weight, font_style::Width::NORMAL, slant);

    let mut builder = ParagraphBuilder::new(&para_style, collection);
    let family_refs: Vec<&str> = families.iter().map(String::as_str).collect();

    for (idx, tok) in tokens.iter().enumerate() {
        let is_active = active_tok == Some(idx);
        let is_past = active_tok.is_some_and(|a| idx < a);

        let mut word_color = parse_hex_color(base_color_hex, 1.0);
        let mut word_scale = 1.0_f32;

        if is_active {
            match style.karaoke_mode {
                KaraokeMode::None => {}
                KaraokeMode::HighlightCurrentWord => {
                    word_color = parse_hex_color(&style.karaoke_color_hex, 1.0);
                    word_scale = style.karaoke_scale.max(1.0);
                }
                KaraokeMode::ProgressiveFill => {
                    let p = f32::from(active_tok_progress.unwrap_or(100)) / 100.0;
                    word_color = if p >= 0.35 {
                        parse_hex_color(&style.karaoke_color_hex, 1.0)
                    } else {
                        parse_hex_color(base_color_hex, 1.0)
                    };
                    word_scale = 1.0 + (style.karaoke_scale - 1.0) * p;
                }
                KaraokeMode::PopCurrent => {
                    let p = f32::from(active_tok_progress.unwrap_or(50)) / 100.0;
                    let bump = (p * std::f32::consts::PI).sin();
                    word_color = parse_hex_color(&style.karaoke_color_hex, 1.0);
                    word_scale = 1.0 + (style.karaoke_scale - 1.0) * bump;
                }
            }
        } else if is_past && style.karaoke_mode == KaraokeMode::ProgressiveFill {
            word_color = parse_hex_color(&style.karaoke_color_hex, 1.0);
        }

        let mut ts = TextStyle::new();
        ts.set_font_families(&family_refs);
        ts.set_font_style(skia_font_style);
        ts.set_font_size(font_size * word_scale);
        ts.set_height(style.line_height);
        ts.set_height_override(true);
        if style.letter_spacing.abs() > 0.01 {
            ts.set_letter_spacing(style.letter_spacing * scale);
        }

        if let Some(sp) = stroke_paint {
            ts.set_foreground_paint(sp);
        } else if let Some(shader) = gradient_shader
            && !is_active
        {
            let mut gp = Paint::default();
            gp.set_anti_alias(true);
            gp.set_shader(shader.clone());
            ts.set_foreground_paint(&gp);
        } else {
            ts.set_color(word_color);
        }

        builder.push_style(&ts);
        builder.add_text(&tok.text);
        builder.pop();

        if idx + 1 < tokens.len() {
            let mut space_ts = TextStyle::new();
            space_ts.set_font_families(&family_refs);
            space_ts.set_font_style(skia_font_style);
            space_ts.set_font_size(font_size);
            space_ts.set_height(style.line_height);
            space_ts.set_height_override(true);
            if let Some(sp) = stroke_paint {
                space_ts.set_foreground_paint(sp);
            } else {
                space_ts.set_color(parse_hex_color(base_color_hex, 1.0));
            }
            builder.push_style(&space_ts);
            builder.add_text(" ");
            builder.pop();
        }
    }

    builder.build()
}

fn raster_paragraph_image(
    paragraph: &Paragraph,
    para_w: f32,
    para_h: f32,
    canvas_w: u32,
    canvas_h: u32,
    pad: i32,
) -> Result<Image, RenderError> {
    let w = (para_w.ceil() as i32).max(1).min(canvas_w as i32 * 2);
    let h = (para_h.ceil() as i32).max(1).min(canvas_h as i32 * 2);
    // Add symmetric padding so outside strokes and glow never clip at surface edges.
    let mut surf = surfaces::raster_n32_premul((w + pad * 2, h + pad * 2)).ok_or(
        RenderError::SurfaceAllocation {
            width: (w + pad * 2) as u32,
            height: (h + pad * 2) as u32,
        },
    )?;
    let c = surf.canvas();
    c.clear(Color::TRANSPARENT);
    paragraph.paint(c, (pad as f32, pad as f32));
    Ok(surf.image_snapshot())
}

fn compose_stroke_and_fill(
    fill_img: &Image,
    stroke_img: &Image,
    pos: StrokePosition,
    out_w: u32,
    out_h: u32,
) -> Result<Image, RenderError> {
    let img_w = fill_img.width();
    let img_h = fill_img.height();
    let mut surf = surfaces::raster_n32_premul((img_w, img_h)).ok_or(
        RenderError::SurfaceAllocation {
            width: out_w,
            height: out_h,
        },
    )?;
    let c = surf.canvas();
    c.clear(Color::TRANSPARENT);

    match pos {
        StrokePosition::Outside => {
            // Draw 2x stroke, subtract exact fill alpha via DstOut, then paint fill on top.
            c.draw_image(stroke_img, (0.0, 0.0), None);
            let mut cut_paint = Paint::default();
            cut_paint.set_blend_mode(BlendMode::DstOut);
            c.draw_image(fill_img, (0.0, 0.0), Some(&cut_paint));
            c.draw_image(fill_img, (0.0, 0.0), None);
        }
        StrokePosition::Inside => {
            // Mask stroke strictly inside fill alpha via DstIn, then composite over fill.
            let mut stroke_surf = surfaces::raster_n32_premul((img_w, img_h)).ok_or(
                RenderError::SurfaceAllocation {
                    width: out_w,
                    height: out_h,
                },
            )?;
            let sc = stroke_surf.canvas();
            sc.clear(Color::TRANSPARENT);
            sc.draw_image(stroke_img, (0.0, 0.0), None);
            let mut mask_paint = Paint::default();
            mask_paint.set_blend_mode(BlendMode::DstIn);
            sc.draw_image(fill_img, (0.0, 0.0), Some(&mask_paint));
            let clipped_stroke = stroke_surf.image_snapshot();

            c.draw_image(fill_img, (0.0, 0.0), None);
            c.draw_image(&clipped_stroke, (0.0, 0.0), None);
        }
        StrokePosition::Center => {
            // Paint fill then centered stroke of exact outline_w.
            c.draw_image(fill_img, (0.0, 0.0), None);
            c.draw_image(stroke_img, (0.0, 0.0), None);
        }
    }

    Ok(surf.image_snapshot())
}

fn draw_background_boxes(
    canvas: &skia_safe::Canvas,
    line_boxes: &[LineBox],
    style: &RenderStyle,
    scale: f32,
) {
    let mut box_paint = Paint::default();
    box_paint.set_anti_alias(true);
    box_paint.set_color(parse_hex_color(
        &style.background_color_hex,
        style.background_opacity,
    ));

    let radius = style.box_radius * scale;

    for lb in line_boxes {
        let rect = Rect::from_ltrb(lb.left, lb.top, lb.right, lb.bottom);
        let rrect = RRect::new_rect_xy(rect, radius, radius);
        canvas.draw_rrect(rrect, &box_paint);
    }
}

fn read_unpremul_rgba(
    surface: &mut Surface,
    out_w: u32,
    out_h: u32,
) -> Result<Vec<u8>, RenderError> {
    let info = ImageInfo::new(
        (out_w as i32, out_h as i32),
        ColorType::RGBA8888,
        AlphaType::Unpremul,
        ColorSpace::new_srgb(),
    );
    let row_bytes = (out_w as usize) * 4;
    let mut pixels = vec![0u8; row_bytes * (out_h as usize)];
    let ok = surface.read_pixels(&info, &mut pixels, row_bytes, (0, 0));
    if !ok {
        return Err(RenderError::PixelReadback);
    }
    Ok(pixels)
}
