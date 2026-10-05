#![doc = include_str!("Images.md")]

use super::display_list::Color;
use pixui_base::{PixuiResult, pixui_error};
use std::{fmt, sync::Arc};

/// Source pixel ceiling, independent of total application memory usage.
pub const MAX_IMAGE_PIXELS: usize = 64_000_000;

struct ImageData {
    width: u32,
    height: u32,
    pixels: Vec<Color>,
    transparent_color: Option<Color>,
}

/// An immutable row-major RGB image with optional exact color-key transparency.
/// Equality compares allocation identity, not pixel content. Each separately
/// constructed image is a new version, even if its pixels match an older image.
#[derive(Clone)]
pub struct Image(Arc<ImageData>);

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ImageIdentity(usize);

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
        Ok(Self(Arc::new(ImageData {
            width,
            height,
            pixels,
            transparent_color,
        })))
    }
    pub fn width(&self) -> u32 {
        self.0.width
    }
    pub fn height(&self) -> u32 {
        self.0.height
    }
    pub fn pixels(&self) -> &[Color] {
        &self.0.pixels
    }
    pub fn transparent_color(&self) -> Option<Color> {
        self.0.transparent_color
    }
    pub(crate) fn identity(&self) -> ImageIdentity {
        ImageIdentity(Arc::as_ptr(&self.0) as usize)
    }
}
impl PartialEq for Image {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for Image {}
impl fmt::Debug for Image {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Image")
            .field("width", &self.width())
            .field("height", &self.height())
            .field("transparent_color", &self.transparent_color())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn table_and_frames_release_their_owned_versions() {
        let image = Image::new(1, 1, vec![Color(1, 2, 3)], None).unwrap();
        let weak = Arc::downgrade(&image.0);
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
