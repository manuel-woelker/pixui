use super::{context::PaintContext, painter::Painter, palette::Palette};
use crate::{components::button::ButtonComponent, ui::geometry::Point};
use pixui_base::PixuiResult;

const TEXT_INSET: f32 = 8.0;
const MIN_HEIGHT: f32 = 36.0;
const TEXT_SIZE: f32 = 16.0;

#[derive(Default)]
pub struct ButtonPainter;
impl Painter<ButtonComponent> for ButtonPainter {
    fn measure(
        &self,
        context: &crate::painters::measure::MeasureContext<'_, ButtonComponent>,
    ) -> PixuiResult<crate::ui::geometry::Size> {
        let text = context.measure_text(&context.props.label, TEXT_SIZE)?;
        Ok(context.constrain(crate::ui::geometry::Size {
            width: text.width + 2.0 * TEXT_INSET,
            height: text.height.max(MIN_HEIGHT),
        }))
    }
    fn paint(&self, context: &mut PaintContext<'_, ButtonComponent>) -> PixuiResult<()> {
        let palette = Palette::for_theme(context.settings.theme);
        context.fill_rect(
            context.bounds(),
            if context.state.active {
                palette.accent
            } else {
                palette.control
            },
        );
        if context.focused || context.hovered {
            context.stroke_rect(context.bounds(), palette.accent, 2.0);
        }
        context.text(
            Point {
                x: TEXT_INSET,
                y: context.centered_text_baseline(&context.props.label, TEXT_SIZE)?,
            },
            &context.props.label,
            TEXT_SIZE,
            palette.foreground,
        )?;
        Ok(())
    }
}
