//! Snapshot ownership, builder lookup, and frame resource validation.
use pixui_engine::ui::{
    display_list::{Color, DisplayList, DrawCommand},
    display_list_builder::{DisplayListBuilder, ImageIndex, MIN_REDRAW_DELAY},
    geometry::Rect,
    image::Image,
};
use std::time::Duration;

fn image(color: Color) -> Image {
    Image::new(1, 1, vec![color], None).unwrap()
}

#[test]
fn image_validation_and_identity() {
    for (width, height, pixels) in [
        (0, 1, vec![]),
        (1, 0, vec![]),
        (2, 2, vec![Color(1, 2, 3)]),
        (u32::MAX, u32::MAX, vec![]),
        (64_000_001, 1, vec![]),
    ] {
        assert!(Image::new(width, height, pixels, None).is_err());
    }
    let first = image(Color(1, 2, 3));
    let clone = first.clone();
    assert_eq!(first, clone);
    assert_eq!(
        first.rgb_pixels().unwrap().as_ptr(),
        clone.rgb_pixels().unwrap().as_ptr()
    );
    assert_ne!(first, image(Color(1, 2, 3)));
    assert_eq!(first.width(), 1);
    assert_eq!(first.height(), 1);
    assert!(!format!("{first:?}").contains("pixels"));
}

#[test]
fn builder_deduplicates_and_retains_snapshots_after_finish() {
    let first = image(Color(1, 2, 3));
    let mut builder = DisplayListBuilder::default();
    assert_eq!(builder.image_index(&first), ImageIndex::from_raw(0));
    assert_eq!(builder.image_index(&first.clone()), ImageIndex::from_raw(0));
    let second = image(Color(1, 2, 3));
    assert_eq!(builder.image_index(&second), ImageIndex::from_raw(1));
    builder.emit(DrawCommand::DrawImage {
        image: ImageIndex::from_raw(0),
        destination: Rect {
            x: 0.0,
            y: 0.0,
            width: 2.0,
            height: 2.0,
        },
    });
    let (display, delay) = builder.finish().unwrap();
    drop(first);
    drop(second);
    assert_eq!(display.images.len(), 2);
    assert_eq!(display.images[0].rgb_pixels().unwrap(), &[Color(1, 2, 3)]);
    assert_eq!(delay, None);
}

#[test]
fn missing_image_indices_and_invalid_geometry_are_errors() {
    for destination in [
        Rect {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        },
        Rect {
            x: f32::NAN,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        },
    ] {
        let display = DisplayList {
            fonts: Default::default(),
            images: Default::default(),
            commands: vec![DrawCommand::DrawImage {
                image: ImageIndex::from_raw(0),
                destination,
            }],
        };
        assert!(display.validate().is_err());
    }
}

#[test]
fn shortest_redraw_request_wins_and_zero_is_clamped() {
    let mut builder = DisplayListBuilder::default();
    builder.request_redraw_after(Duration::from_millis(33));
    builder.request_redraw_after(Duration::from_millis(50));
    assert_eq!(builder.finish().unwrap().1, Some(Duration::from_millis(33)));
    let mut builder = DisplayListBuilder::default();
    builder.request_redraw_after(Duration::ZERO);
    assert_eq!(builder.finish().unwrap().1, Some(MIN_REDRAW_DELAY));
}

#[test]
fn rgba_validation_storage_and_snapshot_identity() {
    use pixui_engine::ui::image::{ImagePixels, RgbaColor};
    let pixels = vec![RgbaColor(10, 20, 30, 128)];
    let first = Image::new_rgba(1, 1, pixels.clone()).unwrap();
    assert!(matches!(first.pixels(), ImagePixels::Rgba { pixels: stored } if *stored == pixels));
    assert!(first.rgb_pixels().is_none());
    assert_eq!(first.transparent_color(), None);
    assert_eq!(first, first.clone());
    assert_ne!(first, Image::new_rgba(1, 1, pixels).unwrap());
    for (width, height) in [(0, 1), (1, 0), (2, 2), (u32::MAX, u32::MAX)] {
        assert!(Image::new_rgba(width, height, vec![]).is_err());
    }
}
