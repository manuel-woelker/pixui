//! Shared device setup and backend-independent window limits.

use std::sync::{Arc, Mutex};

use pixui_base::{PixuiResult, pixui_error};

pub(crate) fn validate_size(width: u32, height: u32, scale: f32) -> PixuiResult<()> {
    if u64::from(width) * u64::from(height) > 64_000_000
        || !scale.is_finite()
        || !(0.1..=16.0).contains(&scale)
    {
        return Err(pixui_error!("invalid renderer dimensions or scale"));
    }
    Ok(())
}

pub(crate) struct Gpu {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    failure: Arc<Mutex<Option<String>>>,
}
impl Gpu {
    pub fn new(instance: wgpu::Instance, surface: Option<&wgpu::Surface<'_>>) -> PixuiResult<Self> {
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: surface,
            power_preference: wgpu::PowerPreference::LowPower,
            ..Default::default()
        }))
        .map_err(|e| pixui_error!("find GPU adapter: {e}"))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("pixui GUI"),
            ..Default::default()
        }))
        .map_err(|e| pixui_error!("create GPU device: {e}"))?;
        let failure = Arc::new(Mutex::new(None));
        let device_failure = failure.clone();
        device.set_device_lost_callback(move |reason, message| {
            *device_failure.lock().expect("GPU error lock") =
                Some(format!("GPU device lost ({reason:?}): {message}"));
        });
        let uncaptured = failure.clone();
        device.on_uncaptured_error(Arc::new(move |error| {
            *uncaptured.lock().expect("GPU error lock") =
                Some(format!("GPU rendering failed: {error}"));
        }));
        eprintln!("Pixui GPU: {:?}", adapter.get_info());
        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            failure,
        })
    }
    pub fn check(&self) -> PixuiResult<()> {
        if let Some(error) = self.failure.lock().expect("GPU error lock").as_ref() {
            return Err(pixui_error!("{error}"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dimensions_and_scale_are_checked_before_surface_configuration() {
        assert!(validate_size(0, 0, 1.0).is_ok());
        assert!(validate_size(8000, 8000, 1.0).is_ok());
        assert!(validate_size(8001, 8000, 1.0).is_err());
        for scale in [0.0, f32::NAN, f32::INFINITY, 17.0] {
            assert!(validate_size(100, 100, scale).is_err());
        }
    }
}
