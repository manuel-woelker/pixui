use super::{context::PaintContext, painter::Painter, palette::Palette};
use crate::{
    components::checkbox::CheckboxComponent,
    ui::geometry::{Point, Rect},
};
use pixui_base::PixuiResult;

#[derive(Default)]
pub struct CheckboxPainter;
impl Painter<CheckboxComponent> for CheckboxPainter {
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
                x: 32.0,
                y: (context.height - 16.0) / 2.0,
            },
            &context.props.label,
            16.0,
            palette.foreground,
        );
        Ok(())
    }
}
