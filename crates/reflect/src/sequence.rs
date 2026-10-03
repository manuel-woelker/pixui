use crate::{DynamicObject, Reflect, TypeDescriptor};
use pixui_base::{PixuiResult, pixui_error};
use std::{
    any::{Any, TypeId, type_name},
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

/// Logical shape, independent of ownership and write capability.
/// Maps and references are not separate reflected shapes in the current API.
pub enum TypeKind {
    Struct,
    Sequence(SequenceDescriptor),
}

/// Sequence metadata. Element descriptors are resolved lazily to allow recursive types.
pub struct SequenceDescriptor {
    pub(super) element: fn() -> &'static TypeDescriptor,
    pub(super) access: Option<SequenceCallbacks>,
}

impl SequenceDescriptor {
    pub fn element_type(&self) -> &'static TypeDescriptor {
        (self.element)()
    }
}

pub(super) struct SequenceCallbacks {
    pub len: fn(&dyn Any) -> PixuiResult<usize>,
    pub get: for<'a> fn(&'a dyn Any, usize) -> PixuiResult<DynamicObject<'a>>,
    pub get_mut: for<'a> fn(&'a mut dyn Any, usize) -> PixuiResult<DynamicObject<'a>>,
}

// Generic functions cannot have a separate local static per monomorphization.
// Retain one descriptor per concrete collection type for the process lifetime.
fn cached<T: ?Sized + 'static>(build: impl FnOnce() -> TypeDescriptor) -> &'static TypeDescriptor {
    static CACHE: OnceLock<Mutex<HashMap<TypeId, &'static TypeDescriptor>>> = OnceLock::new();
    let mut cache = CACHE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("descriptor cache poisoned");
    cache
        .entry(TypeId::of::<T>())
        .or_insert_with(|| Box::leak(Box::new(build())))
}

impl<T: Reflect> Reflect for Vec<T> {
    fn type_descriptor() -> &'static TypeDescriptor {
        cached::<Self>(|| {
            TypeDescriptor::sequence::<Self>(SequenceDescriptor {
                element: T::type_descriptor,
                access: Some(SequenceCallbacks {
                    len: |value| Ok(vector::<T>(value)?.len()),
                    get: |value, index| {
                        let value = vector::<T>(value)?;
                        let element = value.get(index).ok_or_else(|| bounds(index, value.len()))?;
                        Ok(DynamicObject::from_ref(element))
                    },
                    get_mut: |value, index| {
                        let value = value.downcast_mut::<Vec<T>>().ok_or_else(|| {
                            pixui_error!("receiver must have type `{}`", type_name::<Vec<T>>())
                        })?;
                        let len = value.len();
                        let element = value.get_mut(index).ok_or_else(|| bounds(index, len))?;
                        Ok(DynamicObject::from_mut(element))
                    },
                }),
            })
        })
    }
}

fn vector<T: Reflect>(value: &dyn Any) -> PixuiResult<&Vec<T>> {
    value
        .downcast_ref()
        .ok_or_else(|| pixui_error!("receiver must have type `{}`", type_name::<Vec<T>>()))
}

pub(super) fn slice_descriptor<T: Reflect>() -> &'static TypeDescriptor {
    cached::<[T]>(|| {
        TypeDescriptor::sequence::<[T]>(SequenceDescriptor {
            element: T::type_descriptor,
            access: None,
        })
    })
}

pub(super) fn bounds(index: usize, len: usize) -> pixui_base::PixuiError {
    pixui_error!("sequence index {index} out of bounds for length {len}")
}

pub(super) trait SharedSequence {
    fn len(&self) -> usize;
    fn get(&self, index: usize) -> PixuiResult<DynamicObject<'_>>;
}

pub(super) trait MutableSequence: SharedSequence {
    fn get_mut(&mut self, index: usize) -> PixuiResult<DynamicObject<'_>>;
}

pub(super) struct SliceRef<'a, T>(pub &'a [T]);
pub(super) struct SliceMut<'a, T>(pub &'a mut [T]);

impl<T: Reflect> SharedSequence for SliceRef<'_, T> {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn get(&self, index: usize) -> PixuiResult<DynamicObject<'_>> {
        Ok(DynamicObject::from_ref(
            self.0
                .get(index)
                .ok_or_else(|| bounds(index, self.0.len()))?,
        ))
    }
}
impl<T: Reflect> SharedSequence for SliceMut<'_, T> {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn get(&self, index: usize) -> PixuiResult<DynamicObject<'_>> {
        Ok(DynamicObject::from_ref(
            self.0
                .get(index)
                .ok_or_else(|| bounds(index, self.0.len()))?,
        ))
    }
}
impl<T: Reflect> MutableSequence for SliceMut<'_, T> {
    fn get_mut(&mut self, index: usize) -> PixuiResult<DynamicObject<'_>> {
        let len = self.0.len();
        Ok(DynamicObject::from_mut(
            self.0.get_mut(index).ok_or_else(|| bounds(index, len))?,
        ))
    }
}
