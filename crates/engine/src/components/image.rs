//! General resource-path image component, independent of its painter.
use crate::{
    expression::context::ExpressionContext, live_model::component::Component,
    resources::path::ResourcePath, ui::image::Image,
};
use pixui_base::{PixuiResult, PixuiString};

/// Displays a resource selected by relative filename. The application must
/// configure an image loader before the component is prepared.
pub struct ImageComponent;
/// The filename is resolved through the application's layered resource source.
pub struct ImageProps {
    pub path: ResourcePath,
}
impl ImageProps {
    pub fn new(path: impl Into<PixuiString>) -> PixuiResult<Self> {
        Ok(Self {
            path: ResourcePath::new(path)?,
        })
    }
}
/// Prepared snapshot retained between frames. Defaults to unloaded; failures
/// leave the previous snapshot intact and prevent publication of a partial frame.
#[derive(Default)]
pub struct ImageState {
    image: Option<Image>,
}
impl ImageState {
    /// Available after successful preparation, including to custom painters.
    pub fn image(&self) -> Option<&Image> {
        self.image.as_ref()
    }
}
impl Component for ImageComponent {
    type Props = ImageProps;
    type State = ImageState;
    fn prepare(
        props: &ImageProps,
        state: &mut ImageState,
        context: &ExpressionContext<'_>,
    ) -> PixuiResult<()> {
        state.image = Some(context.application()?.load_image(&props.path)?);
        Ok(())
    }
}
