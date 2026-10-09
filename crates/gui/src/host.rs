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
        input::{ButtonState, Modifiers, MouseButton, UiCommand, UiInput, WheelDelta},
        instance::UiInstanceId,
        mailbox::OutputReceiver,
        presentation::PresentationSettings,
        text_input::protocol::{EditingSessionId, NativeTextInput},
        window_properties::WindowCommand,
    },
};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize},
    event::{Ime, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{
        CursorGrabMode, ImeCapabilities, ImeEnableRequest, ImeRequest, ImeRequestData, Window,
        WindowAttributes, WindowId,
    },
};

/// One window for an already-created worker-side UI instance.
pub struct WindowSpec {
    pub title: String,
    pub instance: UiInstanceId,
    pub outputs: OutputReceiver,
    pub settings: PresentationSettings,
}

struct NativeWindow {
    window: Arc<dyn Window>,
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
    text_input: NativeTextInput,
    selecting: bool,
    ime_enabled: bool,
    ime_session: Option<EditingSessionId>,
}

impl NativeWindow {
    fn presentation(&mut self) -> UiCommand {
        let size = self.window.surface_size();
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

    fn editing_input(&self, input: UiInput) -> UiCommand {
        UiCommand::TextInput {
            instance: self.instance,
            revision: self.presentation.revision.unwrap_or_default(),
            session: self.text_input.session,
            input: Box::new(input),
        }
    }

    fn set_text_input(&mut self, input: NativeTextInput) {
        if self.ime_enabled && (input.session != self.text_input.session || !input.editable) {
            let _ = self.window.request_ime_update(ImeRequest::Disable);
            self.ime_enabled = false;
        }
        if input.editable && input.session.is_some() {
            let data = ImeRequestData::default().with_cursor_area(
                LogicalPosition::new(input.caret.x as f64, input.caret.y as f64).into(),
                LogicalSize::new(input.caret.width as f64, input.caret.height as f64).into(),
            );
            let request = if self.ime_enabled {
                ImeRequest::Update(data)
            } else {
                ImeRequest::Enable(
                    ImeEnableRequest::new(ImeCapabilities::new().with_cursor_area(), data)
                        .expect("cursor-area capability has matching data"),
                )
            };
            match self.window.request_ime_update(request) {
                Ok(()) => {
                    self.ime_enabled = true;
                    self.ime_session = input.session;
                }
                Err(error) => eprintln!("native IME unavailable: {error}"),
            }
        }
        if input.capture_pointer != self.text_input.capture_pointer {
            let grab = if input.capture_pointer {
                CursorGrabMode::Confined
            } else {
                CursorGrabMode::None
            };
            // Some platforms provide implicit drag capture or do not support grabs.
            let _ = self.window.set_cursor_grab(grab);
        }
        if !input.capture_pointer {
            self.selecting = false;
        }
        self.text_input = input;
    }

    fn request_redraw(&mut self) {
        if let Some(activity) = &mut self.drawing_activity {
            activity.request(Instant::now());
        }
        self.window.request_redraw();
    }

    fn drawable(&self) -> bool {
        let size = self.window.surface_size();
        self.worker_visible
            && self.presentation.active
            && !self.occluded
            && size.width > 0
            && size.height > 0
    }

    fn visibility_command(&mut self) -> Option<UiCommand> {
        let size = self.window.surface_size();
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
        let size = self.window.surface_size();
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
    error: Rc<RefCell<Option<String>>>,
    factory: Box<dyn RendererFactory>,
    proxy: winit::event_loop::EventLoopProxy,
    wake_pending: Arc<AtomicBool>,
    visibility_check: Instant,
    wayland: bool,
    clipboard: crate::clipboard::ClipboardExecutor,
}

impl Host {
    fn fail(&mut self, event_loop: &dyn ActiveEventLoop, error: impl std::fmt::Debug) {
        *self.error.borrow_mut() = Some(format!("{error:?}"));
        event_loop.exit();
    }

    fn enqueue(&mut self, command: UiCommand) -> PixuiResult<()> {
        self.pending.push(command)?;
        self.pending.flush(&self.application)
    }
}

impl ApplicationHandler for Host {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        let result = (|| -> PixuiResult<()> {
            for specification in self.specifications.drain(..) {
                let attributes = WindowAttributes::default()
                    .with_title(specification.title)
                    .with_surface_size(LogicalSize::new(
                        specification.settings.viewport.width,
                        specification.settings.viewport.height,
                    ));
                let window: Arc<dyn Window> = Arc::from(
                    event_loop
                        .create_window(attributes)
                        .map_err(|e| pixui_error!("create native window: {e}"))?,
                );
                let renderer = self.factory.create(window.clone())?;
                let proxy = self.proxy.clone();
                let wake_pending = self.wake_pending.clone();
                specification.outputs.set_waker(move || {
                    if !wake_pending.swap(true, Ordering::AcqRel) {
                        proxy.wake_up();
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
                    text_input: NativeTextInput::default(),
                    selecting: false,
                    ime_enabled: false,
                    ime_session: None,
                };
                if let Some(command) = native.visibility_command() {
                    self.pending.push(command)?;
                }
                self.pending.push(UiCommand::HostAttached {
                    instance: native.instance,
                    clipboard: true,
                })?;
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

    fn destroy_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
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

    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(native) = self.windows.get_mut(&id) else {
            return;
        };
        if matches!(
            &event,
            WindowEvent::RedrawRequested
                | WindowEvent::Focused(true)
                | WindowEvent::PointerEntered { .. }
                | WindowEvent::PointerMoved { .. }
                | WindowEvent::PointerButton { .. }
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
                | WindowEvent::SurfaceResized(_)
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
            WindowEvent::SurfaceResized(_) | WindowEvent::ScaleFactorChanged { .. } => {
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
                                .and_then(|monitor| monitor.current_video_mode())
                                .and_then(|mode| mode.refresh_rate_millihertz())
                                .map(std::num::NonZeroU32::get)
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
            WindowEvent::PointerMoved {
                position,
                primary: true,
                ..
            } => {
                native.pointer = Point {
                    x: position.x as f32 / native.settings.scale_factor,
                    y: position.y as f32 / native.settings.scale_factor,
                };
                Some(if native.selecting {
                    native.editing_input(UiInput::PointerMoved(native.pointer))
                } else {
                    native.input(UiInput::PointerMoved(native.pointer))
                })
            }
            WindowEvent::PointerEntered {
                position,
                primary: true,
                ..
            } => {
                native.pointer = Point {
                    x: position.x as f32 / native.settings.scale_factor,
                    y: position.y as f32 / native.settings.scale_factor,
                };
                Some(native.input(UiInput::PointerEntered))
            }
            WindowEvent::PointerLeft { primary: true, .. } => {
                Some(native.input(UiInput::PointerLeft))
            }
            WindowEvent::PointerButton {
                button,
                state,
                position,
                primary: true,
                ..
            } => {
                native.pointer = Point {
                    x: position.x as f32 / native.settings.scale_factor,
                    y: position.y as f32 / native.settings.scale_factor,
                };
                let Some(button) = native_input::pointer_button(button) else {
                    return;
                };
                let state = native_input::button_state(state);
                let input = UiInput::MouseButton {
                    button,
                    state,
                    position: native.pointer,
                    modifiers: native.modifiers,
                };
                if button == MouseButton::Left && state == ButtonState::Pressed {
                    // Ordered click-then-type uses the press revision until metadata arrives.
                    if native.ime_enabled {
                        let _ = native.window.request_ime_update(ImeRequest::Disable);
                        native.ime_enabled = false;
                    }
                    native.text_input.session = None;
                    native.selecting = true;
                    Some(native.input(input))
                } else if button == MouseButton::Left && native.selecting {
                    let command = native.editing_input(input);
                    native.selecting = false;
                    Some(command)
                } else {
                    Some(native.input(input))
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let delta = match delta {
                    MouseScrollDelta::LineDelta(x, y) => WheelDelta::Lines { x, y },
                    MouseScrollDelta::PixelDelta(position) => WheelDelta::Pixels {
                        x: position.x as f32 / native.settings.scale_factor,
                        y: position.y as f32 / native.settings.scale_factor,
                    },
                    _ => return,
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
            } => {
                let event = native_input::keyboard(event, native.modifiers, is_synthetic);
                let changes_focus = event.state == ButtonState::Pressed
                    && matches!(&event.key, pixui_engine::ui::input::Key::Named(name) if name == "Tab");
                let command = native.editing_input(UiInput::Keyboard(event));
                if changes_focus {
                    native.text_input.session = None;
                }
                Some(command)
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                native.modifiers = native_input::modifiers(modifiers.state());
                Some(native.input(UiInput::ModifiersChanged(native.modifiers)))
            }
            WindowEvent::Focused(focused) => {
                native.text_input.session = None;
                if !focused {
                    native.selecting = false;
                    let _ = native.window.set_cursor_grab(CursorGrabMode::None);
                    if native.ime_enabled {
                        let _ = native.window.request_ime_update(ImeRequest::Disable);
                        native.ime_enabled = false;
                    }
                }
                Some(native.input(UiInput::Focused(focused)))
            }
            WindowEvent::Ime(ime) => Some(UiCommand::TextInput {
                instance: native.instance,
                revision: native.presentation.revision.unwrap_or_default(),
                // Composition stays attached to the native IME session, even
                // while keyboard events await a new pointer-focus handshake.
                session: native.ime_session,
                input: Box::new(match ime {
                    Ime::Enabled => UiInput::ImeEnabled,
                    Ime::Preedit(text, cursor) => UiInput::ImePreedit { text, cursor },
                    Ime::Commit(text) => UiInput::ImeCommit(text),
                    Ime::Disabled => UiInput::ImeDisabled,
                    _ => return,
                }),
            }),
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

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
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
        while let Some(reply) = self.clipboard.try_recv() {
            if let Err(error) = self.pending.push(UiCommand::Clipboard(reply)) {
                self.fail(event_loop, error);
                return;
            }
        }
        for native in self.windows.values_mut() {
            while let Ok(effect) = native.outputs.effects().try_recv() {
                if let Err(reply) = self.clipboard.submit(native.instance, effect)
                    && let Err(error) = self.pending.push(UiCommand::Clipboard(reply))
                {
                    self.fail(event_loop, error);
                    return;
                }
            }
            // Native metadata is independent of drawable/occluded state and
            // never requires a new display list or GPU work.
            while let Ok(command) = native.outputs.window_commands().try_recv() {
                match command {
                    WindowCommand::SetTextInput(input) => native.set_text_input(input),
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
    let event_loop = EventLoop::new().map_err(|e| pixui_error!("create native event loop: {e}"))?;
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
    // Winit owns the handler; retain its failure result after the loop returns.
    let error = Rc::new(RefCell::new(None));
    let clipboard_proxy = proxy.clone();
    let clipboard = crate::clipboard::ClipboardExecutor::new(
        crate::clipboard::NativeClipboard::default(),
        move || clipboard_proxy.wake_up(),
    );
    let host = Host {
        application,
        specifications,
        windows: HashMap::new(),
        pending: PendingInput::default(),
        error: error.clone(),
        factory,
        proxy,
        wake_pending: Arc::new(AtomicBool::new(false)),
        visibility_check: Instant::now(),
        wayland,
        clipboard,
    };
    event_loop
        .run_app(host)
        .map_err(|e| pixui_error!("run native event loop: {e}"))?;
    if let Some(error) = error.borrow_mut().take() {
        return Err(pixui_error!("GUI host failed: {error}"));
    }
    Ok(())
}
