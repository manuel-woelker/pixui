//! Standard single-line field painter, sharing caret geometry with hit testing.
use crate::{
    components::text_input::TextInputComponent,
    painters::{
        context::PaintContext, measure::MeasureContext, painter::Painter, palette::Palette,
    },
    ui::{
        geometry::{Point, Rect, Size},
        text::font::{FontConfig, FontFace},
        text_input::geometry::TextInputGeometry,
    },
};
use pixui_base::PixuiResult;
const TEXT_SIZE: f32 = 16.0;
const INSET: f32 = 8.0;
const BLINK_US: u64 = 500_000;
pub struct TextInputPainter;
impl Painter<TextInputComponent> for TextInputPainter {
    fn measure(&self, context: &MeasureContext<'_, TextInputComponent>) -> PixuiResult<Size> {
        Ok(context.constrain(Size {
            width: 240.0,
            height: context.line_height(TEXT_SIZE)? + INSET * 2.0,
        }))
    }
    fn text_input_geometry(
        &self,
        context: &MeasureContext<'_, TextInputComponent>,
    ) -> PixuiResult<Option<TextInputGeometry>> {
        Ok(Some(TextInputGeometry::new(
            &context.props.content,
            &FontConfig::new(FontFace::geist()?, TEXT_SIZE, context.settings.scale_factor)?,
            INSET,
        )?))
    }
    fn paint(&self, context: &mut PaintContext<'_, TextInputComponent>) -> PixuiResult<()> {
        let palette = Palette::for_theme(context.settings.theme);
        context.fill_rect(context.bounds(), palette.control);
        context.stroke_rect(
            context.bounds(),
            if context.focused {
                palette.accent
            } else {
                palette.foreground
            },
            1.0,
        );
        let editing = context.text_edit.cloned();
        let scroll = editing.as_ref().map_or(0.0, |edit| edit.scroll);
        let baseline = context.centered_text_baseline(&context.props.content, TEXT_SIZE)?;
        let line_height = context.line_height(TEXT_SIZE)?;
        let top = (context.height - line_height) / 2.0;
        let clip = Rect {
            x: 1.0,
            y: 1.0,
            width: (context.width - 2.0).max(0.0),
            height: (context.height - 2.0).max(0.0),
        };
        context.with_clip(clip, |context| {
            if context.focused
                && let Some(edit) = &editing
            {
                let range = edit.selection.range();
                context.fill_rect(
                    Rect {
                        x: INSET + edit.geometry.x(range.start) - scroll,
                        y: top,
                        width: edit.geometry.x(range.end) - edit.geometry.x(range.start),
                        height: line_height,
                    },
                    palette.accent,
                );
            }
            context.text(
                Point {
                    x: INSET - scroll,
                    y: baseline,
                },
                context.props.content.clone(),
                TEXT_SIZE,
                palette.foreground,
            )?;
            if context.focused
                && let Some(edit) = &editing
                && edit.active
            {
                let elapsed = context.timestamp_us.saturating_sub(edit.blink_reset_us);
                if edit.selection.collapsed() && (elapsed / BLINK_US).is_multiple_of(2) {
                    context.fill_rect(
                        Rect {
                            x: INSET + edit.geometry.x(edit.selection.head) - scroll,
                            y: top,
                            width: 1.0,
                            height: line_height,
                        },
                        palette.foreground,
                    );
                }
                if edit.selection.collapsed() && context.settings.timestamp_us.is_none() {
                    context.request_redraw_after(std::time::Duration::from_micros(
                        BLINK_US - elapsed % BLINK_US,
                    ));
                }
            }
            Ok(())
        })
    }
}
