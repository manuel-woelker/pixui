//! Translation from fontdue's upward-positive bitmap bearings to our baseline.

use super::resource::{FontMetrics, GlyphInfo};
use crate::ui::geometry::Point;
use pixui_base::{PixuiResult, pixui_error};

pub(crate) struct Rasterizer {
    font: fontdue::Font,
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
        if font.horizontal_line_metrics(16.0).is_none() {
            return Err(pixui_error!("font has no horizontal line metrics"));
        }
        Ok(Self { font })
    }
    pub fn line_metrics(&self, size: f32, scale: f32) -> FontMetrics {
        let metrics = self
            .font
            .horizontal_line_metrics(size * scale)
            .expect("validated line metrics");
        FontMetrics {
            ascent: metrics.ascent / scale,
            descent: metrics.descent / scale,
            line_height: metrics.new_line_size / scale,
        }
    }
    /// Unsupported characters alias one replacement, including non-Latin glyphs
    /// present in the source face. This is deliberately not a fallback chain.
    pub fn canonical(&self, character: char) -> char {
        if (('\u{20}'..='\u{24f}').contains(&character) || character == '\u{fffd}')
            && self.font.has_glyph(character)
        {
            character
        } else if self.font.has_glyph('\u{fffd}') {
            '\u{fffd}'
        } else {
            '?'
        }
    }
    pub fn advance(&self, character: char, size: f32, scale: f32) -> f32 {
        self.font
            .metrics(self.canonical(character), size * scale)
            .advance_width
            / scale
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
