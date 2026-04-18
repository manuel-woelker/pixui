use std::any::TypeId;
use std::sync::mpsc::SyncSender;
use pixui_base::bail;
use pixui_base::result::{PixuiResult};
use crate::app::application_message::ApplicationMessage;
use crate::app::event_handler::ApplicationEventHandler;

pub struct ApplicationHandle {
    tx: SyncSender<ApplicationMessage>,
}

impl ApplicationHandle {
    pub fn new(tx: SyncSender<ApplicationMessage>) -> Self {
        ApplicationHandle { tx }
    }
    pub fn send_message(&self, message: ApplicationMessage) -> PixuiResult<()> {
        match self.tx.send(message) {
            Ok(_) => {
                Ok(())
            }
            Err(err) => {
                bail!("Failed to send application message, application is terminated");
            }
        }
    }

    /// Registers an event handler for `E`.
    ///
    /// Multiple handlers can be registered for the same event type. Handlers
    /// are grouped by event type inside the application.
    pub fn add_event_handler<E: 'static, H: ApplicationEventHandler<Event = E> + 'static>(
        &self,
        handler: H,
    ) -> PixuiResult<()> {
        self.send_message(ApplicationMessage::RunOnce(Box::new(|application| {
            application.add_event_handler(handler);
            Ok(())
        })))
    }

    /// Dispatches `event` to every registered handler for `E`.
    ///
    /// Handlers run in the same order they were registered and receive a
    /// shared mutable [`ApplicationEventContext`], which allows earlier
    /// handlers to affect what later handlers observe.
    pub fn handle_event<E: Send + 'static>(&self, event: E) -> PixuiResult<()> {
        self.send_message(ApplicationMessage::RunOnce(Box::new(|application| {
            application.handle_event(event)
        })))
    }
}