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
