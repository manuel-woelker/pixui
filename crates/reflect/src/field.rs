use std::any::{Any, TypeId, type_name};

use crate::{DynamicObject, Reflect};
use pixui_base::{PixuiResult, pixui_error};

/// A borrowed field getter. Its output cannot outlive the receiver.
pub type FieldGetter = dyn for<'a> Fn(&'a dyn Any) -> PixuiResult<&'a dyn Any> + Send + Sync;

/// A named, read-only field exposed by a descriptor.
pub struct Field {
    pub name: &'static str,
    value_type: Option<(TypeId, &'static str)>,
    pub(super) get: Box<FieldGetter>,
    pub(super) get_object: Option<Box<FieldObjectGetter>>,
}

pub type FieldObjectGetter =
    dyn for<'a> Fn(&'a dyn Any) -> PixuiResult<DynamicObject<'a>> + Send + Sync;

impl Field {
    /// Erases a typed getter while preserving its borrowing lifetime.
    pub fn new<T: Any>(name: &'static str, get: for<'a> fn(&'a T) -> &'a dyn Any) -> Self {
        Self {
            name,
            value_type: None,
            get_object: None,
            get: Box::new(move |receiver| {
                let receiver = receiver.downcast_ref::<T>().ok_or_else(|| {
                    pixui_error!("field `{name}` requires receiver `{}`", type_name::<T>())
                })?;
                Ok(get(receiver))
            }),
        }
    }

    /// Registers a getter with concrete field type metadata, for Any reads only.
    /// Use reflected to enable DynamicObject reads for values implementing Reflect.
    pub fn typed<T: Any, V: Any>(name: &'static str, get: fn(&T) -> &V) -> Self {
        Self {
            name,
            value_type: Some((TypeId::of::<V>(), type_name::<V>())),
            get_object: None,
            get: Box::new(move |receiver| {
                let receiver = receiver.downcast_ref::<T>().ok_or_else(|| {
                    pixui_error!("field `{name}` requires receiver `{}`", type_name::<T>())
                })?;
                Ok(get(receiver))
            }),
        }
    }

    /// Exposes a reflected scalar or struct through both Any and object reads.
    pub fn reflected<T: Any, V: Reflect>(name: &'static str, get: fn(&T) -> &V) -> Self {
        let mut field = Self::typed(name, get);
        field.get_object = Some(Box::new(move |receiver| {
            let receiver = receiver.downcast_ref::<T>().ok_or_else(|| {
                pixui_error!("field `{name}` requires receiver `{}`", type_name::<T>())
            })?;
            Ok(DynamicObject::from_ref(get(receiver)))
        }));
        field
    }

    /// Concrete value type, available for automatic and typed registrations.
    pub fn type_id(&self) -> Option<TypeId> {
        self.value_type.map(|(id, _)| id)
    }

    /// Diagnostic type name; not a stable identifier for serialization.
    pub fn type_name(&self) -> Option<&'static str> {
        self.value_type.map(|(_, name)| name)
    }

    /// Registers a vector field for both Any reads and reflected sequence access.
    pub fn sequence<T: Any, E: Reflect>(name: &'static str, get: fn(&T) -> &Vec<E>) -> Self {
        Self {
            name,
            value_type: Some((TypeId::of::<Vec<E>>(), type_name::<Vec<E>>())),
            get: Box::new(move |receiver| {
                let receiver = receiver.downcast_ref::<T>().ok_or_else(|| {
                    pixui_error!("field `{name}` requires receiver `{}`", type_name::<T>())
                })?;
                Ok(get(receiver))
            }),
            get_object: Some(Box::new(move |receiver| {
                let receiver = receiver.downcast_ref::<T>().ok_or_else(|| {
                    pixui_error!("field `{name}` requires receiver `{}`", type_name::<T>())
                })?;
                Ok(DynamicObject::from_ref(get(receiver)))
            })),
        }
    }
}

/// An index into one descriptor's field list.
///
/// Indices are zero-based registration positions, not persistent identifiers.
/// An in-range index from another descriptor cannot be detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldIndex(pub usize);
