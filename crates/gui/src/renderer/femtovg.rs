//! GPU window renderer using femtovg and the worker's immutable image/text resources.
use super::{
    contract::{RenderOutcome, Renderer, RendererTimings},
    gpu::Gpu,
    scene::{DEFAULT_CACHE_BYTES, Scene},
};
use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::ui::display_list::DisplayList;
use std::{rc::Rc, sync::Arc};
use winit::window::Window;

pub use super::scene::CacheStats;

pub struct FemtovgRenderer {
    window: Arc<Window>,
    gpu: Rc<Gpu>,
    surface: Option<wgpu::Surface<'static>>,
    config: Option<wgpu::SurfaceConfiguration>,
    scene: Scene,
    width: u32,
    height: u32,
    scale: f32,
    timings: RendererTimings,
}
impl FemtovgRenderer {
    /// Creates an independent GPU device for this renderer. Built-in factories
    /// instead share one device across compatible windows.
    pub fn new(window: Arc<Window>) -> PixuiResult<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| pixui_error!("create GPU surface: {e}"))?;
        let gpu = Rc::new(Gpu::new(instance, Some(&surface))?);
        Self::with_gpu(window, gpu, surface)
    }
    pub(crate) fn with_gpu(
        window: Arc<Window>,
        gpu: Rc<Gpu>,
        surface: wgpu::Surface<'static>,
    ) -> PixuiResult<Self> {
        let scene = Scene::new(gpu.device.clone(), gpu.queue.clone(), DEFAULT_CACHE_BYTES)?;
        let mut result = Self {
            window,
            gpu,
            surface: Some(surface),
            config: None,
            scene,
            width: 0,
            height: 0,
            scale: 1.0,
            timings: RendererTimings::default(),
        };
        result.configure()?;
        Ok(result)
    }
    pub fn cache_stats(&self) -> CacheStats {
        self.scene.stats()
    }
    fn configure(&mut self) -> PixuiResult<()> {
        let Some(surface) = &self.surface else {
            return Ok(());
        };
        if self.width == 0 || self.height == 0 {
            self.config = None;
            return Ok(());
        }
        if self.width > self.gpu.device.limits().max_texture_dimension_2d
            || self.height > self.gpu.device.limits().max_texture_dimension_2d
        {
            return Err(pixui_error!("window exceeds GPU texture dimensions"));
        }
        let mut config = surface
            .get_default_config(&self.gpu.adapter, self.width, self.height)
            .ok_or_else(|| pixui_error!("GPU adapter cannot present this window"))?;
        // Match software's blending in encoded sRGB, instead of changing text
        // contrast by blending in linear light. Output bytes remain sRGB encoded.
        if let Some(format) = surface
            .get_capabilities(&self.gpu.adapter)
            .formats
            .into_iter()
            .find(|f| !f.is_srgb())
        {
            config.format = format;
        }
        config.present_mode = wgpu::PresentMode::Fifo;
        surface.configure(&self.gpu.device, &config);
        self.config = Some(config);
        Ok(())
    }
}
impl Renderer for FemtovgRenderer {
    fn resize(&mut self, width: u32, height: u32, scale: f32) -> PixuiResult<()> {
        super::gpu::validate_size(width, height, scale)?;
        let changed = self.width != width || self.height != height;
        self.width = width;
        self.height = height;
        self.scale = scale;
        if changed {
            self.configure()?;
        }
        Ok(())
    }
    fn render(&mut self, display: &DisplayList) -> PixuiResult<RenderOutcome> {
        let started = std::time::Instant::now();
        display.validate()?;
        self.gpu.check()?;
        if self.config.is_none() || self.surface.is_none() {
            return Ok(RenderOutcome::Skipped);
        }
        let acquired = self.surface.as_ref().unwrap().get_current_texture();
        let texture = match acquired {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                self.configure()?;
                return Ok(RenderOutcome::Skipped);
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(RenderOutcome::Skipped);
            }
            other => return Err(pixui_error!("acquire GPU surface: {other:?}")),
        };
        let acquisition = started.elapsed();
        let started = std::time::Instant::now();
        self.scene
            .prepare(display, self.width, self.height, self.scale)?;
        let resources = self.scene.resource_time;
        let drawing = started.elapsed().saturating_sub(resources);
        let started = std::time::Instant::now();
        let output = femtovg::renderer::WGPURenderOutput {
            view: texture.texture.create_view(&Default::default()),
            width: self.width,
            height: self.height,
            format: self.config.as_ref().unwrap().format,
        };
        let commands = self.scene.canvas.flush_to_output(output);
        self.gpu.queue.submit(commands);
        self.gpu.check()?;
        self.window.pre_present_notify();
        self.gpu.queue.present(texture);
        self.scene.trim();
        self.timings = RendererTimings {
            acquisition,
            resources,
            drawing,
            submission: started.elapsed(),
        };
        Ok(RenderOutcome::Presented)
    }
    fn timings(&self) -> Option<RendererTimings> {
        Some(self.timings)
    }
    fn suspend(&mut self) {
        self.surface = None;
        self.config = None;
    }
    fn resume(&mut self) -> PixuiResult<()> {
        if self.surface.is_none() {
            self.surface = Some(
                self.gpu
                    .instance
                    .create_surface(self.window.clone())
                    .map_err(|e| pixui_error!("resume GPU surface: {e}"))?,
            );
            self.configure()?;
        }
        Ok(())
    }
}
