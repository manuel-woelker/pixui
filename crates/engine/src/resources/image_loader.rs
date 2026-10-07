//! Bounded synchronous decoding into immutable render snapshots.
use super::{filesystem::ResourceFilesystem, path::ResourcePath};
use crate::ui::{
    display_list::Color,
    image::{Image, MAX_IMAGE_PIXELS, RgbaColor},
};
use image::{
    DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits, codecs::png::PngDecoder,
};
use pixui_base::{PixuiResult, pixui_error};
use std::{
    io::{Cursor, Read},
    sync::Arc,
};

/// Per-load limits, not a budget for all images retained by the application.
/// Decoder allocation limits are best effort in the underlying library. Encoded
/// bytes, pixel count and decoded/final buffer sizes are checked explicitly.
#[derive(Clone, Copy, Debug)]
pub struct ImageLoadLimits {
    pub max_encoded_bytes: usize,
    pub max_pixels: usize,
    pub max_decode_bytes: u64,
}
impl Default for ImageLoadLimits {
    fn default() -> Self {
        Self {
            max_encoded_bytes: 32 * 1024 * 1024,
            max_pixels: MAX_IMAGE_PIXELS,
            max_decode_bytes: 256 * 1024 * 1024,
        }
    }
}
struct ImageLoaderInner {
    filesystem: Arc<dyn ResourceFilesystem>,
    limits: ImageLoadLimits,
}

/// Cheaply cloneable loader with shared immutable configuration, no image cache.
///
/// Each load opens and decodes a fresh snapshot. Retain and clone that image to
/// reuse it across frames/windows. Loading blocks the calling thread: use during
/// setup or explicit actions, never in painters or on the native UI thread.
/// PNG and JPEG are identified by content, regardless of filename extension.
#[derive(Clone)]
pub struct ImageLoader(Arc<ImageLoaderInner>);
impl ImageLoader {
    pub fn new(filesystem: Arc<dyn ResourceFilesystem>) -> Self {
        Self(Arc::new(ImageLoaderInner {
            filesystem,
            limits: ImageLoadLimits::default(),
        }))
    }
    /// Rejects zero limits, unrepresentable read bounds, and pixel limits above
    /// the engine ceiling. Limits apply independently to each invocation.
    pub fn with_limits(
        filesystem: Arc<dyn ResourceFilesystem>,
        limits: ImageLoadLimits,
    ) -> PixuiResult<Self> {
        if limits.max_encoded_bytes == 0
            || limits.max_encoded_bytes.checked_add(1).is_none()
            || limits.max_pixels == 0
            || limits.max_pixels > MAX_IMAGE_PIXELS
            || limits.max_decode_bytes == 0
        {
            return Err(pixui_error!("invalid image loading limits"));
        }
        Ok(Self(Arc::new(ImageLoaderInner { filesystem, limits })))
    }
    pub fn load_str(&self, path: &str) -> PixuiResult<Image> {
        self.load(&ResourcePath::new(path.to_owned())?)
    }
    /// Opens the first matching source. Read/decode failures never trigger
    /// fallback. Alpha-bearing input becomes straight RGBA; opaque input RGB.
    /// Higher precision input is converted to eight-bit channels. APNG is rejected.
    pub fn load(&self, path: &ResourcePath) -> PixuiResult<Image> {
        let result = self.load_inner(path);
        result.map_err(|error| error.attach(format!("load image `{}`", path.as_str())))
    }
    fn load_inner(&self, path: &ResourcePath) -> PixuiResult<Image> {
        let reader = self
            .0
            .filesystem
            .open(path)?
            .ok_or_else(|| pixui_error!("image resource `{}` not found", path.as_str()))?;
        let limits = self.0.limits;
        let mut bytes = Vec::new();
        reader
            .take((limits.max_encoded_bytes + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|error| pixui_error!("read image `{}`: {error}", path.as_str()))?;
        if bytes.len() > limits.max_encoded_bytes {
            return Err(pixui_error!(
                "image `{}` exceeds encoded byte limit",
                path.as_str()
            ));
        }
        let format =
            image::guess_format(&bytes).map_err(|error| pixui_error!("identify image: {error}"))?;
        let mut decoder_limits = Limits::default();
        decoder_limits.max_image_width = Some(limits.max_pixels as u32);
        decoder_limits.max_image_height = Some(limits.max_pixels as u32);
        decoder_limits.max_alloc = Some(limits.max_decode_bytes);
        let decoder: Box<dyn ImageDecoder> = match format {
            ImageFormat::Png => {
                let decoder = PngDecoder::with_limits(Cursor::new(&bytes), decoder_limits)
                    .map_err(|error| pixui_error!("read PNG header: {error}"))?;
                if decoder
                    .is_apng()
                    .map_err(|error| pixui_error!("read PNG animation metadata: {error}"))?
                {
                    return Err(pixui_error!("animated PNG images are unsupported"));
                }
                Box::new(decoder)
            }
            ImageFormat::Jpeg => {
                let mut reader = ImageReader::with_format(Cursor::new(&bytes), format);
                reader.limits(decoder_limits);
                Box::new(
                    reader
                        .into_decoder()
                        .map_err(|error| pixui_error!("read JPEG header: {error}"))?,
                )
            }
            _ => {
                return Err(pixui_error!(
                    "unsupported image format {format:?}; expected PNG or JPEG"
                ));
            }
        };
        let (width, height) = decoder.dimensions();
        let count = u64::from(width) * u64::from(height);
        let has_alpha = decoder.color_type().has_alpha();
        let final_bytes = count.checked_mul(if has_alpha { 4 } else { 3 });
        if width == 0
            || height == 0
            || count > limits.max_pixels as u64
            || final_bytes
                .and_then(|size| size.checked_add(decoder.total_bytes()))
                .is_none_or(|size| size > limits.max_decode_bytes)
        {
            return Err(pixui_error!(
                "image dimensions or decode buffers exceed loading limits"
            ));
        }
        let decoded = DynamicImage::from_decoder(decoder)
            .map_err(|error| pixui_error!("decode image: {error}"))?;
        if has_alpha {
            let pixels = decoded
                .into_rgba8()
                .pixels()
                .map(|pixel| RgbaColor(pixel[0], pixel[1], pixel[2], pixel[3]))
                .collect();
            Image::new_rgba(width, height, pixels)
        } else {
            let pixels = decoded
                .into_rgb8()
                .pixels()
                .map(|pixel| Color(pixel[0], pixel[1], pixel[2]))
                .collect();
            Image::new(width, height, pixels, None)
        }
    }
}
