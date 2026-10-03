
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
use crate::live_model::component::Component;

pub struct GenericComponentState {
    state: Box<dyn Any>
}