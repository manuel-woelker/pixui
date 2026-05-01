use crate::draw_list_renderer::DrawListRenderer;
use femtovg::renderer::OpenGl;
use femtovg::{Canvas, Color};
use glutin::config::{Config, ConfigTemplateBuilder};
use glutin::context::{
    ContextApi, ContextAttributesBuilder, NotCurrentContext, PossiblyCurrentContext, Version,
};
use glutin::display::{Display, GetGlDisplay};
use glutin::prelude::*;
use glutin::surface::{Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface};
use glutin_winit::DisplayBuilder;
use pixui_base::err;
use pixui_base::result::{OptionExt, PixuiResult, ResultExt};
use pixui_engine::engine::Engine;
use pixui_engine::viewport::Viewport;
use raw_window_handle::HasWindowHandle;
use std::num::NonZeroU32;
use winit::dpi::{PhysicalSize, Size};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes, WindowId};

/// Owns a single OpenGL-backed window and its rendering resources.
pub struct WindowRuntime {
    canvas: Canvas<OpenGl>,
    context: PossiblyCurrentContext,
    draw_list_renderer: DrawListRenderer,
    surface: Surface<WindowSurface>,
    window: Window,
}

impl WindowRuntime {
    /// Creates a new window runtime titled with the rendered component name.
    pub fn create(event_loop: &ActiveEventLoop, component_name: &str) -> PixuiResult<Self> {
        let (window, gl_config) = create_window(event_loop, component_name)?;
        let (context, surface) = create_gl_context(&window, &gl_config)?;
        let gl_display = gl_config.display();

        let renderer = unsafe {
            OpenGl::new_from_function_cstr(|symbol| gl_display.get_proc_address(symbol).cast())
        }
        .with_context(|| "failed to create femtovg renderer")?;
        let canvas = Canvas::new(renderer).with_context(|| "failed to create femtovg canvas")?;

        let mut runtime = Self {
            canvas,
            context,
            draw_list_renderer: DrawListRenderer::default(),
            surface,
            window,
        };
        runtime.resize(runtime.window.inner_size())?;
        Ok(runtime)
    }

    /// Returns the current window size.
    pub fn inner_size(&self) -> PhysicalSize<u32> {
        self.window.inner_size()
    }

    /// Returns the runtime window id.
    pub fn window_id(&self) -> WindowId {
        self.window.id()
    }

    /// Requests a redraw for the current window.
    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    /// Resizes the backing surface and canvas.
    pub fn resize(&mut self, size: PhysicalSize<u32>) -> PixuiResult<()> {
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }

        self.surface.resize(
            &self.context,
            NonZeroU32::new(size.width).expect("window width should be non-zero"),
            NonZeroU32::new(size.height).expect("window height should be non-zero"),
        );
        self.canvas
            .set_size(size.width, size.height, self.window.scale_factor() as f32);
        Ok(())
    }

    /// Renders the named component into the current window.
    pub fn render_component(&mut self, engine: &Engine, component_name: &str) -> PixuiResult<()> {
        let size = self.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }

        let viewport = Viewport::new(
            size.width as f32,
            size.height as f32,
            self.window.scale_factor() as f32,
        );
        let draw_list = engine.render_component(component_name.to_string(), viewport)?;

        self.canvas
            .clear_rect(0, 0, size.width, size.height, Color::rgb(15, 18, 26));
        self.draw_list_renderer
            .render(&mut self.canvas, &draw_list)
            .with_context(|| format!("failed to render component {component_name}"))?;
        self.canvas.flush();
        self.surface
            .swap_buffers(&self.context)
            .with_context(|| "failed to swap adapter window buffers")?;
        Ok(())
    }
}

fn create_window(
    event_loop: &ActiveEventLoop,
    component_name: &str,
) -> PixuiResult<(Window, Config)> {
    let window_attributes = WindowAttributes::default()
        .with_title(component_name)
        .with_inner_size(Size::Physical(PhysicalSize::new(960, 540)));

    let template = ConfigTemplateBuilder::new().with_alpha_size(8);
    let display_builder = DisplayBuilder::new().with_window_attributes(Some(window_attributes));

    let (window, gl_config) = display_builder
        .build(event_loop, template, |configs| {
            configs
                .max_by_key(|config| config.num_samples())
                .expect("no OpenGL configuration available")
        })
        .map_err(|error| err!("failed to build winit display: {error}"))?;

    Ok((window.context("window should be available")?, gl_config))
}

fn create_gl_context(
    window: &Window,
    gl_config: &Config,
) -> PixuiResult<(PossiblyCurrentContext, Surface<WindowSurface>)> {
    let raw_window_handle = window
        .window_handle()
        .with_context(|| "window handle should be available")?
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
    }
    .with_context(|| "failed to create OpenGL context")?;

    let size = window.inner_size();
    let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
        raw_window_handle,
        NonZeroU32::new(size.width.max(1)).expect("window width should be non-zero"),
        NonZeroU32::new(size.height.max(1)).expect("window height should be non-zero"),
    );

    let surface = unsafe { gl_display.create_window_surface(gl_config, &attrs) }
        .with_context(|| "failed to create window surface")?;

    let context = make_context_current(not_current_context, &surface)?;
    surface
        .set_swap_interval(
            &context,
            SwapInterval::Wait(NonZeroU32::new(1).expect("swap interval should be non-zero")),
        )
        .with_context(|| "failed to enable vsync")?;

    Ok((context, surface))
}

fn make_context_current(
    not_current_context: NotCurrentContext,
    surface: &Surface<WindowSurface>,
) -> PixuiResult<PossiblyCurrentContext> {
    not_current_context
        .make_current(surface)
        .with_context(|| "failed to make OpenGL context current")
}
