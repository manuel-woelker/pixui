//! Type-erased item addresses for heterogeneous named bindings and expressions.

use super::{app::Application, collection_index::CollectionIndex, object_ref::ObjectRef};
use pixui_base::{PixuiResult, pixui_error};
use pixui_reflect::{DynamicObject, Reflect};
use std::{
    any::{Any, TypeId, type_name},
    sync::Arc,
};

trait Reference: Send + Sync {
    fn as_any(&self) -> &dyn Any;
    fn value_type(&self) -> TypeId;
    fn value_type_name(&self) -> &'static str;
    fn address(&self) -> (CollectionIndex, u64);
    fn read<'a>(&self, application: &'a Application) -> PixuiResult<DynamicObject<'a>>;
}
impl<T: Reflect> Reference for ObjectRef<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn value_type(&self) -> TypeId {
        TypeId::of::<T>()
    }
    fn value_type_name(&self) -> &'static str {
        type_name::<T>()
    }
    fn address(&self) -> (CollectionIndex, u64) {
        (self.collection, self.key.bits())
    }
    fn read<'a>(&self, application: &'a Application) -> PixuiResult<DynamicObject<'a>> {
        Ok(DynamicObject::from_ref(application.resolve(*self)?))
    }
}

/// Cheaply clonable erased `ObjectRef<T>`. Shares only address metadata, never the
/// stored value. Reading checks collection ownership, concrete type and generation.
/// Erasure requires Reflect; values themselves need not be Sync.
#[derive(Clone)]
pub struct ErasedObjectRef(Arc<dyn Reference>);
impl ErasedObjectRef {
    pub fn new<T: Reflect>(reference: ObjectRef<T>) -> Self {
        Self(Arc::new(reference))
    }
    pub fn item_type_id(&self) -> TypeId {
        self.0.value_type()
    }
    pub fn item_type_name(&self) -> &'static str {
        self.0.value_type_name()
    }
    /// Returns the original typed address, without claiming its item is still live.
    pub fn typed<T: Any>(&self) -> PixuiResult<ObjectRef<T>> {
        self.0
            .as_any()
            .downcast_ref::<ObjectRef<T>>()
            .copied()
            .ok_or_else(|| {
                pixui_error!(
                    "entity must contain `{}`, got `{}`",
                    type_name::<T>(),
                    self.item_type_name()
                )
            })
    }
    pub(crate) fn address(&self) -> (CollectionIndex, u64) {
        self.0.address()
    }
    pub(crate) fn read<'a>(&self, application: &'a Application) -> PixuiResult<DynamicObject<'a>> {
        self.0.read(application)
    }
}
