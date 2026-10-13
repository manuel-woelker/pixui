//! Typed input adapters erased at component registration.
use super::interface::{ComponentInput, EventInterface, EventSink};
use crate::live_model::{component::Component, state::GenericComponentState};
use pixui_base::{PixuiResult, pixui_error};
use std::any::Any;

pub(crate) type InputHandler =
    fn(&dyn Any, &mut GenericComponentState, &ComponentInput, &EventSink) -> PixuiResult<bool>;
pub(crate) fn handle<C: Component>(
    props: &dyn Any,
    state: &mut GenericComponentState,
    input: &ComponentInput,
    sink: &EventSink,
) -> PixuiResult<bool> {
    let props = props
        .downcast_ref::<C::Props>()
        .ok_or_else(|| pixui_error!("incorrect component input props"))?;
    let state = state
        .downcast_mut::<C::State>()
        .ok_or_else(|| pixui_error!("incorrect component input state"))?;
    C::Events::with_proxy(sink, |events| C::handle_input(props, state, input, events))
}
