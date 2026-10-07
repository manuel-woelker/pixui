//! Native windows and presentation on the process main thread.
//! The application worker owns UI state; this host retains only complete outputs,
//! native resources, and a finite nonblocking event retry queue.

use super::{native_input, pending_input::PendingInput, redraw_schedule::RedrawSchedule};
use crate::renderer::{
    contract::RendererFactory,
    factory::{BuiltinRendererFactory, RendererSelection},
    presentation::Presentation,
};
use crossbeam_channel::TryRecvError;
use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::{
    application::application_handle::ApplicationHandle,
    ui::{
        display_list::RenderOutput,
        geometry::{Point, Size},
        input::{Modifiers, UiCommand, UiInput, WheelDelta},
        instance::UiInstanceId,
        mailbox::OutputReceiver,
        presentation::PresentationSettings,
        window_properties::WindowCommand,
    },
};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{Ime, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
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
    presentation: Presentation,
    instance: UiInstanceId,
    outputs: OutputReceiver,
    settings: PresentationSettings,
    output: Option<RenderOutput>,
    pointer: Point,
    redraw: RedrawSchedule,
    modifiers: Modifiers,
    occluded: bool,
    worker_visible: bool,
    drawing_activity: Option<crate::drawing_activity::DrawingActivity>,
}

impl NativeWindow {
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

    fn input(&self, input: UiInput) -> UiCommand {
        UiCommand::Input {
            instance: self.instance,
            revision: self.presentation.revision.unwrap_or_default(),
            input,
        }
    }

    fn request_redraw(&mut self) {
        if let Some(activity) = &mut self.drawing_activity {
            activity.request(Instant::now());
        }
        self.window.request_redraw();
    }

    fn drawable(&self) -> bool {
        let size = self.window.inner_size();
        self.worker_visible
            && self.presentation.active
            && !self.occluded
            && size.width > 0
            && size.height > 0
    }

    fn visibility_command(&mut self) -> Option<UiCommand> {
        let size = self.window.inner_size();
        let visible = self.presentation.active
            && !self.occluded
            && size.width > 0
            && size.height > 0
            && !self
                .drawing_activity
                .as_ref()
                .is_some_and(|activity| activity.paused)
            && self.window.is_visible() != Some(false)
            && self.window.is_minimized() != Some(true);
        if visible == self.worker_visible {
            return None;
        }
        self.worker_visible = visible;
        if !visible {
            self.redraw.cancel();
        } else {
            self.request_redraw();
        }
        Some(UiCommand::Visibility {
            instance: self.instance,
            visible,
        })
    }

    fn draw(&mut self) -> PixuiResult<bool> {
        let Some(output) = &self.output else {
            return Ok(false);
        };
        if !self.drawable() {
            return Ok(false);
        }
        let size = self.window.inner_size();
        self.presentation.draw(
            &output.display_list,
            output.revision,
            size.width,
            size.height,
            self.settings.scale_factor,
        )?;
        if self.presentation.retry.is_none()
            && self.presentation.revision == Some(output.revision)
            && size.width > 0
            && size.height > 0
            && self.presentation.active
        {
            return Ok(true);
        }
        Ok(false)
    }
}

struct Host {
    application: ApplicationHandle,
    specifications: Vec<WindowSpec>,
    windows: HashMap<WindowId, NativeWindow>,
    pending: PendingInput,
    error: Option<String>,
    factory: Box<dyn RendererFactory>,
    proxy: winit::event_loop::EventLoopProxy<()>,
    wake_pending: Arc<AtomicBool>,
    visibility_check: Instant,
    wayland: bool,
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
                let renderer = self.factory.create(window.clone())?;
                let proxy = self.proxy.clone();
                let wake_pending = self.wake_pending.clone();
                specification.outputs.set_waker(move || {
                    if !wake_pending.swap(true, Ordering::AcqRel) {
                        let _ = proxy.send_event(());
                    }
                });
                let mut native = NativeWindow {
                    window,
                    presentation: Presentation::new(renderer),
                    instance: specification.instance,
                    outputs: specification.outputs,
                    settings: specification.settings,
                    output: None,
                    pointer: Point::default(),
                    redraw: RedrawSchedule::default(),
                    modifiers: Modifiers::default(),
                    occluded: false,
                    worker_visible: true,
                    drawing_activity: self.wayland.then(Default::default),
                };
                if let Some(command) = native.visibility_command() {
                    self.pending.push(command)?;
                }
                self.pending.push(native.presentation())?;
                self.windows.insert(native.window.id(), native);
            }
            for native in self.windows.values_mut() {
                if !native.presentation.active {
                    native.presentation.resume()?;
                    if let Some(activity) = &mut native.drawing_activity {
                        activity.received();
                    }
                    native.request_redraw();
                    if let Some(command) = native.visibility_command() {
                        self.pending.push(command)?;
                    }
                    self.pending.push(native.presentation())?;
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.fail(event_loop, error);
        }
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        for native in self.windows.values_mut() {
            native.presentation.suspend();
            native.redraw.cancel();
            if let Some(command) = native.visibility_command()
                && let Err(error) = self.pending.push(command)
            {
                self.fail(event_loop, error);
                return;
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(native) = self.windows.get_mut(&id) else {
            return;
        };
        if matches!(
            &event,
            WindowEvent::RedrawRequested
                | WindowEvent::Focused(true)
                | WindowEvent::CursorEntered { .. }
                | WindowEvent::CursorMoved { .. }
                | WindowEvent::MouseInput { .. }
                | WindowEvent::MouseWheel { .. }
                | WindowEvent::KeyboardInput { .. }
        ) {
            let resumed = native
                .drawing_activity
                .as_mut()
                .is_some_and(|activity| activity.received());
            if resumed
                && let Some(command) = native.visibility_command()
                && let Err(error) = self.pending.push(command)
            {
                self.fail(event_loop, error);
                return;
            }
        }
        let mut close = false;
        let check_visibility = matches!(
            &event,
            WindowEvent::Occluded(_)
                | WindowEvent::Resized(_)
                | WindowEvent::ScaleFactorChanged { .. }
                | WindowEvent::Focused(true)
        );
        let command = match event {
            WindowEvent::CloseRequested => {
                close = true;
                Some(UiCommand::Close {
                    instance: native.instance,
                })
            }
            WindowEvent::Occluded(occluded) => {
                native.occluded = occluded;
                if occluded {
                    native.redraw.cancel();
                } else {
                    if let Some(activity) = &mut native.drawing_activity {
                        activity.received();
                    }
                    native.request_redraw();
                }
                None
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(activity) = &mut native.drawing_activity {
                    activity.received();
                }
                if !native.drawable() {
                    native.redraw.cancel();
                }
                Some(native.presentation())
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let already_presented = native
                    .output
                    .as_ref()
                    .is_some_and(|output| native.presentation.revision == Some(output.revision));
                if native.drawable()
                    && already_presented
                    && let Some(request) = native.redraw.take_animation_request(now)
                {
                    Some(UiCommand::AnimationFrame {
                        instance: native.instance,
                        request,
                    })
                } else {
                    match native.draw() {
                        Ok(true) => {
                            // Monitor cadence is a portable rate cap; native callbacks
                            // and FIFO still determine actual presentation opportunities.
                            let millihertz = native
                                .window
                                .current_monitor()
                                .and_then(|monitor| monitor.refresh_rate_millihertz())
                                .filter(|rate| *rate > 0)
                                .unwrap_or(60_000);
                            let interval = Duration::from_secs_f64(1000.0 / f64::from(millihertz));
                            native.redraw.presented(Instant::now(), interval);
                            if let Some(output) = &native.output {
                                let feedback = UiCommand::FramePresented {
                                    instance: native.instance,
                                    paint_revision: output.paint_revision,
                                    timings: native.presentation.renderer.timings(),
                                    timestamp: Instant::now(),
                                };
                                if let Err(error) = self.pending.push(feedback) {
                                    self.fail(event_loop, error);
                                    return;
                                }
                            }
                        }
                        Ok(false) => {}
                        Err(error) => {
                            self.fail(event_loop, error);
                            return;
                        }
                    }
                    None
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                native.pointer = Point {
                    x: position.x as f32 / native.settings.scale_factor,
                    y: position.y as f32 / native.settings.scale_factor,
                };
                Some(native.input(UiInput::PointerMoved(native.pointer)))
            }
            WindowEvent::CursorEntered { .. } => Some(native.input(UiInput::PointerEntered)),
            WindowEvent::CursorLeft { .. } => Some(native.input(UiInput::PointerLeft)),
            WindowEvent::MouseInput { button, state, .. } => {
                Some(native.input(UiInput::MouseButton {
                    button: native_input::mouse_button(button),
                    state: native_input::button_state(state),
                    position: native.pointer,
                    modifiers: native.modifiers,
                }))
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let delta = match delta {
                    MouseScrollDelta::LineDelta(x, y) => WheelDelta::Lines { x, y },
                    MouseScrollDelta::PixelDelta(position) => WheelDelta::Pixels {
                        x: position.x as f32 / native.settings.scale_factor,
                        y: position.y as f32 / native.settings.scale_factor,
                    },
                };
                Some(native.input(UiInput::MouseWheel {
                    delta,
                    modifiers: native.modifiers,
                }))
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => Some(native.input(UiInput::Keyboard(native_input::keyboard(
                event,
                native.modifiers,
                is_synthetic,
            )))),
            WindowEvent::ModifiersChanged(modifiers) => {
                native.modifiers = native_input::modifiers(modifiers.state());
                Some(native.input(UiInput::ModifiersChanged(native.modifiers)))
            }
            WindowEvent::Focused(focused) => Some(native.input(UiInput::Focused(focused))),
            WindowEvent::Ime(ime) => Some(native.input(match ime {
                Ime::Enabled => UiInput::ImeEnabled,
                Ime::Preedit(text, cursor) => UiInput::ImePreedit { text, cursor },
                Ime::Commit(text) => UiInput::ImeCommit(text),
                Ime::Disabled => UiInput::ImeDisabled,
            })),
            _ => None,
        };
        if !close
            && check_visibility
            && let Some(visibility) = native.visibility_command()
            && let Err(error) = self.pending.push(visibility)
        {
            self.fail(event_loop, error);
            return;
        }
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
        // Clear before reading mailboxes: publication racing this pass schedules
        // another wake, while outputs already published are observed below.
        self.wake_pending.store(false, Ordering::Release);
        // Some platforms have no visibility event. Check native state periodically
        // so restoration is detected even after animation and output have stopped.
        let now = Instant::now();
        if now >= self.visibility_check {
            self.visibility_check = now + Duration::from_millis(250);
            for native in self.windows.values_mut() {
                if let Some(activity) = &mut native.drawing_activity {
                    activity.check(now);
                }
                if let Some(command) = native.visibility_command()
                    && let Err(error) = self.pending.push(command)
                {
                    self.fail(event_loop, error);
                    return;
                }
            }
        }
        if let Err(error) = self.pending.flush(&self.application) {
            self.fail(event_loop, error);
            return;
        }
        for native in self.windows.values_mut() {
            // Native metadata is independent of drawable/occluded state and
            // never requires a new display list or GPU work.
            while let Ok(command) = native.outputs.window_commands().try_recv() {
                match command {
                    WindowCommand::SetTitle(title) => native.window.set_title(title.as_str()),
                    WindowCommand::SetIcon(image) => {
                        let icon = match image
                            .as_ref()
                            .map(crate::window_icon::from_image)
                            .transpose()
                        {
                            Ok(icon) => icon,
                            Err(error) => {
                                self.fail(event_loop, error);
                                return;
                            }
                        };
                        native.window.set_window_icon(icon);
                    }
                }
            }
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
                        // Overlay-only outputs must not restart painter deadlines
                        // or acknowledge an animation request that is still pending.
                        if native
                            .output
                            .as_ref()
                            .is_none_or(|previous| previous.paint_revision != output.paint_revision)
                        {
                            native.redraw.received(
                                Instant::now(),
                                output.redraw_after,
                                output.animating,
                                output.animation_request,
                            );
                        }
                        native.output = Some(output);
                        if native.drawable() {
                            native.request_redraw();
                        }
                    }
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.fail(event_loop, "UI instance or application worker stopped");
                    return;
                }
            }
        }
        let now = Instant::now();
        for native in self.windows.values_mut() {
            if native.drawable()
                && native
                    .presentation
                    .retry
                    .is_some_and(|deadline| deadline <= now)
            {
                native.presentation.retry = None;
                native.request_redraw();
            }
            if native.drawable() && native.redraw.take_animation_wakeup(now) {
                native.request_redraw();
            }
            if native.drawable()
                && native.redraw.take_due(now)
                && let Err(error) = self.pending.push(UiCommand::Redraw {
                    instance: native.instance,
                })
            {
                self.fail(event_loop, error);
                return;
            }
        }
        if let Err(error) = self.pending.flush(&self.application) {
            self.fail(event_loop, error);
            return;
        }
        if self.windows.is_empty() && self.specifications.is_empty() && self.pending.is_empty() {
            event_loop.exit();
        } else {
            // Output delivery wakes the loop immediately. A retry timer remains
            // only while worker command admission or replies are pending.
            let retry =
                (!self.pending.is_empty()).then(|| Instant::now() + Duration::from_millis(16));
            let deadline = self
                .windows
                .values()
                .filter(|native| native.drawable())
                .flat_map(|native| [native.redraw.deadline(), native.presentation.retry])
                .flatten()
                .chain(retry)
                .chain(Some(self.visibility_check))
                .min();
            event_loop.set_control_flow(deadline.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
        }
    }
}

/// Runs native windows until all are closed. Call on the process main thread.
/// Linux desktop is the initial target; other desktop platforms use
/// the same backend but require platform validation. Render errors are available
/// via `Application::uis`; the GUI retains the last successful output.
pub fn run(application: ApplicationHandle, specifications: Vec<WindowSpec>) -> PixuiResult<()> {
    run_with_renderer(application, specifications, RendererSelection::Auto)
}

/// Selects a built-in renderer; explicit GPU selection never silently falls back.
pub fn run_with_renderer(
    application: ApplicationHandle,
    specifications: Vec<WindowSpec>,
    selection: RendererSelection,
) -> PixuiResult<()> {
    run_with_factory(
        application,
        specifications,
        Box::new(BuiltinRendererFactory::new(selection)),
    )
}

/// Runs a custom presentation factory on the GUI thread. Each window receives
/// its own renderer; application state and painters remain backend-independent.
pub fn run_with_factory(
    application: ApplicationHandle,
    specifications: Vec<WindowSpec>,
    factory: Box<dyn RendererFactory>,
) -> PixuiResult<()> {
    let mut instances = HashSet::new();
    for specification in &specifications {
        specification.settings.validate()?;
        if !instances.insert(specification.instance) {
            return Err(pixui_error!(
                "each native window must have its own UI instance"
            ));
        }
    }
    let event_loop = EventLoop::<()>::with_user_event()
        .build()
        .map_err(|e| pixui_error!("create native event loop: {e}"))?;
    #[cfg(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    let wayland = {
        use winit::platform::wayland::EventLoopExtWayland;
        event_loop.is_wayland()
    };
    #[cfg(not(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "netbsd",
        target_os = "openbsd"
    )))]
    let wayland = false;
    let proxy = event_loop.create_proxy();
    let mut host = Host {
        application,
        specifications,
        windows: HashMap::new(),
        pending: PendingInput::default(),
        error: None,
        factory,
        proxy,
        wake_pending: Arc::new(AtomicBool::new(false)),
        visibility_check: Instant::now(),
        wayland,
    };
    event_loop
        .run_app(&mut host)
        .map_err(|e| pixui_error!("run native event loop: {e}"))?;
    if let Some(error) = host.error {
        return Err(pixui_error!("GUI host failed: {error}"));
    }
    Ok(())
}
