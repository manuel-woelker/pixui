//! Deterministic CPU painter for owned display lists, also usable without windows.

use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::ui::{
    display_list::{Color, DisplayList, DrawCommand},
    geometry::{Point, Rect},
    text,
};

struct Canvas {
    pixels: Vec<u32>,
    width: u32,
    height: u32,
    scale: f32,
    clips: Vec<Rect>,
}

impl Canvas {
    fn fill(&mut self, rect: Rect, Color(red, green, blue): Color) {
        let rect = rect.intersect(*self.clips.last().expect("viewport clip"));
        let left = (rect.x * self.scale).floor().clamp(0.0, self.width as f32) as usize;
        let top = (rect.y * self.scale).floor().clamp(0.0, self.height as f32) as usize;
        let right = ((rect.x + rect.width) * self.scale)
            .ceil()
            .clamp(0.0, self.width as f32) as usize;
        let bottom = ((rect.y + rect.height) * self.scale)
            .ceil()
            .clamp(0.0, self.height as f32) as usize;
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        let color = (u32::from(red) << 16) | (u32::from(green) << 8) | u32::from(blue);
        for y in top..bottom {
            self.pixels[y * self.width as usize + left..y * self.width as usize + right]
                .fill(color);
        }
    }
}

/// Paints opaque sRGB pixels as `0x00RRGGBB`, matching softbuffer's format.
/// Validates commands before allocating. Buffers larger than 64 million pixels
/// are rejected. Logical geometry and glyph metrics match the worker renderer.
pub fn paint(display: &DisplayList, width: u32, height: u32, scale: f32) -> PixuiResult<Vec<u32>> {
    display.validate()?;
    let count = u64::from(width) * u64::from(height);
    if count > 64_000_000 || !scale.is_finite() || !(0.1..=16.0).contains(&scale) {
        return Err(pixui_error!("invalid raster dimensions or scale"));
    }
    let mut canvas = Canvas {
        pixels: vec![0; count as usize],
        width,
        height,
        scale,
        clips: vec![Rect {
            x: 0.0,
            y: 0.0,
            width: width as f32 / scale,
            height: height as f32 / scale,
        }],
    };
    for command in &display.commands {
        match command {
            DrawCommand::FillRect { rect, color } => canvas.fill(*rect, *color),
            DrawCommand::StrokeRect { rect, color, width } => {
                let thickness = width.min(rect.width).min(rect.height);
                for strip in [
                    Rect {
                        height: thickness,
                        ..*rect
                    },
                    Rect {
                        y: rect.y + rect.height - thickness,
                        height: thickness,
                        ..*rect
                    },
                    Rect {
                        width: thickness,
                        ..*rect
                    },
                    Rect {
                        x: rect.x + rect.width - thickness,
                        width: thickness,
                        ..*rect
                    },
                ] {
                    canvas.fill(strip, *color);
                }
            }
            DrawCommand::DrawText {
                origin,
                text: content,
                size,
                color,
                ..
            } => {
                let cell = size / 8.0;
                for (index, character) in content.chars().enumerate() {
                    for (row, bits) in text::glyph(character).into_iter().enumerate() {
                        for column in 0..8 {
                            if bits & (1 << column) != 0 {
                                canvas.fill(
                                    Rect {
                                        x: origin.x + index as f32 * size + column as f32 * cell,
                                        y: origin.y + row as f32 * cell,
                                        width: cell,
                                        height: cell,
                                    },
                                    *color,
                                );
                            }
                        }
                    }
                }
            }
            DrawCommand::DrawImage { image, destination } => {
                let source = &display.images[image.0];
                let visible = destination.intersect(*canvas.clips.last().expect("viewport clip"));
                if visible.width <= 0.0 || visible.height <= 0.0 {
                    continue;
                }
                let left = (visible.x * scale).floor().clamp(0.0, width as f32) as u32;
                let top = (visible.y * scale).floor().clamp(0.0, height as f32) as u32;
                let right = ((visible.x + visible.width) * scale)
                    .ceil()
                    .clamp(0.0, width as f32) as u32;
                let bottom = ((visible.y + visible.height) * scale)
                    .ceil()
                    .clamp(0.0, height as f32) as u32;
                for y in top..bottom {
                    for x in left..right {
                        let logical_x = (x as f32 + 0.5) / scale;
                        let logical_y = (y as f32 + 0.5) / scale;
                        if !visible.contains(Point {
                            x: logical_x,
                            y: logical_y,
                        }) {
                            continue;
                        }
                        let sx = (((logical_x - destination.x) / destination.width)
                            * source.width() as f32)
                            .floor()
                            .clamp(0.0, (source.width() - 1) as f32)
                            as usize;
                        let sy = (((logical_y - destination.y) / destination.height)
                            * source.height() as f32)
                            .floor()
                            .clamp(0.0, (source.height() - 1) as f32)
                            as usize;
                        let color = source.pixels()[sy * source.width() as usize + sx];
                        if source.transparent_color() != Some(color) {
                            canvas.pixels[y as usize * width as usize + x as usize] =
                                (u32::from(color.0) << 16)
                                    | (u32::from(color.1) << 8)
                                    | u32::from(color.2);
                        }
                    }
                }
            }
            DrawCommand::PushClip { rect } => canvas
                .clips
                .push(rect.intersect(*canvas.clips.last().expect("viewport clip"))),
            DrawCommand::PopClip => {
                canvas.clips.pop();
            }
        }
    }
    Ok(canvas.pixels)
}
