//! Background fills, independent of component geometry and renderer resources.
use crate::ui::{
    display_list::Color,
    geometry::{Point, Rect},
};
use pixui_base::{PixuiResult, pixui_error};

/// Opaque sRGB fill. Gradients interpolate encoded sRGB channels and clamp to
/// their endpoint colors outside the gradient segment. No textures are needed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Brush {
    Solid(Color),
    LinearGradient(LinearGradient),
}

/// Two-color linear gradient in rectangle-relative coordinates: (0, 0) is its
/// top left, (1, 1) its bottom right. Clipping does not change this coordinate
/// system. Endpoints must be finite, within [0, 1], and distinct.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinearGradient {
    pub start: Point,
    pub end: Point,
    pub start_color: Color,
    pub end_color: Color,
}
impl LinearGradient {
    pub fn horizontal(start_color: Color, end_color: Color) -> Self {
        Self {
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 1.0, y: 0.0 },
            start_color,
            end_color,
        }
    }
    pub fn vertical(start_color: Color, end_color: Color) -> Self {
        Self {
            start: Point { x: 0.0, y: 0.0 },
            end: Point { x: 0.0, y: 1.0 },
            start_color,
            end_color,
        }
    }
    /// Logical endpoints in the original fill rectangle, before clipping.
    pub fn endpoints(self, rect: Rect) -> (Point, Point) {
        let point = |p: Point| Point {
            x: rect.x + p.x * rect.width,
            y: rect.y + p.y * rect.height,
        };
        (point(self.start), point(self.end))
    }
    /// Interpolates channels with rounding, clamping outside the segment.
    pub fn color_at(self, position: f32) -> Color {
        let t = position.clamp(0.0, 1.0);
        let channel =
            |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
        Color(
            channel(self.start_color.0, self.end_color.0),
            channel(self.start_color.1, self.end_color.1),
            channel(self.start_color.2, self.end_color.2),
        )
    }
}
impl From<Color> for Brush {
    fn from(color: Color) -> Self {
        Self::Solid(color)
    }
}
impl From<LinearGradient> for Brush {
    fn from(gradient: LinearGradient) -> Self {
        Self::LinearGradient(gradient)
    }
}
impl Brush {
    pub fn validate(self) -> PixuiResult<()> {
        let Self::LinearGradient(gradient) = self else {
            return Ok(());
        };
        if ![
            gradient.start.x,
            gradient.start.y,
            gradient.end.x,
            gradient.end.y,
        ]
        .into_iter()
        .all(|v| v.is_finite() && (0.0..=1.0).contains(&v))
            || gradient.start == gradient.end
        {
            return Err(pixui_error!("invalid linear gradient endpoints"));
        }
        Ok(())
    }
    pub(crate) fn key(self) -> BrushKey {
        match self {
            Self::Solid(color) => BrushKey::Solid(color),
            Self::LinearGradient(g) => BrushKey::Linear(
                [g.start.x, g.start.y, g.end.x, g.end.y]
                    .map(|v| if v == 0.0 { 0 } else { v.to_bits() }),
                g.start_color,
                g.end_color,
            ),
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum BrushKey {
    Solid(Color),
    Linear([u32; 4], Color, Color),
}
