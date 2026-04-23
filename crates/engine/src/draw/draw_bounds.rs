/// Explicit bounds for a draw list.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawBounds {
    /// Left edge of the drawing area.
    pub x: f32,
    /// Top edge of the drawing area.
    pub y: f32,
    /// Width of the drawing area.
    pub width: f32,
    /// Height of the drawing area.
    pub height: f32,
}

impl DrawBounds {
    /// Creates explicit draw bounds.
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}
