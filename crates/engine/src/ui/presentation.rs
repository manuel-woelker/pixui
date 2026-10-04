//! Independently configured presentation for each UI instance.

use super::geometry::Size;
use pixui_base::{PixuiResult, pixui_error};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PresentationSettings {
    pub theme: Theme,
    /// Application-defined locale identifier; translation is supplied by widgets.
    pub locale: String,
    pub viewport: Size,
    pub scale_factor: f32,
}

impl Default for PresentationSettings {
    fn default() -> Self {
        Self {
            theme: Theme::Light,
            locale: "en".into(),
            viewport: Size {
                width: 800.0,
                height: 600.0,
            },
            scale_factor: 1.0,
        }
    }
}

impl PresentationSettings {
    /// Rejects nonfinite, negative, or impractically large geometry before layout.
    /// Zero-sized viewports are valid while a window is minimized.
    pub fn validate(&self) -> PixuiResult<()> {
        if !self.viewport.width.is_finite()
            || !self.viewport.height.is_finite()
            || !(0.0..=16384.0).contains(&self.viewport.width)
            || !(0.0..=16384.0).contains(&self.viewport.height)
            || !self.scale_factor.is_finite()
            || !(0.1..=16.0).contains(&self.scale_factor)
            || self.locale.is_empty()
        {
            return Err(pixui_error!("invalid presentation settings"));
        }
        Ok(())
    }
}
