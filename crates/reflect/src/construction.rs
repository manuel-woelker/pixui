//! Diagnostics used by automatically generated field constructors.

use pixui_base::{PixuiError, pixui_error};

/// Identifies a positional constructor argument with the wrong concrete type.
pub fn field_error<T: 'static>(index: usize, name: &str) -> PixuiError {
    pixui_error!(
        "field {index} `{name}` expects `{}`",
        std::any::type_name::<T>()
    )
}
