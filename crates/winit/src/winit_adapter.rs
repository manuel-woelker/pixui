use crate::winit_adapter_application::WinitAdapterApplication;
use pixui_base::result::{PixuiResult, ResultExt};
use pixui_base::shared_string::SharedString;
use pixui_engine::draw::component_draw_renderer::ComponentDrawRenderer;
use pixui_engine::draw::component_draw_system::ComponentDrawSystem;
use pixui_engine::draw::draw_list::DrawList;
use pixui_engine::draw::painter::ComponentPainter;
use pixui_engine::engine::Engine;
use pixui_engine::viewport::Viewport;
use winit::event_loop::{ControlFlow, EventLoop};

/// Bridges the engine draw-command API to a native winit window.
pub struct WinitAdapter {
    component_draw_system: ComponentDrawSystem,
    engine: Engine,
}

impl WinitAdapter {
    /// Creates a new adapter that can render engine-managed components.
    pub fn new(engine: &Engine) -> PixuiResult<Self> {
        Ok(Self {
            component_draw_system: ComponentDrawSystem::default(),
            engine: engine.clone(),
        })
    }

    /// Registers a named component renderer with the adapter-owned draw system.
    pub fn register_component_renderer<R>(
        &self,
        component_name: impl Into<SharedString>,
        renderer: R,
    ) where
        R: ComponentDrawRenderer + 'static,
    {
        self.component_draw_system
            .register_component_renderer(component_name, renderer);
    }

    /// Registers a named component painter with the adapter.
    pub fn register_painter<P>(&self, component_name: impl Into<SharedString>)
    where
        P: ComponentPainter + Send + Sync + 'static,
    {
        self.component_draw_system
            .register_painter::<P>(component_name);
    }

    /// Renders a named component through the adapter-owned draw system.
    pub fn render_component(
        &self,
        component_name: &str,
        viewport: Viewport,
    ) -> PixuiResult<DrawList> {
        let draw_system = self.component_draw_system.clone();
        let component_name = component_name.to_string();
        self.engine.run_application(move |application| {
            draw_system.render_component(application, &component_name, &viewport)
        })
    }

    /// Creates a window for the named component and runs the winit event loop.
    pub fn create_window(&self, component_name: &str) -> PixuiResult<()> {
        let event_loop = EventLoop::new().with_context(|| "failed to create event loop")?;
        event_loop.set_control_flow(ControlFlow::Wait);

        let mut application = WinitAdapterApplication::new(
            self.component_draw_system.clone(),
            self.engine.clone(),
            component_name.to_string(),
        );

        event_loop
            .run_app(&mut application)
            .with_context(|| format!("failed to run winit adapter for {component_name}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::WinitAdapter;
    use pixui_engine::components::label_painter::LabelPainter;
    use pixui_engine::draw::command::DrawCommand;
    use pixui_engine::engine::Engine;
    use pixui_engine::viewport::Viewport;

    #[test]
    fn register_painter_registers_a_named_component_renderer() {
        let engine = Engine::new().unwrap();
        let adapter = WinitAdapter::new(&engine).unwrap();

        adapter.register_painter::<LabelPainter>("Label");

        let draw_list = adapter
            .render_component("Label", Viewport::new(320.0, 240.0, 1.0))
            .unwrap();

        assert!(matches!(
            draw_list.commands.as_slice(),
            [DrawCommand::DrawText { text, .. }] if text == "Label"
        ));
    }
}
