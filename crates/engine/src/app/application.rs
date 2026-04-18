use crate::app::event_handler::{ApplicationEventContext, ApplicationEventHandler};
use crate::entity::store::EntityStore;
use pixui_base::result::{OptionExt, PixuiResult};
use pixui_base::type_map::TypeMap;
use std::any::Any;
use std::any::TypeId;
use std::marker::PhantomData;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use pixui_base::logging::error;
use crate::app::application_handle::ApplicationHandle;
use crate::app::application_message::ApplicationMessage;

/// Stores application-wide state and event handler registrations.
pub struct Application {
    /// The application's entity storage.
    store: EntityStore,
    /// Event handlers grouped by their event type.
    event_handlers: TypeMap<Vec<Box<dyn DynEventHandlerHolder>>>,
    /// Message receiver
    message_rx: Receiver<ApplicationMessage>,
}

impl Application {
    /// Creates an empty application.
    pub fn new() -> PixuiResult<ApplicationHandle> {
        let (tx, rx) = mpsc::sync_channel(1024);
        std::thread::Builder::new().name("pixui Application".to_string()).spawn(move || {
            let application = Application {
                store: EntityStore::default(),
                event_handlers: TypeMap::default(),
                message_rx: rx,
            };
            match application.run() {
                Ok(_) => {

                }
                Err(e) => {
                    error!("{:?}", e);
                }
            }
        })?;
        Ok(ApplicationHandle::new(tx))
    }

    fn run(mut self) -> PixuiResult<()> {
        loop {
            let message = self.message_rx.recv()?;
            match message {}
        }
    }

    /// Returns the entity store owned by this application.
    pub fn entity_store(&self) -> &EntityStore {
        &self.store
    }

    /// Returns mutable access to the entity store owned by this application.
    pub fn entity_store_mut(&mut self) -> &mut EntityStore {
        &mut self.store
    }

    /// Registers an event handler for `E`.
    ///
    /// Multiple handlers can be registered for the same event type. Handlers
    /// are grouped by event type inside the application.
    pub fn add_event_handler<E: 'static, H: ApplicationEventHandler<Event = E> + 'static>(
        &mut self,
        handler: H,
    ) {
        let handler = EventHandlerHolder::<E, H>::new(handler);
        debug_assert_eq!(handler.event_type_id(), TypeId::of::<E>());
        let _ = handler.handler_type_id();
        self.event_handlers
            .get_or_insert_default_mut::<E>()
            .push(Box::new(handler));
    }

    /// Dispatches `event` to every registered handler for `E`.
    ///
    /// Handlers run in the same order they were registered and receive a
    /// shared mutable [`ApplicationEventContext`], which allows earlier
    /// handlers to affect what later handlers observe.
    pub fn handle_event<E: 'static>(&mut self, event: E) -> PixuiResult<()> {
        let Some(handlers) = self.event_handlers.get_mut::<E>() else {
            return Ok(());
        };

        let mut context = ApplicationEventContext { event };
        for handler in handlers {
            handler.handle_event(&mut context)?;
        }

        Ok(())
    }
}

/// Stores a concrete event handler together with the event type it handles.
pub struct EventHandlerHolder<E, H: ApplicationEventHandler<Event = E>> {
    /// The concrete event handler implementation.
    event_handler: H,
    /// Tracks `E` at the type level without storing a runtime value.
    event_marker: PhantomData<fn(E)>,
}

impl<E, H: ApplicationEventHandler<Event = E>> EventHandlerHolder<E, H> {
    /// Creates a holder for a concrete event handler.
    pub fn new(event_handler: H) -> Self {
        Self {
            event_handler,
            event_marker: PhantomData,
        }
    }
}

/// Type-erased metadata for stored event handlers.
pub trait DynEventHandlerHolder {
    /// Returns the event type handled by this value.
    fn event_type_id(&self) -> TypeId;

    /// Returns the concrete handler type stored by this value.
    fn handler_type_id(&self) -> TypeId;

    /// Dispatches a type-erased event context to the stored handler.
    fn handle_event(&mut self, context: &mut dyn Any) -> PixuiResult<()>;
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

    fn handle_event(&mut self, context: &mut dyn Any) -> PixuiResult<()> {
        let context = context
            .downcast_mut::<ApplicationEventContext<E>>()
            .with_context(|| {
                format!(
                    "failed to downcast event context for handler {:?}",
                    TypeId::of::<H>()
                )
            })?;
        self.event_handler.handle_event(context)
    }
}

#[cfg(test)]
mod tests {
    use super::Application;
    use crate::app::event_handler::{ApplicationEventContext, ApplicationEventHandler};
    use pixui_base::{Mutex, err};
    use std::sync::Arc;

    #[derive(Debug, PartialEq, Eq)]
    struct TestEvent {
        value: i32,
    }

    struct RecordingHandler {
        seen_values: Arc<Mutex<Vec<i32>>>,
        delta: i32,
    }

    impl ApplicationEventHandler for RecordingHandler {
        type Event = TestEvent;

        fn handle_event(
            &mut self,
            context: &mut ApplicationEventContext<Self::Event>,
        ) -> pixui_base::result::PixuiResult<()> {
            self.seen_values.lock().push(context.event.value);
            context.event.value += self.delta;
            Ok(())
        }
    }

    struct FailingHandler;

    impl ApplicationEventHandler for FailingHandler {
        type Event = TestEvent;

        fn handle_event(
            &mut self,
            _context: &mut ApplicationEventContext<Self::Event>,
        ) -> pixui_base::result::PixuiResult<()> {
            Err(err!("handler failed"))
        }
    }

    #[test]
    fn handle_event_returns_ok_when_no_handler_is_registered() {
        let mut application = Application::new();

        application.handle_event(TestEvent { value: 1 }).unwrap();
    }

    #[test]
    fn handle_event_dispatches_handlers_in_registration_order() {
        let mut application = Application::new();
        let seen_values = Arc::new(Mutex::new(Vec::new()));

        application.add_event_handler(RecordingHandler {
            seen_values: Arc::clone(&seen_values),
            delta: 5,
        });
        application.add_event_handler(RecordingHandler {
            seen_values: Arc::clone(&seen_values),
            delta: 7,
        });

        application.handle_event(TestEvent { value: 10 }).unwrap();

        assert_eq!(*seen_values.lock(), vec![10, 15]);
    }

    #[test]
    fn handle_event_propagates_handler_errors() {
        let mut application = Application::new();
        application.add_event_handler(FailingHandler);

        let error = application
            .handle_event(TestEvent { value: 10 })
            .unwrap_err();

        assert!(error.to_test_string().contains("handler failed"));
    }
}
