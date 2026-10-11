//! Translation from fontdue's upward-positive bitmap bearings to our baseline.

use super::resource::{FontMetrics, GlyphInfo};
use crate::ui::geometry::Point;
use pixui_base::{PixuiResult, pixui_error};

pub(crate) struct Rasterizer {
    font: fontdue::Font,
    // Immutable face metrics at size 1. Logical metrics scale with font size;
    // DPI affects bitmap rounding, but cancels out of advances and line metrics.
    latin: [MeasuredGlyph; 560],
    replacement: MeasuredGlyph,
    line: FontMetrics,
}

#[cfg(test)]
mod tests {
    use super::Rasterizer;
    use crate::ui::text::font::{FontConfig, FontFace, GEIST_MONO_REGULAR_TTF, GEIST_REGULAR_TTF};

    fn close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 0.0001 * expected.abs().max(1.0),
            "{actual} != {expected}"
        );
    }

    #[test]
    fn cached_metrics_match_rasterizer_at_multiple_sizes_and_scales() {
        for bytes in [GEIST_REGULAR_TTF, GEIST_MONO_REGULAR_TTF] {
            let rasterizer = Rasterizer::new(bytes, 0).unwrap();
            for size in [1.0, 13.5, 32.0] {
                for scale in [0.5, 1.0, 1.25, 2.0, 4.0] {
                    for character in (0x20..=0x24f)
                        .map(|code| char::from_u32(code).unwrap())
                        .chain(['\u{fffd}', '中', '🦀', '\u{10ffff}'])
                    {
                        let expected_character = if (('\u{20}'..='\u{24f}').contains(&character)
                            || character == '\u{fffd}')
                            && rasterizer.font.has_glyph(character)
                        {
                            character
                        } else if rasterizer.font.has_glyph('\u{fffd}') {
                            '\u{fffd}'
                        } else {
                            '?'
                        };
                        assert_eq!(rasterizer.canonical(character), expected_character);
                        close(
                            rasterizer.advance(character, size),
                            rasterizer
                                .font
                                .metrics(expected_character, size * scale)
                                .advance_width
                                / scale,
                        );
                    }
                    let actual = rasterizer.line_metrics(size);
                    let expected = rasterizer
                        .font
                        .horizontal_line_metrics(size * scale)
                        .unwrap();
                    close(actual.ascent, expected.ascent / scale);
                    close(actual.descent, expected.descent / scale);
                    close(actual.line_height, expected.new_line_size / scale);
                }
            }
        }
    }

    #[test]
    fn text_measurement_scales_with_size_and_is_independent_of_dpi() {
        let face = FontFace::geist().unwrap();
        let text = "A\té\r\nWide 🦀";
        let base = FontConfig::new(face.clone(), 12.0, 1.0)
            .unwrap()
            .measure_text(text)
            .unwrap();
        for scale in [0.5, 1.25, 2.0, 4.0] {
            let measured = FontConfig::new(face.clone(), 24.0, scale)
                .unwrap()
                .measure_text(text)
                .unwrap();
            close(measured.width, base.width * 2.0);
            close(measured.height, base.height * 2.0);
        }
    }
}

#[derive(Clone, Copy)]
struct MeasuredGlyph {
    character: char,
    advance: f32,
}

pub(crate) struct RasterGlyph {
    pub info: GlyphInfo,
    pub width: u32,
    pub height: u32,
    pub coverage: Vec<u8>,
}

impl Rasterizer {
    pub fn new(bytes: &[u8], collection_index: u32) -> PixuiResult<Self> {
        let font = fontdue::Font::from_bytes(
            bytes,
            fontdue::FontSettings {
                collection_index,
                ..Default::default()
            },
        )
        .map_err(|error| pixui_error!("invalid font bytes: {error}"))?;
        let metrics = font
            .horizontal_line_metrics(1.0)
            .ok_or_else(|| pixui_error!("font has no horizontal line metrics"))?;
        let replacement_character = if font.has_glyph('\u{fffd}') {
            '\u{fffd}'
        } else {
            '?'
        };
        let replacement = MeasuredGlyph {
            character: replacement_character,
            advance: font.metrics(replacement_character, 1.0).advance_width,
        };
        let latin = std::array::from_fn(|index| {
            let character = char::from_u32(0x20 + index as u32).expect("Latin repertoire");
            if font.has_glyph(character) {
                MeasuredGlyph {
                    character,
                    advance: font.metrics(character, 1.0).advance_width,
                }
            } else {
                replacement
            }
        });
        Ok(Self {
            font,
            latin,
            replacement,
            line: FontMetrics {
                ascent: metrics.ascent,
                descent: metrics.descent,
                line_height: metrics.new_line_size,
            },
        })
    }
    pub fn line_metrics(&self, size: f32) -> FontMetrics {
        FontMetrics {
            ascent: self.line.ascent * size,
            descent: self.line.descent * size,
            line_height: self.line.line_height * size,
        }
    }
    /// Unsupported characters alias one replacement, including non-Latin glyphs
    /// present in the source face. This is deliberately not a fallback chain.
    pub fn canonical(&self, character: char) -> char {
        self.measured_glyph(character).character
    }
    pub fn advance(&self, character: char, size: f32) -> f32 {
        self.measured_glyph(character).advance * size
    }
    fn measured_glyph(&self, character: char) -> MeasuredGlyph {
        if ('\u{20}'..='\u{24f}').contains(&character) {
            self.latin[character as usize - 0x20]
        } else {
            self.replacement
        }
    }
    pub fn dimensions(&self, character: char, size: f32, scale: f32) -> PixuiResult<(u32, u32)> {
        let bounds = self.font.metrics(character, size * scale);
        if bounds.width > 2046 || bounds.height > 2046 {
            return Err(pixui_error!("glyph exceeds maximum atlas dimensions"));
        }
        Ok((bounds.width as u32, bounds.height as u32))
    }
    pub fn rasterize(&self, character: char, size: f32, scale: f32) -> PixuiResult<RasterGlyph> {
        self.dimensions(character, size, scale)?;
        let (metrics, coverage) = self.font.rasterize(character, size * scale);
        Ok(RasterGlyph {
            info: GlyphInfo {
                atlas_rect: None,
                advance: metrics.advance_width / scale,
                offset: Point {
                    x: metrics.xmin as f32 / scale,
                    y: -(metrics.ymin as f32 + metrics.height as f32) / scale,
                },
            },
            width: metrics.width as u32,
            height: metrics.height as u32,
            coverage,
        })
    }
}
