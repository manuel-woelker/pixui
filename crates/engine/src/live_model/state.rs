/*
pub struct LiveState {
    part_states: Vec<PartState>,
}

pub enum PartState {
    Component(ComponentState),
    Composite(CompositeState),
}

 */
use std::any::Any;

pub struct GenericComponentState {
    state: Box<dyn Any>,
}

impl GenericComponentState {
    /// Owns component state while erasing its concrete type.
    pub fn new(state: impl Any) -> Self {
        Self {
            state: Box::new(state),
        }
    }

    /// Returns the state when the requested concrete type matches.
    pub fn downcast_ref<T: Any>(&self) -> Option<&T> {
        self.state.downcast_ref()
    }

    /// Returns exclusive state access when the requested concrete type matches.
    pub fn downcast_mut<T: Any>(&mut self) -> Option<&mut T> {
        self.state.downcast_mut()
    }
}
