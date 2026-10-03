use std::any::Any;

use pixui_base::{PixuiResult, pixui_error};

use crate::sequence::{MutableSequence, SharedSequence, SliceMut, SliceRef, slice_descriptor};
use crate::{FieldIndex, MethodIndex, Reflect, TypeDescriptor};

enum Storage<'a> {
    Owned(Box<dyn Any>),
    Shared(&'a dyn Any),
    Mutable(&'a mut dyn Any),
    SharedSequence(Box<dyn SharedSequence + 'a>),
    MutableSequence(Box<dyn MutableSequence + 'a>),
}

impl Storage<'_> {
    fn as_any(&self) -> Option<&dyn Any> {
        match self {
            Self::Owned(value) => Some(value.as_ref()),
            Self::Shared(value) => Some(*value),
            Self::Mutable(value) => Some(&**value),
            Self::SharedSequence(_) | Self::MutableSequence(_) => None,
        }
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn Any> {
        match self {
            Self::Owned(value) => Some(value.as_mut()),
            Self::Shared(_) | Self::SharedSequence(_) | Self::MutableSequence(_) => None,
            Self::Mutable(value) => Some(&mut **value),
        }
    }
}

/// An owned or borrowed erased value with a static descriptor.
/// Construction checks the descriptor's receiver type.
///
/// Concrete value types must be `'static`, but receiver borrows may be shorter.
/// Objects are not required to be `Send` or `Sync`.
/// Descriptors live in static storage and are shared without reference counting.
pub struct DynamicObject<'a> {
    storage: Storage<'a>,
    descriptor: &'static TypeDescriptor,
}

impl DynamicObject<'static> {
    /// Erases a reflected value using its automatically generated descriptor.
    pub fn from_reflect<T: Reflect>(value: T) -> Self {
        Self::new(value, T::type_descriptor())
            .expect("Reflect implementation must describe its own receiver type")
    }

    pub fn new<T: Any>(value: T, descriptor: &'static TypeDescriptor) -> PixuiResult<Self> {
        descriptor.check_receiver(&value)?;
        Ok(Self {
            storage: Storage::Owned(Box::new(value)),
            descriptor,
        })
    }
}

impl<'a> DynamicObject<'a> {
    /// Slice adapters borrow their elements; slices cannot be downcast through Any.
    pub fn from_slice<T: Reflect>(value: &'a [T]) -> Self {
        Self {
            storage: Storage::SharedSequence(Box::new(SliceRef(value))),
            descriptor: slice_descriptor::<T>(),
        }
    }
    pub fn from_slice_mut<T: Reflect>(value: &'a mut [T]) -> Self {
        Self {
            storage: Storage::MutableSequence(Box::new(SliceMut(value))),
            descriptor: slice_descriptor::<T>(),
        }
    }

    fn as_any(&self) -> PixuiResult<&dyn Any> {
        self.storage.as_any().ok_or_else(|| {
            pixui_error!(
                "slice storage supports sequence operations, not struct or method operations"
            )
        })
    }

    pub fn len(&self) -> PixuiResult<usize> {
        match &self.storage {
            Storage::SharedSequence(value) => Ok(value.len()),
            Storage::MutableSequence(value) => Ok(value.len()),
            _ => (self.descriptor.sequence_access()?.len)(self.as_any()?),
        }
    }
    pub fn is_empty(&self) -> PixuiResult<bool> {
        Ok(self.len()? == 0)
    }
    pub fn get(&self, index: usize) -> PixuiResult<DynamicObject<'_>> {
        match &self.storage {
            Storage::SharedSequence(value) => value.get(index),
            Storage::MutableSequence(value) => value.get(index),
            _ => (self.descriptor.sequence_access()?.get)(self.as_any()?, index),
        }
    }
    pub fn get_mut(&mut self, index: usize) -> PixuiResult<DynamicObject<'_>> {
        match &mut self.storage {
            Storage::MutableSequence(value) => value.get_mut(index),
            Storage::SharedSequence(_) | Storage::Shared(_) => Err(pixui_error!(
                "object is shared; mutable access is unavailable"
            )),
            storage => {
                let access = self.descriptor.sequence_access()?;
                (access.get_mut)(
                    storage.as_any_mut().expect("owned or mutable storage"),
                    index,
                )
            }
        }
    }

    /// Borrows a reflected value without taking ownership.
    pub fn from_ref<T: Reflect>(value: &'a T) -> Self {
        Self::borrow(value, T::type_descriptor()).expect("Reflect must describe its own type")
    }

    /// Exclusively borrows a reflected value without taking ownership.
    pub fn from_mut<T: Reflect>(value: &'a mut T) -> Self {
        Self::borrow_mut(value, T::type_descriptor()).expect("Reflect must describe its own type")
    }

    pub fn borrow(value: &'a dyn Any, descriptor: &'static TypeDescriptor) -> PixuiResult<Self> {
        descriptor.check_receiver(value)?;
        Ok(Self {
            storage: Storage::Shared(value),
            descriptor,
        })
    }

    pub fn borrow_mut(
        value: &'a mut dyn Any,
        descriptor: &'static TypeDescriptor,
    ) -> PixuiResult<Self> {
        descriptor.check_receiver(value)?;
        Ok(Self {
            storage: Storage::Mutable(value),
            descriptor,
        })
    }

    /// Consumes owned storage without cloning. Borrowed values are rejected.
    pub fn into_owned<T: Any>(self) -> PixuiResult<T> {
        match self.storage {
            Storage::Owned(value) => value
                .downcast::<T>()
                .map(|value| *value)
                .map_err(|_| pixui_error!("expected owned `{}`", std::any::type_name::<T>())),
            _ => Err(pixui_error!(
                "expected owned `{}`",
                std::any::type_name::<T>()
            )),
        }
    }

    pub fn is_mutable(&self) -> bool {
        !matches!(
            self.storage,
            Storage::Shared(_) | Storage::SharedSequence(_)
        )
    }
    pub fn is_owned(&self) -> bool {
        matches!(self.storage, Storage::Owned(_))
    }

    pub fn descriptor(&self) -> &'static TypeDescriptor {
        self.descriptor
    }
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        self.storage.as_any()?.downcast_ref()
    }
    pub fn downcast_mut<T: Any>(&mut self) -> Option<&mut T> {
        self.storage.as_any_mut()?.downcast_mut()
    }
    pub fn field_index(&self, name: &str) -> PixuiResult<FieldIndex> {
        self.descriptor.field_index(name)
    }
    pub fn method_index(&self, name: &str) -> PixuiResult<MethodIndex> {
        self.descriptor.method_index(name)
    }
    pub fn read(&self, index: FieldIndex) -> PixuiResult<&dyn Any> {
        self.descriptor.read(self.as_any()?, index)
    }
    pub fn read_named(&self, name: &str) -> PixuiResult<&dyn Any> {
        self.read(self.field_index(name)?)
    }
    /// Reads a registered reflected field without knowing its concrete type.
    pub fn read_object(&self, index: FieldIndex) -> PixuiResult<DynamicObject<'_>> {
        self.descriptor.read_object(self.as_any()?, index)
    }

    pub fn read_object_named(&self, name: &str) -> PixuiResult<DynamicObject<'_>> {
        self.read_object(self.field_index(name)?)
    }
    pub fn invoke(
        &mut self,
        index: MethodIndex,
        arguments: &[&dyn Any],
    ) -> PixuiResult<Box<dyn Any>> {
        match self.storage.as_any_mut() {
            Some(value) => self.descriptor.invoke(value, index, arguments),
            None => self
                .descriptor
                .invoke_shared(self.as_any()?, index, arguments),
        }
    }
    /// Calls an owned-returning method requiring only shared receiver access.
    pub fn invoke_shared(
        &self,
        index: MethodIndex,
        arguments: &[&dyn Any],
    ) -> PixuiResult<Box<dyn Any>> {
        self.descriptor
            .invoke_shared(self.as_any()?, index, arguments)
    }

    pub fn invoke_shared_named(
        &self,
        name: &str,
        arguments: &[&dyn Any],
    ) -> PixuiResult<Box<dyn Any>> {
        self.invoke_shared(self.method_index(name)?, arguments)
    }

    /// Returns a shared object borrowed from this receiver.
    pub fn invoke_ref(
        &self,
        index: MethodIndex,
        arguments: &[&dyn Any],
    ) -> PixuiResult<DynamicObject<'_>> {
        self.descriptor.invoke_ref(self.as_any()?, index, arguments)
    }

    pub fn invoke_ref_named(
        &self,
        name: &str,
        arguments: &[&dyn Any],
    ) -> PixuiResult<DynamicObject<'_>> {
        self.invoke_ref(self.method_index(name)?, arguments)
    }

    /// Returns an exclusively borrowed object. Shared storage rejects this call.
    pub fn invoke_mut(
        &mut self,
        index: MethodIndex,
        arguments: &[&dyn Any],
    ) -> PixuiResult<DynamicObject<'_>> {
        let value = self
            .storage
            .as_any_mut()
            .ok_or_else(|| pixui_error!("object is shared; mutable access is unavailable"))?;
        self.descriptor.invoke_mut(value, index, arguments)
    }

    pub fn invoke_mut_named(
        &mut self,
        name: &str,
        arguments: &[&dyn Any],
    ) -> PixuiResult<DynamicObject<'_>> {
        self.invoke_mut(self.method_index(name)?, arguments)
    }

    pub fn invoke_named(
        &mut self,
        name: &str,
        arguments: &[&dyn Any],
    ) -> PixuiResult<Box<dyn Any>> {
        self.invoke(self.method_index(name)?, arguments)
    }
}
