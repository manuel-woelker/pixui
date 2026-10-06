//! Owned handles used in action requests instead of borrowed item references.

use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

use pixui_base::Key;
use pixui_reflect::{Reflect, TypeDescriptor};

use super::collection_index::CollectionIndex;

/// An opaque item address: collection index and typed arena key.
///
/// Created by `Application::object_ref` after checking the item exists. It
/// retains no borrow. Dispatch validates it again, rejecting foreign or stale
/// handles. Identities survive reordering; handles are process-local and must
/// not be persisted. Removing an item invalidates the handle; removing a slice
/// leaves it valid because collection storage belongs to the application.
pub struct ObjectRef<T> {
    pub(super) collection: CollectionIndex,
    pub(super) key: Key<T>,
}

impl<T> Copy for ObjectRef<T> {}
impl<T> Clone for ObjectRef<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Any> Reflect for ObjectRef<T> {
    fn type_descriptor() -> &'static TypeDescriptor {
        static DESCRIPTORS: OnceLock<Mutex<HashMap<TypeId, &'static TypeDescriptor>>> =
            OnceLock::new();
        let mut descriptors = DESCRIPTORS
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .expect("object reference descriptor cache poisoned");
        descriptors.entry(TypeId::of::<Self>()).or_insert_with(|| {
            Box::leak(Box::new(
                TypeDescriptor::new::<Self>(vec![], vec![])
                    .expect("opaque references have no members"),
            ))
        })
    }
}
