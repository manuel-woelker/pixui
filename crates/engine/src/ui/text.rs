//! Fixed-cell text metrics and glyphs shared by the worker and GUI painter.
//! Supports basic Latin and Latin extensions. Unsupported characters use `?`.
//! Complex shaping, bidi, kerning, and font fallback are not implemented.

use font8x8::UnicodeFonts;

pub fn glyph(character: char) -> [u8; 8] {
    font8x8::BASIC_FONTS
        .get(character)
        .or_else(|| font8x8::LATIN_FONTS.get(character))
        .unwrap_or_else(|| font8x8::BASIC_FONTS.get('?').expect("builtin fallback"))
}

/// Character wrapping with fixed cell width; explicit newlines are preserved.
/// A positive size is the glyph cell height; line spacing is 25% of that height.
pub fn wrap(text: &str, size: f32, width: f32) -> Vec<String> {
    let columns = (width / size).floor().max(1.0) as usize;
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        let mut count = 0;
        for character in paragraph.chars() {
            if count == columns {
                lines.push(std::mem::take(&mut line));
                count = 0;
            }
            line.push(character);
            count += 1;
        }
        lines.push(line);
    }
    lines
}

pub fn line_height(size: f32) -> f32 {
    size * 1.25
}
