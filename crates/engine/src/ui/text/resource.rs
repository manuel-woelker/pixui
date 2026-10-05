//! Self-contained, immutable text resources crossing the worker/GUI boundary.

use crate::ui::geometry::Point;
use crate::ui::{resource::Resource, resource_table::ResourceIndex};
use pixui_base::{PixuiResult, pixui_error};
use std::collections::HashMap;

/// A font index is local to one display list, just like its image indices.
pub type FontIndex = ResourceIndex<FontResource>;
pub type Font = Resource<FontResource>;
pub type GlyphAtlas = Resource<GlyphAtlasData>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Logical pixels, with positive ascent, negative descent, and positive spacing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub line_height: f32,
}

impl FontMetrics {
    /// First baseline that centers a single line's ascent/descent in a row.
    pub fn centered_baseline(self, height: f32) -> f32 {
        (height - (self.ascent - self.descent)) / 2.0 + self.ascent
    }
}

/// Logical advance and downward-positive offset from the baseline to the
/// bitmap's top left. Spaces can advance without owning any atlas pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphInfo {
    pub atlas_rect: Option<PixelRect>,
    pub advance: f32,
    pub offset: Point,
}

#[derive(PartialEq)]
pub struct GlyphAtlasData {
    width: u32,
    height: u32,
    coverage: Vec<u8>,
}

impl std::fmt::Debug for GlyphAtlasData {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GlyphAtlas")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("coverage_bytes", &self.coverage.len())
            .finish()
    }
}

impl GlyphAtlas {
    /// Coverage is row-major, 0 transparent and 255 fully covered. Dimensions
    /// are bounded to 2048 each; invalid data is rejected before publication.
    pub fn new(width: u32, height: u32, coverage: Vec<u8>) -> PixuiResult<Self> {
        if width == 0
            || height == 0
            || width > 2048
            || height > 2048
            || u64::from(width) * u64::from(height) != coverage.len() as u64
        {
            return Err(pixui_error!(
                "invalid glyph atlas dimensions or coverage length"
            ));
        }
        Ok(Self::from_value(GlyphAtlasData {
            width,
            height,
            coverage,
        }))
    }
}

impl GlyphAtlasData {
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn coverage(&self) -> &[u8] {
        &self.coverage
    }
}

/// An exact snapshot: map, metrics, and coverage always travel together. No
/// parser or allocator lives here. Retained outputs survive repacking/eviction.
#[derive(Debug, PartialEq)]
pub struct FontResource {
    atlas: GlyphAtlas,
    characters: HashMap<char, GlyphInfo>,
    metrics: FontMetrics,
    scale: f32,
}

impl FontResource {
    pub fn new(
        atlas: GlyphAtlas,
        characters: HashMap<char, GlyphInfo>,
        metrics: FontMetrics,
        scale: f32,
    ) -> PixuiResult<Self> {
        if !scale.is_finite()
            || !(0.1..=16.0).contains(&scale)
            || ![metrics.ascent, metrics.descent, metrics.line_height]
                .iter()
                .all(|v| v.is_finite())
            || metrics.line_height <= 0.0
            || metrics.ascent < metrics.descent
            || characters.len() > 4096
        {
            return Err(pixui_error!(
                "invalid font resource metrics or character limit"
            ));
        }
        for glyph in characters.values() {
            if !glyph.advance.is_finite()
                || !glyph.offset.x.is_finite()
                || !glyph.offset.y.is_finite()
            {
                return Err(pixui_error!("invalid glyph metrics"));
            }
            if let Some(rect) = glyph.atlas_rect
                && (rect.width == 0
                    || rect.height == 0
                    || u64::from(rect.x) + u64::from(rect.width) > u64::from(atlas.width)
                    || u64::from(rect.y) + u64::from(rect.height) > u64::from(atlas.height))
            {
                return Err(pixui_error!("glyph rectangle outside atlas"));
            }
        }
        Ok(Self {
            atlas,
            characters,
            metrics,
            scale,
        })
    }
    pub fn atlas(&self) -> &GlyphAtlas {
        &self.atlas
    }
    pub fn characters(&self) -> &HashMap<char, GlyphInfo> {
        &self.characters
    }
    pub fn glyph(&self, character: char) -> Option<&GlyphInfo> {
        self.characters.get(&character)
    }
    pub fn metrics(&self) -> FontMetrics {
        self.metrics
    }
    pub fn scale(&self) -> f32 {
        self.scale
    }
}
