//! Internal bounded MPSC worker and typed action replies.

use crossbeam_channel::{Receiver, Sender};
use pixui_base::{PixuiResult, pixui_error};

use super::{
    action::{ActionCall, ActionOutput},
    app::Application,
};

pub(super) type CommandSender = Sender<ApplicationCommand>;
pub(super) type CommandReceiver = Receiver<ApplicationCommand>;
pub(super) type ReplySender<T> = Sender<PixuiResult<T>>;
type ReplyReceiver<T> = Receiver<PixuiResult<T>>;
type ApplicationTask = Box<dyn FnOnce(&mut Application) + Send>;

pub(super) enum ApplicationCommand {
    Dispatch {
        call: ActionCall,
        reply: ReplySender<ActionOutput>,
    },
    Task(ApplicationTask),
    Ui {
        command: crate::ui::input::UiCommand,
        reply: ReplySender<()>,
    },
}

impl ApplicationCommand {
    fn run(self, application: &mut Application) {
        match self {
            Self::Dispatch { call, reply } => {
                let _ = reply.send(application.dispatch(call));
            }
            Self::Task(task) => {
                // Inspection is a barrier: observe UI output and geometry for
                // preceding commands, even inside a rendering batch.
                application.render_dirty();
                task(application);
            }
            Self::Ui { command, reply } => {
                let _ = reply.send(application.ui_command(command));
            }
        }
    }
}

/// An action's eventual result. Dropping the reply does not cancel its action.
pub type PendingAction = ApplicationReply<ActionOutput>;

/// A single owned result from the application worker.
pub struct ApplicationReply<T> {
    receiver: ReplyReceiver<T>,
}

impl<T> ApplicationReply<T> {
    /// Polls completion without blocking a native event loop.
    pub fn try_recv(&self) -> Result<PixuiResult<T>, crossbeam_channel::TryRecvError> {
        self.receiver.try_recv()
    }

    pub(super) fn new(receiver: ReplyReceiver<T>) -> Self {
        Self { receiver }
    }

    /// Waits for completion. Worker failure returns an error instead of hanging.
    pub fn wait(self) -> PixuiResult<T> {
        self.receiver
            .recv()
            .map_err(|_| pixui_error!("application worker stopped before replying"))?
    }
}

/// Executes commands on one owner thread until every sender is dropped.
/// A panicking command invalidates the state. The failed worker drains and drops
/// remaining commands, disconnecting their replies, until all handles are gone.
/// This prevents replies retained by the bounded queue from leaving callers stuck.
pub(super) fn run(receiver: CommandReceiver) {
    let mut application = Some(Application::default());
    loop {
        let command = match application.as_ref().and_then(Application::next_ui_refresh) {
            Some(deadline) => {
                receiver.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
            }
            None => receiver
                .recv()
                .map_err(|_| crossbeam_channel::RecvTimeoutError::Disconnected),
        };
        let command = match command {
            Ok(command) => Some(command),
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => None,
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        };
        if let Some(state) = application.as_mut() {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if let Some(command) = command {
                    command.run(state);
                }
                // Bounded batches avoid rendering once per queued action while
                // ensuring a busy producer cannot indefinitely starve painting.
                for _ in 0..31 {
                    let Ok(command) = receiver.try_recv() else {
                        break;
                    };
                    command.run(state);
                }
                state.render_dirty();
            }));
            if result.is_err() {
                application = None;
            }
        }
        // In the failed state, dropping each command disconnects its reply.
    }
}
