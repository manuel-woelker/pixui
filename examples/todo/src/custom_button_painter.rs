//! An alternative appearance for the standard button, retaining its behavior.

use pixui_base::PixuiResult;
use pixui_engine::{
    components::button::ButtonComponent,
    painters::{button::ButtonPainter, context::PaintContext, painter::Painter},
    ui::{display_list::Color, geometry::Rect},
};

pub struct CustomButtonPainter;

impl Painter<ButtonComponent> for CustomButtonPainter {
    fn measure(
        &self,
        context: &pixui_engine::painters::measure::MeasureContext<'_, ButtonComponent>,
    ) -> PixuiResult<pixui_engine::ui::geometry::Size> {
        ButtonPainter.measure(context)
    }
    fn paint(&self, context: &mut PaintContext<'_, ButtonComponent>) -> PixuiResult<()> {
        ButtonPainter.paint(context)?;
        context.fill_rect(
            Rect {
                x: 0.0,
                y: 0.0,
                width: context.width.min(4.0),
                height: context.height,
            },
            Color(210, 90, 40),
        );
        Ok(())
    }
}
