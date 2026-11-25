//! Hex color parsing utilities.

use skia_safe::Color;

/// Parse `#RRGGBB`, `#RGB`, or `#RRGGBBAA` into a Skia [`Color`] with optional opacity multiplier.
#[must_use]
pub fn parse_hex_color(hex: &str, opacity_mul: f32) -> Color {
    let clean = hex.trim().trim_start_matches('#');
    let (r, g, b, a) = match clean.len() {
        3 => {
            let bytes = clean.as_bytes();
            let nibble = |c: u8| -> u8 {
                let v = hex_nibble(c);
                (v << 4) | v
            };
            (nibble(bytes[0]), nibble(bytes[1]), nibble(bytes[2]), 255)
        }
        6 => {
            let r = u8::from_str_radix(&clean[0..2], 16).unwrap_or(255);
            let g = u8::from_str_radix(&clean[2..4], 16).unwrap_or(255);
            let b = u8::from_str_radix(&clean[4..6], 16).unwrap_or(255);
            (r, g, b, 255)
        }
        8 => {
            let r = u8::from_str_radix(&clean[0..2], 16).unwrap_or(255);
            let g = u8::from_str_radix(&clean[2..4], 16).unwrap_or(255);
            let b = u8::from_str_radix(&clean[4..6], 16).unwrap_or(255);
            let a = u8::from_str_radix(&clean[6..8], 16).unwrap_or(255);
            (r, g, b, a)
        }
        _ => (255, 255, 255, 255),
    };

    let final_a = ((f32::from(a) * opacity_mul.clamp(0.0, 1.0)).round() as i32).clamp(0, 255) as u8;
    Color::from_argb(final_a, r, g, b)
}

fn hex_nibble(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => 15,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_short_and_long_hex() {
        assert_eq!(parse_hex_color("#FFE600", 1.0), Color::from_argb(255, 0xFF, 0xE6, 0x00));
        assert_eq!(parse_hex_color("#000", 0.5), Color::from_argb(128, 0, 0, 0));
    }
}

