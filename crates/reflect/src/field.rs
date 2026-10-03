use std::any::{Any, type_name};

use pixui_base::{PixuiResult, pixui_error};

/// A borrowed field getter. Its output cannot outlive the receiver.
pub type FieldGetter = dyn for<'a> Fn(&'a dyn Any) -> PixuiResult<&'a dyn Any> + Send + Sync;

/// A named, read-only field exposed by a descriptor.
pub struct Field {
    pub name: &'static str,
    pub(super) get: Box<FieldGetter>,
}

impl Field {
    /// Erases a typed getter while preserving its borrowing lifetime.
    pub fn new<T: Any>(name: &'static str, get: for<'a> fn(&'a T) -> &'a dyn Any) -> Self {
        Self {
            name,
            get: Box::new(move |receiver| {
                let receiver = receiver.downcast_ref::<T>().ok_or_else(|| {
                    pixui_error!("field `{name}` requires receiver `{}`", type_name::<T>())
                })?;
                Ok(get(receiver))
            }),
        }
    }
}

/// An index into one descriptor's field list.
///
/// Indices are zero-based registration positions, not persistent identifiers.
/// An in-range index from another descriptor cannot be detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldIndex(pub usize);
