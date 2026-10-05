//! Software window presentation; the pure CPU painter remains available separately.

use super::contract::{RenderOutcome, Renderer, RendererTimings};
use crate::painter;
use pixui_base::{PixuiResult, pixui_error};
use pixui_engine::ui::display_list::DisplayList;
use softbuffer::{Context, Surface};
use std::{num::NonZeroU32, sync::Arc};
use winit::window::Window;

pub struct SoftwareRenderer {
    window: Arc<Window>,
    surface: Option<Surface<Arc<Window>, Arc<Window>>>,
    width: u32,
    height: u32,
    scale: f32,
    timings: RendererTimings,
}
impl SoftwareRenderer {
    pub fn new(window: Arc<Window>) -> PixuiResult<Self> {
        let mut renderer = Self {
            window,
            surface: None,
            width: 0,
            height: 0,
            scale: 1.0,
            timings: RendererTimings::default(),
        };
        renderer.resume()?;
        Ok(renderer)
    }
}
impl Renderer for SoftwareRenderer {
    fn resize(&mut self, width: u32, height: u32, scale: f32) -> PixuiResult<()> {
        super::gpu::validate_size(width, height, scale)?;
        self.width = width;
        self.height = height;
        self.scale = scale;
        Ok(())
    }
    fn render(&mut self, display: &DisplayList) -> PixuiResult<RenderOutcome> {
        let started = std::time::Instant::now();
        display.validate()?;
        let (Some(width), Some(height), Some(surface)) = (
            NonZeroU32::new(self.width),
            NonZeroU32::new(self.height),
            self.surface.as_mut(),
        ) else {
            return Ok(RenderOutcome::Skipped);
        };
        let acquisition = started.elapsed();
        let started = std::time::Instant::now();
        let pixels = painter::paint(display, self.width, self.height, self.scale)?;
        let drawing = started.elapsed();
        let started = std::time::Instant::now();
        surface
            .resize(width, height)
            .map_err(|e| pixui_error!("resize software surface: {e}"))?;
        let mut buffer = surface
            .buffer_mut()
            .map_err(|e| pixui_error!("acquire software buffer: {e}"))?;
        buffer.copy_from_slice(&pixels);
        self.window.pre_present_notify();
        buffer
            .present()
            .map_err(|e| pixui_error!("present software buffer: {e}"))?;
        self.timings = RendererTimings {
            acquisition,
            resources: std::time::Duration::ZERO,
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
    }
    fn resume(&mut self) -> PixuiResult<()> {
        if self.surface.is_none() {
            let context = Context::new(self.window.clone())
                .map_err(|e| pixui_error!("create software context: {e}"))?;
            self.surface = Some(
                Surface::new(&context, self.window.clone())
                    .map_err(|e| pixui_error!("create software surface: {e}"))?,
            );
        }
        Ok(())
    }
}
