use pixui_engine::ui::{
    display_list::{Color, DisplayList, DrawCommand, FontId},
    geometry::{Point, Rect},
    text,
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
        images: Vec::new(),
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
fn stroke_and_text_use_the_shared_font_and_cell_metrics() {
    let display = DisplayList {
        images: Vec::new(),
        commands: vec![
            DrawCommand::StrokeRect {
                rect: rect(0.0, 0.0, 24.0, 24.0),
                color: Color(0, 255, 0),
                width: 1.0,
            },
            DrawCommand::DrawText {
                origin: Point { x: 4.0, y: 4.0 },
                text: "Aä".into(),
                font: FontId::Builtin,
                size: 8.0,
                color: Color(255, 255, 255),
            },
        ],
    };
    let pixels = paint(&display, 24, 24, 1.0).unwrap();
    for (row, bits) in text::glyph('A').into_iter().enumerate() {
        for column in 0..8 {
            assert_eq!(
                pixels[(4 + row) * 24 + 4 + column],
                if bits & (1 << column) == 0 {
                    0
                } else {
                    0xffffff
                }
            );
        }
    }
    assert_eq!(pixels[0], 0x00ff00);
    assert_ne!(text::glyph('ä'), text::glyph('?'));
    assert_eq!(text::wrap("abcdef\nxy", 8.0, 24.0), ["abc", "def", "xy"]);
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
            font: FontId::Builtin,
            size: 0.0,
            color: Color(0, 0, 0),
        }],
    ] {
        assert!(
            paint(
                &DisplayList {
                    images: Vec::new(),
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
        images: vec![
            Image::new(
                2,
                2,
                vec![Color(255, 0, 0), key, Color(0, 255, 0), Color(0, 0, 255)],
                transparent,
            )
            .unwrap(),
        ],
        commands: vec![
            DrawCommand::FillRect {
                rect: rect(0.0, 0.0, 8.0, 8.0),
                color: Color(20, 30, 40),
            },
            DrawCommand::DrawImage {
                image: ImageIndex(0),
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
