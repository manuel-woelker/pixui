use super::{context::PaintContext, painter::Painter, palette::Palette};
use crate::{components::label::LabelComponent, ui::geometry::Point};
use pixui_base::PixuiResult;

#[derive(Default)]
pub struct LabelPainter;
impl Painter<LabelComponent> for LabelPainter {
    fn paint(&self, context: &mut PaintContext<'_, LabelComponent>) -> PixuiResult<()> {
        context.text(
            Point {
                x: 0.0,
                y: context
                    .font_metrics(16.0)?
                    .centered_baseline(context.height),
            },
            &context.props.text,
            16.0,
            Palette::for_theme(context.settings.theme).foreground,
        )?;
        Ok(())
    }
}
