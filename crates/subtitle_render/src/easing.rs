//! Closed-form, deterministic easing functions for subtitle animations.

use crate::types::EasingKind;

/// Evaluate an easing curve at normalized time `t` in `[0.0, 1.0]`.
#[must_use]
pub fn evaluate_easing(kind: EasingKind, t: f32) -> f32 {
    let x = t.clamp(0.0, 1.0);
    match kind {
        EasingKind::Linear => x,
        EasingKind::Cubic => {
            let inv = 1.0 - x;
            1.0 - inv * inv * inv
        }
        EasingKind::Back => {
            const C1: f32 = 1.70158;
            const C3: f32 = C1 + 1.0;
            let xm1 = x - 1.0;
            1.0 + C3 * xm1 * xm1 * xm1 + C1 * xm1 * xm1
        }
        EasingKind::Elastic => {
            if x <= 0.0 {
                0.0
            } else if x >= 1.0 {
                1.0
            } else {
                const C4: f32 = (2.0 * std::f32::consts::PI) / 3.0;
                2.0_f32.powf(-10.0 * x) * ((x * 10.0 - 0.75) * C4).sin() + 1.0
            }
        }
        EasingKind::Spring => {
            if x <= 0.0 {
                0.0
            } else if x >= 1.0 {
                1.0
            } else {
                // Under-damped closed-form spring normalized to reach 1.0 cleanly at t=1.
                let decay = (-6.5 * x).exp();
                let raw = 1.0 - decay * ((12.0 * x).cos() + 0.54 * (12.0 * x).sin());
                let end_val = 1.0 - (-6.5_f32).exp() * (12.0_f32.cos() + 0.54 * 12.0_f32.sin());
                raw / end_val
            }
        }
    }
}

/// Evaluate a deterministic bounce-out curve in `[0.0, 1.0]`.
#[must_use]
pub fn bounce_out(t: f32) -> f32 {
    let x = t.clamp(0.0, 1.0);
    const N1: f32 = 7.5625;
    const D1: f32 = 2.75;
    if x < 1.0 / D1 {
        N1 * x * x
    } else if x < 2.0 / D1 {
        let u = x - 1.5 / D1;
        N1 * u * u + 0.75
    } else if x < 2.5 / D1 {
        let u = x - 2.25 / D1;
        N1 * u * u + 0.9375
    } else {
        let u = x - 2.625 / D1;
        N1 * u * u + 0.984375
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easings_respect_endpoints() {
        for kind in [
            EasingKind::Linear,
            EasingKind::Cubic,
            EasingKind::Back,
            EasingKind::Elastic,
            EasingKind::Spring,
        ] {
            assert!((evaluate_easing(kind, 0.0) - 0.0).abs() < 1e-4);
            assert!((evaluate_easing(kind, 1.0) - 1.0).abs() < 1e-4);
        }
        assert!((bounce_out(0.0) - 0.0).abs() < 1e-4);
        assert!((bounce_out(1.0) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn back_and_spring_overshoot_midway() {
        assert!(evaluate_easing(EasingKind::Back, 0.5) > 1.0);
        assert!(evaluate_easing(EasingKind::Spring, 0.3) > 1.0);
    }
}

