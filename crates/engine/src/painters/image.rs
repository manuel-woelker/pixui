//! Aspect-preserving image painting with no resource I/O.
use crate::{
    components::image::ImageComponent,
    painters::{context::PaintContext, painter::Painter},
    ui::geometry::Rect,
};
use pixui_base::{PixuiResult, pixui_error};

/// Centers the prepared image within the available dimensions, scaling to fit
/// while preserving aspect ratio. No background is painted through transparency.
pub struct ImagePainter;
impl Painter<ImageComponent> for ImagePainter {
    fn measure(
        &self,
        context: &crate::painters::measure::MeasureContext<'_, ImageComponent>,
    ) -> PixuiResult<crate::ui::geometry::Size> {
        let image = context
            .state
            .image()
            .ok_or_else(|| pixui_error!("image component has not been prepared"))?;
        let ratio = image.width() as f32 / image.height() as f32;
        let width = context.constraints.width.unwrap_or_else(|| {
            context
                .constraints
                .height
                .map_or(image.width() as f32, |h| h * ratio)
        });
        let height = context.constraints.height.unwrap_or(width / ratio);
        Ok(crate::ui::geometry::Size { width, height })
    }
    fn paint(&self, context: &mut PaintContext<'_, ImageComponent>) -> PixuiResult<()> {
        let image = context
            .state
            .image()
            .ok_or_else(|| pixui_error!("image component has not been prepared"))?;
        let scale =
            (context.width / image.width() as f32).min(context.height / image.height() as f32);
        let width = image.width() as f32 * scale;
        let height = image.height() as f32 * scale;
        context.image(
            image,
            Rect {
                x: (context.width - width) / 2.0,
                y: (context.height - height) / 2.0,
                width,
                height,
            },
        );
        Ok(())
    }
}
