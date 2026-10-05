#![doc = include_str!("Resources.md")]

use super::resource::{Resource, ResourceIdentity};
use std::{
    collections::HashMap,
    fmt,
    hash::{Hash, Hasher},
    marker::PhantomData,
    ops::Index,
};

/// Index into a table of T. The type prevents using an image index for a font;
/// it does not identify a particular table/frame. Validate untrusted indices.
///
/// ```compile_fail
/// use pixui_engine::ui::resource_table::ResourceIndex;
/// let image: ResourceIndex<u8> = ResourceIndex::from_raw(0);
/// let font: ResourceIndex<String> = image;
/// ```
pub struct ResourceIndex<T> {
    index: usize,
    marker: PhantomData<fn() -> T>,
}
impl<T> ResourceIndex<T> {
    /// Constructs an index for deserialization/validation; no bounds are checked.
    pub const fn from_raw(index: usize) -> Self {
        Self {
            index,
            marker: PhantomData,
        }
    }
    pub const fn as_usize(self) -> usize {
        self.index
    }
}
impl<T> Copy for ResourceIndex<T> {}
impl<T> Clone for ResourceIndex<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> PartialEq for ResourceIndex<T> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}
impl<T> Eq for ResourceIndex<T> {}
impl<T> Hash for ResourceIndex<T> {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        self.index.hash(hasher);
    }
}
impl<T> fmt::Debug for ResourceIndex<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ResourceIndex")
            .field(&self.index)
            .finish()
    }
}

/// Finished immutable table. Cloning clones handles, not payloads. Equality
/// compares ordered allocation identities. Iteration cannot mutate entries.
pub struct ResourceTable<T> {
    entries: Vec<Resource<T>>,
}
impl<T> ResourceTable<T> {
    pub(crate) fn storage_bytes(&self) -> usize {
        self.entries.capacity() * std::mem::size_of::<Resource<T>>()
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn get(&self, index: ResourceIndex<T>) -> Option<&Resource<T>> {
        self.entries.get(index.index)
    }
    pub fn iter(&self) -> std::slice::Iter<'_, Resource<T>> {
        self.entries.iter()
    }
}
impl<T> Default for ResourceTable<T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}
impl<T> Clone for ResourceTable<T> {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
        }
    }
}
impl<T> PartialEq for ResourceTable<T> {
    fn eq(&self, other: &Self) -> bool {
        self.entries == other.entries
    }
}
impl<T> Eq for ResourceTable<T> {}
impl<T: fmt::Debug> fmt::Debug for ResourceTable<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.entries.fmt(formatter)
    }
}
impl<T> From<Vec<Resource<T>>> for ResourceTable<T> {
    /// Preserves order and duplicates for externally constructed command lists.
    fn from(entries: Vec<Resource<T>>) -> Self {
        Self { entries }
    }
}
impl<T> Index<ResourceIndex<T>> for ResourceTable<T> {
    type Output = Resource<T>;
    fn index(&self, index: ResourceIndex<T>) -> &Self::Output {
        &self.entries[index.index]
    }
}
// Positional inspection is useful in tests and diagnostics. Commands should use
// typed indices, and untrusted input should use get before indexing.
impl<T> Index<usize> for ResourceTable<T> {
    type Output = Resource<T>;
    fn index(&self, index: usize) -> &Self::Output {
        &self.entries[index]
    }
}

/// Builder retains ownership while its address-based reverse lookup exists,
/// preventing reused allocation addresses from being mistaken for old entries.
pub struct ResourceTableBuilder<T> {
    table: ResourceTable<T>,
    indices: HashMap<ResourceIdentity<T>, ResourceIndex<T>>,
}
impl<T> Default for ResourceTableBuilder<T> {
    fn default() -> Self {
        Self {
            table: ResourceTable::default(),
            indices: HashMap::new(),
        }
    }
}
impl<T> ResourceTableBuilder<T> {
    pub fn insert(&mut self, resource: &Resource<T>) -> ResourceIndex<T> {
        *self.indices.entry(resource.identity()).or_insert_with(|| {
            let index = ResourceIndex::from_raw(self.table.len());
            self.table.entries.push(resource.clone());
            index
        })
    }
    /// Discards reverse lookup metadata while retaining all snapshot ownership.
    pub fn finish(self) -> ResourceTable<T> {
        self.table
    }
}
