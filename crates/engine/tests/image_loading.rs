//! Image loading contracts, using tiny generated fixtures and in-memory sources.
use image::{
    ExtendedColorType, ImageEncoder,
    codecs::{jpeg::JpegEncoder, png::PngEncoder},
};
use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::{
    resources::{
        filesystem::{ResourceFilesystem, ResourceReader},
        image_loader::{ImageLoadLimits, ImageLoader},
        layered::LayeredFilesystem,
        path::ResourcePath,
    },
    ui::{
        display_list::Color,
        display_list_builder::DisplayListBuilder,
        image::{ImagePixels, RgbaColor},
    },
};
use std::{
    io::{self, Cursor, Read},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

struct Memory(Vec<u8>);
impl ResourceFilesystem for Memory {
    fn open(&self, _: &ResourcePath) -> PixuiResult<Option<ResourceReader>> {
        Ok(Some(Box::new(Cursor::new(self.0.clone()))))
    }
}
fn loader(bytes: Vec<u8>) -> ImageLoader {
    ImageLoader::new(Arc::new(Memory(bytes)))
}
fn png(pixels: &[u8], color: ExtendedColorType, width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    PngEncoder::new(&mut bytes)
        .write_image(pixels, width, height, color)
        .unwrap();
    bytes
}
#[test]
fn rgb_rgba_and_jpeg_decode_by_content_not_filename() {
    let rgb = loader(png(&[10, 20, 30], ExtendedColorType::Rgb8, 1, 1))
        .load_str("no-extension")
        .unwrap();
    assert_eq!(rgb.rgb_pixels().unwrap(), &[Color(10, 20, 30)]);
    let rgba = loader(png(
        &[10, 20, 30, 0, 40, 50, 60, 128, 70, 80, 90, 255],
        ExtendedColorType::Rgba8,
        3,
        1,
    ))
    .load_str("misleading.jpg")
    .unwrap();
    assert!(
        matches!(rgba.pixels(), ImagePixels::Rgba { pixels } if pixels == &[RgbaColor(10,20,30,0), RgbaColor(40,50,60,128), RgbaColor(70,80,90,255)])
    );
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, 100)
        .write_image(&[64, 64, 64], 1, 1, ExtendedColorType::Rgb8)
        .unwrap();
    let jpeg = loader(bytes).load_str("misleading.png").unwrap();
    assert!(
        jpeg.rgb_pixels()
            .unwrap()
            .iter()
            .all(|p| p.0.abs_diff(64) <= 1 && p.1.abs_diff(64) <= 1 && p.2.abs_diff(64) <= 1)
    );
}
#[test]
fn malformed_truncated_unsupported_and_animated_images_fail_with_filename_context() {
    let mut apng = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut apng, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_animated(1, 0).unwrap();
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[1, 2, 3, 128])
            .unwrap();
    }
    let valid = png(&[1, 2, 3], ExtendedColorType::Rgb8, 1, 1);
    for bytes in [
        vec![],
        b"not an image".to_vec(),
        valid[..valid.len() / 2].to_vec(),
        b"GIF89a".to_vec(),
        apng,
    ] {
        let error = loader(bytes).load_str("broken.png").unwrap_err();
        assert!(format!("{error:?}").contains("broken.png"));
    }
    assert!(loader(valid).load_str("../bad.png").is_err());
    assert!(
        ImageLoader::new(Arc::new(LayeredFilesystem::new(vec![])))
            .load_str("missing.png")
            .unwrap_err()
            .to_string()
            .contains("not found")
    );
}
#[test]
fn encoded_pixel_and_decode_limits_are_enforced() {
    let bytes = png(&[1, 2, 3, 4, 5, 6], ExtendedColorType::Rgb8, 2, 1);
    let source: Arc<dyn ResourceFilesystem> = Arc::new(Memory(bytes.clone()));
    for limits in [
        ImageLoadLimits {
            max_encoded_bytes: bytes.len() - 1,
            ..Default::default()
        },
        ImageLoadLimits {
            max_pixels: 1,
            ..Default::default()
        },
        ImageLoadLimits {
            max_decode_bytes: 5,
            ..Default::default()
        },
    ] {
        assert!(
            ImageLoader::with_limits(source.clone(), limits)
                .unwrap()
                .load_str("limited.png")
                .is_err()
        );
    }
    let exact = ImageLoadLimits {
        max_encoded_bytes: bytes.len(),
        ..Default::default()
    };
    assert!(
        ImageLoader::with_limits(source.clone(), exact)
            .unwrap()
            .load_str("exact.png")
            .is_ok()
    );
    assert!(
        ImageLoader::with_limits(
            source.clone(),
            ImageLoadLimits {
                max_encoded_bytes: usize::MAX,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(
        ImageLoader::with_limits(
            source,
            ImageLoadLimits {
                max_pixels: 0,
                ..Default::default()
            }
        )
        .is_err()
    );
}
struct CountingReader {
    bytes: Arc<AtomicUsize>,
}
impl Read for CountingReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        buffer.fill(0);
        self.bytes.fetch_add(buffer.len(), Ordering::SeqCst);
        Ok(buffer.len())
    }
}
struct CountingSource(Arc<AtomicUsize>);
impl ResourceFilesystem for CountingSource {
    fn open(&self, _: &ResourcePath) -> PixuiResult<Option<ResourceReader>> {
        Ok(Some(Box::new(CountingReader {
            bytes: self.0.clone(),
        })))
    }
}
#[test]
fn an_unending_reader_is_bounded_before_decoding() {
    let bytes = Arc::new(AtomicUsize::new(0));
    let loader = ImageLoader::with_limits(
        Arc::new(CountingSource(bytes.clone())),
        ImageLoadLimits {
            max_encoded_bytes: 10,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(loader.load_str("unending.png").is_err());
    assert_eq!(bytes.load(Ordering::SeqCst), 11);
}
struct Broken;
impl Read for Broken {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("read failure"))
    }
}
impl ResourceFilesystem for Broken {
    fn open(&self, _: &ResourcePath) -> PixuiResult<Option<ResourceReader>> {
        Ok(Some(Box::new(Broken)))
    }
}
struct Unexpected;
impl ResourceFilesystem for Unexpected {
    fn open(&self, _: &ResourcePath) -> PixuiResult<Option<ResourceReader>> {
        Err(pixui_error!("unexpected fallback"))
    }
}
#[test]
fn read_and_decode_errors_do_not_trigger_fallback() {
    for source in [
        Arc::new(Broken) as Arc<dyn ResourceFilesystem>,
        Arc::new(Memory(vec![])),
    ] {
        let loader = ImageLoader::new(Arc::new(LayeredFilesystem::new(vec![
            source,
            Arc::new(Unexpected),
        ])));
        let error = loader.load_str("broken.png").unwrap_err();
        assert!(!format!("{error:?}").contains("unexpected fallback"));
    }
}
#[test]
fn override_snapshot_is_retained_and_reload_creates_new_identity() {
    let high = Arc::new(Memory(png(&[10, 20, 30], ExtendedColorType::Rgb8, 1, 1)));
    let low = Arc::new(Memory(png(&[40, 50, 60], ExtendedColorType::Rgb8, 1, 1)));
    let loader = ImageLoader::new(Arc::new(LayeredFilesystem::new(vec![high, low])));
    let first = loader.load_str("image.png").unwrap();
    let mut builder = DisplayListBuilder::default();
    let index = builder.image_index(&first);
    assert_eq!(index, builder.image_index(&first.clone()));
    let display = builder.finish().unwrap().0;
    let replacement = loader.clone().load_str("image.png").unwrap();
    assert_ne!(first, replacement);
    drop(first);
    drop(loader);
    assert_eq!(
        display.images[index].rgb_pixels().unwrap(),
        &[Color(10, 20, 30)]
    );
}
