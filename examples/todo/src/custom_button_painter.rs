//! An alternative appearance for the standard button, retaining its behavior.

use pixui_base::PixuiResult;
use pixui_engine::{
    components::button::ButtonComponent,
    painters::{button::ButtonPainter, context::PaintContext, painter::Painter},
    ui::{display_list::Color, geometry::Rect},
};

#[pixui_reflect::reflect(send)]
mod reflected {
    use super::*;
    use pixui_engine::styles::index::StyleIndex;
    #[derive(Default)]
    pub struct CustomButtonPainter {
        pub body: StyleIndex,
        pub focus_ring: StyleIndex,
    }

    impl Painter<ButtonComponent> for CustomButtonPainter {
        fn register(
            self,
            registry: &mut pixui_engine::painters::registry::PainterRegistry,
            components: &pixui_engine::component_registry::registry::ComponentRegistry,
            styles: &mut pixui_engine::styles::registry::StyleRegistry,
        ) -> PixuiResult<()> {
            let defaults = pixui_engine::styles::standard::assignments(
                styles,
                &[("body", "control"), ("focus_ring", "focus")],
            )?;
            registry.register_styled::<ButtonComponent, Self>(components, styles, defaults)
        }

        fn measure(
            &self,
            context: &pixui_engine::painters::measure::MeasureContext<'_, ButtonComponent>,
        ) -> PixuiResult<pixui_engine::ui::geometry::Size> {
            ButtonPainter {
                body: self.body,
                focus_ring: self.focus_ring,
            }
            .measure(context)
        }
        fn paint(&self, context: &mut PaintContext<'_, ButtonComponent>) -> PixuiResult<()> {
            ButtonPainter {
                body: self.body,
                focus_ring: self.focus_ring,
            }
            .paint(context)?;
            context.fill_rect(
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: context.width.min(4.0),
                    height: context.height,
                },
                Color(210, 90, 40),
            );
            Ok(())
        }
    }
}
pub use reflected::CustomButtonPainter;
