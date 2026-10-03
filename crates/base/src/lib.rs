//! Shared arena, string, error, and result types for pixui.

mod arena;
pub mod erased_value;
mod error;

pub use arena::{Arena, Key};

pub use error::{BoxedError, MessageError, PixuiError, PixuiResult, message, report};
pub use hipstr::HipStr;

/// The common owned string representation used by pixui data types.
pub type PixuiString = HipStr<'static>;
