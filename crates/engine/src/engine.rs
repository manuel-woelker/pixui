use crate::engine_handle::EngineHandle;
use crate::engine_message::EngineMessage;
use crate::engine_state::EngineState;
use pixui_base::logging::error;
use pixui_base::result::PixuiResult;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;

/// Core engine entry point for the pixui project.
pub struct Engine {
    handle: EngineHandle,
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

        Ok(Self {
            handle: EngineHandle::new(tx),
        })
    }

    /// Returns the handle for interacting with the engine thread.
    pub fn handle(&self) -> EngineHandle {
        self.handle.clone()
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
mod tests {}
