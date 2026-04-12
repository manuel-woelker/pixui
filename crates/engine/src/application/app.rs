use crate::application::event_handler::ApplicationEventHandler;
use crate::entity::store::EntityStore;
use std::any::TypeId;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::marker::PhantomData;

pub struct EventHandlerHolder<E, H: ApplicationEventHandler<Event = E>> {
    event_handler: H,
    event_marker: PhantomData<fn(E)>,
}

impl<E, H: ApplicationEventHandler<Event = E>> EventHandlerHolder<E, H> {
    pub fn new(event_handler: H) -> Self {
        Self {
            event_handler,
            event_marker: PhantomData,
        }
    }
}

pub trait DynEventHandlerHolder {
    fn event_type_id(&self) -> TypeId;

    fn handler_type_id(&self) -> TypeId;
}

impl<E, H> DynEventHandlerHolder for EventHandlerHolder<E, H>
where
    E: 'static,
    H: ApplicationEventHandler<Event = E> + 'static,
{
    fn event_type_id(&self) -> TypeId {
        TypeId::of::<E>()
    }

    fn handler_type_id(&self) -> TypeId {
        let _ = &self.event_handler;
        TypeId::of::<H>()
    }
}

#[derive(Default)]
pub struct Application {
    store: EntityStore,
    event_handlers: Vec<Vec<Box<dyn DynEventHandlerHolder>>>,
    event_type_map: HashMap<TypeId, usize>,
}

impl Application {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn entity_store(&self) -> &EntityStore {
        &self.store
    }

    pub fn entity_store_mut(&mut self) -> &mut EntityStore {
        &mut self.store
    }

    pub fn add_event_handler<E: 'static, H: ApplicationEventHandler<Event = E> + 'static>(
        &mut self,
        handler: impl Into<H>,
    ) {
        let handler = EventHandlerHolder::<E, H>::new(handler.into());
        debug_assert_eq!(handler.event_type_id(), TypeId::of::<E>());
        let _ = handler.handler_type_id();
        let handler_index = match self.event_type_map.entry(TypeId::of::<E>()) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let index = self.event_handlers.len();
                self.event_handlers.push(Vec::new());
                entry.insert(index);
                index
            }
        };
        self.event_handlers[handler_index].push(Box::new(handler));
    }
}
