/// A drawing command emitted by the engine.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawCommand {
    /// Selects the style used by following draw commands.
    SelectStyle { style_id: StyleId },

    /// Draws a filled rounded rectangle using the selected style.
    FillRoundedRectangle {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: f32,
    },

    /// Draws an outlined rounded rectangle using the selected style.
    OutlineRoundedRectangle {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: f32,
    },
}

/// Identifier for a style in the draw command stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StyleId(usize);

impl StyleId {
    /// Creates a new style identifier.
    pub fn new(index: usize) -> Self {
        Self(index)
    }

    /// Returns the raw style index.
    pub fn index(self) -> usize {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::{DrawCommand, StyleId};

    #[test]
    fn rounded_rectangle_commands_do_not_embed_style() {
        let commands = [
            DrawCommand::SelectStyle {
                style_id: StyleId::new(2),
            },
            DrawCommand::FillRoundedRectangle {
                x: 10.0,
                y: 20.0,
                width: 120.0,
                height: 48.0,
                radius: 8.0,
            },
            DrawCommand::OutlineRoundedRectangle {
                x: 10.0,
                y: 20.0,
                width: 120.0,
                height: 48.0,
                radius: 8.0,
            },
        ];

        assert_eq!(
            commands[0],
            DrawCommand::SelectStyle {
                style_id: StyleId::new(2)
            }
        );
    }
}
