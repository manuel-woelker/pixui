//! Typed, process-local component addresses scoped to one application registry.

use crate::live_model::component::Component;
use std::{any::TypeId, marker::PhantomData};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ComponentAddress {
    pub registry: u64,
    pub index: usize,
    pub component_type: TypeId,
}

/// An append-only registration index and registry identity. Copying this handle
/// requires no bounds on the component, props, or state; it owns no state.
pub struct ComponentId<C: Component> {
    pub(crate) address: ComponentAddress,
    pub(crate) marker: PhantomData<fn() -> C>,
}

impl<C: Component> Copy for ComponentId<C> {}
impl<C: Component> Clone for ComponentId<C> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<C: Component> std::fmt::Debug for ComponentId<C> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.address.fmt(formatter)
    }
}
