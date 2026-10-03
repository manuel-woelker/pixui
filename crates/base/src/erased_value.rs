//! Owned erased values used at thread boundaries.

use std::any::Any;

/// An owned value that can move between threads. Shared access is not required.
pub type SendValue = Box<dyn Any + Send>;

/// Positional owned inputs, each independently safe to move between threads.
pub type SendValues = Vec<SendValue>;
