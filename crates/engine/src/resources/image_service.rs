//! Worker-local sharing of images selected by resource path.
use super::reload::shared::Shared;
use super::{image_loader::ImageLoader, path::ResourcePath};
use crate::ui::image::{Image, ImageData};
use pixui_base::{PixuiResult, pixui_error};
use std::{
    collections::HashMap,
    sync::{Arc, Weak},
};

/// Ordinary lookup is weak; component state and outputs own pixels.
/// A live reload session additionally retains watched snapshots in Shared.
/// Dead weak entries are pruned on misses. Loader replacement resets lookup.
#[derive(Default)]
pub(crate) struct ImageService {
    loader: Option<ImageLoader>,
    images: HashMap<ResourcePath, Weak<ImageData>>,
    reload: Option<Arc<Shared>>,
}
impl ImageService {
    pub fn new(loader: ImageLoader) -> Self {
        Self {
            loader: Some(loader),
            images: HashMap::new(),
            reload: None,
        }
    }
    pub fn loader(&self) -> Option<&ImageLoader> {
        self.loader.as_ref()
    }
    pub fn replace(&mut self, path: ResourcePath, image: &Image) {
        self.images.insert(path, image.downgrade());
    }
    pub fn watch(&mut self, shared: Arc<Shared>) -> PixuiResult<()> {
        let mut images = shared.images.lock().unwrap();
        if self.images.len() > shared.image_capacity {
            return Err(pixui_error!("too many image reload targets"));
        }
        for (path, weak) in &self.images {
            if images.requested.insert(path.clone()) {
                images.subscriptions.push(path.clone());
            }
            if let Some(image) = Image::upgrade(weak) {
                images.snapshots.insert(path.clone(), image);
            }
        }
        drop(images);
        self.reload = Some(shared);
        Ok(())
    }
    pub fn load(&mut self, path: &ResourcePath) -> PixuiResult<Image> {
        if let Some(shared) = &self.reload {
            if shared.active() {
                let mut images = shared.images.lock().unwrap();
                if !images.requested.contains(path) {
                    if images.requested.len() >= shared.image_capacity {
                        return Err(pixui_error!("too many image reload targets"));
                    }
                    if images.requested.insert(path.clone()) {
                        images.subscriptions.push(path.clone());
                    }
                    shared.wake();
                }
                if let Some(image) = images.snapshots.get(path) {
                    return Ok(image.clone());
                }
                return Err(pixui_error!(
                    "image resource `{}` is loading in background",
                    path.as_str()
                ));
            }
            self.reload = None;
        }
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
