use crate::app::{Application, ApplicationHandle, ApplicationMessage};
use pixui_base::logging::error;
use pixui_base::result::PixuiResult;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;

/// Core engine entry point for the pixui project.
pub struct Engine {
    application: ApplicationHandle,
}

impl Engine {
    /// Creates a new engine instance.
    pub fn new() -> PixuiResult<Self> {
        let (tx, rx) = mpsc::sync_channel(1024);
        std::thread::Builder::new()
            .name("pixui Application".to_string())
            .spawn(move || {
                if let Err(error) = run_application(rx) {
                    error!("{:?}", error);
                }
            })?;

        Ok(Self {
            application: ApplicationHandle::new(tx),
        })
    }

    /// Returns the handle for interacting with the application thread.
    pub fn application(&self) -> &ApplicationHandle {
        &self.application
    }
}

fn run_application(message_rx: Receiver<ApplicationMessage>) -> PixuiResult<()> {
    let mut application = Application::new();

    loop {
        let message = message_rx.recv()?;
        match message {
            ApplicationMessage::RunOnce(run_once) => run_once(&mut application)?,
        }
    }
}

#[cfg(test)]
mod tests {}
