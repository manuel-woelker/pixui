use super::{context::PaintContext, painter::Painter, palette::Palette};
use crate::{
    components::checkbox::CheckboxComponent,
    ui::geometry::{Point, Rect},
};
use pixui_base::PixuiResult;

const TEXT_X: f32 = 32.0;
const TRAILING_INSET: f32 = 8.0;
const MIN_HEIGHT: f32 = 32.0;
const TEXT_SIZE: f32 = 16.0;

#[derive(Default)]
pub struct CheckboxPainter;
impl Painter<CheckboxComponent> for CheckboxPainter {
    fn measure(
        &self,
        context: &crate::painters::measure::MeasureContext<'_, CheckboxComponent>,
    ) -> PixuiResult<crate::ui::geometry::Size> {
        let text = context.measure_text(&context.props.label, TEXT_SIZE)?;
        Ok(context.constrain(crate::ui::geometry::Size {
            width: text.width + TEXT_X + TRAILING_INSET,
            height: text.height.max(MIN_HEIGHT),
        }))
    }
    fn paint(&self, context: &mut PaintContext<'_, CheckboxComponent>) -> PixuiResult<()> {
        let palette = Palette::for_theme(context.settings.theme);
        context.fill_rect(context.bounds(), palette.control);
        if context.focused || context.hovered {
            context.stroke_rect(context.bounds(), palette.accent, 2.0);
        }
        let checkbox = Rect {
            x: 6.0,
            y: (context.height - 18.0) / 2.0,
            width: 18.0,
            height: 18.0,
        };
        context.stroke_rect(checkbox, palette.foreground, 2.0);
        if context.props.checked {
            context.fill_rect(
                Rect {
                    x: checkbox.x + 4.0,
                    y: checkbox.y + 4.0,
                    width: 10.0,
                    height: 10.0,
                },
                palette.accent,
            );
        }
        context.text(
            Point {
                x: TEXT_X,
                y: context.centered_text_baseline(&context.props.label, TEXT_SIZE)?,
            },
            &context.props.label,
            TEXT_SIZE,
            palette.foreground,
        )?;
        Ok(())
    }
}
