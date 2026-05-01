use crate::app::Application;
use crate::app::component_draw_renderer::ComponentDrawRenderer;
use crate::draw::draw_list::DrawList;
use crate::viewport::Viewport;
use pixui_base::result::{OptionExt, PixuiResult};
use pixui_base::shared_string::SharedString;
use std::collections::HashMap;

/// Stores named component renderers used to build draw lists.
#[derive(Default)]
pub struct ComponentDrawRegistry {
    renderers: HashMap<SharedString, Box<dyn ComponentDrawRenderer>>,
}

impl ComponentDrawRegistry {
    /// Registers or replaces a named component renderer.
    pub fn register<R>(&mut self, component_name: impl Into<SharedString>, renderer: R)
    where
        R: ComponentDrawRenderer + 'static,
    {
        self.renderers
            .insert(component_name.into(), Box::new(renderer));
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
            .get(component_name)
            .with_context(|| format!("no such component renderer: {component_name}"))?;
        renderer.render(application, viewport)
    }
}
