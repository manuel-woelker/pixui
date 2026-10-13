//! Explicit adapters for owned callback snapshots and generational references.
use crate::application::{app::Application, collection_key::CollectionKey};
use pixui_base::{PixuiResult, erased_value::SendValue, pixui_error};
use pixui_reflect::{DynamicObject, Reflect, TypeDescriptor};
use std::{
    any::{Any, TypeId},
    sync::Arc,
};
pub type Snapshot = Arc<dyn Any + Send + Sync>;
#[derive(Clone)]
pub struct ValueType {
    pub descriptor: &'static TypeDescriptor,
    pub(crate) snapshot: fn(&DynamicObject<'_>) -> PixuiResult<Snapshot>,
    pub(crate) send: fn(&dyn Any) -> PixuiResult<SendValue>,
}
impl ValueType {
    pub fn of<T: Reflect + Clone + Send + Sync>() -> Self {
        Self {
            descriptor: T::type_descriptor(),
            snapshot: |value| {
                Ok(Arc::new(
                    value
                        .downcast_ref::<T>()
                        .ok_or_else(|| pixui_error!("expected {}", std::any::type_name::<T>()))?
                        .clone(),
                ))
            },
            send: |value| {
                Ok(Box::new(
                    value
                        .downcast_ref::<T>()
                        .ok_or_else(|| pixui_error!("incorrect snapshot type"))?
                        .clone(),
                ))
            },
        }
    }
}
#[derive(Clone)]
pub struct ReferenceExport {
    pub item: TypeId,
    pub value: ValueType,
    pub(crate) entity:
        fn(&crate::application::erased_object_ref::ErasedObjectRef) -> PixuiResult<Snapshot>,
    pub(crate) make: fn(&Application, CollectionKey, usize) -> PixuiResult<Snapshot>,
}
