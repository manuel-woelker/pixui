//! Native windows and presentation on the process main thread.
//! The application worker owns UI state; this host retains only complete outputs,
//! native resources, and a finite nonblocking event retry queue.

use super::{painter, pending_input::PendingInput};
use crossbeam_channel::TryRecvError;
use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::{
    application::application_handle::ApplicationHandle,
    ui::{
        display_list::RenderOutput,
        geometry::{Point, Size},
        input::{UiCommand, UiInput},
        instance::UiInstanceId,
        mailbox::OutputReceiver,
        presentation::PresentationSettings,
    },
};
use softbuffer::{Context, Surface};
use std::{
    collections::{HashMap, HashSet},
    num::NonZeroU32,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Window, WindowId},
};

/// One window for an already-created worker-side UI instance.
pub struct WindowSpec {
    pub title: String,
    pub instance: UiInstanceId,
    pub outputs: OutputReceiver,
    pub settings: PresentationSettings,
}

struct NativeWindow {
    window: Arc<Window>,
    surface: Option<Surface<Arc<Window>, Arc<Window>>>,
    instance: UiInstanceId,
    outputs: OutputReceiver,
    settings: PresentationSettings,
    output: Option<RenderOutput>,
    presented: Option<Presented>,
    pointer: Point,
}

/// Input is tagged with the revision actually painted, rather than a newer
/// output that was received but is still waiting for a native redraw.
struct Presented {
    revision: pixui_engine::ui::display_list::RenderRevision,
}

impl NativeWindow {
    fn surface(&mut self) -> PixuiResult<()> {
        let context = Context::new(self.window.clone())
            .map_err(|e| pixui_error!("create graphics context: {e}"))?;
        self.surface = Some(
            Surface::new(&context, self.window.clone())
                .map_err(|e| pixui_error!("create window surface: {e}"))?,
        );
        Ok(())
    }

    fn presentation(&mut self) -> UiCommand {
        let size = self.window.inner_size();
        self.settings.scale_factor = self.window.scale_factor() as f32;
        self.settings.viewport = Size {
            width: size.width as f32 / self.settings.scale_factor,
            height: size.height as f32 / self.settings.scale_factor,
        };
        UiCommand::Present {
            instance: self.instance,
            settings: self.settings.clone(),
        }
    }

    fn input(&self, input: UiInput) -> Option<UiCommand> {
        self.presented.as_ref().map(|presented| UiCommand::Input {
            instance: self.instance,
            revision: presented.revision,
            input,
        })
    }

    fn draw(&mut self) -> PixuiResult<()> {
        let Some(output) = &self.output else {
            return Ok(());
        };
        let size = self.window.inner_size();
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return Ok(());
        };
        let Some(surface) = &mut self.surface else {
            return Ok(());
        };
        let pixels = painter::paint(
            &output.display_list,
            size.width,
            size.height,
            self.settings.scale_factor,
        )?;
        surface
            .resize(width, height)
            .map_err(|e| pixui_error!("resize surface: {e}"))?;
        let mut buffer = surface
            .buffer_mut()
            .map_err(|e| pixui_error!("acquire surface buffer: {e}"))?;
        buffer.copy_from_slice(&pixels);
        buffer
            .present()
            .map_err(|e| pixui_error!("present surface: {e}"))?;
        self.presented = Some(Presented {
            revision: output.revision,
        });
        Ok(())
    }
}

struct Host {
    application: ApplicationHandle,
    specifications: Vec<WindowSpec>,
    windows: HashMap<WindowId, NativeWindow>,
    pending: PendingInput,
    error: Option<String>,
}

impl Host {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: impl std::fmt::Debug) {
        self.error = Some(format!("{error:?}"));
        event_loop.exit();
    }

    fn enqueue(&mut self, command: UiCommand) -> PixuiResult<()> {
        self.pending.push(command)?;
        self.pending.flush(&self.application)
    }
}

impl ApplicationHandler for Host {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let result = (|| -> PixuiResult<()> {
            for specification in self.specifications.drain(..) {
                let attributes = Window::default_attributes()
                    .with_title(specification.title)
                    .with_inner_size(LogicalSize::new(
                        specification.settings.viewport.width,
                        specification.settings.viewport.height,
                    ));
                let window = Arc::new(
                    event_loop
                        .create_window(attributes)
                        .map_err(|e| pixui_error!("create native window: {e}"))?,
                );
                let mut native = NativeWindow {
                    window,
                    surface: None,
                    instance: specification.instance,
                    outputs: specification.outputs,
                    settings: specification.settings,
                    output: None,
                    presented: None,
                    pointer: Point::default(),
                };
                native.surface()?;
                self.pending.push(native.presentation())?;
                self.windows.insert(native.window.id(), native);
            }
            for native in self.windows.values_mut() {
                if native.surface.is_none() {
                    native.surface()?;
                    self.pending.push(native.presentation())?;
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.fail(event_loop, error);
        }
    }

    fn suspended(&mut self, _: &ActiveEventLoop) {
        for native in self.windows.values_mut() {
            native.surface = None;
            native.presented = None;
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(native) = self.windows.get_mut(&id) else {
            return;
        };
        let mut close = false;
        let command = match event {
            WindowEvent::CloseRequested => {
                close = true;
                Some(UiCommand::Close {
                    instance: native.instance,
                })
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                Some(native.presentation())
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = native.draw() {
                    self.fail(event_loop, error);
                }
                None
            }
            WindowEvent::CursorMoved { position, .. } => {
                native.pointer = Point {
                    x: position.x as f32 / native.settings.scale_factor,
                    y: position.y as f32 / native.settings.scale_factor,
                };
                native.input(UiInput::PointerMoved(native.pointer))
            }
            WindowEvent::CursorLeft { .. } => {
                native.input(UiInput::PointerMoved(Point { x: -1.0, y: -1.0 }))
            }
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state: ElementState::Released,
                ..
            } => native.input(UiInput::Activate(native.pointer)),
            WindowEvent::MouseWheel { delta, .. } => {
                let distance = match delta {
                    MouseScrollDelta::LineDelta(_, y) => -y * 40.0,
                    MouseScrollDelta::PixelDelta(position) => {
                        -position.y as f32 / native.settings.scale_factor
                    }
                };
                native.input(UiInput::Scroll(distance))
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed && !event.repeat =>
            {
                match event.logical_key {
                    Key::Named(NamedKey::Tab) => native.input(UiInput::FocusNext),
                    Key::Named(NamedKey::Enter | NamedKey::Space) => {
                        native.input(UiInput::ActivateFocused)
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        if let Some(command) = command
            && let Err(error) = self.enqueue(command)
        {
            self.fail(event_loop, error);
        }
        if close {
            self.windows.remove(&id);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.pending.flush(&self.application) {
            self.fail(event_loop, error);
            return;
        }
        for native in self.windows.values_mut() {
            match native.outputs.try_recv() {
                Ok(output) => {
                    if output.instance_id != native.instance {
                        self.fail(
                            event_loop,
                            "output subscription belongs to a different UI instance",
                        );
                        return;
                    }
                    if native
                        .output
                        .as_ref()
                        .is_none_or(|previous| output.revision > previous.revision)
                    {
                        native.output = Some(output);
                        native.window.request_redraw();
                    }
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.fail(event_loop, "UI instance or application worker stopped");
                    return;
                }
            }
        }
        if self.windows.is_empty() && self.specifications.is_empty() && self.pending.is_empty() {
            event_loop.exit();
        } else {
            // A finite polling interval avoids a forwarding thread per window.
            // Native input still wakes the loop immediately.
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(16),
            ));
        }
    }
}

/// Runs native windows until all are closed. Call on the process main thread.
/// Linux desktop is the initial target; other desktop platforms use
/// the same backend but require platform validation. Render errors are available
/// via `Application::uis`; the GUI retains the last successful output.
pub fn run(application: ApplicationHandle, specifications: Vec<WindowSpec>) -> PixuiResult<()> {
    let mut instances = HashSet::new();
    for specification in &specifications {
        specification.settings.validate()?;
        if !instances.insert(specification.instance) {
            return Err(pixui_error!(
                "each native window must have its own UI instance"
            ));
        }
    }
    let event_loop = EventLoop::new().map_err(|e| pixui_error!("create native event loop: {e}"))?;
    let mut host = Host {
        application,
        specifications,
        windows: HashMap::new(),
        pending: PendingInput::default(),
        error: None,
    };
    event_loop
        .run_app(&mut host)
        .map_err(|e| pixui_error!("run native event loop: {e}"))?;
    if let Some(error) = host.error {
        return Err(pixui_error!("GUI host failed: {error}"));
    }
    Ok(())
}
