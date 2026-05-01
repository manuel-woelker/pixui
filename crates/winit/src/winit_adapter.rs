use crate::winit_adapter_application::WinitAdapterApplication;
use pixui_base::file_path::FilePath;
use pixui_base::result::{PixuiResult, ResultExt};
use pixui_base::shared_string::SharedString;
use pixui_base::source_file::SourceFile;
use pixui_engine::draw::component_draw_system::ComponentDrawSystem;
use pixui_engine::draw::draw_list::DrawList;
use pixui_engine::draw::painter::ComponentPainter;
use pixui_engine::engine::Engine;
use pixui_engine::viewport::Viewport;
use pixui_pal::pal::PalHandle;
use pixui_pal::pal_real::PalReal;
use winit::event_loop::{ControlFlow, EventLoop};

/// Bridges the engine draw-command API to a native winit window.
pub struct WinitAdapter {
    component_draw_system: ComponentDrawSystem,
    engine: Engine,
    pal: PalHandle,
}

impl WinitAdapter {
    /// Creates a new adapter that can render engine-managed components.
    pub fn new(engine: &Engine) -> PixuiResult<Self> {
        Self::new_with_pal(engine, PalReal::new_handle()?)
    }

    /// Creates a new adapter with an explicit platform abstraction.
    pub fn new_with_pal(engine: &Engine, pal: impl Into<PalHandle>) -> PixuiResult<Self> {
        Ok(Self {
            component_draw_system: ComponentDrawSystem::default(),
            engine: engine.clone(),
            pal: pal.into(),
        })
    }

    /// Registers a named component painter with the adapter-owned draw system.
    pub fn register_component_painter<P>(&self, component_name: impl Into<SharedString>, painter: P)
    where
        P: ComponentPainter + Send + Sync + 'static,
    {
        self.component_draw_system
            .register_component_painter(component_name, painter);
    }

    /// Registers a composite component backed by a UI description file.
    pub fn register_component_description_file(
        &self,
        component_name: impl Into<SharedString>,
        path: impl Into<FilePath>,
    ) -> PixuiResult<()> {
        let path = path.into();
        let source = self
            .pal
            .read_file_to_string(&path)
            .with_context(|| format!("failed to read ui_description file `{path}`"))?;
        let source_file = SourceFile::new(path, source);
        self.component_draw_system
            .register_component_description_source_file(component_name, &source_file)
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
    use pixui_pal::pal::PalHandle;
    use pixui_pal::pal_mock::PalMock;

    #[test]
    fn register_component_painter_registers_a_named_component_renderer() {
        let engine = Engine::new().unwrap();
        let adapter = WinitAdapter::new_with_pal(&engine, PalHandle::new(PalMock::new())).unwrap();

        adapter.register_component_painter("Label", LabelPainter);

        let draw_list = adapter
            .render_component("Label", Viewport::new(320.0, 240.0, 1.0))
            .unwrap();

        assert!(matches!(
            draw_list.commands.as_slice(),
            [
                DrawCommand::SelectStyle { .. },
                DrawCommand::DrawText { text, .. }
            ] if text == "Label"
        ));
    }

    #[test]
    fn register_component_description_renders_composite_components() {
        let engine = Engine::new().unwrap();
        let pal = PalMock::new();
        pal.set_file("examples/app.pixui", "<Root><Label /><Header /></Root>");
        pal.set_file("examples/header.pixui", "<Stack><Label /></Stack>");
        let adapter = WinitAdapter::new_with_pal(&engine, PalHandle::new(pal)).unwrap();

        adapter.register_component_painter("Label", LabelPainter);
        adapter
            .register_component_description_file("App", "examples/app.pixui")
            .unwrap();
        adapter
            .register_component_description_file("Header", "examples/header.pixui")
            .unwrap();

        let draw_list = adapter
            .render_component("App", Viewport::new(320.0, 240.0, 1.0))
            .unwrap();

        assert_eq!(draw_list.commands.len(), 4);
    }
}
