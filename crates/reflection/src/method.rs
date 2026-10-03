use std::any::{Any, type_name};

use pixui_base::{PixuiResult, pixui_error};

/// A method adapter with borrowed arguments and an owned return value.
///
/// Use [`argument`] to validate argument types. A method returning nothing
/// should return `Box::new(())`. Adapters may mutate the receiver.
pub type MethodInvoker =
    dyn Fn(&mut dyn Any, &[&dyn Any]) -> PixuiResult<Box<dyn Any>> + Send + Sync;

/// A named method exposed by a descriptor.
pub struct Method {
    pub name: &'static str,
    /// Exact number of arguments, checked before invoking the adapter.
    pub arity: usize,
    pub(super) invoke: Box<MethodInvoker>,
}

impl Method {
    /// Erases a typed method adapter. Validate arguments before mutation.
    pub fn new<T: Any>(
        name: &'static str,
        arity: usize,
        invoke: fn(&mut T, &[&dyn Any]) -> PixuiResult<Box<dyn Any>>,
    ) -> Self {
        Self {
            name,
            arity,
            invoke: Box::new(move |receiver, arguments| {
                let receiver = receiver.downcast_mut::<T>().ok_or_else(|| {
                    pixui_error!("method `{name}` requires receiver `{}`", type_name::<T>())
                })?;
                invoke(receiver, arguments)
            }),
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
