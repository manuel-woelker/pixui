//! Theme-derived standard painter with reflected, occurrence-bound slots.
use super::{context::PaintContext, painter::Painter};
use crate::{
    components::checkbox::CheckboxComponent,
    styles::index::StyleIndex,
    ui::geometry::{Point, Rect, Size},
};
use pixui_base::PixuiResult;
#[pixui_reflect::reflect(send)]
mod reflected {
    use super::*;
    #[derive(Default)]
    pub struct CheckboxPainter {
        pub body: StyleIndex,
        pub box_style: StyleIndex,
        pub mark: StyleIndex,
        pub focus_ring: StyleIndex,
    }
    impl Painter<CheckboxComponent> for CheckboxPainter {
        fn register(
            self,
            registry: &mut crate::painters::registry::PainterRegistry,
            components: &crate::component_registry::registry::ComponentRegistry,
            styles: &mut crate::styles::registry::StyleRegistry,
        ) -> PixuiResult<()> {
            let defaults = crate::styles::standard::assignments(
                styles,
                &[
                    ("body", "checkbox"),
                    ("box_style", "box"),
                    ("mark", "mark"),
                    ("focus_ring", "focus"),
                ],
            )?;
            registry.register_styled::<CheckboxComponent, Self>(components, styles, defaults)
        }

        fn measure(
            &self,
            context: &crate::painters::measure::MeasureContext<'_, CheckboxComponent>,
        ) -> PixuiResult<Size> {
            let style = context.style(self.body)?;
            let text = style
                .font_config(context.settings.scale_factor)?
                .measure_text(&context.props.label)?;
            Ok(context.constrain(Size {
                width: text.width + 32.0 + style.inset,
                height: text.height.max(style.minimum_height),
            }))
        }
        fn paint(&self, context: &mut PaintContext<'_, CheckboxComponent>) -> PixuiResult<()> {
            context.styled_fill_rect(context.bounds(), self.body)?;
            if context.focused || context.hovered {
                context.styled_stroke_rect(context.bounds(), self.focus_ring)?;
            }
            let checkbox = Rect {
                x: 6.0,
                y: (context.height - 18.0) / 2.0,
                width: 18.0,
                height: 18.0,
            };
            context.styled_stroke_rect(checkbox, self.box_style)?;
            if context.props.checked {
                context.styled_fill_rect(
                    Rect {
                        x: checkbox.x + 4.0,
                        y: checkbox.y + 4.0,
                        width: 10.0,
                        height: 10.0,
                    },
                    self.mark,
                )?;
            }
            context.styled_text(
                Point {
                    x: 32.0,
                    y: context.styled_baseline(&context.props.label, self.body)?,
                },
                &context.props.label,
                self.body,
            )
        }
        fn style_states(
            props: &crate::components::checkbox::CheckboxProps,
            _: &crate::components::checkbox::CheckboxState,
        ) -> crate::styles::style::StyleStates {
            crate::styles::style::StyleStates {
                active: props.checked,
                ..Default::default()
            }
        }
    }
}
pub use reflected::CheckboxPainter;
