use super::application_slice::SliceId;

/// Process-local address of a collection: stable slice identity and collection index.
/// Collection registration is append-only, so indices survive later additions.
/// Removing the slice invalidates the key; a replacement with the same name has a
/// different identity. The key does not encode the collection's item type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CollectionKey {
    pub slice: SliceId,
    pub collection_index: usize,
}

impl CollectionKey {
    pub fn new(slice: SliceId, collection_index: usize) -> Self {
        Self {
            slice,
            collection_index,
        }
    }

    pub fn slice_id(&self) -> SliceId {
        self.slice
    }

    pub fn collection_index(&self) -> usize {
        self.collection_index
    }
}
