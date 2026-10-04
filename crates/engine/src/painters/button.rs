use super::{context::PaintContext, painter::Painter, palette::Palette};
use crate::{components::button::ButtonComponent, ui::geometry::Point};
use pixui_base::PixuiResult;

#[derive(Default)]
pub struct ButtonPainter;
impl Painter<ButtonComponent> for ButtonPainter {
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
                x: 8.0,
                y: (context.height - 16.0) / 2.0,
            },
            &context.props.label,
            16.0,
            palette.foreground,
        );
        Ok(())
    }
}
