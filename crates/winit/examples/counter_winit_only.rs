use femtovg::renderer::OpenGl;
use femtovg::{Canvas, Color, Paint, Path};
use glutin::config::{Config, ConfigTemplateBuilder};
use glutin::context::{
    ContextApi, ContextAttributesBuilder, NotCurrentContext, PossiblyCurrentContext, Version,
};
use glutin::display::{Display, GetGlDisplay};
use glutin::prelude::*;
use glutin::surface::{Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface};
use glutin_winit::DisplayBuilder;
use raw_window_handle::HasWindowHandle;
use std::num::NonZeroU32;
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalSize, Size};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowAttributes, WindowId};

fn main() {
    let event_loop = EventLoop::new().expect("failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = CounterApplication::default();
    event_loop.run_app(&mut app).expect("event loop failed");
}

#[derive(Default)]
struct CounterApplication {
    app: Option<CounterApp>,
}

impl ApplicationHandler for CounterApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.app.is_none() {
            self.app = Some(CounterApp::create(event_loop));
        }

        if let Some(app) = &self.app {
            app.window.request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(app) = self.app.as_mut() else {
            return;
        };

        if window_id != app.window.id() {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                app.resize(size);
                app.window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                app.window.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    match event.logical_key {
                        Key::Named(NamedKey::Escape) => event_loop.exit(),
                        Key::Named(NamedKey::Space) => {
                            app.increment();
                            app.window.request_redraw();
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                app.increment();
                app.window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = app.render() {
                    eprintln!("render failed: {error}");
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }
}

struct CounterApp {
    counter: u32,
    window: Window,
    context: PossiblyCurrentContext,
    surface: Surface<WindowSurface>,
    canvas: Canvas<OpenGl>,
}

#[derive(Copy, Clone)]
struct SegmentRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl CounterApp {
    fn create(event_loop: &ActiveEventLoop) -> Self {
        let (window, gl_config) = create_window(event_loop);
        let (context, surface) = create_gl_context(&window, &gl_config);
        let gl_display = gl_config.display();

        let renderer = unsafe {
            OpenGl::new_from_function_cstr(|symbol| gl_display.get_proc_address(symbol).cast())
        }
        .expect("failed to create femtovg renderer");
        let canvas = Canvas::new(renderer).expect("failed to create femtovg canvas");

        Self::new(window, context, surface, canvas)
    }

    fn new(
        window: Window,
        context: PossiblyCurrentContext,
        surface: Surface<WindowSurface>,
        canvas: Canvas<OpenGl>,
    ) -> Self {
        let mut app = Self {
            counter: 0,
            window,
            context,
            surface,
            canvas,
        };
        app.resize(app.window.inner_size());
        app.update_title();
        app
    }

    fn increment(&mut self) {
        self.counter = self.counter.saturating_add(1);
        self.update_title();
    }

    fn update_title(&self) {
        self.window.set_title(&format!(
            "pixui-winit counter: {} | click or press Space",
            self.counter
        ));
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }

        self.surface.resize(
            &self.context,
            NonZeroU32::new(size.width).expect("window width should be non-zero"),
            NonZeroU32::new(size.height).expect("window height should be non-zero"),
        );

        self.canvas
            .set_size(size.width, size.height, self.window.scale_factor() as f32);
    }

    fn render(&mut self) -> Result<(), String> {
        let size = self.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }

        self.canvas
            .clear_rect(0, 0, size.width, size.height, Color::rgb(15, 18, 26));

        let second_counter = self.counter.saturating_mul(2);
        let digit_text = second_counter.to_string();
        let digit_count = digit_text.len().max(1) as f32;

        let window_width = size.width as f32;
        let window_height = size.height as f32;
        let digit_height = (window_height * 0.55).clamp(120.0, 280.0);
        let digit_width = digit_height * 0.56;
        let spacing = digit_width * 0.18;
        let total_width = digit_count * digit_width + (digit_count - 1.0) * spacing;
        let start_x = (window_width - total_width) * 0.5;
        let start_y = (window_height - digit_height) * 0.5;

        draw_counter_panel(
            &mut self.canvas,
            start_x - 32.0,
            start_y - 32.0,
            total_width + 64.0,
            digit_height + 64.0,
        );

        for (index, digit) in digit_text.chars().enumerate() {
            let x = start_x + index as f32 * (digit_width + spacing);
            draw_digit(
                &mut self.canvas,
                x,
                start_y,
                digit_width,
                digit_height,
                digit,
            );
        }

        self.canvas.flush();
        self.surface
            .swap_buffers(&self.context)
            .map_err(|error| error.to_string())
    }
}

fn create_window(event_loop: &ActiveEventLoop) -> (Window, Config) {
    let window_attributes = WindowAttributes::default()
        .with_title("pixui-winit counter")
        .with_inner_size(Size::Physical(PhysicalSize::new(960, 540)));

    let template = ConfigTemplateBuilder::new().with_alpha_size(8);
    let display_builder = DisplayBuilder::new().with_window_attributes(Some(window_attributes));

    let (window, gl_config) = display_builder
        .build(event_loop, template, |configs| {
            configs
                .max_by_key(|config| config.num_samples())
                .expect("no OpenGL configuration available")
        })
        .expect("failed to build display");

    (window.expect("window should be available"), gl_config)
}

fn create_gl_context(
    window: &Window,
    gl_config: &Config,
) -> (PossiblyCurrentContext, Surface<WindowSurface>) {
    let raw_window_handle = window
        .window_handle()
        .expect("window handle should be available")
        .as_raw();
    let gl_display: Display = gl_config.display();

    let context_attributes = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
        .build(Some(raw_window_handle));
    let fallback_context_attributes =
        ContextAttributesBuilder::new().build(Some(raw_window_handle));

    let not_current_context = unsafe {
        gl_display
            .create_context(gl_config, &context_attributes)
            .or_else(|_| gl_display.create_context(gl_config, &fallback_context_attributes))
            .expect("failed to create OpenGL context")
    };

    let size = window.inner_size();
    let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
        raw_window_handle,
        NonZeroU32::new(size.width.max(1)).expect("window width should be non-zero"),
        NonZeroU32::new(size.height.max(1)).expect("window height should be non-zero"),
    );

    let surface = unsafe {
        gl_display
            .create_window_surface(gl_config, &attrs)
            .expect("failed to create window surface")
    };

    let context = make_context_current(not_current_context, &surface);
    surface
        .set_swap_interval(
            &context,
            SwapInterval::Wait(NonZeroU32::new(1).expect("swap interval should be non-zero")),
        )
        .expect("failed to enable vsync");

    (context, surface)
}

fn make_context_current(
    not_current_context: NotCurrentContext,
    surface: &Surface<WindowSurface>,
) -> PossiblyCurrentContext {
    not_current_context
        .make_current(surface)
        .expect("failed to make OpenGL context current")
}

fn draw_counter_panel(canvas: &mut Canvas<OpenGl>, x: f32, y: f32, width: f32, height: f32) {
    let mut panel = Path::new();
    panel.rounded_rect(x, y, width, height, 28.0);

    let panel_fill = Paint::color(Color::rgb(27, 32, 44));
    canvas.fill_path(&panel, &panel_fill);

    let mut border = Path::new();
    border.rounded_rect(x, y, width, height, 28.0);

    let mut border_paint = Paint::color(Color::rgb(78, 88, 117));
    border_paint.set_line_width(3.0);
    canvas.stroke_path(&border, &border_paint);
}

fn draw_digit(canvas: &mut Canvas<OpenGl>, x: f32, y: f32, width: f32, height: f32, digit: char) {
    let thickness = width * 0.18;
    let horizontal_length = width - thickness * 1.5;
    let vertical_height = (height - thickness * 3.0) * 0.5;
    let radius = thickness * 0.45;

    let active_segments = active_segments(digit);
    let on = Color::rgb(255, 120, 64);
    let off = Color::rgb(70, 45, 40);

    draw_segment(
        canvas,
        SegmentRect {
            x: x + thickness * 0.75,
            y,
            width: horizontal_length,
            height: thickness,
        },
        radius,
        active_segments[0],
        on,
        off,
    );
    draw_segment(
        canvas,
        SegmentRect {
            x: x + width - thickness,
            y: y + thickness * 0.6,
            width: thickness,
            height: vertical_height,
        },
        radius,
        active_segments[1],
        on,
        off,
    );
    draw_segment(
        canvas,
        SegmentRect {
            x: x + width - thickness,
            y: y + height * 0.5 + thickness * 0.1,
            width: thickness,
            height: vertical_height,
        },
        radius,
        active_segments[2],
        on,
        off,
    );
    draw_segment(
        canvas,
        SegmentRect {
            x: x + thickness * 0.75,
            y: y + height - thickness,
            width: horizontal_length,
            height: thickness,
        },
        radius,
        active_segments[3],
        on,
        off,
    );
    draw_segment(
        canvas,
        SegmentRect {
            x,
            y: y + height * 0.5 + thickness * 0.1,
            width: thickness,
            height: vertical_height,
        },
        radius,
        active_segments[4],
        on,
        off,
    );
    draw_segment(
        canvas,
        SegmentRect {
            x,
            y: y + thickness * 0.6,
            width: thickness,
            height: vertical_height,
        },
        radius,
        active_segments[5],
        on,
        off,
    );
    draw_segment(
        canvas,
        SegmentRect {
            x: x + thickness * 0.75,
            y: y + (height - thickness) * 0.5,
            width: horizontal_length,
            height: thickness,
        },
        radius,
        active_segments[6],
        on,
        off,
    );
}

fn draw_segment(
    canvas: &mut Canvas<OpenGl>,
    rect: SegmentRect,
    radius: f32,
    is_on: bool,
    on_color: Color,
    off_color: Color,
) {
    let mut segment = Path::new();
    segment.rounded_rect(rect.x, rect.y, rect.width, rect.height, radius);
    let paint = Paint::color(if is_on { on_color } else { off_color });
    canvas.fill_path(&segment, &paint);
}

fn active_segments(digit: char) -> [bool; 7] {
    match digit {
        '0' => [true, true, true, true, true, true, false],
        '1' => [false, true, true, false, false, false, false],
        '2' => [true, true, false, true, true, false, true],
        '3' => [true, true, true, true, false, false, true],
        '4' => [false, true, true, false, false, true, true],
        '5' => [true, false, true, true, false, true, true],
        '6' => [true, false, true, true, true, true, true],
        '7' => [true, true, true, false, false, false, false],
        '8' => [true, true, true, true, true, true, true],
        '9' => [true, true, true, true, false, true, true],
        _ => [false, false, false, false, false, false, false],
    }
}
