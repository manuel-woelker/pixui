//! Ordered overrides. Only absence permits fallback.
use super::{
    filesystem::{ResourceFilesystem, ResourceReader},
    path::ResourcePath,
};
use pixui_base::PixuiResult;
use std::sync::Arc;

/// Sources are tried in constructor order. The first opened reader wins;
/// subsequent read/decode failures never fall back to a lower-priority source.
/// Layers may themselves be layered. An empty stack contains no resources.
pub struct LayeredFilesystem {
    sources: Vec<Arc<dyn ResourceFilesystem>>,
}
impl LayeredFilesystem {
    pub fn new(sources: Vec<Arc<dyn ResourceFilesystem>>) -> Self {
        Self { sources }
    }
}
impl ResourceFilesystem for LayeredFilesystem {
    fn open(&self, path: &ResourcePath) -> PixuiResult<Option<ResourceReader>> {
        for source in &self.sources {
            if let Some(reader) = source.open(path)? {
                return Ok(Some(reader));
            }
        }
        Ok(None)
    }
}
