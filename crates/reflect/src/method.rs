use std::any::{Any, type_name};

use crate::DynamicObject;
use pixui_base::{PixuiResult, pixui_error};

/// A method adapter with borrowed arguments and an owned return value.
///
/// Use [`argument`] to validate argument types. A method returning nothing
/// should return `Box::new(())`. Adapters may mutate the receiver.
pub type MethodInvoker =
    dyn Fn(&mut dyn Any, &[&dyn Any]) -> PixuiResult<Box<dyn Any>> + Send + Sync;

pub type SharedMethodInvoker =
    dyn Fn(&dyn Any, &[&dyn Any]) -> PixuiResult<Box<dyn Any>> + Send + Sync;
pub type RefMethodInvoker =
    dyn for<'a> Fn(&'a dyn Any, &[&dyn Any]) -> PixuiResult<DynamicObject<'a>> + Send + Sync;
pub type MutMethodInvoker =
    dyn for<'a> Fn(&'a mut dyn Any, &[&dyn Any]) -> PixuiResult<DynamicObject<'a>> + Send + Sync;

/// Typed adapter for methods returning owned values with mutable receiver access.
pub type TypedMethodInvoker<T> = fn(&mut T, &[&dyn Any]) -> PixuiResult<Box<dyn Any>>;
/// Typed adapter for methods returning owned values with shared receiver access.
pub type TypedSharedMethodInvoker<T> = fn(&T, &[&dyn Any]) -> PixuiResult<Box<dyn Any>>;

pub(super) enum Invocation {
    Owned(Box<MethodInvoker>),
    SharedOwned(Box<SharedMethodInvoker>),
    Ref(Box<RefMethodInvoker>),
    Mut(Box<MutMethodInvoker>),
}

/// A named method exposed by a descriptor.
pub struct Method {
    pub name: &'static str,
    /// Exact number of arguments, checked before invoking the adapter.
    pub arity: usize,
    pub(super) invocation: Invocation,
}

impl Method {
    /// Erases a typed method adapter. Validate arguments before mutation.
    pub fn new<T: Any>(name: &'static str, arity: usize, invoke: TypedMethodInvoker<T>) -> Self {
        Self {
            name,
            arity,
            invocation: Invocation::Owned(Box::new(move |receiver, arguments| {
                let receiver = receiver.downcast_mut::<T>().ok_or_else(|| {
                    pixui_error!("method `{name}` requires receiver `{}`", type_name::<T>())
                })?;
                invoke(receiver, arguments)
            })),
        }
    }

    /// Registers an owned-returning method callable with shared receiver access.
    pub fn shared<T: Any>(
        name: &'static str,
        arity: usize,
        invoke: TypedSharedMethodInvoker<T>,
    ) -> Self {
        Self {
            name,
            arity,
            invocation: Invocation::SharedOwned(Box::new(move |receiver, args| {
                let receiver = receiver.downcast_ref::<T>().ok_or_else(|| {
                    pixui_error!("method `{name}` requires receiver `{}`", type_name::<T>())
                })?;
                invoke(receiver, args)
            })),
        }
    }

    /// Registers a reflected shared result whose lifetime follows the receiver.
    pub fn returning_ref<T: Any>(
        name: &'static str,
        arity: usize,
        invoke: for<'a> fn(&'a T, &[&dyn Any]) -> PixuiResult<DynamicObject<'a>>,
    ) -> Self {
        Self {
            name,
            arity,
            invocation: Invocation::Ref(Box::new(move |receiver, args| {
                let receiver = receiver.downcast_ref::<T>().ok_or_else(|| {
                    pixui_error!("method `{name}` requires receiver `{}`", type_name::<T>())
                })?;
                invoke(receiver, args)
            })),
        }
    }

    /// Registers a reflected mutable result whose lifetime follows the receiver.
    pub fn returning_mut<T: Any>(
        name: &'static str,
        arity: usize,
        invoke: for<'a> fn(&'a mut T, &[&dyn Any]) -> PixuiResult<DynamicObject<'a>>,
    ) -> Self {
        Self {
            name,
            arity,
            invocation: Invocation::Mut(Box::new(move |receiver, args| {
                let receiver = receiver.downcast_mut::<T>().ok_or_else(|| {
                    pixui_error!("method `{name}` requires receiver `{}`", type_name::<T>())
                })?;
                invoke(receiver, args)
            })),
        }
    }
}

/// An index into one descriptor's method list. See [`crate::FieldIndex`] for scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MethodIndex(pub usize);

/// Reads an adapter argument with an exact concrete type, without coercion.
///
/// `Any` requires the value's type to be `'static`; the borrow itself can be
/// short-lived. In particular, `String` and `&'static str` are distinct types.
pub fn argument<'a, T: Any>(arguments: &[&'a dyn Any], index: usize) -> PixuiResult<&'a T> {
    arguments
        .get(index)
        .ok_or_else(|| pixui_error!("missing argument {index}"))?
        .downcast_ref::<T>()
        .ok_or_else(|| pixui_error!("argument {index} must have type `{}`", type_name::<T>()))
}
