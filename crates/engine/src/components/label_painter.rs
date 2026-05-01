use crate::components::label::LabelComponent;
use crate::draw::command::DrawCommand;
use crate::draw::painter::{ComponentPainter, PaintContext};
use pixui_base::result::PixuiResult;

pub struct LabelPainter;

impl ComponentPainter for LabelPainter {
    type Component = LabelComponent;

    fn paint(paint_context: &mut PaintContext<Self::Component>) -> PixuiResult<()> {
        paint_context.push_command(DrawCommand::DrawText {
            x: 0.0,
            y: 0.0,
            text: "Label".into(),
        });
        Ok(())
    }
}
