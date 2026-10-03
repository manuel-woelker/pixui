use std::{any::Any, sync::Arc};

use pixui_base::PixuiResult;

use crate::{FieldIndex, MethodIndex, Reflect, TypeDescriptor};

/// Owns an erased value and a shared descriptor. Construction checks their types.
///
/// Values must be `'static`; objects are not required to be `Send` or `Sync`.
/// Descriptors can be shared across objects through `Arc`.
pub struct DynamicObject {
    value: Box<dyn Any>,
    descriptor: Arc<TypeDescriptor>,
}

impl DynamicObject {
    /// Erases a reflected value using its automatically generated descriptor.
    pub fn from_reflect<T: Reflect>(value: T) -> Self {
        Self::new(value, T::type_descriptor())
            .expect("Reflect implementation must describe its own receiver type")
    }

    pub fn new<T: Any>(value: T, descriptor: Arc<TypeDescriptor>) -> PixuiResult<Self> {
        descriptor.check_receiver(&value)?;
        Ok(Self {
            value: Box::new(value),
            descriptor,
        })
    }

    pub fn descriptor(&self) -> &TypeDescriptor {
        &self.descriptor
    }
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        self.value.downcast_ref()
    }
    pub fn downcast_mut<T: Any>(&mut self) -> Option<&mut T> {
        self.value.downcast_mut()
    }
    pub fn field_index(&self, name: &str) -> PixuiResult<FieldIndex> {
        self.descriptor.field_index(name)
    }
    pub fn method_index(&self, name: &str) -> PixuiResult<MethodIndex> {
        self.descriptor.method_index(name)
    }
    pub fn read(&self, index: FieldIndex) -> PixuiResult<&dyn Any> {
        self.descriptor.read(self.value.as_ref(), index)
    }
    pub fn read_named(&self, name: &str) -> PixuiResult<&dyn Any> {
        self.read(self.field_index(name)?)
    }
    pub fn invoke(
        &mut self,
        index: MethodIndex,
        arguments: &[&dyn Any],
    ) -> PixuiResult<Box<dyn Any>> {
        self.descriptor
            .invoke(self.value.as_mut(), index, arguments)
    }
    pub fn invoke_named(
        &mut self,
        name: &str,
        arguments: &[&dyn Any],
    ) -> PixuiResult<Box<dyn Any>> {
        self.invoke(self.method_index(name)?, arguments)
    }
}
