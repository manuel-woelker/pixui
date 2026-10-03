/*
pub struct LiveState {
    part_states: Vec<PartState>,
}

pub enum PartState {
    Component(ComponentState),
    Composite(CompositeState),
}

 */
use crate::live_model::component::Component;
use std::any::Any;

pub struct GenericComponentState {
    state: Box<dyn Any>,
}
