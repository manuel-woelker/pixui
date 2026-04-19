use crate::app::Application;
use crate::engine_event_handler::EngineEventHandler;
use crate::engine_message::EngineMessage;
use crate::engine_state::EngineState;
use pixui_base::bail;
use pixui_base::result::{PixuiResult, ResultExt};
use std::sync::mpsc;
use std::sync::mpsc::SyncSender;

/// Thread-safe handle for submitting work to the engine.
#[derive(Clone)]
pub struct EngineHandle {
    tx: SyncSender<EngineMessage>,
}

impl EngineHandle {
    /// Creates a handle backed by `tx`.
    pub(crate) fn new(tx: SyncSender<EngineMessage>) -> Self {
        EngineHandle { tx }
    }

    /// Sends `message` to the engine thread.
    fn send_message(&self, message: EngineMessage) -> PixuiResult<()> {
        match self.tx.send(message) {
            Ok(_) => Ok(()),
            Err(_err) => {
                bail!("Failed to send engine message, engine is terminated");
            }
        }
    }

    /// Runs `callback` on the engine thread and returns its result.
    fn run<T, F>(&self, callback: F) -> PixuiResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut EngineState) -> PixuiResult<T> + Send + 'static,
    {
        let (result_tx, result_rx) = mpsc::sync_channel(1);
        self.send_message(EngineMessage::RunOnce(Box::new(move |engine| {
            let result = callback(engine);
            match result_tx.send(result) {
                Ok(()) => {}
                Err(_err) => {
                    bail!("Failed to send engine run result");
                }
            }
            Ok(())
        })))?;
        result_rx
            .recv()
            .with_context(|| "Failed to receive engine run result")?
    }

    /// Runs `callback` against the engine-owned application state.
    pub fn run_application<T, F>(&self, callback: F) -> PixuiResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Application) -> PixuiResult<T> + Send + 'static,
    {
        self.run(|engine| callback(engine.application_mut()))
    }

    /// Registers an event handler for `E`.
    ///
    /// Multiple handlers can be registered for the same event type. Handlers
    /// are invoked in registration order when matching events are submitted.
    pub fn register_event_handler<E: 'static, H: EngineEventHandler<Event = E> + 'static>(
        &self,
        handler: H,
    ) -> PixuiResult<()> {
        self.run(|engine| {
            engine.register_event_handler(handler);
            Ok(())
        })
    }

    /// Submits `event` to every registered handler for `E`.
    ///
    /// Handlers run in the same order they were registered and receive a
    /// shared mutable event context, which allows earlier handlers to affect
    /// what later handlers observe.
    pub fn submit_event<E: Send + 'static>(&self, event: E) -> PixuiResult<()> {
        self.run(|engine| engine.submit_event(event))
    }
}

#[cfg(test)]
mod tests {
    use crate::engine::Engine;
    use crate::engine_event_context::EngineEventContext;
    use crate::engine_event_handler::EngineEventHandler;
    use facet::Facet;
    use pixui_base::result::PixuiResult;
    use pixui_base::{Mutex, err};
    use std::sync::Arc;

    #[derive(Debug, Facet)]
    struct TestEntity {
        name: String,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct TestEvent {
        value: i32,
    }

    struct RecordingHandler {
        seen_values: Arc<Mutex<Vec<i32>>>,
        delta: i32,
    }

    impl EngineEventHandler for RecordingHandler {
        type Event = TestEvent;

        fn handle_event(
            &mut self,
            context: &mut EngineEventContext<Self::Event>,
        ) -> pixui_base::result::PixuiResult<()> {
            self.seen_values.lock().push(context.event.value);
            context.event.value += self.delta;
            Ok(())
        }
    }

    struct FailingHandler;

    impl EngineEventHandler for FailingHandler {
        type Event = TestEvent;

        fn handle_event(
            &mut self,
            _context: &mut EngineEventContext<Self::Event>,
        ) -> pixui_base::result::PixuiResult<()> {
            Err(err!("handler failed"))
        }
    }

    #[test]
    fn run_application_executes_callback_on_engine_thread_and_returns_result() -> PixuiResult<()> {
        let engine = Engine::new()?;
        let engine_handle = engine.handle();

        let entity_store_ptr = engine_handle
            .run_application(|application| Ok(application.entity_store() as *const _ as usize))?;

        assert_ne!(entity_store_ptr, 0);
        Ok(())
    }

    #[test]
    fn run_application_returns_callback_result_after_mutating_application() -> PixuiResult<()> {
        let engine = Engine::new()?;
        let engine_handle = engine.handle();

        let entity_count = engine_handle.run_application(|application| -> PixuiResult<usize> {
            let store = application.entity_store_mut();
            store.register_entity_type::<TestEntity>()?;
            Ok(store
                .add_entities([TestEntity {
                    name: "alpha".to_string(),
                }])?
                .len())
        })?;

        assert_eq!(entity_count, 1);
        Ok(())
    }

    #[test]
    fn submit_event_dispatches_registered_handlers_in_order() -> PixuiResult<()> {
        let engine = Engine::new()?;
        let engine_handle = engine.handle();
        let seen_values = Arc::new(Mutex::new(Vec::new()));

        engine_handle.register_event_handler(RecordingHandler {
            seen_values: Arc::clone(&seen_values),
            delta: 5,
        })?;
        engine_handle.register_event_handler(RecordingHandler {
            seen_values: Arc::clone(&seen_values),
            delta: 7,
        })?;

        engine_handle.submit_event(TestEvent { value: 10 })?;

        assert_eq!(*seen_values.lock(), vec![10, 15]);
        Ok(())
    }

    #[test]
    fn submit_event_propagates_handler_errors() -> PixuiResult<()> {
        let engine = Engine::new()?;
        let engine_handle = engine.handle();
        engine_handle.register_event_handler(FailingHandler)?;

        let error = engine_handle
            .submit_event(TestEvent { value: 10 })
            .unwrap_err();

        assert!(error.to_test_string().contains("handler failed"));
        Ok(())
    }
}
