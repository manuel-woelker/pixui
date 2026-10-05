#![doc = include_str!("Images.md")]

use super::{display_list::Color, resource::Resource};
use pixui_base::{PixuiResult, pixui_error};
use std::fmt;

/// Source pixel ceiling, independent of total application memory usage.
pub const MAX_IMAGE_PIXELS: usize = 64_000_000;

pub struct ImageData {
    width: u32,
    height: u32,
    pixels: Vec<Color>,
    transparent_color: Option<Color>,
}

/// An immutable row-major RGB image with optional exact color-key transparency.
/// Equality compares allocation identity, not pixel content. Each separately
/// constructed image is a new version, even if its pixels match an older image.
pub type Image = Resource<ImageData>;

impl Image {
    /// Consumes the pixels without copying. Rejects zero dimensions, overflow,
    /// incorrect pixel count, and images exceeding MAX_IMAGE_PIXELS.
    pub fn new(
        width: u32,
        height: u32,
        pixels: Vec<Color>,
        transparent_color: Option<Color>,
    ) -> PixuiResult<Self> {
        let count = u64::from(width) * u64::from(height);
        if width == 0
            || height == 0
            || count > MAX_IMAGE_PIXELS as u64
            || count != pixels.len() as u64
        {
            return Err(pixui_error!("invalid image dimensions or pixel count"));
        }
        Ok(Self::from_value(ImageData {
            width,
            height,
            pixels,
            transparent_color,
        }))
    }
}

impl ImageData {
    pub(crate) fn storage_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + self.pixels.capacity() * std::mem::size_of::<Color>()
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn pixels(&self) -> &[Color] {
        &self.pixels
    }
    pub fn transparent_color(&self) -> Option<Color> {
        self.transparent_color
    }
}
impl fmt::Debug for ImageData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Image")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("transparent_color", &self.transparent_color)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn table_and_frames_release_their_owned_versions() {
        let image = Image::new(1, 1, vec![Color(1, 2, 3)], None).unwrap();
        let weak = image.downgrade();
        let mut builder = super::super::display_list_builder::DisplayListBuilder::default();
        builder.image_index(&image);
        let (frame, _) = builder.finish().unwrap();
        let retained = frame.clone();
        drop(image);
        drop(frame);
        assert!(weak.upgrade().is_some());
        drop(retained);
        assert!(weak.upgrade().is_none());
    }
}
