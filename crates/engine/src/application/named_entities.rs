//! Named values backed by one unnamed application collection per reflected type.

use super::{
    app::Application, application_slice::SliceId, collection::Collection, object_ref::ObjectRef,
};
use pixui_base::{PixuiResult, pixui_error};
use pixui_reflect::Reflect;
use std::any::{Any, TypeId};

pub(super) type PendingEntity = Box<
    dyn FnOnce(&mut Application) -> PixuiResult<super::erased_object_ref::ErasedObjectRef> + Send,
>;

impl Application {
    /// Inserts into the application's lazily allocated unnamed collection for T.
    /// Explicit collections are separate. Storage survives slice removal.
    pub fn create_entity<T: Reflect + Send>(&mut self, value: T) -> PixuiResult<ObjectRef<T>> {
        let index = match self.ad_hoc_collections.get(&TypeId::of::<T>()).copied() {
            Some(index) => index,
            None => {
                let index = self.store_collection(Collection::new_reflected::<T>(""));
                self.ad_hoc_collections.insert(TypeId::of::<T>(), index);
                index
            }
        };
        let key = self.resolve_collection_mut::<T>(index)?.insert(value);
        self.object_ref_at(index, key)
    }

    /// Adds a named value to a registered slice. Validate the name before any
    /// allocation/insertion, so failed duplicate bindings leave no orphan item.
    pub fn bind<T: Reflect + Send>(
        &mut self,
        slice: SliceId,
        name: impl Into<String>,
        value: T,
    ) -> PixuiResult<()> {
        let name = name.into();
        self.slice(slice)?.validate_entity_name(&name)?;
        let reference = self.create_entity(value)?;
        self.slice_mut(slice)?.bind_entity(name, reference)?;
        self.uis.invalidate_all();
        Ok(())
    }

    /// Shares an existing item under a new slice-local name, after validating it.
    pub fn bind_entity<T: Reflect>(
        &mut self,
        slice: SliceId,
        name: impl Into<String>,
        reference: ObjectRef<T>,
    ) -> PixuiResult<()> {
        self.resolve(reference)?;
        self.slice_mut(slice)?.bind_entity(name, reference)?;
        self.uis.invalidate_all();
        Ok(())
    }

    /// Returns a checked, typed ref for a resolved binding. No borrow is retained.
    pub fn entity_ref<T: Any>(&self, slice: SliceId, name: &str) -> PixuiResult<ObjectRef<T>> {
        let reference = self.slice(slice)?.entity(name)?.typed::<T>()?;
        self.resolve(reference)?;
        Ok(reference)
    }

    pub fn entity<T: Any>(&self, slice: SliceId, name: &str) -> PixuiResult<&T> {
        self.resolve(self.entity_ref(slice, name)?)
    }

    pub fn entity_mut<T: Any>(&mut self, slice: SliceId, name: &str) -> PixuiResult<&mut T> {
        let reference = self.entity_ref(slice, name)?;
        self.resolve_mut(reference)
    }

    /// Borrows an erased reference for expression evaluation. It need not still
    /// have a named binding, but its collection and arena slot must remain live.
    pub fn read_entity<'a>(
        &'a self,
        reference: &super::erased_object_ref::ErasedObjectRef,
    ) -> PixuiResult<pixui_reflect::DynamicObject<'a>> {
        reference.read(self)
    }
}

pub(super) fn validate_name(name: &str) -> PixuiResult<()> {
    if name.is_empty() {
        return Err(pixui_error!("empty entity binding name"));
    }
    Ok(())
}
