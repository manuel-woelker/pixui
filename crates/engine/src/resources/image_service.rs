//! Worker-local sharing of images selected by resource path.
use super::{image_loader::ImageLoader, path::ResourcePath};
use crate::ui::image::{Image, ImageData};
use pixui_base::{PixuiResult, pixui_error};
use std::{collections::HashMap, sync::Weak};

/// Weak cache: component state and render outputs own pixels, not this service.
/// Dead entries are pruned on misses. Replacing the service resets path lookup.
#[derive(Default)]
pub(crate) struct ImageService {
    loader: Option<ImageLoader>,
    images: HashMap<ResourcePath, Weak<ImageData>>,
}
impl ImageService {
    pub fn new(loader: ImageLoader) -> Self {
        Self {
            loader: Some(loader),
            images: HashMap::new(),
        }
    }
    pub fn load(&mut self, path: &ResourcePath) -> PixuiResult<Image> {
        if let Some(image) = self.images.get(path).and_then(Image::upgrade) {
            return Ok(image);
        }
        let loader = self.loader.as_ref().ok_or_else(|| {
            pixui_error!(
                "no application image loader configured for `{}`",
                path.as_str()
            )
        })?;
        let image = loader.load(path)?;
        self.images.retain(|_, image| image.strong_count() > 0);
        self.images.insert(path.clone(), image.downgrade());
        Ok(image)
    }
}
