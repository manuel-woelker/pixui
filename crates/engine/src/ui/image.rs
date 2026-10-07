#![doc = include_str!("Images.md")]

use super::{display_list::Color, resource::Resource};
use pixui_base::{PixuiResult, pixui_error};
use std::fmt;

/// Source pixel ceiling, independent of total application memory usage.
pub const MAX_IMAGE_PIXELS: usize = 64_000_000;

pub struct ImageData {
    width: u32,
    height: u32,
    pixels: ImagePixels,
}

/// Straight (unpremultiplied) RGBA channels, in the range 0..=255.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RgbaColor(pub u8, pub u8, pub u8, pub u8);

/// Row-major pixel storage. RGB retains optional exact color-key transparency;
/// RGBA supports partial alpha. No variant contains a second conflicting format.
#[derive(Debug, PartialEq, Eq)]
pub enum ImagePixels {
    Rgb {
        pixels: Vec<Color>,
        transparent_color: Option<Color>,
    },
    Rgba {
        pixels: Vec<RgbaColor>,
    },
}

/// An immutable row-major RGB or straight-alpha RGBA image.
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
            pixels: ImagePixels::Rgb {
                pixels,
                transparent_color,
            },
        }))
    }
    /// Consumes straight-alpha RGBA pixels with the same limits as `new`.
    pub fn new_rgba(width: u32, height: u32, pixels: Vec<RgbaColor>) -> PixuiResult<Self> {
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
            pixels: ImagePixels::Rgba { pixels },
        }))
    }
}

impl ImageData {
    pub(crate) fn storage_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + match &self.pixels {
                ImagePixels::Rgb { pixels, .. } => pixels.capacity() * std::mem::size_of::<Color>(),
                ImagePixels::Rgba { pixels } => {
                    pixels.capacity() * std::mem::size_of::<RgbaColor>()
                }
            }
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn pixels(&self) -> &ImagePixels {
        &self.pixels
    }
    /// RGB storage, if this snapshot uses RGB. Use `pixels` to handle both formats.
    pub fn rgb_pixels(&self) -> Option<&[Color]> {
        match &self.pixels {
            ImagePixels::Rgb { pixels, .. } => Some(pixels),
            ImagePixels::Rgba { .. } => None,
        }
    }
    /// Exact color key for RGB; RGBA uses its alpha channels instead.
    pub fn transparent_color(&self) -> Option<Color> {
        match &self.pixels {
            ImagePixels::Rgb {
                transparent_color, ..
            } => *transparent_color,
            ImagePixels::Rgba { .. } => None,
        }
    }
}
impl fmt::Debug for ImageData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Image")
            .field("width", &self.width)
            .field("height", &self.height)
            .field(
                "format",
                &match self.pixels {
                    ImagePixels::Rgb { .. } => "RGB",
                    ImagePixels::Rgba { .. } => "RGBA",
                },
            )
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
