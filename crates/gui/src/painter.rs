//! Deterministic CPU painter for owned display lists, also usable without windows.

use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::styles::brush::{Brush, LinearGradient};
use pixui_engine::ui::{
    display_list::Color,
    geometry::{Point, Rect},
    image::{ImagePixels, RgbaColor},
    render_input::{RenderInput, ResolvedCommand as DrawCommand},
    text::resource::FontResource,
};

struct Canvas {
    pixels: Vec<u32>,
    width: u32,
    height: u32,
    scale: f32,
    clips: Vec<Rect>,
}

impl Canvas {
    fn text(&mut self, font: &FontResource, origin: Point, text: &str, color: Color) {
        for (destination, source) in
            crate::renderer::glyphs::positioned(font, origin, text, self.scale)
        {
            self.glyph(font, destination, source, color);
        }
    }

    fn glyph(
        &mut self,
        font: &FontResource,
        destination: Rect,
        source: pixui_engine::ui::text::resource::PixelRect,
        color: Color,
    ) {
        let visible = destination.intersect(*self.clips.last().expect("viewport clip"));
        if visible.width <= 0.0 || visible.height <= 0.0 {
            return;
        }
        let left = (visible.x * self.scale)
            .floor()
            .clamp(0.0, self.width as f32) as u32;
        let top = (visible.y * self.scale)
            .floor()
            .clamp(0.0, self.height as f32) as u32;
        let right = ((visible.x + visible.width) * self.scale)
            .ceil()
            .clamp(0.0, self.width as f32) as u32;
        let bottom = ((visible.y + visible.height) * self.scale)
            .ceil()
            .clamp(0.0, self.height as f32) as u32;
        let atlas = font.atlas();
        for y in top..bottom {
            for x in left..right {
                let point = Point {
                    x: (x as f32 + 0.5) / self.scale,
                    y: (y as f32 + 0.5) / self.scale,
                };
                if !visible.contains(point) {
                    continue;
                }
                let sx = ((point.x - destination.x) * font.scale())
                    .floor()
                    .clamp(0.0, (source.width - 1) as f32) as u32
                    + source.x;
                let sy = ((point.y - destination.y) * font.scale())
                    .floor()
                    .clamp(0.0, (source.height - 1) as f32) as u32
                    + source.y;
                let coverage = u32::from(atlas.coverage()[(sy * atlas.width() + sx) as usize]);
                let pixel = &mut self.pixels[(y * self.width + x) as usize];
                // Rounded interpolation in encoded sRGB, not linear-light or LCD AA.
                let blend = |foreground: u8, shift: u32| {
                    (u32::from(foreground) * coverage
                        + ((*pixel >> shift) & 255_u32) * (255 - coverage)
                        + 127)
                        / 255
                };
                *pixel = (blend(color.0, 16) << 16) | (blend(color.1, 8) << 8) | blend(color.2, 0);
            }
        }
    }

    fn gradient(&mut self, rect: Rect, gradient: LinearGradient) {
        let visible = rect.intersect(*self.clips.last().expect("viewport clip"));
        if visible.width <= 0.0 || visible.height <= 0.0 {
            return;
        }
        let (start, end) = gradient.endpoints(rect);
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let length_squared = dx * dx + dy * dy;
        let left = (visible.x * self.scale)
            .floor()
            .clamp(0.0, self.width as f32) as usize;
        let top = (visible.y * self.scale)
            .floor()
            .clamp(0.0, self.height as f32) as usize;
        let right = ((visible.x + visible.width) * self.scale)
            .ceil()
            .clamp(0.0, self.width as f32) as usize;
        let bottom = ((visible.y + visible.height) * self.scale)
            .ceil()
            .clamp(0.0, self.height as f32) as usize;
        // Project pixel centers onto the gradient in the unclipped rectangle.
        // Compute the coefficients once; clipping must not restart the gradient.
        let step = dx / (self.scale * length_squared);
        for y in top..bottom {
            let row = (((left as f32 + 0.5) / self.scale - start.x) * dx
                + ((y as f32 + 0.5) / self.scale - start.y) * dy)
                / length_squared;
            for x in left..right {
                let Color(r, g, b) = gradient.color_at(row + (x - left) as f32 * step);
                self.pixels[y * self.width as usize + x] =
                    (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
            }
        }
    }

    fn rounded(&mut self, rect: Rect, requested_radius: f32, brush: Brush, border: Option<f32>) {
        use crate::rounded_rect::{coverage, inset, radius};
        let visible = rect.intersect(*self.clips.last().expect("viewport clip"));
        if visible.width <= 0.0 || visible.height <= 0.0 {
            return;
        }
        let radius = radius(rect, requested_radius);
        let inner = border.map(|width| (inset(rect, width), (radius - width).max(0.0)));
        let projection = if let Brush::LinearGradient(gradient) = brush {
            let (start, end) = gradient.endpoints(rect);
            let dx = end.x - start.x;
            let dy = end.y - start.y;
            let length_squared = dx * dx + dy * dy;
            Some((start, dx / length_squared, dy / length_squared))
        } else {
            None
        };
        let left = (visible.x * self.scale)
            .floor()
            .clamp(0.0, self.width as f32) as usize;
        let top = (visible.y * self.scale)
            .floor()
            .clamp(0.0, self.height as f32) as usize;
        let right = ((visible.x + visible.width) * self.scale)
            .ceil()
            .clamp(0.0, self.width as f32) as usize;
        let bottom = ((visible.y + visible.height) * self.scale)
            .ceil()
            .clamp(0.0, self.height as f32) as usize;
        for y in top..bottom {
            for x in left..right {
                let point = Point {
                    x: (x as f32 + 0.5) / self.scale,
                    y: (y as f32 + 0.5) / self.scale,
                };
                if !visible.contains(point) {
                    continue;
                }
                let mut alpha = coverage(rect, radius, point, self.scale);
                if let Some((inner, radius)) = inner {
                    alpha = (alpha - coverage(inner, radius, point, self.scale)).max(0.0);
                }
                if alpha == 0.0 {
                    continue;
                }
                let color = match brush {
                    Brush::Solid(color) => color,
                    Brush::LinearGradient(gradient) => {
                        let (start, dx, dy) = projection.expect("gradient projection");
                        gradient.color_at((point.x - start.x) * dx + (point.y - start.y) * dy)
                    }
                };
                let pixel = &mut self.pixels[y * self.width as usize + x];
                let blend = |channel: u8, shift: u32| {
                    (f32::from(channel) * alpha + ((*pixel >> shift) & 255) as f32 * (1.0 - alpha))
                        .round() as u32
                };
                *pixel = (blend(color.0, 16) << 16) | (blend(color.1, 8) << 8) | blend(color.2, 0);
            }
        }
    }

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
pub fn paint<'a>(
    input: impl Into<RenderInput<'a>>,
    width: u32,
    height: u32,
    scale: f32,
) -> PixuiResult<Vec<u32>> {
    let input = input.into();
    input.validate()?;
    let display = input.display;
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
        let command = input.resolve(command)?;
        match &command {
            DrawCommand::FillRect {
                rect,
                brush,
                corner_radius,
            } if *corner_radius > 0.0 => {
                canvas.rounded(*rect, *corner_radius, *brush, None);
            }
            DrawCommand::FillRect { rect, brush, .. } => match brush {
                Brush::Solid(color) => canvas.fill(*rect, *color),
                Brush::LinearGradient(gradient) => canvas.gradient(*rect, *gradient),
            },
            DrawCommand::StrokeRect {
                rect,
                color,
                width,
                corner_radius,
            } if *corner_radius > 0.0 => {
                canvas.rounded(*rect, *corner_radius, (*color).into(), Some(*width));
            }
            DrawCommand::StrokeRect {
                rect, color, width, ..
            } => {
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
                text,
                font,
                color,
            } => {
                canvas.text(&display.fonts[*font], *origin, text, *color);
            }
            DrawCommand::DrawImage { image, destination } => {
                let source = &display.images[*image];
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
                        let index = sy * source.width() as usize + sx;
                        let RgbaColor(r, g, b, a) = match source.pixels() {
                            ImagePixels::Rgb {
                                pixels,
                                transparent_color,
                            } => {
                                let Color(r, g, b) = pixels[index];
                                RgbaColor(
                                    r,
                                    g,
                                    b,
                                    if *transparent_color == Some(pixels[index]) {
                                        0
                                    } else {
                                        255
                                    },
                                )
                            }
                            ImagePixels::Rgba { pixels } => pixels[index],
                        };
                        if a == 0 {
                            continue;
                        }
                        let pixel = &mut canvas.pixels[y as usize * width as usize + x as usize];
                        if a == 255 {
                            *pixel = (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
                            continue;
                        }
                        // Source-over into an opaque framebuffer, in encoded sRGB.
                        let blend = |channel: u8, shift: u32| {
                            (u32::from(channel) * u32::from(a)
                                + ((*pixel >> shift) & 255) * (255 - u32::from(a))
                                + 127)
                                / 255
                        };
                        *pixel = (blend(r, 16) << 16) | (blend(g, 8) << 8) | blend(b, 0);
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
