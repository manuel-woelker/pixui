use crate::application::event_handler::ApplicationEventHandler;
use crate::entity::store::EntityStore;
use std::any::TypeId;
use std::collections::HashMap;

pub struct EventHandlerHolder<E, H: ApplicationEventHandler<Event = E>> {
    event_handler: H,
}

pub trait DynEventHandlerHolder {}

#[derive(Default)]
pub struct Application {
    store: EntityStore,
    event_handler: Vec<Vec<Box<dyn DynEventHandlerHolder>>>,
    event_type_map: HashMap<TypeId, usize>,
}

impl Application {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn addEventHandler<E, H: ApplicationEventHandler<Event = E>>(
        &mut self,
        handler: impl Into<H>,
    ) {
        let handler = handler.into();
        //        self.event_type_map.entry(TypeId::of::<E>())
    }
}
