use pixui_engine::ui::{
    display_list::{Color, DisplayList, DrawCommand},
    display_list_builder::DisplayListBuilder,
    geometry::{Point, Rect},
    text::{
        font::{FontConfig, FontFace},
        resource::FontIndex,
    },
};
use pixui_gui::painter::paint;

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

#[test]
fn ordered_painting_nested_clips_and_scaling_match_logical_geometry() {
    let display = DisplayList {
        images: Default::default(),
        fonts: Default::default(),
        commands: vec![
            DrawCommand::FillRect {
                rect: rect(0.0, 0.0, 4.0, 4.0),
                color: Color(255, 0, 0),
            },
            DrawCommand::PushClip {
                rect: rect(1.0, 1.0, 2.0, 2.0),
            },
            DrawCommand::FillRect {
                rect: rect(0.0, 0.0, 4.0, 4.0),
                color: Color(0, 255, 0),
            },
            DrawCommand::PushClip {
                rect: rect(2.0, 0.0, 4.0, 4.0),
            },
            DrawCommand::FillRect {
                rect: rect(0.0, 0.0, 4.0, 4.0),
                color: Color(0, 0, 255),
            },
            DrawCommand::PopClip,
            DrawCommand::PopClip,
        ],
    };
    let pixels = paint(&display, 8, 8, 2.0).unwrap();
    assert_eq!(pixels[0], 0xff0000);
    assert_eq!(pixels[2 * 8 + 2], 0x00ff00);
    assert_eq!(pixels[2 * 8 + 4], 0x0000ff);
    assert_eq!(pixels[2 * 8 + 6], 0xff0000);
    assert_eq!(paint(&display, 8, 8, 2.0).unwrap(), pixels);
}

#[test]
fn stroke_and_text_use_the_worker_atlas() {
    let mut builder = DisplayListBuilder::default();
    builder.emit(DrawCommand::StrokeRect {
        rect: rect(0.0, 0.0, 40.0, 30.0),
        color: Color(0, 255, 0),
        width: 1.0,
    });
    builder
        .text(
            FontConfig::new(FontFace::geist().unwrap(), 16.0, 1.0).unwrap(),
            Point { x: 4.0, y: 20.0 },
            "Aä",
            Color(255, 255, 255),
        )
        .unwrap();
    let (display, _) = builder.finish().unwrap();
    let pixels = paint(&display, 40, 30, 1.0).unwrap();
    assert_eq!(pixels[0], 0x00ff00);
    assert!(
        pixels
            .iter()
            .any(|&pixel| pixel != 0 && pixel != 0x00ff00 && pixel != 0xffffff)
    );
    assert_ne!(display.fonts[0].glyph('ä'), display.fonts[0].glyph('A'));
    assert_eq!(pixels, paint(&display, 40, 30, 1.0).unwrap());
}

#[test]
fn invalid_lists_and_raster_sizes_fail_before_painting() {
    for commands in [
        vec![DrawCommand::PopClip],
        vec![DrawCommand::PushClip {
            rect: rect(0.0, 0.0, 1.0, 1.0),
        }],
        vec![DrawCommand::FillRect {
            rect: rect(f32::NAN, 0.0, 1.0, 1.0),
            color: Color(0, 0, 0),
        }],
        vec![DrawCommand::DrawText {
            origin: Point::default(),
            text: "bad".into(),
            font: FontIndex::from_raw(0),
            color: Color(0, 0, 0),
        }],
    ] {
        assert!(
            paint(
                &DisplayList {
                    images: Default::default(),
                    fonts: Default::default(),
                    commands
                },
                10,
                10,
                1.0
            )
            .is_err()
        );
    }
    assert!(paint(&DisplayList::default(), u32::MAX, u32::MAX, 1.0).is_err());
    assert!(paint(&DisplayList::default(), 10, 10, f32::NAN).is_err());
    assert!(
        paint(&DisplayList::default(), 0, 0, 1.0)
            .unwrap()
            .is_empty()
    );
}

fn image_display(transparent: Option<Color>, destination: Rect) -> DisplayList {
    use pixui_engine::ui::{display_list_builder::ImageIndex, image::Image};
    let key = Color(255, 0, 255);
    DisplayList {
        fonts: Default::default(),
        images: vec![
            Image::new(
                2,
                2,
                vec![Color(255, 0, 0), key, Color(0, 255, 0), Color(0, 0, 255)],
                transparent,
            )
            .unwrap(),
        ]
        .into(),
        commands: vec![
            DrawCommand::FillRect {
                rect: rect(0.0, 0.0, 8.0, 8.0),
                color: Color(20, 30, 40),
            },
            DrawCommand::DrawImage {
                image: ImageIndex::from_raw(0),
                destination,
            },
        ],
    }
}

#[test]
fn image_color_keys_scaling_and_clips_preserve_background() {
    let display = image_display(Some(Color(255, 0, 255)), rect(1.0, 1.0, 2.0, 2.0));
    let pixels = paint(&display, 8, 8, 2.0).unwrap();
    assert_eq!(pixels[2 * 8 + 2], 0xff0000);
    assert_eq!(pixels[2 * 8 + 4], 0x141e28);
    assert_eq!(pixels[4 * 8 + 2], 0x00ff00);
    assert_eq!(pixels[4 * 8 + 4], 0x0000ff);
    let opaque = image_display(None, rect(1.0, 1.0, 2.0, 2.0));
    assert_eq!(paint(&opaque, 8, 8, 2.0).unwrap()[2 * 8 + 4], 0xff00ff);
    let mut clipped = image_display(None, rect(0.0, 0.0, 4.0, 4.0));
    clipped.commands.insert(
        1,
        DrawCommand::PushClip {
            rect: rect(2.0, 0.0, 2.0, 4.0),
        },
    );
    clipped.commands.push(DrawCommand::PopClip);
    let pixels = paint(&clipped, 8, 8, 1.0).unwrap();
    assert_eq!(pixels[0], 0x141e28);
    assert_eq!(pixels[2], 0xff00ff); // Clipping must not remap to source red.
    assert_eq!(pixels[2 * 8 + 2], 0x0000ff);
}

#[test]
fn images_sample_pixel_centers_at_fractional_positions_and_downscale() {
    let display = image_display(None, rect(0.25, 0.25, 2.0, 2.0));
    let pixels = paint(&display, 4, 4, 1.0).unwrap();
    assert_eq!(pixels[0], 0xff0000);
    assert_eq!(pixels[1], 0xff00ff);
    assert_eq!(pixels[2], 0x141e28);
    let downscaled = image_display(None, rect(0.0, 0.0, 1.0, 1.0));
    assert_eq!(paint(&downscaled, 4, 4, 1.0).unwrap()[0], 0x0000ff);
    let empty = image_display(None, rect(0.0, 0.0, 0.0, 1.0));
    assert!(
        paint(&empty, 4, 4, 1.0)
            .unwrap()
            .iter()
            .all(|pixel| *pixel == 0x141e28)
    );
}

fn coverage_display(origin: Point, scale: f32) -> DisplayList {
    use pixui_engine::ui::text::resource::{
        FontMetrics, FontResource, GlyphAtlas, GlyphInfo, PixelRect,
    };
    use std::collections::HashMap;
    let atlas = GlyphAtlas::new(3, 1, vec![0, 128, 255]).unwrap();
    let glyph = GlyphInfo {
        atlas_rect: Some(PixelRect {
            x: 0,
            y: 0,
            width: 3,
            height: 1,
        }),
        advance: 4.0 / scale,
        offset: Point {
            x: -1.0 / scale,
            y: -1.0 / scale,
        },
    };
    let font = FontResource::new(
        atlas,
        HashMap::from([('A', glyph)]),
        FontMetrics {
            ascent: 1.0 / scale,
            descent: 0.0,
            line_height: 2.0 / scale,
        },
        scale,
    )
    .unwrap();
    DisplayList {
        images: Default::default(),
        fonts: vec![pixui_engine::ui::text::resource::Font::from_value(font)].into(),
        commands: vec![
            DrawCommand::FillRect {
                rect: rect(0.0, 0.0, 16.0, 8.0),
                color: Color(20, 40, 60),
            },
            DrawCommand::DrawText {
                origin,
                text: "AA\nA".into(),
                font: FontIndex::from_raw(0),
                color: Color(220, 140, 60),
            },
        ],
    }
}

#[test]
fn grayscale_blending_bearings_advances_and_newlines_are_exact() {
    let display = coverage_display(Point { x: 1.0, y: 1.0 }, 1.0);
    let pixels = paint(&display, 16, 8, 1.0).unwrap();
    assert_eq!(pixels[0], 0x14283c); // zero coverage preserves background
    assert_eq!(pixels[1], 0x785a3c); // rounded 128/255 encoded RGB blend
    assert_eq!(pixels[2], 0xdc8c3c); // opaque coverage writes foreground
    assert_eq!(pixels[3], 0x14283c); // advance is four, not glyph width three
    assert_eq!(pixels[5], pixels[1]);
    assert_eq!(pixels[6], pixels[2]);
    assert_eq!(pixels[2 * 16 + 1], pixels[1]); // newline returns to initial x
    assert_eq!(pixels[2 * 16 + 5], 0x14283c);
    let mut clipped = display.clone();
    clipped.commands.insert(
        1,
        DrawCommand::PushClip {
            rect: rect(2.0, 0.0, 1.0, 1.0),
        },
    );
    clipped.commands.push(DrawCommand::PopClip);
    let pixels = paint(&clipped, 16, 8, 1.0).unwrap();
    assert_eq!(pixels[1], 0x14283c);
    assert_eq!(pixels[2], 0xdc8c3c); // clipping doesn't shift sampling
    assert_eq!(pixels[6], 0x14283c);
}

#[test]
fn snapping_and_dpi_transitions_preserve_safe_sampling() {
    let display = coverage_display(Point { x: 1.25, y: 1.25 }, 1.0);
    assert_eq!(
        paint(&display, 16, 8, 1.0).unwrap(),
        paint(&coverage_display(Point { x: 1.0, y: 1.0 }, 1.0), 16, 8, 1.0).unwrap()
    );
    let pixels = paint(
        &coverage_display(Point { x: 1.0, y: 1.0 }, 1.0),
        32,
        16,
        2.0,
    )
    .unwrap();
    assert_eq!(pixels[2], 0x785a3c);
    assert_eq!(pixels[3], pixels[2]);
    assert_eq!(pixels[4], 0xdc8c3c);
    assert_eq!(pixels[32 + 4], pixels[4]);
    let physical = paint(
        &coverage_display(Point { x: 0.5, y: 0.5 }, 2.0),
        32,
        16,
        2.0,
    )
    .unwrap();
    assert_eq!(physical[1], 0x785a3c);
    assert_eq!(physical[2], 0xdc8c3c);
    // A consumer that appears later needs only the retained list, no uploads.
    assert_eq!(
        pixels,
        paint(
            &coverage_display(Point { x: 1.0, y: 1.0 }, 1.0),
            32,
            16,
            2.0
        )
        .unwrap()
    );
}
