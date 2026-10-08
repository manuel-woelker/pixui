use super::{context::PaintContext, painter::Painter, palette::Palette};
use crate::{components::label::LabelComponent, ui::geometry::Point};
use pixui_base::PixuiResult;

const TEXT_SIZE: f32 = 16.0;

#[derive(Default)]
pub struct LabelPainter;
impl Painter<LabelComponent> for LabelPainter {
    fn measure(
        &self,
        context: &crate::painters::measure::MeasureContext<'_, LabelComponent>,
    ) -> PixuiResult<crate::ui::geometry::Size> {
        Ok(context.constrain(context.measure_text(&context.props.text, TEXT_SIZE)?))
    }
    fn paint(&self, context: &mut PaintContext<'_, LabelComponent>) -> PixuiResult<()> {
        context.text(
            Point {
                x: 0.0,
                y: context.centered_text_baseline(&context.props.text, TEXT_SIZE)?,
            },
            &context.props.text,
            TEXT_SIZE,
            Palette::for_theme(context.settings.theme).foreground,
        )?;
        Ok(())
    }
}
