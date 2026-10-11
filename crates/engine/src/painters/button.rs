//! Theme-derived standard painter with reflected, occurrence-bound slots.
use super::{context::PaintContext, painter::Painter};
use crate::{
    components::button::ButtonComponent,
    styles::index::StyleIndex,
    ui::geometry::{Point, Size},
};
use pixui_base::PixuiResult;
#[pixui_reflect::reflect(send)]
mod reflected {
    use super::*;
    #[derive(Default)]
    pub struct ButtonPainter {
        pub body: StyleIndex,
        pub focus_ring: StyleIndex,
    }
    impl Painter<ButtonComponent> for ButtonPainter {
        fn register(
            self,
            registry: &mut crate::painters::registry::PainterRegistry,
            components: &crate::component_registry::registry::ComponentRegistry,
            styles: &mut crate::styles::registry::StyleRegistry,
        ) -> PixuiResult<()> {
            let defaults = crate::styles::standard::assignments(
                styles,
                &[("body", "control"), ("focus_ring", "focus")],
            )?;
            registry.register_styled::<ButtonComponent, Self>(components, styles, defaults)
        }

        fn measure(
            &self,
            context: &crate::painters::measure::MeasureContext<'_, ButtonComponent>,
        ) -> PixuiResult<Size> {
            let style = context.style(self.body)?;
            let text = style
                .font_config(context.settings.scale_factor)?
                .measure_text(&context.props.label)?;
            Ok(context.constrain(Size {
                width: text.width + 2.0 * style.inset,
                height: text.height.max(style.minimum_height),
            }))
        }
        fn paint(&self, context: &mut PaintContext<'_, ButtonComponent>) -> PixuiResult<()> {
            context.styled_fill_rect(context.bounds(), self.body)?;
            if context.focused || context.hovered {
                context.styled_stroke_rect(context.bounds(), self.focus_ring)?;
            }
            context.styled_text(
                Point {
                    x: context.style(self.body)?.inset,
                    y: context.styled_baseline(&context.props.label, self.body)?,
                },
                &context.props.label,
                self.body,
            )
        }
        fn style_states(
            _: &crate::components::button::ButtonProps,
            state: &crate::components::button::ButtonState,
        ) -> crate::styles::style::StyleStates {
            crate::styles::style::StyleStates {
                active: state.active,
                ..Default::default()
            }
        }
    }
}
pub use reflected::ButtonPainter;
