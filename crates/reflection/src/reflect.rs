use std::{any::Any, sync::Arc};

use crate::TypeDescriptor;

/// Implemented automatically for structs inside a [`crate::reflect`] module.
pub trait Reflect: Any {
    /// Returns the lazily initialized descriptor shared by all instances.
    fn type_descriptor() -> Arc<TypeDescriptor>;
}
