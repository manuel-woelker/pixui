use crate::draw::brush::Brush;
use crate::draw::text_style::TextStyle;

/// Style data referenced by draw commands.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawStyle {
    /// Brush used to paint the shape.
    pub brush: Brush,
    /// Stroke width used by outline operations.
    pub width: f32,
    /// Text style used by text draw operations.
    pub text_style: TextStyle,
}
