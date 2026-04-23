use crate::draw::command::DrawCommand;
use crate::draw::draw_bounds::DrawBounds;
use crate::draw::draw_style::DrawStyle;
use crate::draw::style_id::StyleId;

/// A complete draw list containing styles and drawing commands.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawList {
    /// Explicit bounds of the drawing area.
    pub bounds: DrawBounds,
    /// Styles referenced by the command stream.
    pub styles: Vec<DrawStyle>,
    /// Commands executed in order.
    pub commands: Vec<DrawCommand>,
}

impl DrawList {
    /// Creates a draw list with explicit bounds.
    pub fn new(bounds: DrawBounds) -> Self {
        Self {
            bounds,
            styles: Vec::new(),
            commands: Vec::new(),
        }
    }

    /// Adds a style and returns its identifier.
    pub fn push_style(&mut self, style: DrawStyle) -> StyleId {
        let style_id = StyleId::new(self.styles.len());
        self.styles.push(style);
        style_id
    }

    /// Appends a draw command to the list.
    pub fn push_command(&mut self, command: DrawCommand) {
        self.commands.push(command);
    }
}

#[cfg(test)]
mod tests {
    use super::DrawList;
    use crate::draw::brush::Brush;
    use crate::draw::color::Color;
    use crate::draw::command::DrawCommand;
    use crate::draw::draw_bounds::DrawBounds;
    use crate::draw::draw_style::DrawStyle;

    #[test]
    fn push_style_returns_the_inserted_style_id() {
        let mut draw_list = DrawList::new(DrawBounds::new(0.0, 0.0, 100.0, 50.0));

        let style_id = draw_list.push_style(DrawStyle {
            brush: Brush::SolidColor(Color::rgba(10, 20, 30, 255)),
            width: 2.0,
        });

        assert_eq!(style_id.index(), 0);
        assert_eq!(draw_list.styles.len(), 1);
    }

    #[test]
    fn draw_list_stores_commands_in_order() {
        let mut draw_list = DrawList::new(DrawBounds::new(0.0, 0.0, 100.0, 50.0));

        draw_list.push_command(DrawCommand::FillRoundedRectangle {
            x: 1.0,
            y: 2.0,
            width: 3.0,
            height: 4.0,
            radius: 5.0,
        });

        assert_eq!(draw_list.commands.len(), 1);
    }

    #[test]
    fn draw_list_keeps_explicit_bounds() {
        let draw_list = DrawList::new(DrawBounds::new(10.0, 20.0, 300.0, 200.0));

        assert_eq!(draw_list.bounds, DrawBounds::new(10.0, 20.0, 300.0, 200.0));
    }
}
