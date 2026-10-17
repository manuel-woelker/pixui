//! Rounded styles use the same shape for fills and inside borders.
use pixui_engine::{
    styles::{brush::LinearGradient, index::StyleIndex, style::Border},
    ui::{
        display_list::{Color, DisplayList, DrawCommand},
        geometry::Rect,
        render_input::{RenderInput, RenderStyle},
    },
};
use pixui_gui::painter::paint;

fn display() -> DisplayList {
    DisplayList {
        commands: vec![
            DrawCommand::StyledFillRect {
                rect: Rect {
                    x: 2.0,
                    y: 2.0,
                    width: 24.0,
                    height: 16.0,
                },
                style: StyleIndex { index: 0 },
            },
            DrawCommand::StyledStrokeRect {
                rect: Rect {
                    x: 2.0,
                    y: 2.0,
                    width: 24.0,
                    height: 16.0,
                },
                style: StyleIndex { index: 0 },
            },
        ],
        ..Default::default()
    }
}
fn style(radius: f32) -> RenderStyle {
    RenderStyle {
        fill: Some(Color(20, 180, 40).into()),
        border: Some(Border {
            color: Color(240, 40, 20),
            width: 2.0,
        }),
        corner_radius: radius,
        foreground: Color(0, 0, 0),
        font: None,
    }
}
fn pixels(display: &DisplayList, style: &RenderStyle, scale: f32) -> Vec<u32> {
    paint(
        RenderInput {
            display,
            styles: std::slice::from_ref(style),
        },
        (32.0 * scale) as u32,
        (24.0 * scale) as u32,
        scale,
    )
    .unwrap()
}
#[test]
fn rounded_fill_and_inside_border_preserve_background_and_scale() {
    let display = display();
    for scale in [1.0, 2.0] {
        let pixels = pixels(&display, &style(8.0), scale);
        let at = |x: f32, y: f32| {
            pixels[(y * scale) as usize * (32.0 * scale) as usize + (x * scale) as usize]
        };
        assert_eq!(at(2.0, 2.0), 0, "outer corners reveal the background");
        assert_eq!(at(14.0, 3.0), 0xf02814, "border stays inside");
        assert_eq!(
            at(14.0, 10.0),
            0x14b428,
            "border does not fill the interior"
        );
        assert_eq!(at(14.0, 0.0), 0, "nothing outside the rectangle");
    }
    assert_eq!(
        pixels(&display, &style(8.0), 1.0),
        pixels(&display, &style(1000.0), 1.0),
        "radius clamps to half the shorter side"
    );
    let square = pixels(&display, &style(0.0), 1.0);
    assert_eq!(
        square[2 * 32 + 2],
        0xf02814,
        "zero keeps existing square behavior"
    );
}
#[test]
fn border_only_and_oversized_borders_do_not_require_a_fill() {
    let mut display = display();
    display.commands.remove(0);
    let pixels = pixels(
        &display,
        &RenderStyle {
            fill: None,
            ..style(8.0)
        },
        1.0,
    );
    assert_eq!(pixels[10 * 32 + 14], 0);
    assert_eq!(pixels[3 * 32 + 14], 0xf02814);
    let wide = RenderStyle {
        border: Some(Border {
            color: Color(240, 40, 20),
            width: 30.0,
        }),
        fill: None,
        ..style(8.0)
    };
    let painted = self::pixels(&display, &wide, 1.0);
    assert_eq!(painted[10 * 32 + 14], 0xf02814);
    assert_eq!(painted[2 * 32 + 2], 0);
}
#[test]
fn clipping_preserves_rounded_gradient_coordinates() {
    let mut display = display();
    display.commands.pop();
    let style = RenderStyle {
        fill: Some(LinearGradient::horizontal(Color(0, 0, 0), Color(240, 120, 60)).into()),
        ..style(8.0)
    };
    let full = pixels(&display, &style, 1.0);
    display.commands.insert(
        0,
        DrawCommand::PushClip {
            rect: Rect {
                x: 12.0,
                y: 0.0,
                width: 20.0,
                height: 24.0,
            },
        },
    );
    display.commands.push(DrawCommand::PopClip);
    let clipped = pixels(&display, &style, 1.0);
    for (index, value) in clipped.into_iter().enumerate() {
        assert_eq!(value, if index % 32 >= 12 { full[index] } else { 0 });
    }
}
