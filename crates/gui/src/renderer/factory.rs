//! Built-in renderer selection. Auto fallback happens only during initialization.
use super::{
    contract::{Renderer, RendererFactory},
    femtovg::FemtovgRenderer,
    gpu::Gpu,
    software::SoftwareRenderer,
};
use pixui_base::{PixuiResult, pixui_error};
use std::{rc::Rc, str::FromStr, sync::Arc};
use winit::window::Window;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RendererSelection {
    #[default]
    Auto,
    Software,
    Femtovg,
}
impl FromStr for RendererSelection {
    type Err = pixui_base::PixuiError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "auto" => Ok(Self::Auto),
            "software" => Ok(Self::Software),
            "femtovg" => Ok(Self::Femtovg),
            _ => Err(pixui_error!(
                "unknown renderer `{value}`; use auto, software, or femtovg"
            )),
        }
    }
}
pub struct BuiltinRendererFactory {
    selection: RendererSelection,
    gpu: Option<Rc<Gpu>>,
}
impl BuiltinRendererFactory {
    pub fn new(selection: RendererSelection) -> Self {
        Self {
            selection,
            gpu: None,
        }
    }
    fn gpu(&mut self, window: Arc<dyn Window>) -> PixuiResult<Box<dyn Renderer>> {
        if let Some(gpu) = &self.gpu {
            let surface = gpu
                .instance
                .create_surface(window.clone())
                .map_err(|e| pixui_error!("create GPU surface: {e}"))?;
            if surface.get_capabilities(&gpu.adapter).formats.is_empty() {
                return Err(pixui_error!("shared adapter cannot present this window"));
            }
            return Ok(Box::new(FemtovgRenderer::with_gpu(
                window,
                gpu.clone(),
                surface,
            )?));
        }
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| pixui_error!("create GPU surface: {e}"))?;
        let gpu = Rc::new(Gpu::new(instance, Some(&surface))?);
        if self.selection == RendererSelection::Auto
            && gpu.adapter.get_info().device_type == wgpu::DeviceType::Cpu
        {
            return Err(pixui_error!(
                "adapter is a CPU implementation; prefer software"
            ));
        }
        let renderer = FemtovgRenderer::with_gpu(window, gpu.clone(), surface)?;
        self.gpu = Some(gpu);
        Ok(Box::new(renderer))
    }
}
impl RendererFactory for BuiltinRendererFactory {
    fn create(&mut self, window: Arc<dyn Window>) -> PixuiResult<Box<dyn Renderer>> {
        if self.selection != RendererSelection::Software {
            match self.gpu(window.clone()) {
                Ok(renderer) => return Ok(renderer),
                Err(error) if self.selection == RendererSelection::Auto => {
                    eprintln!("GPU initialization failed; using software: {error:?}")
                }
                Err(error) => return Err(error),
            }
        }
        Ok(Box::new(SoftwareRenderer::new(window)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn renderer_selection_is_explicit_and_rejects_unknown_values() {
        assert_eq!(RendererSelection::default(), RendererSelection::Auto);
        for (name, selection) in [
            ("auto", RendererSelection::Auto),
            ("software", RendererSelection::Software),
            ("femtovg", RendererSelection::Femtovg),
        ] {
            assert_eq!(name.parse::<RendererSelection>().unwrap(), selection);
        }
        assert!("other".parse::<RendererSelection>().is_err());
    }
}
