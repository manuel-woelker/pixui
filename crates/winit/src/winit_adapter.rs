use crate::winit_adapter_application::WinitAdapterApplication;
use pixui_base::result::{PixuiResult, ResultExt};
use pixui_engine::engine::Engine;
use winit::event_loop::{ControlFlow, EventLoop};

/// Bridges the engine draw-command API to a native winit window.
pub struct WinitAdapter {
    engine: Engine,
}

impl WinitAdapter {
    /// Creates a new adapter that can render engine-managed components.
    pub fn new(engine: &Engine) -> PixuiResult<Self> {
        Ok(Self {
            engine: engine.clone(),
        })
    }

    /// Creates a window for the named component and runs the winit event loop.
    pub fn create_window(&self, component_name: &str) -> PixuiResult<()> {
        let event_loop = EventLoop::new().with_context(|| "failed to create event loop")?;
        event_loop.set_control_flow(ControlFlow::Wait);

        let mut application =
            WinitAdapterApplication::new(self.engine.clone(), component_name.to_string());

        event_loop
            .run_app(&mut application)
            .with_context(|| format!("failed to run winit adapter for {component_name}"))?;
        Ok(())
    }
}
