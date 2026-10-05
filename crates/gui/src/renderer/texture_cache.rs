//! Weak identity cache with bounded residency. Current-frame resources are
//! retained through command submission; trimming occurs only after submission.
use femtovg::ImageId;
use pixui_engine::ui::resource::{Resource, ResourceIdentity};
use std::{collections::HashMap, sync::Weak};

struct Entry<T> {
    source: Weak<T>,
    image: ImageId,
    bytes: usize,
    used: u64,
}
pub(crate) struct TextureCache<T> {
    entries: HashMap<ResourceIdentity<T>, Entry<T>>,
    pub uploads: u64,
}
impl<T> Default for TextureCache<T> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            uploads: 0,
        }
    }
}
impl<T> TextureCache<T> {
    pub fn get(&mut self, source: &Resource<T>, frame: u64) -> Option<ImageId> {
        let entry = self.entries.get_mut(&source.identity())?;
        entry.source.upgrade()?;
        entry.used = frame;
        Some(entry.image)
    }
    pub fn insert(&mut self, source: &Resource<T>, image: ImageId, bytes: usize, frame: u64) {
        self.entries.insert(
            source.identity(),
            Entry {
                source: source.downgrade(),
                image,
                bytes,
                used: frame,
            },
        );
        self.uploads += 1;
    }
    pub fn bytes(&self) -> usize {
        self.entries.values().map(|e| e.bytes).sum()
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn prune(&mut self) -> Vec<ImageId> {
        let dead: Vec<_> = self
            .entries
            .iter()
            .filter(|(_, e)| e.source.upgrade().is_none())
            .map(|(key, _)| *key)
            .collect();
        dead.into_iter()
            .map(|key| self.entries.remove(&key).unwrap().image)
            .collect()
    }
    pub fn oldest(&self) -> Option<u64> {
        self.entries.values().map(|e| e.used).min()
    }
    pub fn evict(&mut self) -> Option<ImageId> {
        let key = *self.entries.iter().min_by_key(|(_, e)| e.used)?.0;
        Some(self.entries.remove(&key).unwrap().image)
    }
}
