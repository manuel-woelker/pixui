use crate::draw::color::Color;

/// A brush used to paint draw commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Brush {
    /// Paints with a solid color.
    SolidColor(Color),
}
