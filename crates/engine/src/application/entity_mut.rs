//! Explicit named-entity injection for ordinary action functions.

use std::ops::{Deref, DerefMut};

/// A mutable borrow injected from the slice entity matching the parameter name.
/// `#[action]` omits it from the request. Unlike `&mut T`, which consumes a
/// caller-supplied ObjectRef, this wrapper selects a registered named binding.
/// It cannot escape the action's borrow lifetime. Aliases and explicit lifetime
/// parameters are not recognized by the action macro.
pub struct EntityMut<'a, T>(&'a mut T);
impl<'a, T> EntityMut<'a, T> {
    /// Also allows calling an action directly without application dispatch.
    pub fn new(value: &'a mut T) -> Self {
        Self(value)
    }
}
impl<T> Deref for EntityMut<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.0
    }
}
impl<T> DerefMut for EntityMut<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.0
    }
}
