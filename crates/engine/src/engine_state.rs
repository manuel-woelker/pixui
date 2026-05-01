use crate::app::Application;
use crate::engine_event_context::EngineEventContext;
use crate::engine_event_handler::EngineEventHandler;
use pixui_base::result::{OptionExt, PixuiResult};
use pixui_base::type_map::TypeMap;
use std::any::Any;
use std::any::TypeId;
use std::marker::PhantomData;

/// Engine-owned runtime state.
#[derive(Default)]
pub(crate) struct EngineState {
    /// Application state owned by the engine.
    application: Application,
    /// Event handlers grouped by their event type.
    event_handlers: TypeMap<Vec<Box<dyn DynEventHandlerHolder>>>,
}

impl EngineState {
    /// Creates empty engine state.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Returns mutable application state owned by this engine.
    pub(crate) fn application_mut(&mut self) -> &mut Application {
        &mut self.application
    }

    /// Registers an event handler for `E`.
    ///
    /// Multiple handlers can be registered for the same event type. Handlers
    /// are grouped by event type inside the engine.
    pub(crate) fn register_event_handler<E: 'static, H: EngineEventHandler<Event = E> + 'static>(
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

    /// Submits `event` to every registered handler for `E`.
    ///
    /// Handlers run in the same order they were registered and receive a
    /// shared mutable [`EngineEventContext`], which allows earlier handlers to
    /// affect what later handlers observe.
    pub(crate) fn submit_event<E: 'static>(&mut self, event: E) -> PixuiResult<()> {
        let Some(handlers) = self.event_handlers.get_mut::<E>() else {
            return Ok(());
        };

        let mut context = EngineEventContext { event };
        for handler in handlers {
            handler.handle_event(&mut self.application, &mut context)?;
        }

        Ok(())
    }
}

/// Stores a concrete event handler together with the event type it handles.
struct EventHandlerHolder<E, H: EngineEventHandler<Event = E>> {
    /// The concrete event handler implementation.
    event_handler: H,
    /// Tracks `E` at the type level without storing a runtime value.
    event_marker: PhantomData<fn(E)>,
}

impl<E, H: EngineEventHandler<Event = E>> EventHandlerHolder<E, H> {
    /// Creates a holder for a concrete event handler.
    fn new(event_handler: H) -> Self {
        Self {
            event_handler,
            event_marker: PhantomData,
        }
    }
}

/// Type-erased metadata for stored event handlers.
trait DynEventHandlerHolder {
    /// Returns the event type handled by this value.
    fn event_type_id(&self) -> TypeId;

    /// Returns the concrete handler type stored by this value.
    fn handler_type_id(&self) -> TypeId;

    /// Dispatches a type-erased event context to the stored handler.
    fn handle_event(
        &mut self,
        application: &mut Application,
        context: &mut dyn Any,
    ) -> PixuiResult<()>;
}

impl<E, H> DynEventHandlerHolder for EventHandlerHolder<E, H>
where
    E: 'static,
    H: EngineEventHandler<Event = E> + 'static,
{
    fn event_type_id(&self) -> TypeId {
        TypeId::of::<E>()
    }

    fn handler_type_id(&self) -> TypeId {
        let _ = &self.event_handler;
        TypeId::of::<H>()
    }

    fn handle_event(
        &mut self,
        application: &mut Application,
        context: &mut dyn Any,
    ) -> PixuiResult<()> {
        let context = context
            .downcast_mut::<EngineEventContext<E>>()
            .with_context(|| {
                format!(
                    "failed to downcast event context for handler {:?}",
                    TypeId::of::<H>()
                )
            })?;
        self.event_handler.handle_event(application, context)
    }
}

#[cfg(test)]
mod tests {
    use super::EngineState;
    use crate::app::Application;
    use crate::engine_event_context::EngineEventContext;
    use crate::engine_event_handler::EngineEventHandler;
    use crate::entity::store::TypedEntityKey;
    use facet::Facet;
    use pixui_base::result::PixuiResult;
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

    #[derive(Debug, Facet, PartialEq, Eq)]
    struct TestEntity {
        value: i32,
    }

    struct ApplicationMutatingHandler {
        entity_key: TypedEntityKey<TestEntity>,
    }

    impl EngineEventHandler for RecordingHandler {
        type Event = TestEvent;

        fn handle_event(
            &mut self,
            _application: &mut Application,
            context: &mut EngineEventContext<Self::Event>,
        ) -> PixuiResult<()> {
            self.seen_values.lock().push(context.event.value);
            context.event.value += self.delta;
            Ok(())
        }
    }

    impl EngineEventHandler for ApplicationMutatingHandler {
        type Event = TestEvent;

        fn handle_event(
            &mut self,
            application: &mut Application,
            context: &mut EngineEventContext<Self::Event>,
        ) -> PixuiResult<()> {
            let entity = application
                .entity_store_mut()
                .get_entity_mut(self.entity_key)?;
            entity.value += context.event.value;
            Ok(())
        }
    }

    struct FailingHandler;

    impl EngineEventHandler for FailingHandler {
        type Event = TestEvent;

        fn handle_event(
            &mut self,
            _application: &mut Application,
            _context: &mut EngineEventContext<Self::Event>,
        ) -> PixuiResult<()> {
            Err(err!("handler failed"))
        }
    }

    #[test]
    fn submit_event_returns_ok_when_no_handler_is_registered() -> PixuiResult<()> {
        let mut engine = EngineState::new();

        engine.submit_event(TestEvent { value: 1 })?;
        Ok(())
    }

    #[test]
    fn submit_event_dispatches_handlers_in_registration_order() -> PixuiResult<()> {
        let mut engine = EngineState::new();
        let seen_values = Arc::new(Mutex::new(Vec::new()));

        engine.register_event_handler(RecordingHandler {
            seen_values: Arc::clone(&seen_values),
            delta: 5,
        });
        engine.register_event_handler(RecordingHandler {
            seen_values: Arc::clone(&seen_values),
            delta: 7,
        });

        engine.submit_event(TestEvent { value: 10 })?;

        assert_eq!(*seen_values.lock(), vec![10, 15]);
        Ok(())
    }

    #[test]
    fn submit_event_handlers_can_mutate_application_state() -> PixuiResult<()> {
        let mut engine = EngineState::new();
        let entity_key = engine
            .application_mut()
            .add_entity(TestEntity { value: 1 })?;

        engine.register_event_handler(ApplicationMutatingHandler { entity_key });
        engine.submit_event(TestEvent { value: 4 })?;

        assert_eq!(
            engine
                .application_mut()
                .entity_store()
                .get_entity(entity_key)?
                .value,
            5
        );
        Ok(())
    }

    #[test]
    fn submit_event_propagates_handler_errors() -> PixuiResult<()> {
        let mut engine = EngineState::new();
        engine.register_event_handler(FailingHandler);

        let error = engine.submit_event(TestEvent { value: 10 }).unwrap_err();

        assert!(error.to_test_string().contains("handler failed"));
        Ok(())
    }
}
