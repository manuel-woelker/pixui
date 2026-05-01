use crate::app::Application;
use crate::component::Component;
use crate::draw::component_draw_renderer::ComponentDrawRenderer;
use crate::draw::draw_bounds::DrawBounds;
use crate::draw::draw_list::DrawList;
use crate::draw::painter::{ComponentPainter, PaintContext};
use crate::ui_description::{UiElement, parse_ui_description, parse_ui_description_source_file};
use crate::viewport::Viewport;
use pixui_base::RwLock;
use pixui_base::error::PixuiError;
use pixui_base::result::{OptionExt, PixuiResult};
use pixui_base::shared_string::SharedString;
use pixui_base::source_file::SourceFile;
use std::collections::HashMap;
use std::sync::Arc;

/// Winit-agnostic component drawing system that maps names to draw-list producers.
#[derive(Clone, Default)]
pub struct ComponentDrawSystem {
    component_descriptions: Arc<RwLock<HashMap<SharedString, UiElement>>>,
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

    /// Registers a custom component backed by a parsed UI description tree.
    pub fn register_component_description(
        &self,
        component_name: impl Into<SharedString>,
        ui_description: &str,
    ) -> PixuiResult<()> {
        let parsed = parse_ui_description(ui_description)?;
        self.register_parsed_component_description(component_name, parsed);
        Ok(())
    }

    /// Registers a custom component backed by a source file.
    pub fn register_component_description_source_file(
        &self,
        component_name: impl Into<SharedString>,
        source_file: &SourceFile,
    ) -> PixuiResult<()> {
        let parsed = parse_ui_description_source_file(source_file)?;
        self.register_parsed_component_description(component_name, parsed);
        Ok(())
    }

    fn register_parsed_component_description(
        &self,
        component_name: impl Into<SharedString>,
        parsed: UiElement,
    ) {
        self.component_descriptions
            .write()
            .insert(component_name.into(), parsed);
    }

    /// Renders a named component for the provided viewport.
    pub fn render_component(
        &self,
        application: &Application,
        component_name: &str,
        viewport: &Viewport,
    ) -> PixuiResult<DrawList> {
        self.render_component_by_name(application, component_name, viewport, &mut Vec::new())
    }

    fn render_component_by_name(
        &self,
        application: &Application,
        component_name: &str,
        viewport: &Viewport,
        render_stack: &mut Vec<SharedString>,
    ) -> PixuiResult<DrawList> {
        if let Some(renderer) = self.renderers.read().get(component_name).cloned() {
            return renderer.render(application, viewport);
        }

        let component_name = SharedString::from(component_name);
        if render_stack.contains(&component_name) {
            let cycle = render_stack
                .iter()
                .cloned()
                .chain(std::iter::once(component_name.clone()))
                .map(|name| name.to_string())
                .collect::<Vec<_>>()
                .join(" -> ");
            return Err(PixuiError::message(format!(
                "cyclic ui_description component reference: {cycle}"
            )));
        }

        let element = self
            .component_descriptions
            .read()
            .get(&component_name)
            .cloned()
            .with_context(|| format!("no such component renderer: {component_name}"))?;
        render_stack.push(component_name);
        let result = self.render_element(application, &element, viewport, render_stack);
        render_stack.pop();
        result
    }

    fn render_element(
        &self,
        application: &Application,
        element: &UiElement,
        viewport: &Viewport,
        render_stack: &mut Vec<SharedString>,
    ) -> PixuiResult<DrawList> {
        if !element.properties.is_empty() {
            return Err(PixuiError::message(format!(
                "ui_description properties are not supported yet for component `{}`",
                element.tag_name
            )));
        }

        if self.renderers.read().contains_key(&element.tag_name) {
            if !element.children.is_empty() {
                return Err(PixuiError::message(format!(
                    "leaf component `{}` cannot have children in ui_description",
                    element.tag_name
                )));
            }

            return self.render_component_by_name(
                application,
                &element.tag_name,
                viewport,
                render_stack,
            );
        }

        if self
            .component_descriptions
            .read()
            .contains_key(&element.tag_name)
        {
            if !element.children.is_empty() {
                return Err(PixuiError::message(format!(
                    "custom component `{}` cannot have inline children in ui_description yet",
                    element.tag_name
                )));
            }

            return self.render_component_by_name(
                application,
                &element.tag_name,
                viewport,
                render_stack,
            );
        }

        let mut draw_list =
            DrawList::new(DrawBounds::new(0.0, 0.0, viewport.width, viewport.height));
        for child in &element.children {
            draw_list.append(self.render_element(application, child, viewport, render_stack)?);
        }
        Ok(draw_list)
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
    use crate::draw::command::DrawCommand;
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

    #[test]
    fn register_component_description_renders_composite_components() {
        let draw_system = ComponentDrawSystem::default();
        draw_system.register_component_renderer("Label", render_label);
        draw_system
            .register_component_description("App", "<Stack><Label /><Label /></Stack>")
            .unwrap();

        let draw_list = draw_system
            .render_component(
                &Application::new(),
                "App",
                &Viewport::new(320.0, 240.0, 1.0),
            )
            .unwrap();

        assert_eq!(draw_list.commands.len(), 2);
        assert!(draw_list.commands.iter().all(|command| matches!(
            command,
            DrawCommand::DrawText { text, .. } if text == "Label"
        )));
    }

    #[test]
    fn register_component_description_supports_custom_components() {
        let draw_system = ComponentDrawSystem::default();
        draw_system.register_component_renderer("Label", render_label);
        draw_system
            .register_component_description("Header", "<Stack><Label /></Stack>")
            .unwrap();
        draw_system
            .register_component_description("App", "<Root><Header /><Header /></Root>")
            .unwrap();

        let draw_list = draw_system
            .render_component(
                &Application::new(),
                "App",
                &Viewport::new(320.0, 240.0, 1.0),
            )
            .unwrap();

        assert_eq!(draw_list.commands.len(), 2);
    }

    #[test]
    fn render_component_rejects_recursive_custom_components() {
        let draw_system = ComponentDrawSystem::default();
        draw_system
            .register_component_description("Loop", "<Root><Loop /></Root>")
            .unwrap();

        let error = draw_system
            .render_component(
                &Application::new(),
                "Loop",
                &Viewport::new(320.0, 240.0, 1.0),
            )
            .unwrap_err();

        assert!(
            error
                .to_test_string()
                .contains("cyclic ui_description component reference: Loop -> Loop")
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

    fn render_label(_application: &Application, viewport: &Viewport) -> PixuiResult<DrawList> {
        let mut draw_list =
            DrawList::new(DrawBounds::new(0.0, 0.0, viewport.width, viewport.height));
        draw_list.push_command(DrawCommand::DrawText {
            x: 0.0,
            y: 0.0,
            text: "Label".into(),
        });
        Ok(draw_list)
    }
}
