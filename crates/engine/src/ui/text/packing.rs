//! Transactional atlas allocation. Clone allocator metadata before changing it;
//! copy coverage once per successful batch, and preserve old snapshots.

use super::{
    rasterizer::RasterGlyph,
    resource::{GlyphAtlas, GlyphAtlasData, GlyphInfo, PixelRect},
};
use pixui_base::{PixuiResult, pixui_error};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone)]
pub(crate) struct Packing {
    allocator: etagere::AtlasAllocator,
    pub glyphs: HashMap<char, GlyphInfo>,
    dimension: u32,
}

impl Packing {
    pub fn new(dimension: u32) -> Self {
        Self {
            allocator: etagere::AtlasAllocator::new(etagere::size2(
                dimension as i32,
                dimension as i32,
            )),
            glyphs: HashMap::new(),
            dimension,
        }
    }

    fn allocate(&mut self, character: char, mut info: GlyphInfo, width: u32, height: u32) -> bool {
        if width != 0 && height != 0 {
            let Some(allocation) = self
                .allocator
                .allocate(etagere::size2(width as i32 + 2, height as i32 + 2))
            else {
                return false;
            };
            info.atlas_rect = Some(PixelRect {
                x: allocation.rectangle.min.x as u32 + 1,
                y: allocation.rectangle.min.y as u32 + 1,
                width,
                height,
            });
        }
        self.glyphs.insert(character, info);
        true
    }

    /// Adds all glyphs or none. Repacking copies existing coverage rather than
    /// rasterizing existing glyphs again. Sorted input makes allocation stable.
    pub fn prepare(
        &self,
        old: Option<&GlyphAtlasData>,
        new: &BTreeMap<char, RasterGlyph>,
        maximum: u32,
    ) -> PixuiResult<(Self, GlyphAtlas)> {
        let mut candidate = Self {
            allocator: self.allocator.clone(),
            glyphs: self.glyphs.clone(),
            dimension: self.dimension,
        };
        let mut fits = new.iter().all(|(&character, glyph)| {
            candidate.allocate(character, glyph.info, glyph.width, glyph.height)
        });
        while !fits {
            let dimension = candidate.dimension * 2;
            if dimension > maximum {
                return Err(pixui_error!(
                    "font glyphs exceed single-atlas limit {maximum}"
                ));
            }
            candidate = Self::new(dimension);
            let mut characters: Vec<_> = self
                .glyphs
                .keys()
                .copied()
                .chain(new.keys().copied())
                .collect();
            characters.sort_unstable();
            fits = characters.into_iter().all(|character| {
                if let Some(glyph) = new.get(&character) {
                    candidate.allocate(character, glyph.info, glyph.width, glyph.height)
                } else {
                    let info = self.glyphs[&character];
                    let (width, height) = info
                        .atlas_rect
                        .map_or((0, 0), |rect| (rect.width, rect.height));
                    candidate.allocate(character, info, width, height)
                }
            });
        }
        let dimension = candidate.dimension as usize;
        let mut pixels = vec![0; dimension * dimension];
        if let Some(old) = old {
            for (&character, glyph) in &self.glyphs {
                if let Some(source) = glyph.atlas_rect {
                    let destination = candidate.glyphs[&character]
                        .atlas_rect
                        .expect("drawable existing glyph");
                    copy_rect(
                        old.coverage(),
                        old.width() as usize,
                        source,
                        &mut pixels,
                        dimension,
                        destination,
                    );
                }
            }
        }
        for (&character, glyph) in new {
            if let Some(destination) = candidate.glyphs[&character].atlas_rect {
                copy_rect(
                    &glyph.coverage,
                    glyph.width as usize,
                    PixelRect {
                        x: 0,
                        y: 0,
                        width: glyph.width,
                        height: glyph.height,
                    },
                    &mut pixels,
                    dimension,
                    destination,
                );
            }
        }
        let atlas = GlyphAtlas::new(candidate.dimension, candidate.dimension, pixels)?;
        Ok((candidate, atlas))
    }
}

fn copy_rect(
    source: &[u8],
    source_width: usize,
    rect: PixelRect,
    destination: &mut [u8],
    destination_width: usize,
    target: PixelRect,
) {
    for row in 0..rect.height as usize {
        let start = (rect.y as usize + row) * source_width + rect.x as usize;
        let output = (target.y as usize + row) * destination_width + target.x as usize;
        destination[output..output + rect.width as usize]
            .copy_from_slice(&source[start..start + rect.width as usize]);
    }
}
