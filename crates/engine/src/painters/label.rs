//! Theme-derived standard painter with reflected, occurrence-bound slots.
use super::{context::PaintContext, painter::Painter};
use crate::{
    components::label::LabelComponent,
    styles::index::StyleIndex,
    ui::geometry::{Point, Size},
};
use pixui_base::PixuiResult;
#[pixui_reflect::reflect(send)]
mod reflected {
    use super::*;
    #[derive(Default)]
    pub struct LabelPainter {
        pub text: StyleIndex,
    }
    impl Painter<LabelComponent> for LabelPainter {
        fn register(
            self,
            registry: &mut crate::painters::registry::PainterRegistry,
            components: &crate::component_registry::registry::ComponentRegistry,
            styles: &mut crate::styles::registry::StyleRegistry,
        ) -> PixuiResult<()> {
            let defaults = crate::styles::standard::assignments(styles, &[("text", "text")])?;
            registry.register_styled::<LabelComponent, Self>(components, styles, defaults)
        }

        fn measure(
            &self,
            context: &crate::painters::measure::MeasureContext<'_, LabelComponent>,
        ) -> PixuiResult<Size> {
            Ok(context.constrain(
                context
                    .style(self.text)?
                    .font_config(context.settings.scale_factor)?
                    .measure_text(&context.props.text)?,
            ))
        }
        fn paint(&self, context: &mut PaintContext<'_, LabelComponent>) -> PixuiResult<()> {
            context.styled_text(
                Point {
                    x: 0.0,
                    y: context.styled_baseline(&context.props.text, self.text)?,
                },
                &context.props.text,
                self.text,
            )
        }
    }
}
pub use reflected::LabelPainter;
