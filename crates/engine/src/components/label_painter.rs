use crate::components::label::LabelComponent;
use crate::draw::brush::Brush;
use crate::draw::color::Color;
use crate::draw::command::DrawCommand;
use crate::draw::draw_style::DrawStyle;
use crate::draw::painter::{ComponentPainter, PaintContext};
use crate::draw::text_style::TextStyle;
use pixui_base::result::PixuiResult;

pub struct LabelPainter;

impl ComponentPainter for LabelPainter {
    type Component = LabelComponent;

    fn paint(&self, paint_context: &mut PaintContext<Self::Component>) -> PixuiResult<()> {
        let style_id = paint_context.push_style(DrawStyle {
            brush: Brush::SolidColor(Color::rgba(255, 255, 255, 255)),
            width: 1.0,
            text_style: TextStyle::new("DejaVuSans", 18.0),
        });
        paint_context.push_command(DrawCommand::SelectStyle { style_id });
        paint_context.push_command(DrawCommand::DrawText {
            x: 0.0,
            y: 20.0,
            text: "Label".into(),
        });
        Ok(())
    }
}
