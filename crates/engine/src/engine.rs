use crate::app::Application;
use crate::draw::draw_list::DrawList;
use crate::engine_event_handler::EngineEventHandler;
use crate::engine_message::EngineMessage;
use crate::engine_state::EngineState;
use crate::viewport::Viewport;
use pixui_base::bail;
use pixui_base::logging::error;
use pixui_base::result::{PixuiResult, ResultExt};
use pixui_base::shared_string::SharedString;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::SyncSender;

/// Core engine entry point for the pixui project.
#[derive(Clone)]
pub struct Engine {
    tx: SyncSender<EngineMessage>,
}

impl Engine {
    /// Creates a new engine instance.
    pub fn new() -> PixuiResult<Self> {
        let (tx, rx) = mpsc::sync_channel(1024);
        std::thread::Builder::new()
            .name("pixui Engine".to_string())
            .spawn(move || {
                if let Err(error) = run_engine(rx) {
                    error!("{:?}", error);
                }
            })?;

        Ok(Self { tx })
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
    pub fn register_event_handler<E: 'static, H: EngineEventHandler<E> + 'static>(
        &self,
        handler: H,
    ) -> PixuiResult<()> {
        self.run(|engine| {
            engine.register_event_handler(handler);
            Ok(())
        })
    }

    /// Registers a closure as an event handler for `E`.
    pub fn on_event<E: 'static>(
        &self,
        handler: impl FnMut(
            &mut crate::engine_event_context::EngineEventContext<'_, E>,
        ) -> PixuiResult<()>
        + Send
        + 'static,
    ) -> PixuiResult<()> {
        self.register_event_handler(handler)
    }

    /// Registers a named component renderer that can produce draw commands.
    pub fn register_component_renderer<R>(
        &self,
        component_name: impl Into<SharedString>,
        renderer: R,
    ) -> PixuiResult<()>
    where
        R: for<'a, 'b> Fn(&'a Application, &'b Viewport) -> PixuiResult<DrawList> + Send + 'static,
    {
        let component_name = component_name.into();
        self.run(move |engine| {
            engine
                .application_mut()
                .register_component_renderer(component_name, renderer);
            Ok(())
        })
    }

    /// Renders a named component for the provided viewport.
    pub fn render_component(
        &self,
        component_name: impl Into<SharedString>,
        viewport: Viewport,
    ) -> PixuiResult<DrawList> {
        let component_name = component_name.into();
        self.run_application(move |application| {
            application.render_component(component_name.as_ref(), &viewport)
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

    /// Sends `message` to the engine thread.
    fn send_message(&self, message: EngineMessage) -> PixuiResult<()> {
        match self.tx.send(message) {
            Ok(_) => Ok(()),
            Err(_err) => {
                bail!("Failed to send engine message, engine is terminated");
            }
        }
    }
}

fn run_engine(message_rx: Receiver<EngineMessage>) -> PixuiResult<()> {
    let mut engine = EngineState::new();

    loop {
        let message = message_rx.recv()?;
        match message {
            EngineMessage::RunOnce(run_once) => run_once(&mut engine)?,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Engine;
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

    impl EngineEventHandler<TestEvent> for RecordingHandler {
        fn handle_event(
            &mut self,
            context: &mut EngineEventContext<'_, TestEvent>,
        ) -> pixui_base::result::PixuiResult<()> {
            self.seen_values.lock().push(context.event.value);
            context.event.value += self.delta;
            Ok(())
        }
    }

    struct FailingHandler;

    impl EngineEventHandler<TestEvent> for FailingHandler {
        fn handle_event(
            &mut self,
            _context: &mut EngineEventContext<'_, TestEvent>,
        ) -> pixui_base::result::PixuiResult<()> {
            Err(err!("handler failed"))
        }
    }

    #[test]
    fn run_application_executes_callback_on_engine_thread_and_returns_result() -> PixuiResult<()> {
        let engine = Engine::new()?;

        let entity_store_ptr = engine
            .run_application(|application| Ok(application.entity_store() as *const _ as usize))?;

        assert_ne!(entity_store_ptr, 0);
        Ok(())
    }

    #[test]
    fn run_application_returns_callback_result_after_mutating_application() -> PixuiResult<()> {
        let engine = Engine::new()?;

        let entity_count = engine.run_application(|application| -> PixuiResult<usize> {
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
        let seen_values = Arc::new(Mutex::new(Vec::new()));

        engine.register_event_handler(RecordingHandler {
            seen_values: Arc::clone(&seen_values),
            delta: 5,
        })?;
        engine.register_event_handler(RecordingHandler {
            seen_values: Arc::clone(&seen_values),
            delta: 7,
        })?;

        engine.submit_event(TestEvent { value: 10 })?;

        assert_eq!(*seen_values.lock(), vec![10, 15]);
        Ok(())
    }

    #[test]
    fn register_event_handler_accepts_closures() -> PixuiResult<()> {
        let engine = Engine::new()?;
        let seen_values = Arc::new(Mutex::new(Vec::new()));
        let seen_values_for_handler = Arc::clone(&seen_values);

        engine.on_event::<TestEvent>(move |context| {
            seen_values_for_handler.lock().push(context.event.value);
            Ok(())
        })?;

        engine.submit_event(TestEvent { value: 10 })?;

        assert_eq!(*seen_values.lock(), vec![10]);
        Ok(())
    }

    #[test]
    fn submit_event_propagates_handler_errors() -> PixuiResult<()> {
        let engine = Engine::new()?;
        engine.register_event_handler(FailingHandler)?;

        let error = engine.submit_event(TestEvent { value: 10 }).unwrap_err();

        assert!(error.to_test_string().contains("handler failed"));
        Ok(())
    }
}
