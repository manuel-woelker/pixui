use crate::window_runtime::WindowRuntime;
use pixui_engine::draw::component_draw_system::ComponentDrawSystem;
use pixui_engine::engine::Engine;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::WindowId;

/// Winit application wrapper that owns a single adapter window runtime.
pub struct WinitAdapterApplication {
    component_draw_system: ComponentDrawSystem,
    component_name: String,
    engine: Engine,
    runtime: Option<WindowRuntime>,
}

impl WinitAdapterApplication {
    /// Creates an application that renders a single named component.
    pub fn new(
        component_draw_system: ComponentDrawSystem,
        engine: Engine,
        component_name: String,
    ) -> Self {
        Self {
            component_draw_system,
            component_name,
            engine,
            runtime: None,
        }
    }
}

impl ApplicationHandler for WinitAdapterApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.runtime.is_none() {
            self.runtime = Some(
                WindowRuntime::create(event_loop, &self.component_name)
                    .expect("failed to create winit adapter window"),
            );
        }

        if let Some(runtime) = &self.runtime {
            runtime.request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(runtime) = self.runtime.as_mut() else {
            return;
        };

        if window_id != runtime.window_id() {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                runtime
                    .resize(size)
                    .expect("failed to resize adapter window");
                runtime.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                let size: PhysicalSize<u32> = runtime.inner_size();
                runtime
                    .resize(size)
                    .expect("failed to resize adapter window");
                runtime.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state.is_pressed() && event.logical_key == Key::Named(NamedKey::Escape) {
                    event_loop.exit();
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = runtime.render_component(
                    &self.component_draw_system,
                    &self.engine,
                    &self.component_name,
                ) {
                    eprintln!("render failed: {error:?}");
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }
}
