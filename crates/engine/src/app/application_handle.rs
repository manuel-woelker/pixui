use crate::app::Application;
use crate::app::application_message::ApplicationMessage;
use crate::app::event_handler::ApplicationEventHandler;
use pixui_base::bail;
use pixui_base::result::{PixuiResult, ResultExt};
use std::sync::mpsc;
use std::sync::mpsc::SyncSender;

pub struct ApplicationHandle {
    tx: SyncSender<ApplicationMessage>,
}

impl ApplicationHandle {
    pub fn new(tx: SyncSender<ApplicationMessage>) -> Self {
        ApplicationHandle { tx }
    }
    pub fn send_message(&self, message: ApplicationMessage) -> PixuiResult<()> {
        match self.tx.send(message) {
            Ok(_) => Ok(()),
            Err(_err) => {
                bail!("Failed to send application message, application is terminated");
            }
        }
    }

    /// Runs `callback` on the application thread and returns its result.
    pub fn run<T, F>(&self, callback: F) -> PixuiResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Application) -> PixuiResult<T> + Send + 'static,
    {
        let (result_tx, result_rx) = mpsc::sync_channel(1);
        self.send_message(ApplicationMessage::RunOnce(Box::new(move |application| {
            let result = callback(application);
            match result_tx.send(result) {
                Ok(()) => {}
                Err(_err) => {
                    bail!("Failed to send application run result");
                }
            }
            Ok(())
        })))?;
        result_rx
            .recv()
            .with_context(|| "Failed to receive application run result")?
    }

    /// Registers an event handler for `E`.
    ///
    /// Multiple handlers can be registered for the same event type. Handlers
    /// are grouped by event type inside the application.
    pub fn add_event_handler<E: 'static, H: ApplicationEventHandler<Event = E> + 'static>(
        &self,
        handler: H,
    ) -> PixuiResult<()> {
        self.run(|application| {
            application.add_event_handler(handler);
            Ok(())
        })
    }

    /// Dispatches `event` to every registered handler for `E`.
    ///
    /// Handlers run in the same order they were registered and receive a
    /// shared mutable [`ApplicationEventContext`], which allows earlier
    /// handlers to affect what later handlers observe.
    pub fn handle_event<E: Send + 'static>(&self, event: E) -> PixuiResult<()> {
        self.run(|application| application.handle_event(event))
    }
}

#[cfg(test)]
mod tests {
    use crate::app::Application;
    use facet::Facet;
    use pixui_base::result::PixuiResult;

    #[derive(Debug, Facet)]
    struct TestEntity {
        name: String,
    }

    #[test]
    fn run_executes_callback_on_application_thread_and_returns_result() -> PixuiResult<()> {
        let application = Application::spawn()?;

        let entity_store_ptr =
            application.run(|application| Ok(application.entity_store() as *const _ as usize))?;

        assert_ne!(entity_store_ptr, 0);
        Ok(())
    }

    #[test]
    fn run_returns_callback_result_after_mutating_application() -> PixuiResult<()> {
        let application = Application::spawn()?;

        let entity_count = application.run(|application| -> PixuiResult<usize> {
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
}
