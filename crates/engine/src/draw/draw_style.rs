use crate::draw::brush::Brush;

/// Style data referenced by draw commands.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawStyle {
    /// Brush used to paint the shape.
    pub brush: Brush,
    /// Stroke width used by outline operations.
    pub width: f32,
}
