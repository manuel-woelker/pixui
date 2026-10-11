//! Standard single-line field painter, sharing caret geometry with hit testing.
use crate::{
    components::text_input::TextInputComponent,
    painters::{context::PaintContext, measure::MeasureContext, painter::Painter},
    ui::{
        geometry::{Point, Rect, Size},
        text_input::geometry::TextInputGeometry,
    },
};
use pixui_base::PixuiResult;
const BLINK_US: u64 = 500_000;
#[pixui_reflect::reflect(send)]
mod reflected {
    use super::*;
    use crate::styles::index::StyleIndex;
    #[derive(Default)]
    pub struct TextInputPainter {
        pub body: StyleIndex,
        pub selection: StyleIndex,
        pub caret: StyleIndex,
        pub focus_ring: StyleIndex,
    }

    impl Painter<TextInputComponent> for TextInputPainter {
        fn register(
            self,
            registry: &mut crate::painters::registry::PainterRegistry,
            components: &crate::component_registry::registry::ComponentRegistry,
            styles: &mut crate::styles::registry::StyleRegistry,
        ) -> PixuiResult<()> {
            let defaults = crate::styles::standard::assignments(
                styles,
                &[
                    ("body", "input"),
                    ("selection", "selection"),
                    ("caret", "caret"),
                    ("focus_ring", "input_focus"),
                ],
            )?;
            registry.register_styled::<TextInputComponent, Self>(components, styles, defaults)
        }

        fn measure(&self, context: &MeasureContext<'_, TextInputComponent>) -> PixuiResult<Size> {
            Ok(context.constrain(Size {
                width: 240.0,
                height: context
                    .style(self.body)?
                    .font_config(context.settings.scale_factor)?
                    .metrics()
                    .line_height
                    + context.style(self.body)?.inset * 2.0,
            }))
        }
        fn text_input_geometry(
            &self,
            context: &MeasureContext<'_, TextInputComponent>,
        ) -> PixuiResult<Option<TextInputGeometry>> {
            Ok(Some(TextInputGeometry::new(
                &context.props.content,
                &context
                    .style(self.body)?
                    .font_config(context.settings.scale_factor)?,
                context.style(self.body)?.inset,
            )?))
        }
        fn paint(&self, context: &mut PaintContext<'_, TextInputComponent>) -> PixuiResult<()> {
            context.styled_fill_rect(context.bounds(), self.body)?;
            context.styled_stroke_rect(
                context.bounds(),
                if context.focused {
                    self.focus_ring
                } else {
                    self.body
                },
            )?;
            let inset = context.style(self.body)?.inset;
            let editing = context.text_edit.cloned();
            let scroll = editing.as_ref().map_or(0.0, |edit| edit.scroll);
            let baseline = context.styled_baseline(&context.props.content, self.body)?;
            let line_height = context
                .style(self.body)?
                .font_config(context.settings.scale_factor)?
                .metrics()
                .line_height;
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
                    context.styled_fill_rect(
                        Rect {
                            x: inset + edit.geometry.x(range.start) - scroll,
                            y: top,
                            width: edit.geometry.x(range.end) - edit.geometry.x(range.start),
                            height: line_height,
                        },
                        self.selection,
                    )?;
                }
                context.styled_text(
                    Point {
                        x: inset - scroll,
                        y: baseline,
                    },
                    context.props.content.clone(),
                    self.body,
                )?;
                if context.focused
                    && let Some(edit) = &editing
                    && edit.active
                {
                    let elapsed = context.timestamp_us.saturating_sub(edit.blink_reset_us);
                    if edit.selection.collapsed() && (elapsed / BLINK_US).is_multiple_of(2) {
                        context.styled_fill_rect(
                            Rect {
                                x: inset + edit.geometry.x(edit.selection.head) - scroll,
                                y: top,
                                width: 1.0,
                                height: line_height,
                            },
                            self.caret,
                        )?;
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
}
pub use reflected::TextInputPainter;
