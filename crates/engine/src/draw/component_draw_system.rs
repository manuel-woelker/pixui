use crate::app::Application;
use crate::component::Component;
use crate::draw::component_draw_renderer::ComponentDrawRenderer;
use crate::draw::draw_bounds::DrawBounds;
use crate::draw::draw_list::DrawList;
use crate::draw::painter::{ComponentPainter, PaintContext};
use crate::viewport::Viewport;
use pixui_base::RwLock;
use pixui_base::result::{OptionExt, PixuiResult};
use pixui_base::shared_string::SharedString;
use std::collections::HashMap;
use std::sync::Arc;

/// Winit-agnostic component drawing system that maps names to draw-list producers.
#[derive(Clone, Default)]
pub struct ComponentDrawSystem {
    renderers: Arc<RwLock<HashMap<SharedString, Arc<dyn ComponentDrawRenderer>>>>,
}

impl ComponentDrawSystem {
    /// Registers or replaces a named component renderer.
    pub fn register_component_renderer<R>(
        &self,
        component_name: impl Into<SharedString>,
        renderer: R,
    ) where
        R: ComponentDrawRenderer + 'static,
    {
        self.renderers
            .write()
            .insert(component_name.into(), Arc::new(renderer));
    }

    /// Registers a named component painter by wrapping it in a draw-list renderer.
    pub fn register_component_painter<P>(&self, component_name: impl Into<SharedString>, painter: P)
    where
        P: ComponentPainter + Send + Sync + 'static,
        P::Component: Component,
    {
        self.register_component_renderer(component_name, PainterRenderer { painter });
    }

    /// Renders a named component for the provided viewport.
    pub fn render_component(
        &self,
        application: &Application,
        component_name: &str,
        viewport: &Viewport,
    ) -> PixuiResult<DrawList> {
        let renderer = self
            .renderers
            .read()
            .get(component_name)
            .cloned()
            .with_context(|| format!("no such component renderer: {component_name}"))?;
        renderer.render(application, viewport)
    }
}

struct PainterRenderer<P: ComponentPainter> {
    painter: P,
}

impl<P> ComponentDrawRenderer for PainterRenderer<P>
where
    P: ComponentPainter + Send + Sync + 'static,
    P::Component: Component,
{
    fn render(&self, _application: &Application, viewport: &Viewport) -> PixuiResult<DrawList> {
        let mut draw_list =
            DrawList::new(DrawBounds::new(0.0, 0.0, viewport.width, viewport.height));
        let mut paint_context = PaintContext::<P::Component>::new(&mut draw_list);
        self.painter.paint(&mut paint_context)?;
        Ok(draw_list)
    }
}

#[cfg(test)]
mod tests {
    use super::ComponentDrawSystem;
    use crate::app::Application;
    use crate::draw::draw_bounds::DrawBounds;
    use crate::draw::draw_list::DrawList;
    use crate::viewport::Viewport;
    use pixui_base::result::PixuiResult;

    #[test]
    fn register_component_renderer_renders_named_components() {
        let draw_system = ComponentDrawSystem::default();
        draw_system.register_component_renderer("CounterApp", render_counter);

        let draw_list = draw_system
            .render_component(
                &Application::new(),
                "CounterApp",
                &Viewport::new(320.0, 240.0, 1.0),
            )
            .unwrap();

        assert_eq!(draw_list.bounds, DrawBounds::new(0.0, 0.0, 320.0, 240.0));
    }

    #[test]
    fn render_component_returns_context_for_unknown_component_names() {
        let draw_system = ComponentDrawSystem::default();

        let error = draw_system
            .render_component(
                &Application::new(),
                "Missing",
                &Viewport::new(320.0, 240.0, 1.0),
            )
            .unwrap_err();

        assert!(
            error
                .to_test_string()
                .contains("no such component renderer: Missing")
        );
    }

    fn render_counter(_application: &Application, viewport: &Viewport) -> PixuiResult<DrawList> {
        Ok(DrawList::new(DrawBounds::new(
            0.0,
            0.0,
            viewport.width,
            viewport.height,
        )))
    }
}
