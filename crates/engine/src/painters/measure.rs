//! Pure intrinsic measurement. The solver may call a painter repeatedly; this
//! context intentionally cannot mutate state, draw, dispatch, or schedule work.
use crate::{
    live_model::component::Component,
    ui::{
        geometry::Size,
        presentation::PresentationSettings,
        text::{
            font::{FontConfig, FontFace},
            resource::FontMetrics,
        },
    },
};
use pixui_base::PixuiResult;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AvailableSpace {
    Definite(f32),
    MinContent,
    MaxContent,
}
#[derive(Clone, Copy, Debug)]
pub struct MeasureConstraints {
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub available_width: AvailableSpace,
    pub available_height: AvailableSpace,
}
/// Known dimensions are content-box dimensions and must be respected. Available
/// space is a probe, not an instruction to wrap text. Font access and advances
/// share painting's normalization/scale policy and allocate no atlas coverage.
pub struct MeasureContext<'a, C: Component> {
    pub props: &'a C::Props,
    pub state: &'a C::State,
    pub settings: &'a PresentationSettings,
    pub constraints: MeasureConstraints,
}
impl<C: Component> MeasureContext<'_, C> {
    pub fn measure_text(&self, text: &str, size: f32) -> PixuiResult<Size> {
        self.font_config(size)?.measure_text(text)
    }
    pub fn font_metrics(&self, size: f32) -> PixuiResult<FontMetrics> {
        Ok(self.font_config(size)?.metrics())
    }
    pub fn line_height(&self, size: f32) -> PixuiResult<f32> {
        Ok(self.font_metrics(size)?.line_height)
    }
    fn font_config(&self, size: f32) -> PixuiResult<FontConfig> {
        FontConfig::new(FontFace::geist()?, size, self.settings.scale_factor)
    }
    /// Override natural size on axes fixed by the solver.
    pub fn constrain(&self, natural: Size) -> Size {
        Size {
            width: self.constraints.width.unwrap_or(natural.width),
            height: self.constraints.height.unwrap_or(natural.height),
        }
    }
}
