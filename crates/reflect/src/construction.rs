//! Diagnostics used by automatically generated field constructors.

use pixui_base::erased_value::{SendValue, SendValues};
use pixui_base::{PixuiError, pixui_error};

/// A field-ordered constructor whose inputs and output can cross threads.
pub type SendConstructor = fn(SendValues) -> pixui_base::PixuiResult<SendValue>;

/// Identifies a positional constructor argument with the wrong concrete type.
pub fn field_error<T: 'static>(index: usize, name: &str) -> PixuiError {
    pixui_error!(
        "field {index} `{name}` expects `{}`",
        std::any::type_name::<T>()
    )
}

/// Consumes a sendable input, checking its exact concrete field type.
pub fn take_send<T: 'static>(
    value: SendValue,
    index: usize,
    name: &str,
) -> pixui_base::PixuiResult<T> {
    value
        .downcast::<T>()
        .map(|value| *value)
        .map_err(|_| field_error::<T>(index, name))
}
